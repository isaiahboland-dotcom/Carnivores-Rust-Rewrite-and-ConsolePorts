//! The recipe editor (owner, 2026-11-02: "an edit option in the gui and
//! also a clone option so people can easily tweak a recipe"): a recipe's
//! name, version and base, its entries (credit and download page edited,
//! removed, add-ons put in another load order, new ones added from a
//! folder, pinned as "Save as instance" pins them) and how standalones
//! are combined. Saved over the recipe or as a new one; "Clone" on the
//! Instances list is a save-as straight away. The `[settings]` are left as
//! they are: "Save as instance" takes the other tabs' choices into them.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use c2_mods::recipe::{self, Recipe, RecipeMod};
use eframe::egui::{self, Color32, RichText};

use crate::app::{help, panel};
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

/// An entry pinned from a folder, with that folder.
type Added = Result<(Kind, RecipeMod, PathBuf), String>;

pub struct Editor {
    pub path: PathBuf,
    pub recipe: Recipe,
    saved: Recipe,
    /// Base profile ids found beside the recipe (`bases/*.toml`).
    bases: Vec<String>,
    adding: Option<Receiver<Added>>,
    /// Folders of entries added here: remembered once saved.
    pub added: Vec<(String, PathBuf)>,
    note: Option<(String, Color32)>,
}

/// What the editor asks of the launcher.
pub enum Ask {
    Nothing,
    Close,
    /// Written to `to` (from the recipe at `from`): its folders are to be
    /// remembered under `to` too.
    Saved {
        from: PathBuf,
        to: PathBuf,
    },
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

/// Why a recipe can't be written to `to` (from `from`), if it can't: its
/// patches are found beside it.
pub fn save_problem(r: &Recipe, from: &Path, to: &Path) -> Option<String> {
    (!r.patches.is_empty() && to.parent() != from.parent())
        .then(|| "the recipe has patches beside it: save it in the same folder".to_string())
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

/// Asks where to save a copy of the recipe at `from` (beside it, named
/// `<name>-copy.toml`).
pub fn pick_copy_path(from: &Path) -> Option<PathBuf> {
    let stem = from
        .file_stem()
        .map_or("recipe".into(), |s| s.to_string_lossy().into_owned());
    let mut d = rfd::FileDialog::new()
        .set_file_name(format!("{stem}-copy.toml"))
        .add_filter("Recipe", &["toml"]);
    if let Some(dir) = from.parent() {
        d = d.set_directory(dir);
    }
    d.save_file()
}

impl Editor {
    pub fn open(path: &Path) -> Result<Self, String> {
        let r = Recipe::load(path).map_err(|e| e.to_string())?;
        Ok(Editor {
            path: path.to_path_buf(),
            saved: r.clone(),
            recipe: r,
            bases: base_ids(path),
            adding: None,
            added: Vec::new(),
            note: None,
        })
    }

    pub fn changed(&self) -> bool {
        self.recipe != self.saved
    }

    pub fn working(&self) -> bool {
        self.adding.is_some()
    }

    /// An entry being pinned, once it is.
    pub fn poll(&mut self) {
        let Some(rx) = &self.adding else {
            return;
        };
        let Ok(made) = rx.try_recv() else {
            return;
        };
        self.adding = None;
        self.note = Some(
            match made.and_then(|(kind, m, dir)| {
                let id = m.id.clone();
                add_entry(&mut self.recipe, kind, m)?;
                self.added.push((id.clone(), dir));
                Ok(id)
            }) {
                Ok(id) => (format!("Added '{id}': save to keep it."), TEXT),
                Err(e) => (format!("Not added: {e}"), RED),
            },
        );
    }

    /// Pins the folder (hashing its files) off the UI thread. An add-on
    /// is read against `game_dirs` (its `_RES.TXT` changes are what it
    /// pins with).
    fn add_from(&mut self, kind: Kind, dir: PathBuf, game_dirs: Vec<PathBuf>) {
        if self.working() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let made = (|| -> Added {
                let m = match kind {
                    Kind::Overlay | Kind::Standalone => {
                        recipe::pin_game(&dir).map_err(|e| e.to_string())?
                    }
                    Kind::AddOn => {
                        if game_dirs.is_empty() {
                            return Err(
                                "an add-on is read against the game: set the Game tab's game"
                                    .into(),
                            );
                        }
                        let v = c2_mods::vanilla(&game_dirs).map_err(|e| e.to_string())?;
                        let src = c2_mods::load_mod(&v, &dir).map_err(|e| e.to_string())?;
                        recipe::new_recipe("x", "x", "", &[src])
                            .map_err(|e| e.to_string())?
                            .mods
                            .pop()
                            .ok_or("nothing to pin in it")?
                    }
                };
                Ok((kind, m, dir))
            })();
            let _ = tx.send(made);
        });
        self.adding = Some(rx);
        self.note = Some(("Pinning its files...".into(), TEXT));
    }

