//! The instance editor (owner, 2026-11-02: "an edit option in the gui and
//! also a clone option so people can easily tweak a recipe"; reworked
//! 2026-11-05 into pages): Base game (`base_tab`: the instance's name, its
//! game folder checked against the recipe's base hashes and pinned
//! overlays), Add-ons (`entries`: standalone games and add-ons, each with
//! its folder, load order and parts), Balance (`instance`) and, when
//! Settings shows it, Advanced (the base game's parts, `rules` and
//! `predation`). The recipe (shareable: credits, links, hashes) is written
//! over its file on save; the folders (`Draft`) are kept by the launcher.

mod base_tab;
pub use base_tab::BaseAsk;
mod entries;
mod instance;
mod predation;
mod rules;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use c2_mods::recipe::{self, Recipe, RecipeMod};
use eframe::egui::{self, Color32, RichText};

use crate::theme::*;

/// Which list an entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Overlay,
    Standalone,
    AddOn,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Overlay => "Overlays (copied over the base game first)",
            Kind::Standalone => "Standalone games (combined beside the base)",
            Kind::AddOn => "Add-ons (in load order: later ones win)",
        }
    }
}

/// The instance's own folders, kept by the launcher (not in the recipe,
/// which is shareable): its game, any unpinned overlays over it, and each
/// entry's folder.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Draft {
    pub game: Option<PathBuf>,
    pub overlays: Vec<PathBuf>,
    pub folders: BTreeMap<String, PathBuf>,
}

impl Draft {
    /// The game and its overlays, in order.
    pub fn game_dirs(&self) -> Vec<PathBuf> {
        self.game.iter().chain(&self.overlays).cloned().collect()
    }
}

/// Which base profile the instance's game folder is (read off the UI
/// thread from the hashes in `bases/*.toml`).
#[derive(Debug, Clone, PartialEq)]
enum BaseMatch {
    Unchecked,
    Checking(PathBuf),
    Done(PathBuf, Option<String>),
}

pub struct Editor {
    pub path: PathBuf,
    pub recipe: Recipe,
    saved: Recipe,
    pub draft: Draft,
    saved_draft: Draft,
    /// Base profile ids found beside the recipe (`bases/*.toml`).
    bases: Vec<String>,
    base_match: BaseMatch,
    /// A game folder was just picked: its base profile becomes the
    /// recipe's once read.
    take_base: bool,
    adding: bool,
    /// Folders still to pin, one job at a time.
    queue: Vec<(Kind, PathBuf)>,
    note: Option<(String, Color32)>,
    extras: instance::Extras,
}

/// The ids of the base profiles in the first `bases/` folder beside or
/// above `recipe_path`.
fn base_ids(recipe_path: &Path) -> Vec<String> {
    let Some(dir) = recipe_path
        .ancestors()
        .skip(1)
        .map(|d| d.join("bases"))
        .find(|d| d.is_dir())
    else {
        return Vec::new();
    };
    let mut ids: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let t = std::fs::read_to_string(e.path()).ok()?;
            Some(toml::from_str::<recipe::Base>(&t).ok()?.base.id)
        })
        .filter(|id| !id.is_empty())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The recipe with entry `id` taken out of every list, the load order and
/// the resolutions it wins.
pub fn remove_entry(r: &mut Recipe, id: &str) {
    for list in [&mut r.overlays, &mut r.standalones, &mut r.mods] {
        list.retain(|m| m.id != id);
    }
    r.load_order.order.retain(|o| o != id);
    r.resolve.retain(|x| x.winner != id);
    if r.standalones.is_empty() {
        r.combine = None;
    }
}

/// The add-ons' ids in load order (the recipe's `load_order`, else as listed).
pub fn add_on_order(r: &Recipe) -> Vec<String> {
    if r.load_order.order.is_empty() {
        r.mods.iter().map(|m| m.id.clone()).collect()
    } else {
        r.load_order.order.clone()
    }
}

/// Add-on `id` moved `by` places in the load order (the list kept in the
/// same order, so the file reads as it loads).
pub fn move_add_on(r: &mut Recipe, id: &str, by: isize) {
    let mut order = add_on_order(r);
    let Some(i) = order.iter().position(|o| o == id) else {
        return;
    };
    let j = i as isize + by;
    if j < 0 || j as usize >= order.len() {
        return;
    }
    order.swap(i, j as usize);
    r.mods
        .sort_by_key(|m| order.iter().position(|o| *o == m.id).unwrap_or(usize::MAX));
    r.load_order.order = order;
}

