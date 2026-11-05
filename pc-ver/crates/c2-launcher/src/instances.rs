//! Instances (owner, 2026-11-01): every recipe under the recipes folder,
//! any folder deep (`recipes/instances/testing/` is gitignored for the
//! owner's own), each a whole game: its downloads, the base it expects and
//! its `[settings]`. The launcher's home screen (owner, 2026-11-05): a box
//! per instance with Play, Build (then the platform) and Options (Edit,
//! Clone, Fix missing, Delete; a right click opens them too). Play and the
//! builds go through the recipe page, which finds the downloads in the
//! search folders and checks the base game against the recipe's hashes.

use std::path::{Path, PathBuf};

use c2_mods::recipe::{self, Base, Recipe};
use eframe::egui::{self, RichText};

use crate::app::help;
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
    /// The same game in a headset (`c2-vr`, a first draft).
    PlayVr,
    Windows,
    ThreeDs,
    WiiU,
}

impl Destination {
    /// The platforms this launcher builds for, as offered after "Build".
    pub fn builds(mode: crate::Mode) -> &'static [(Self, &'static str)] {
        match mode {
            crate::Mode::Full => &[
                (Self::Windows, "Windows"),
                (Self::ThreeDs, "3DS"),
                (Self::WiiU, "Wii U"),
            ],
            crate::Mode::ThreeDs => &[(Self::ThreeDs, "3DS")],
            crate::Mode::WiiU => &[(Self::WiiU, "Wii U")],
        }
    }
}

/// What the home screen asks of the launcher.
pub enum Ask {
    Nothing,
    Play(PathBuf),
    PlayVr(PathBuf),
    Build(PathBuf),
    Edit(PathBuf),
    Clone(PathBuf),
    Delete(PathBuf),
    FixMissing(PathBuf),
    New,
    /// Make an instance from the old Game and Add-ons tabs' setup.
    FromOldSetup,
}

#[derive(Default)]
pub struct List {
    pub dir: PathBuf,
    pub items: Vec<Item>,
    looked: bool,
}

/// An instance box's size (points): two or three across on a Steam Deck.
const BOX: egui::Vec2 = egui::vec2(270.0, 128.0);

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

    /// The home screen: a box per instance (Play, Build, Options; a right
    /// click opens Options too) and one to make a new instance.
    pub fn ui(&mut self, ui: &mut egui::Ui, cfg: &Config, mode: crate::Mode, busy: bool) -> Ask {
        if !self.looked {
            self.refresh(cfg);
        }
        let mut ask = Ask::Nothing;
        help(
            ui,
            "Each instance is a whole game: a base game, its add-ons and its choices. Play it, \
             build it for another platform, or use Options (or right click) to edit it.",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Refresh").clicked() {
                self.refresh(cfg);
            }
            if ui.button("Open the instances folder").clicked() {
                let _ = std::fs::create_dir_all(&self.dir);
                ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                    "file://{}",
                    self.dir.display()
                )));
            }
            if cfg.base.is_some()
                && ui
                    .add_enabled(!busy, egui::Button::new("Instance from the old setup..."))
                    .on_hover_text(
                        "The game, overlays, add-ons and standalone games set up on the old \
                         Game and Add-ons tabs, saved as an instance.",
                    )
                    .clicked()
            {
                ask = Ask::FromOldSetup;
            }
        });
        ui.label(RichText::new(self.dir.display().to_string()).color(DIM));
        ui.add_space(6.0);
        let in_use = cfg.recipe.as_ref().map(|r| r.path.clone());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for it in &self.items {
                let using = in_use.as_ref() == Some(&it.path);
                instance_box(ui, it, using, mode, busy, &mut ask);
            }
            new_box(ui, busy, &mut ask);
        });
        ask
    }
}

/// The name shown for a recipe.
pub fn display_name(r: &Recipe) -> String {
    if r.recipe.name.is_empty() {
        r.recipe.id.clone()
    } else {
        r.recipe.name.clone()
    }
}

/// A box on the home screen, `add` drawing in it; its response (to click
/// or right click).
fn boxed(
    ui: &mut egui::Ui,
    stroke: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let r = egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.5, stroke))
        .inner_margin(10.0)
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_min_size(BOX);
            ui.set_max_width(BOX.x);
            ui.vertical(add);
        })
        .response;
    ui.interact(r.rect, r.id.with("box"), egui::Sense::click())
}

