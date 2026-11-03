//! Instances (owner, 2026-11-01): every recipe under the recipes folder,
//! any folder deep (`recipes/instances/testing/` is gitignored for the
//! owner's own), each a whole game: its downloads, the base it expects and
//! its `[settings]`. Play goes through the recipe page, which finds the
//! downloads in the search folders and lists what is missing with the
//! authors' download pages. "Save as instance" writes the launcher's
//! setup as a recipe.

use std::path::{Path, PathBuf};

use c2_mods::recipe::{self, Base, Recipe};
use eframe::egui::{self, RichText};

use crate::app::{help, panel};
use crate::config::Config;
use crate::theme::*;

pub struct Item {
    pub path: PathBuf,
    /// The path under the recipes folder, for the list.
    pub shown: String,
    pub recipe: Recipe,
    /// The recipe's folders were given before (it can play straight away).
    pub known: bool,
}

/// Where to go once a saved instance's downloads have been checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Play,
    Windows,
    ThreeDs,
    WiiU,
}

impl Destination {
    fn visible(self, mode: crate::Mode, cfg: &Config) -> bool {
        mode != crate::Mode::Full
            || match self {
                Self::ThreeDs => cfg.show_3ds == Some(true),
                Self::WiiU => cfg.show_wiiu != Some(false),
                _ => true,
            }
    }

    pub fn offered(mode: crate::Mode) -> &'static [(Self, &'static str)] {
        match mode {
            crate::Mode::Full => &[
                (Self::Play, "Play"),
                (Self::Windows, "Build Windows…"),
                (Self::ThreeDs, "Build 3DS…"),
                (Self::WiiU, "Build Wii U…"),
            ],
            crate::Mode::ThreeDs => &[(Self::ThreeDs, "Build 3DS…")],
            crate::Mode::WiiU => &[(Self::WiiU, "Build Wii U…")],
        }
    }
}

/// What the list asks of the launcher.
pub enum Ask {
    Nothing,
    Open(PathBuf),
    Start(PathBuf, Destination),
    Edit(PathBuf),
    Clone(PathBuf),
    Save,
}

#[derive(Default)]
pub struct List {
    pub dir: PathBuf,
    pub items: Vec<Item>,
    looked: bool,
}