/// Puts a pinned entry in its list (an add-on last in the load order).
/// Refused if the recipe has an entry by that id already.
pub fn add_entry(r: &mut Recipe, kind: Kind, m: RecipeMod) -> Result<(), String> {
    if recipe::entries(r).any(|(_, e, _)| e.id == m.id) {
        return Err(format!("the recipe has '{}' already", m.id));
    }
    match kind {
        Kind::Overlay => r.overlays.push(m),
        Kind::Standalone => {
            r.standalones.push(m);
            r.combine.get_or_insert_with(|| recipe::Combine {
                ambients: "tied".into(),
            });
        }
        Kind::AddOn => {
            if !r.load_order.order.is_empty() {
                r.load_order.order.push(m.id.clone());
            }
            r.mods.push(m);
        }
    }
    Ok(())
}

/// A recipe saved at a new path: its id and name from the file name.
pub fn renamed_for(r: &Recipe, to: &Path) -> Recipe {
    let stem = to
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let mut r = r.clone();
    r.recipe.id = crate::instances::id_from(&stem);
    r.recipe.name = stem;
    r
}

/// `toml` (a recipe written out) under the comment block that opens
/// `old` (the file it replaces or copies): the instructions at the top of
/// a recipe are kept; comments further down are not.
pub fn with_header(old: &str, toml: &str) -> String {
    let header: Vec<&str> = old
        .lines()
        .take_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#'))
        .collect();
    let n = header
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(0, |i| i + 1);
    if n == 0 {
        return toml.to_string();
    }
    format!("{}\n\n{toml}", header[..n].join("\n"))
}

impl Editor {
    /// The recipe at `path`, with the instance's folders as last kept.
    pub fn open(path: &Path, draft: Draft) -> Result<Self, String> {
        let r = Recipe::load(path).map_err(|e| e.to_string())?;
        Ok(Editor {
            extras: instance::Extras::new(&r),
            path: path.to_path_buf(),
            saved: r.clone(),
            recipe: r,
            saved_draft: draft.clone(),
            draft,
            bases: base_ids(path),
            base_match: BaseMatch::Unchecked,
            take_base: false,
            adding: false,
            queue: Vec::new(),
            note: None,
        })
    }

    /// The settings as the launcher writes them, taken as the saved ones
    /// (so opening an instance is not a change).
    pub fn settle_settings(&mut self, s: c2_mods::recipe::Settings) {
        self.recipe.settings = s.clone();
        self.saved.settings = s;
    }

    pub fn changed(&self) -> bool {
        self.recipe != self.saved || self.draft != self.saved_draft
    }

    /// Pinning folders (the editor waits for it).
    pub fn working(&self) -> bool {
        self.adding || !self.queue.is_empty()
    }

    /// Reading the instance's values (the editor stays usable).
    pub fn reading(&self) -> bool {
        self.extras.reading() || matches!(self.base_match, BaseMatch::Checking(_))
    }

    /// The saved recipe's settings (to put the launcher back on discard).
    pub fn saved_settings(&self) -> &c2_mods::recipe::Settings {
        &self.saved.settings
    }

    /// The instance as the launcher keeps it.
    pub fn to_use(&self, on: bool) -> crate::config::RecipeUse {
        crate::config::RecipeUse {
            path: self.path.clone(),
            game: self.draft.game.clone(),
            overlays: Some(self.draft.overlays.clone()),
            folders: self.draft.folders.clone(),
            on,
        }
    }

    pub(crate) fn values_read(&mut self, values: c2_mods::balance::Balance) {
        self.extras.values_read(values);
    }

    pub(crate) fn base_matched(&mut self, game: PathBuf, id: Option<String>) {
        if self.base_match != BaseMatch::Checking(game.clone()) {
            return;
        }
        if self.take_base {
            self.take_base = false;
            if let Some(id) = &id {
                self.recipe.recipe.base = id.clone();
            }
        }
        self.base_match = BaseMatch::Done(game, id);
    }

    pub(crate) fn pinned(&mut self, kind: Kind, entry: RecipeMod, dir: PathBuf) {
        self.adding = false;
        let id = entry.id.clone();
        self.note = Some(match add_entry(&mut self.recipe, kind, entry) {
            Ok(()) => {
                self.draft.folders.insert(id.clone(), dir);
                (format!("Added '{id}'."), TEXT)
            }
            Err(e) => (format!("Not added: {e}"), RED),
        });
    }

    pub(crate) fn failed(&mut self, error: &str) {
        self.extras.failed(error);
        if let BaseMatch::Checking(g) = &self.base_match {
            self.base_match = BaseMatch::Done(g.clone(), None);
        }
        if self.adding {
            self.note = Some((format!("Not added: {error}"), RED));
        }
        self.adding = false;
    }

    /// The game folder (the game in it, when a download's parent is given).
    pub fn set_game(&mut self, dir: PathBuf) {
        self.draft.game = Some(crate::config::game_root(&dir));
        self.base_match = BaseMatch::Unchecked;
        self.take_base = true;
        self.extras.refresh_list();
    }