fn instance_box(
    ui: &mut egui::Ui,
    it: &Item,
    using: bool,
    mode: crate::Mode,
    busy: bool,
    ask: &mut Ask,
) {
    let r = &it.recipe;
    let n = recipe::entries(r).count();
    let resp = boxed(ui, if using { GOLD } else { PANEL_HI }, |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(display_name(r))
                    .size(17.0)
                    .color(if using { GOLD } else { SAND })
                    .strong(),
            )
            .truncate(),
        )
        .on_hover_text(&it.shown);
        ui.label(
            RichText::new(format!(
                "{} · {n} download{}",
                if r.recipe.base.is_empty() {
                    "any game"
                } else {
                    &r.recipe.base
                },
                if n == 1 { "" } else { "s" }
            ))
            .color(DIM),
        );
        if !it.known {
            ui.label(RichText::new("not set up yet").color(DIM))
                .on_hover_text(
                    "Play finds its downloads in the mod and add-on folders (Settings).",
                );
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if mode == crate::Mode::Full
                && ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(RichText::new("Play").color(GOLD).strong()).fill(GREEN),
                    )
                    .clicked()
            {
                *ask = Ask::Play(it.path.clone());
            }
            if ui.add_enabled(!busy, egui::Button::new("Build")).clicked() {
                *ask = Ask::Build(it.path.clone());
            }
            ui.add_enabled_ui(!busy, |ui| {
                ui.menu_button("Options", |ui| options_menu(ui, &it.path, mode, ask));
            });
        });
    });
    if !busy {
        resp.context_menu(|ui| options_menu(ui, &it.path, mode, ask));
    }
}

/// An instance's Options: the same on its button and on a right click.
fn options_menu(ui: &mut egui::Ui, path: &Path, mode: crate::Mode, ask: &mut Ask) {
    let p = || path.to_path_buf();
    if ui.button("Edit").clicked() {
        *ask = Ask::Edit(p());
    }
    if ui.button("Clone").clicked() {
        *ask = Ask::Clone(p());
    }
    if ui
        .button("Fix missing")
        .on_hover_text(
            "Looks for its base game and downloads in the mod and add-on folders (Settings) and \
             keeps what is found.",
        )
        .clicked()
    {
        *ask = Ask::FixMissing(p());
    }
    if mode == crate::Mode::Full
        && ui
            .button("Play in VR")
            .on_hover_text(
                "A first draft, through OpenXR (start SteamVR, Monado or WiVRn first): it \
                 checks your headset runtime and the game, then shows a test scene. Its report \
                 is in the log.",
            )
            .clicked()
    {
        *ask = Ask::PlayVr(p());
    }
    ui.separator();
    if ui.button(RichText::new("Delete").color(RED)).clicked() {
        *ask = Ask::Delete(p());
    }
}

fn new_box(ui: &mut egui::Ui, busy: bool, ask: &mut Ask) {
    let resp = boxed(ui, PANEL_HI, |ui| {
        ui.label(
            RichText::new("New instance")
                .size(17.0)
                .color(SAND)
                .strong(),
        );
        help(ui, "Start from a base game, then add add-ons.");
        ui.add_space(4.0);
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(RichText::new("+ New").color(GOLD)).fill(GREEN),
            )
            .clicked()
        {
            *ask = Ask::New;
        }
    });
    if resp.clicked() && !busy {
        *ask = Ask::New;
    }
}

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
    fn every_platform_is_offered_after_build() {
        let all: Vec<_> = Destination::builds(crate::Mode::Full)
            .iter()
            .map(|(d, _)| *d)
            .collect();
        assert_eq!(
            all,
            [
                Destination::Windows,
                Destination::ThreeDs,
                Destination::WiiU
            ]
        );
        assert_eq!(Destination::builds(crate::Mode::ThreeDs).len(), 1);
        assert_eq!(Destination::builds(crate::Mode::WiiU).len(), 1);
    }

    #[test]
    fn ids_from_file_names() {
        assert_eq!(id_from("My Test Game 2"), "my-test-game-2");
        assert_eq!(id_from("  --  "), "my-instance");
        assert_eq!(id_from("c2_everything"), "c2-everything");
    }
}