    fn write(&mut self, to: &Path) -> Option<Ask> {
        let r = if to == self.path {
            self.recipe.clone()
        } else {
            renamed_for(&self.recipe, to)
        };
        if let Some(p) = save_problem(&r, &self.path, to) {
            self.note = Some((format!("Not saved: {p}."), RED));
            return None;
        }
        let old = std::fs::read_to_string(&self.path).unwrap_or_default();
        if let Err(e) = std::fs::write(to, with_header(&old, &r.to_toml())) {
            self.note = Some((format!("Not saved: {e}"), RED));
            return None;
        }
        let from = std::mem::replace(&mut self.path, to.to_path_buf());
        self.recipe = r.clone();
        self.saved = r;
        self.note = Some((format!("Saved: {}", to.display()), TEXT));
        Some(Ask::Saved {
            from,
            to: to.to_path_buf(),
        })
    }

    /// `game_dirs`: the game add-ons are read against.
    pub fn ui(&mut self, ui: &mut egui::Ui, game_dirs: &[PathBuf]) -> Ask {
        let mut ask = Ask::Nothing;
        ui.label(
            RichText::new(format!("Editing: {}", self.path.display()))
                .size(18.0)
                .color(SAND)
                .strong(),
        );
        help(
            ui,
            "Change the recipe's entries and details here. Files stay pinned by their hashes; \
             an added folder is pinned now. Saving keeps the comments at the top of the file, not \
             ones further down. The menu, survival and 3DS choices are taken from \
             the other tabs with \"Save as instance\".",
        );
        ui.add_space(4.0);
        let r = &mut self.recipe;
        panel(ui, |ui| {
            egui::Grid::new("recipe-edit-info")
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut r.recipe.name);
                    ui.end_row();
                    ui.label("Id");
                    ui.text_edit_singleline(&mut r.recipe.id).on_hover_text(
                        "Lower case letters, digits and '-': the built game's folder name.",
                    );
                    ui.end_row();
                    ui.label("Version");
                    ui.text_edit_singleline(&mut r.recipe.version);
                    ui.end_row();
                    ui.label("Base game");
                    egui::ComboBox::from_id_salt("recipe-edit-base")
                        .selected_text(if r.recipe.base.is_empty() {
                            "any game".to_string()
                        } else {
                            r.recipe.base.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut r.recipe.base, String::new(), "any game");
                            for b in &self.bases {
                                ui.selectable_value(&mut r.recipe.base, b.clone(), b);
                            }
                        });
                    ui.end_row();
                    if let Some(c) = &mut r.combine {
                        ui.label("Ambient animals");
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut c.ambients,
                                "tied".to_string(),
                                "on their own game's maps",
                            );
                            ui.radio_value(&mut c.ambients, "random".to_string(), "on every map");
                        });
                        ui.end_row();
                    }
                });
        });
        ui.add_space(6.0);
        let mut remove: Option<String> = None;
        let mut shift: Option<(String, isize)> = None;
        let mut add: Option<Kind> = None;
        let order = add_on_order(r);
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .id_salt("recipe-edit-rows")
            .show(ui, |ui| {
                for kind in [Kind::Overlay, Kind::Standalone, Kind::AddOn] {
                    let list: Vec<&mut RecipeMod> = match kind {
                        Kind::Overlay => r.overlays.iter_mut().collect(),
                        Kind::Standalone => r.standalones.iter_mut().collect(),
                        Kind::AddOn => {
                            let mut v: Vec<&mut RecipeMod> = r.mods.iter_mut().collect();
                            v.sort_by_key(|m| {
                                order.iter().position(|o| *o == m.id).unwrap_or(usize::MAX)
                            });
                            v
                        }
                    };
                    panel(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(kind.label()).color(SAND).strong());
                            if ui
                                .add_enabled(
                                    self.adding.is_none(),
                                    egui::Button::new("Add from a folder..."),
                                )
                                .clicked()
                            {
                                add = Some(kind);
                            }
                        });
                        if list.is_empty() {
                            ui.label(RichText::new("none").color(DIM));
                            return;
                        }
                        let n = list.len();
                        egui::Grid::new(("recipe-edit", kind as u8))
                            .striped(true)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("id").color(DIM));
                                ui.label(RichText::new("by").color(DIM));
                                ui.label(RichText::new("download page").color(DIM));
                                ui.label("");
                                ui.end_row();
                                for (i, m) in list.into_iter().enumerate() {
                                    ui.label(&m.id)
                                        .on_hover_text(format!("{} files", m.files.len()));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut m.credit)
                                            .desired_width(140.0),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut m.url).desired_width(260.0),
                                    );
                                    ui.horizontal(|ui| {
                                        if kind == Kind::AddOn {
                                            if ui
                                                .add_enabled(i > 0, egui::Button::new("⏶").small())
                                                .on_hover_text("Load earlier")
                                                .clicked()
                                            {
                                                shift = Some((m.id.clone(), -1));
                                            }
                                            if ui
                                                .add_enabled(
                                                    i + 1 < n,
                                                    egui::Button::new("⏷").small(),
                                                )
                                                .on_hover_text("Load later")
                                                .clicked()
                                            {
                                                shift = Some((m.id.clone(), 1));
                                            }
                                        }
                                        if ui.small_button("Remove").clicked() {
                                            remove = Some(m.id.clone());
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                    });
                    ui.add_space(4.0);
                }
            });
        if let Some(id) = remove {
            remove_entry(&mut self.recipe, &id);
            self.added.retain(|(a, _)| *a != id);
            self.note = Some((format!("Removed '{id}': save to keep it."), TEXT));
        }
        if let Some((id, by)) = shift {
            move_add_on(&mut self.recipe, &id, by);
        }
        if let Some(kind) = add {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.add_from(kind, dir, game_dirs.to_vec());
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let changed = self.changed();
            if ui
                .add_enabled(
                    changed && !self.working(),
                    egui::Button::new(RichText::new("Save").color(GOLD).strong()).fill(GREEN),
                )
                .clicked()
            {
                let p = self.path.clone();
                if let Some(a) = self.write(&p) {
                    ask = a;
                }
            }
            if ui
                .add_enabled(!self.working(), egui::Button::new("Save as a new recipe..."))
                .clicked()
            {
                if let Some(to) = pick_copy_path(&self.path) {
                    if let Some(a) = self.write(&to) {
                        ask = a;
                    }
                }
            }
            if ui
                .button("Open in a text editor")
                .on_hover_text("The recipe file itself, for anything not here (resolutions, patches, settings).")
                .clicked()
            {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab(format!("file://{}", self.path.display())));
            }
            if ui
                .button(if changed { "Close without saving" } else { "Close" })
                .clicked()
            {
                ask = Ask::Close;
            }
            if self.working() {
                ui.spinner();
            }
            if let Some((n, c)) = &self.note {
                ui.label(RichText::new(n).color(*c));
            }
        });
        ask
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
    fn a_copy_takes_its_file_name_and_keeps_patches_beside_it() {
        let mut r = sample();
        let c = renamed_for(&r, Path::new("/x/recipes/My Copy.toml"));
        assert_eq!(
            (c.recipe.id.as_str(), c.recipe.name.as_str()),
            ("my-copy", "My Copy")
        );
        let from = Path::new("/x/recipes/a.toml");
        assert!(save_problem(&r, from, Path::new("/y/b.toml")).is_none());
        r.patches.push(recipe::PatchRef::default());
        assert!(save_problem(&r, from, Path::new("/x/recipes/b.toml")).is_none());
        assert!(save_problem(&r, from, Path::new("/y/b.toml")).is_some());
    }
}