    /// Folders to pin as `kind` entries, one after another.
    pub fn queue_pins(&mut self, kind: Kind, dirs: Vec<PathBuf>) {
        self.queue.extend(dirs.into_iter().map(|d| (kind, d)));
    }

    /// Starts the next waiting job (a pin, or reading the game's base), if
    /// the launcher's worker is free.
    pub fn pump(&mut self, handle: &mut crate::jobs::Handle) {
        if self.adding || handle.busy() {
            return;
        }
        if !self.queue.is_empty() {
            let (kind, dir) = self.queue.remove(0);
            self.add_from(kind, dir, self.draft.game_dirs(), handle);
            return;
        }
        if self.base_match == BaseMatch::Unchecked {
            let Some(game) = self.draft.game.clone() else {
                return;
            };
            let recipe = self.path.clone();
            let g = game.clone();
            match handle.start_task("Reading the base game", move |_| {
                crate::jobs::recipe_edit::match_base(recipe, g)
            }) {
                Ok(()) => self.base_match = BaseMatch::Checking(game),
                Err(e) => {
                    self.note = Some((e, RED));
                    self.base_match = BaseMatch::Done(game, None);
                }
            }
        }
    }

    /// Pins the folder (hashing its files) off the UI thread. An add-on
    /// is read against `game_dirs` (its `_RES.TXT` changes are what it
    /// pins with).
    fn add_from(
        &mut self,
        kind: Kind,
        dir: PathBuf,
        game_dirs: Vec<PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        if self.adding || handle.busy() {
            return;
        }
        match handle.start_task("Pinning recipe entry", move |_| {
            crate::jobs::recipe_edit::pin(kind, dir, game_dirs)
        }) {
            Ok(()) => {
                self.adding = true;
                self.note = Some(("Pinning its files...".into(), TEXT));
            }
            Err(e) => self.failed(&e),
        }
    }

    /// Takes an entry out (with its folder).
    pub fn remove(&mut self, id: &str) {
        remove_entry(&mut self.recipe, id);
        self.draft.folders.remove(id);
        self.note = Some((format!("Removed '{id}'."), TEXT));
    }

    /// Back to the instance as last saved.
    pub fn discard(&mut self) {
        self.recipe = self.saved.clone();
        self.draft = self.saved_draft.clone();
        self.extras = instance::Extras::new(&self.recipe);
        self.base_match = BaseMatch::Unchecked;
        self.take_base = false;
        self.queue.clear();
        self.note = Some(("Changes discarded.".into(), TEXT));
    }

    /// Writes the recipe over its file (the comments at its top kept).
    pub fn save(&mut self) -> Result<(), String> {
        if self.working() {
            return Err("wait for the folders being added".into());
        }
        let ids: std::collections::HashSet<String> = recipe::entries(&self.recipe)
            .map(|(_, m, _)| m.id.clone())
            .collect();
        self.draft.folders.retain(|id, _| ids.contains(id));
        let old = std::fs::read_to_string(&self.path).unwrap_or_default();
        std::fs::write(&self.path, with_header(&old, &self.recipe.to_toml()))
            .map_err(|e| e.to_string())?;
        self.saved = self.recipe.clone();
        self.saved_draft = self.draft.clone();
        self.note = Some(("Saved.".into(), TEXT));
        Ok(())
    }

    /// The editor's last message.
    pub fn note_ui(&self, ui: &mut egui::Ui) {
        if self.working() {
            ui.spinner();
        }
        if let Some((n, c)) = &self.note {
            ui.label(RichText::new(n).color(*c));
        }
    }

    /// The Balance page: starting points, prices and scores.
    pub fn balance_ui(&mut self, ui: &mut egui::Ui, handle: &mut crate::jobs::Handle) {
        let game = self.draft.game_dirs();
        self.extras
            .balance_ui(ui, &mut self.recipe, &game, &self.draft.folders, handle)
    }

    /// The Advanced page: the base game's parts, the instance rules
    /// (`rules`) and "Carnivores hunt herbivores" (`predation`), a panel
    /// each.
    pub fn advanced_ui(&mut self, ui: &mut egui::Ui, handle: &mut crate::jobs::Handle) {
        use crate::app::panel;
        let game = self.draft.game_dirs();
        let (r, extras, folders) = (&mut self.recipe, &mut self.extras, &self.draft.folders);
        if game.is_empty() {
            ui.label(RichText::new("Choose the base game first (Base game page).").color(RED));
        } else {
            panel(ui, |ui| extras.base_parts_ui(ui, r, &game, folders));
        }
        for section in [rules::times_ui, rules::locks_ui] {
            ui.add_space(6.0);
            panel(ui, |ui| section(ui, &mut r.settings.rules));
        }
        ui.add_space(6.0);
        panel(ui, |ui| extras.spawn_ui(ui, r, &game, folders, handle));
        ui.add_space(6.0);
        panel(ui, |ui| extras.predation_ui(ui, r, &game, folders, handle));
    }

