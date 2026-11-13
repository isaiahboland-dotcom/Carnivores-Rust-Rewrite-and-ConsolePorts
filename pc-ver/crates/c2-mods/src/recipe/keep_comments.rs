//! Saving a recipe over its file without losing the author's comments or
//! layout. The recipe is written out as usual and merged into the existing
//! file's document (`toml_edit`): unchanged keys stay as they were written
//! (comments, order, blank lines), changed values are replaced in place with
//! their comments kept, removed keys go with the comments attached to them,
//! and new keys and tables are added at the end of their table, except those
//! that only restate a default (`url = ""`, an empty `[mods.files]`).
//!
//! Arrays of tables (`[[mods]]`, `[[settings.flee.animals]]`, ...) are
//! merged entry by entry by position. Should the merged text ever read back
//! as something other than the recipe, the plain form is written instead.

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use super::Recipe;

impl Recipe {
    /// The recipe written out over `old` (the text of the file it replaces
    /// or is copied from), keeping `old`'s comments and layout where the
    /// recipe leaves them in place. [`Recipe::to_toml`] when `old` is empty
    /// (a new file) or not a readable TOML document.
    pub fn to_toml_over(&self, old: &str) -> String {
        let plain = self.to_toml();
        if old.trim().is_empty() {
            return plain;
        }
        let (Ok(mut doc), Ok(new)) = (old.parse::<DocumentMut>(), plain.parse::<DocumentMut>())
        else {
            return plain;
        };
        let mut added = Vec::new();
        merge_table(
            doc.as_table_mut(),
            new.as_table(),
            &mut Vec::new(),
            &mut added,
        );
        for path in added {
            let mut trial = doc.clone();
            if remove_at(trial.as_table_mut(), &path)
                && Recipe::parse(&trial.to_string()).is_ok_and(|r| r == *self)
            {
                doc = trial;
            }
        }
        let merged = doc.to_string();
        match Recipe::parse(&merged) {
            Ok(back) if back == *self => merged,
            _ => plain,
        }
    }

    /// Saves the recipe to `path`, over the file there (comments kept, see
    /// [`Recipe::to_toml_over`]) or as a new file.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let old = std::fs::read_to_string(path).unwrap_or_default();
        std::fs::write(path, self.to_toml_over(&old))
    }
}

/// A step from a table to one of its items: a key, or an entry of an
/// array of tables.
#[derive(Clone)]
enum Step {
    Key(String),
    Entry(usize),
}

/// The keys the merge added, outermost first, each followed by the keys
/// inside it: the candidates for leaving out again.
type Added = Vec<Vec<Step>>;

fn merge_table(old: &mut Table, new: &Table, at: &mut Vec<Step>, added: &mut Added) {
    old.retain(|k, _| new.contains_key(k));
    for (k, n) in new.iter() {
        at.push(Step::Key(k.to_string()));
        match old.get_mut(k) {
            Some(o) => merge_item(o, n, at, added),
            None => {
                note_added(n, at, added);
                old.insert(k, unpositioned(n.clone()));
            }
        }
        at.pop();
    }
}

fn merge_item(old: &mut Item, new: &Item, at: &mut Vec<Step>, added: &mut Added) {
    match (&mut *old, new) {
        (Item::Table(o), Item::Table(n)) => merge_table(o, n, at, added),
        (Item::ArrayOfTables(o), Item::ArrayOfTables(n)) => merge_array(o, n, at, added),
        (Item::Value(o), Item::Value(n)) => merge_value(o, n),
        // A table the old file writes inline: merged as one, kept inline.
        (Item::Value(Value::InlineTable(o)), Item::Table(n)) => {
            let mut t = o.clone().into_table();
            merge_table(&mut t, n, &mut Vec::new(), &mut Vec::new());
            *o = t.into_inline_table();
        }
        _ => *old = unpositioned(new.clone()),
    }
}

fn merge_array(
    old: &mut ArrayOfTables,
    new: &ArrayOfTables,
    at: &mut Vec<Step>,
    added: &mut Added,
) {
    while old.len() > new.len() {
        old.remove(old.len() - 1);
    }
    for (i, n) in new.iter().enumerate() {
        at.push(Step::Entry(i));
        match old.get_mut(i) {
            Some(o) => merge_table(o, n, at, added),
            None => {
                for (k, item) in n.iter() {
                    at.push(Step::Key(k.to_string()));
                    note_added(item, at, added);
                    at.pop();
                }
                let mut t = n.clone();
                clear_positions(&mut t);
                old.push(t);
            }
        }
        at.pop();
    }
}

/// Records `item`, added at `at`, and the keys of the tables inside it.
fn note_added(item: &Item, at: &mut Vec<Step>, added: &mut Added) {
    added.push(at.clone());
    if let Item::Table(t) = item {
        for (k, i) in t.iter() {
            at.push(Step::Key(k.to_string()));
            note_added(i, at, added);
            at.pop();
        }
    }
}

/// Removes the item at `path`; false when it is no longer there.
fn remove_at(t: &mut Table, path: &[Step]) -> bool {
    let Some((Step::Key(last), parents)) = path.split_last() else {
        return false;
    };
    let mut item: Option<&mut Table> = Some(t);
    let mut i = 0;
    while i < parents.len() {
        let (Some(t), Step::Key(k)) = (item.take(), &parents[i]) else {
            return false;
        };
        item = match (t.get_mut(k), parents.get(i + 1)) {
            (Some(Item::ArrayOfTables(a)), Some(Step::Entry(n))) => {
                i += 1;
                a.get_mut(*n)
            }
            (Some(Item::Table(t)), _) => Some(t),
            _ => None,
        };
        i += 1;
    }
    item.is_some_and(|t| t.remove(last).is_some())
}

