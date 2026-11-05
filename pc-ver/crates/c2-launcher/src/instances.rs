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

mod grid;

pub struct Item {
    pub path: PathBuf,
    /// The path under the recipes folder, for the list.
    pub shown: String,
    pub recipe: Recipe,
    /// The base game's name: its profile id, else the game folder's name.
    pub base: String,
    /// The game and downloads with no folder, or a folder that is gone
    /// (not hash-checked: Play does that).
    pub missing: usize,
    /// Seconds played from the launcher.
    pub played: u64,
}

impl Item {
    fn new(path: PathBuf, dir: &Path, recipe: Recipe, cfg: &Config) -> Self {
        let used = cfg.remembered(&path);
        let game = used.and_then(|u| u.game.clone());
        let base = if !recipe.recipe.base.is_empty() {
            recipe.recipe.base.clone()
        } else {
            game.as_deref()
                .and_then(Path::file_name)
                .map_or("no base game".into(), |n| n.to_string_lossy().into_owned())
        };
        let gone = |p: Option<&PathBuf>| !p.is_some_and(|p| p.is_dir());
        let missing = usize::from(gone(game.as_ref()))
            + recipe::entries(&recipe)
                .filter(|(_, m, _)| gone(used.and_then(|u| u.folders.get(&m.id))))
                .count();
        Item {
            shown: path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .display()
                .to_string(),
            played: cfg.playtime.get(&path).copied().unwrap_or(0),
            path,
            recipe,
            base,
            missing,
        }
    }
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
    Export(PathBuf),
    New,
    Import,
    /// Make an instance from the old Game and Add-ons tabs' setup.
    FromOldSetup,
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
                Some(Item::new(path, &self.dir, r, cfg))
            })
            .collect();
        self.looked = true;
    }

    /// The home screen: a box per instance (Play, Build, Options; a right
    /// click opens Options too) and one to make a new instance, in a grid
    /// as wide as the window.
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
            if ui
                .add_enabled(!busy, egui::Button::new("Import recipe..."))
                .on_hover_text("A recipe someone shared (.toml): copied in as a new instance.")
                .clicked()
            {
                ask = Ask::Import;
            }
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
        ui.add_space(6.0);
        let in_use = cfg.recipe.as_ref().map(|r| r.path.clone());
        grid::show(ui, &self.items, in_use.as_deref(), mode, busy, &mut ask);
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

/// Seconds as "3h 12m" (or "5m").
pub fn played_text(secs: u64) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m}m"),
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

/// A recipe copied from `from` to `to` as it is (its comments too), with
/// the patches it names beside it and its base game's hash list in
/// `bases/` beside it (unless one by that id is there already). Only
/// credits, links and hashes: no game or mod files.
pub fn copy_recipe(from: &Path, to: &Path) -> Result<(), String> {
    let r = Recipe::load(from).map_err(|e| e.to_string())?;
    let (src, dst) = (
        from.parent().unwrap_or(Path::new(".")),
        to.parent().unwrap_or(Path::new(".")),
    );
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    std::fs::copy(from, to).map_err(|e| e.to_string())?;
    for p in &r.patches {
        let (a, b) = (src.join(&p.patch), dst.join(&p.patch));
        if a.is_file() && !b.exists() {
            if let Some(d) = b.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&a, &b).map_err(|e| format!("{}: {e}", a.display()))?;
        }
    }
    if let Some(base) = recipe::load_base(from, &r) {
        let out = dst.join("bases").join(format!("{}.toml", base.base.id));
        let known = recipe::load_base(to, &r).is_some();
        if !known && !out.exists() {
            let text = toml::to_string_pretty(&base).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(dst.join("bases")).map_err(|e| e.to_string())?;
            std::fs::write(&out, text).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
    fn a_recipe_is_copied_with_its_base_and_patches() {
        let t = crate::publish::tests::Temp::new();
        let (a, b) = (t.0.join("a"), t.0.join("b"));
        std::fs::create_dir_all(a.join("bases")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            a.join("bases/sa.toml"),
            "[base]\nid = 'sa'\n\n[files]\n'X' = 'y'\n",
        )
        .unwrap();
        std::fs::write(a.join("fix.patch"), "synthetic").unwrap();
        std::fs::write(
            a.join("r.toml"),
            "# shared\n[recipe]\nid = 'r'\nbase = 'sa'\n\n[[patches]]\ntarget = 'T'\npatch = 'fix.patch'\n",
        )
        .unwrap();
        copy_recipe(&a.join("r.toml"), &b.join("r.toml")).unwrap();
        assert!(std::fs::read_to_string(b.join("r.toml"))
            .unwrap()
            .starts_with("# shared"));
        assert!(b.join("fix.patch").is_file());
        let r = Recipe::load(&b.join("r.toml")).unwrap();
        assert_eq!(
            recipe::load_base(&b.join("r.toml"), &r).map(|x| x.files.len()),
            Some(1)
        );
    }

    #[test]
    fn ids_from_file_names() {
        assert_eq!(id_from("My Test Game 2"), "my-test-game-2");
        assert_eq!(id_from("  --  "), "my-instance");
        assert_eq!(id_from("c2_everything"), "c2-everything");
    }
}