    /// Reads the instance's areas, animals and weapons again (after its
    /// base game or add-ons changed).
    pub fn refresh_list(&mut self) {
        self.extras.refresh_list();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> RecipeMod {
        RecipeMod {
            id: id.into(),
            ..Default::default()
        }
    }

    fn sample() -> Recipe {
        Recipe {
            mods: vec![entry("a"), entry("b"), entry("c")],
            load_order: recipe::LoadOrder {
                order: vec!["a".into(), "b".into(), "c".into()],
            },
            standalones: vec![entry("game")],
            combine: Some(recipe::Combine {
                ambients: "tied".into(),
            }),
            resolve: vec![recipe::Resolve {
                path: "HUNTDAT/X.CAR".into(),
                entry: String::new(),
                winner: "b".into(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn removing_an_entry_takes_its_order_and_wins() {
        let mut r = sample();
        remove_entry(&mut r, "b");
        assert_eq!(add_on_order(&r), ["a", "c"]);
        assert!(r.resolve.is_empty());
        remove_entry(&mut r, "game");
        assert!(r.standalones.is_empty() && r.combine.is_none());
    }

    #[test]
    fn add_ons_move_in_the_load_order() {
        let mut r = sample();
        move_add_on(&mut r, "c", -1);
        assert_eq!(add_on_order(&r), ["a", "c", "b"]);
        let ids: Vec<&str> = r.mods.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["a", "c", "b"]);
        // Past either end: nothing.
        move_add_on(&mut r, "a", -1);
        assert_eq!(add_on_order(&r), ["a", "c", "b"]);
    }

    #[test]
    fn an_entry_is_added_once() {
        let mut r = sample();
        add_entry(&mut r, Kind::AddOn, entry("d")).expect("added");
        assert_eq!(add_on_order(&r), ["a", "b", "c", "d"]);
        assert!(add_entry(&mut r, Kind::Overlay, entry("a")).is_err());
        let mut r = Recipe::default();
        add_entry(&mut r, Kind::Standalone, entry("g")).expect("added");
        assert_eq!(r.combine.map(|c| c.ambients).as_deref(), Some("tied"));
    }

    #[test]
    fn the_opening_comments_are_kept() {
        let old = "# How to get it\n#   c2-pc play ...\n\n[recipe]\n# lost\nid = \"x\"\n";
        assert_eq!(
            with_header(old, "[recipe]\nid = \"y\"\n"),
            "# How to get it\n#   c2-pc play ...\n\n[recipe]\nid = \"y\"\n"
        );
        assert_eq!(with_header("[recipe]\n", "a = 1\n"), "a = 1\n");
    }

    #[test]
    fn a_copy_takes_its_file_name() {
        let r = sample();
        let c = renamed_for(&r, Path::new("/x/recipes/My Copy.toml"));
        assert_eq!(
            (c.recipe.id.as_str(), c.recipe.name.as_str()),
            ("my-copy", "My Copy")
        );
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;

    #[test]
    fn pinning_cannot_overlap_and_completed_entry_is_applied_once() {
        let temp = crate::publish::tests::Temp::new();
        let path = temp.0.join("recipe.toml");
        std::fs::write(&path, "[recipe]\nid = 'test'\n").unwrap();
        let dir = temp.0.join("overlay");
        std::fs::create_dir_all(dir.join("HUNTDAT")).unwrap();
        std::fs::write(dir.join("HUNTDAT/X.CAR"), b"synthetic").unwrap();
        std::fs::write(dir.join("HUNTDAT/_RES.TXT"), b"synthetic config").unwrap();
        let mut editor = Editor::open(&path, Draft::default()).unwrap();
        let mut handle = crate::jobs::Handle::default();
        editor.queue_pins(Kind::Overlay, vec![dir.clone()]);
        editor.pump(&mut handle);
        editor.add_from(Kind::Standalone, dir.clone(), vec![], &mut handle);
        assert!(handle.start("overlap", vec![]).is_err());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while handle.result().is_none() {
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        assert!(editor.working());
        match handle.poll() {
            Some(Ok(crate::jobs::Completion::Pinned(pinned))) => {
                let (kind, entry, folder) = *pinned;
                editor.pinned(kind, entry, folder);
            }
            _ => panic!("expected pinned entry"),
        }
        assert!(!editor.working());
        assert_eq!(editor.recipe.overlays.len(), 1);
        assert!(editor.recipe.standalones.is_empty());
        assert_eq!(editor.draft.folders.get("overlay"), Some(&dir));
        assert!(handle.poll().is_none());
    }
}