impl List {
    pub fn refresh(&mut self, cfg: &Config) {
        self.dir = crate::config::recipes_dir();
        self.items = recipe::find_recipes(&self.dir)
            .into_iter()
            .filter_map(|path| {
                let r = Recipe::load(&path).ok()?;
                let shown = path
                    .strip_prefix(&self.dir)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                Some(Item {
                    known: cfg.remembered(&path).is_some(),
                    path,
                    shown,
                    recipe: r,
                })
            })
            .collect();
        self.looked = true;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cfg: &Config, mode: crate::Mode, busy: bool) -> Ask {
        if !self.looked {
            self.refresh(cfg);
        }
        let mut ask = Ask::Nothing;
        help(
            ui,
            "Each recipe is a whole game: its add-ons and standalone games (credits, download \
             pages and file hashes, never the files), the base game it needs and the choices \
             made for it. Play and the console builders find the downloads in the search folders (Settings) and list \
             any that are missing, with their download pages.",
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new(RichText::new("Save as instance...").color(GOLD).strong())
                        .fill(GREEN),
                )
                .on_hover_text(
                    "Create a new instance: the game, add-ons and standalone games set up on \
                     the other tabs, or the recipe in use, with the menu, survival and 3DS \
                     choices, saved as a recipe.",
                )
                .clicked()
            {
                ask = Ask::Save;
            }
            if ui.button("Refresh").clicked() {
                self.refresh(cfg);
            }
            if ui.button("Open the recipes folder").clicked() {
                let _ = std::fs::create_dir_all(&self.dir);
                ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                    "file://{}",
                    self.dir.display()
                )));
            }
        });
        ui.label(RichText::new(self.dir.display().to_string()).color(DIM));
        ui.add_space(6.0);
        if self.items.is_empty() {
            help(ui, "No recipes found there yet.");
            return ask;
        }
        let in_use = cfg.recipe.as_ref().filter(|r| r.on).map(|r| r.path.clone());
        panel(ui, |ui| {
            egui::Grid::new("instances")
                .striped(true)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for it in &self.items {
                        let r = &it.recipe;
                        let name = if r.recipe.name.is_empty() {
                            r.recipe.id.clone()
                        } else {
                            r.recipe.name.clone()
                        };
                        let using = in_use.as_ref() == Some(&it.path);
                        ui.label(RichText::new(name).color(if using { GOLD } else { SAND }))
                            .on_hover_text(it.path.display().to_string());
                        ui.label(RichText::new(&it.shown).color(DIM));
                        let n = recipe::entries(r).count();
                        ui.label(format!(
                            "{} · {n} download{}",
                            if r.recipe.base.is_empty() {
                                "any game"
                            } else {
                                &r.recipe.base
                            },
                            if n == 1 { "" } else { "s" }
                        ));
                        ui.label(RichText::new(if using {
                            "in use"
                        } else if it.known {
                            "set up before"
                        } else {
                            ""
                        }))
                        .on_hover_text("Its folders were given before: it plays straight away.");
                        ui.horizontal(|ui| {
                            for &(destination, label) in Destination::offered(mode) {
                                if !destination.visible(mode, cfg) {
                                    continue;
                                }
                                if ui
                                    .add_enabled(!busy, egui::Button::new(label).fill(GREEN))
                                    .clicked()
                                {
                                    ask = Ask::Start(it.path.clone(), destination);
                                }
                            }
                            if ui
                                .add_enabled(!busy, egui::Button::new("Open"))
                                .on_hover_text(
                                    "Its downloads, what's missing and their download pages.",
                                )
                                .clicked()
                            {
                                ask = Ask::Open(it.path.clone());
                            }
                            if ui
                                .add_enabled(!busy, egui::Button::new("Edit"))
                                .on_hover_text(
                                    "Its name, base and entries: remove, reorder or add some.",
                                )
                                .clicked()
                            {
                                ask = Ask::Edit(it.path.clone());
                            }
                            if ui
                                .add_enabled(!busy, egui::Button::new("Clone..."))
                                .on_hover_text(
                                    "A copy to tweak, under a new name; its folders go with it.",
                                )
                                .clicked()
                            {
                                ask = Ask::Clone(it.path.clone());
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        ask
    }
}

/// What a running "Save as instance" sends back.
/// The id of the base profile (`bases/*.toml` near the recipe) that the
/// game folder matches, if one does.
pub(crate) fn match_base(recipe_path: &Path, game: &Path) -> Option<String> {
    let dir = recipe_path
        .ancestors()
        .skip(1)
        .map(|d| d.join("bases"))
        .find(|d| d.is_dir())?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    let bases: Vec<Base> = files
        .iter()
        .filter_map(|p| toml::from_str(&std::fs::read_to_string(p).ok()?).ok())
        .collect();
    // A game built from an APK names its APK; others by their files.
    if let Some(b) = bases
        .iter()
        .find(|b| recipe::check_apk(b, game) == Some(true))
    {
        return Some(b.base.id.clone());
    }
    let v = c2_mods::pack::vanilla_files(&[game.to_path_buf()]).ok()?;
    bases
        .iter()
        .filter(|b| b.base.apk.is_empty() && !b.files.is_empty())
        .find(|b| {
            let (missing, bad) = recipe::check_base(b, &v);
            missing.is_empty() && bad.is_empty()
        })
        .map(|b| b.base.id.clone())
}

/// `name` as a recipe id: lower case letters, digits and `-`.
pub fn id_from(name: &str) -> String {
    let mut id = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.ends_with('-') && !id.is_empty() {
            id.push('-');
        }
    }
    let id = id.trim_end_matches('-').to_string();
    if id.is_empty() {
        "my-instance".into()
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiding_consoles_keeps_pc_actions_and_dedicated_builders() {
        let cfg = Config {
            show_3ds: Some(false),
            show_wiiu: Some(false),
            ..Default::default()
        };
        let offered: Vec<_> = Destination::offered(crate::Mode::Full)
            .iter()
            .filter(|(d, _)| d.visible(crate::Mode::Full, &cfg))
            .map(|(d, _)| *d)
            .collect();
        assert_eq!(offered, [Destination::Play, Destination::Windows]);
        assert!(Destination::ThreeDs.visible(crate::Mode::ThreeDs, &cfg));
        assert!(Destination::WiiU.visible(crate::Mode::WiiU, &cfg));
    }

    #[test]
    fn ids_from_file_names() {
        assert_eq!(id_from("My Test Game 2"), "my-test-game-2");
        assert_eq!(id_from("  --  "), "my-instance");
        assert_eq!(id_from("c2_everything"), "c2-everything");
    }
}