/// A changed value keeps the old one's surrounding comments; an equal one
/// is left exactly as written.
fn merge_value(old: &mut Value, new: &Value) {
    if same(old, new) {
        return;
    }
    let decor = old.decor().clone();
    *old = new.clone();
    *old.decor_mut() = decor;
}

/// Whether two values are equal, however they are written.
fn same(a: &Value, b: &Value) -> bool {
    fn read(v: &Value) -> Option<toml::Value> {
        let mut v = v.clone();
        v.decor_mut().clear();
        toml::from_str::<toml::Table>(&format!("v = {v}"))
            .ok()?
            .remove("v")
    }
    matches!((read(a), read(b)), (Some(x), Some(y)) if x == y)
}

/// A table from the written-out recipe placed in the old document: its
/// position there means nothing, so it follows the table before it.
fn unpositioned(mut item: Item) -> Item {
    match &mut item {
        Item::Table(t) => clear_positions(t),
        Item::ArrayOfTables(a) => a.iter_mut().for_each(clear_positions),
        _ => {}
    }
    item
}

fn clear_positions(t: &mut Table) {
    t.set_position(None);
    for (_, item) in t.iter_mut() {
        match item {
            Item::Table(t) => clear_positions(t),
            Item::ArrayOfTables(a) => a.iter_mut().for_each(clear_positions),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = r#"# My modpack: read me first.
# Needs the Modder's Edition.

[recipe]
id = "pack"
name = "Pack"   # shown in the launcher
version = "1"
base = "c2-vanilla"

# Credits: Someone, for the raptor skins.
[[mods]]
id = "raptors"
folder = "raptors"   # the folder name
credit = "Someone"

[[mods]]
id = "maps"   # second
folder = "maps"

[settings.predation]
enabled = true   # carnivores hunt herbivores
# Raptors fight each other too.
carnivore_fights = true

[settings.flee]
enabled = true
max_distance = 30   # cells
"#;

    fn old() -> Recipe {
        Recipe::parse(OLD).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn an_unchanged_recipe_is_written_back_exactly() {
        assert_eq!(old().to_toml_over(OLD), OLD);
    }

    #[test]
    fn a_change_keeps_every_comment_it_does_not_touch() {
        let mut r = old();
        r.recipe.name = "Pack 2".into();
        r.mods[1].folder = "maps-v2".into();
        let text = r.to_toml_over(OLD);
        assert_eq!(Recipe::parse(&text).ok(), Some(r));
        for c in [
            "# My modpack: read me first.",
            "# Needs the Modder's Edition.",
            "name = \"Pack 2\"   # shown in the launcher",
            "# Credits: Someone, for the raptor skins.",
            "folder = \"raptors\"   # the folder name",
            "id = \"maps\"   # second",
            "folder = \"maps-v2\"",
            "# Raptors fight each other too.",
            "max_distance = 30   # cells",
        ] {
            assert!(text.contains(c), "{c}:\n{text}");
        }
    }

    #[test]
    fn a_removed_section_takes_its_comments_with_it() {
        let mut r = old();
        crate::classic::remove(&mut r, crate::classic::Advanced::Predation);
        let text = r.to_toml_over(OLD);
        assert_eq!(Recipe::parse(&text).ok(), Some(r));
        assert!(
            !text.contains("predation") && !text.contains("fight each other"),
            "{text}"
        );
        assert!(text.contains("max_distance = 30   # cells"), "{text}");
        assert!(text.contains("# Credits: Someone"), "{text}");
    }

    #[test]
    fn added_and_removed_entries_keep_the_others() {
        let mut r = old();
        r.mods.remove(1);
        let mut extra = r.mods[0].clone();
        extra.id = "birds".into();
        extra.folder = "birds".into();
        extra.credit = "Another".into();
        r.mods.push(extra);
        if let Some(f) = r.settings.flee.as_mut() {
            f.animals.push(Default::default());
        }
        let text = r.to_toml_over(OLD);
        assert_eq!(Recipe::parse(&text).ok(), Some(r));
        assert!(
            text.contains("folder = \"raptors\"   # the folder name"),
            "{text}"
        );
        assert!(text.contains("id = \"birds\""), "{text}");
        assert!(
            !text.contains("url = \"\""),
            "defaults are not restated:\n{text}"
        );
    }

    #[test]
    fn unreadable_or_empty_old_text_gives_the_plain_form() {
        let r = old();
        assert_eq!(r.to_toml_over(""), r.to_toml());
        assert_eq!(r.to_toml_over("not = = toml"), r.to_toml());
    }

    #[test]
    fn save_keeps_the_files_comments() {
        let dir = std::env::temp_dir().join(format!("c2-keep-comments-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let p = dir.join("pack.toml");
        std::fs::write(&p, OLD).unwrap_or_else(|e| panic!("{e}"));
        let mut r = old();
        r.recipe.version = "2".into();
        r.save(&p).unwrap_or_else(|e| panic!("{e}"));
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        assert!(
            text.starts_with("# My modpack") && text.contains("version = \"2\""),
            "{text}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
