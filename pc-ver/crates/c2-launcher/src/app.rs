//! The launcher window: a tab per job (game folders, add-ons, balance,
//! play, 3DS, Wii U, settings), the job log at the bottom.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::config::{describe_game, install_name, AddOn, Config, MenuStyle, RecipeUse, Target};
use crate::jobs::{is_archive, Job, Step};
use crate::theme::*;
use crate::{find_tool, Mode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Instances,
    Game,
    AddOns,
    Balance,
    Play,
    Windows,
    ThreeDs,
    WiiU,
    Settings,
}

pub struct Launcher {
    mode: Mode,
    cfg: Config,
    tab: Tab,
    job: Option<Job>,
    /// Text being typed in the path fields.
    base_text: String,
    note: Option<(String, Color32)>,
    balance: crate::balance::Editor,
    /// An APK import running: where it writes its add-ons.
    importing: Option<PathBuf>,
    /// A game being built from the player's APK, to become the base.
    apk_base: Option<PathBuf>,
    /// ... the recipe page's game instead.
    apk_for_recipe: bool,
    /// A recipe being set up (Add-ons or Instances tab).
    recipe_page: Option<crate::recipe_page::Page>,
    /// A recipe being edited (Instances tab).
    recipe_edit: Option<crate::recipe_edit::Editor>,
    instances: crate::instances::List,
    /// "Save as instance" making a recipe from the setup.
    saving: Option<std::sync::mpsc::Receiver<crate::instances::Saved>>,
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(10, 14, 10);
    v.faint_bg_color = PANEL;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = GOLD;
    v.selection.bg_fill = GREEN;
    v.selection.stroke.color = GOLD;
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.weak_bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke.color = PANEL_HI;
    v.widgets.inactive.bg_fill = BUTTON;
    v.widgets.inactive.weak_bg_fill = BUTTON;
    v.widgets.hovered.bg_fill = PANEL_HI;
    v.widgets.hovered.weak_bg_fill = PANEL_HI;
    v.widgets.hovered.fg_stroke.color = HOVER;
    v.widgets.active.bg_fill = GREEN;
    v.widgets.active.weak_bg_fill = GREEN;
    ctx.set_visuals(v);
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(20.0).color(SAND).strong());
}

/// A row picking the user's own picture (a PNG of `size`): the file, a
/// warning if it is another size, Browse and Clear. Whether it changed.
fn art_picker(
    ui: &mut egui::Ui,
    label: &str,
    size: (u32, u32),
    slot: &mut Option<std::path::PathBuf>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(format!("{label} ({}x{} PNG):", size.0, size.1));
        match slot.as_ref() {
            Some(p) => {
                ui.label(RichText::new(p.display().to_string()).color(SAND));
                match crate::cia::png_size(p) {
                    Some(s) if s == size => {}
                    Some((w, h)) => {
                        ui.label(RichText::new(format!("is {w}x{h}")).color(RED));
                    }
                    None => {
                        ui.label(RichText::new("not a PNG").color(RED));
                    }
                }
            }
            None => help(ui, "from the game's menu picture"),
        }
        if ui.button("Browse...").clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter(label, &["png"])
                .pick_file()
            {
                *slot = Some(p);
                changed = true;
            }
        }
        if slot.is_some() && ui.small_button("Clear").clicked() {
            *slot = None;
            changed = true;
        }
    });
    changed
}

pub(crate) fn help(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(DIM));
}

/// A framed box like the game menus' panels.
pub(crate) fn panel<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, PANEL_HI))
        .inner_margin(10.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn big_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(18.0).color(GOLD).strong())
            .min_size(egui::vec2(180.0, 40.0))
            .fill(GREEN),
    )
    .clicked()
}

impl Launcher {
    pub fn new(cc: &eframe::CreationContext<'_>, mode: Mode) -> Self {
        style(&cc.egui_ctx);
        let cfg = Config::load();
        let mobile = cfg_mobile(&cfg);
        let base_text = cfg
            .base
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Self {
            mode,
            tab: Tab::Instances,
            cfg,
            job: None,
            base_text,
            note: None,
            balance: {
                let mut b = crate::balance::Editor::load();
                b.mobile = mobile;
                b
            },
            importing: None,
            apk_base: None,
            apk_for_recipe: false,
            recipe_page: None,
            recipe_edit: None,
            instances: Default::default(),
            saving: None,
        }
    }

    /// Imports the mobile game's animals and weapons from an APK the user
    /// picked into the data folder (`mobile/<apk>/`); `import_done` adds
    /// them as add-ons.
    fn import_apk(&mut self, apk: PathBuf) {
        let (bases, _) = self.game_folders();
        if bases.is_empty() {
            self.note = Some(("Set the game folder first (The game tab).".into(), RED));
            return;
        }
        let stem = apk
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mobile".into());
        let out = crate::config::data_dir().join("mobile").join(&stem);
        let to = out.clone();
        let step = Step::Task(Box::new(move |job| {
            job.log(format!("Reading {}", apk.display()));
            let base = c2_mods::vanilla(&bases).map_err(|e| e.to_string())?;
            let done = c2_mods::mobile::import(&apk, &base, &to).map_err(|e| e.to_string())?;
            for s in done.skipped {
                job.log(format!("Not imported: {s}"));
            }
            for a in done.added {
                job.log(format!("{} (kind of the {})", a.name, a.donor));
                for n in a.notes {
                    job.log(format!("  {n}"));
                }
            }
            Ok(())
        }));
        self.importing = Some(out);
        self.job = Some(Job::start("Importing from the mobile game", vec![step]));
    }

    /// Carnivores: Ice Age's animals and areas as add-ons beside C2's (the
    /// combined game, `c2_mods::ice_age::import`).
    fn import_ice_age(&mut self, ia: PathBuf) {
        let (bases, _) = self.game_folders();
        if bases.is_empty() {
            self.note = Some(("Set the game folder first (The game tab).".into(), RED));
            return;
        }
        self.cfg.ice_age = Some(ia.clone());
        self.save();
        let out = crate::config::data_dir().join("ice-age");
        let to = out.clone();
        let step = Step::Task(Box::new(move |job| {
            job.log(format!("Reading {}", ia.display()));
            let base = c2_mods::vanilla(&bases).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_dir_all(&to);
            let done = c2_mods::ice_age::import(&ia, &base, &to).map_err(|e| e.to_string())?;
            for s in done.skipped {
                job.log(format!("Not imported: {s}"));
            }
            for d in done.added {
                job.log(d.display().to_string());
            }
            Ok(())
        }));
        self.importing = Some(out);
        self.job = Some(Job::start("Importing from Carnivores: Ice Age", vec![step]));
    }

    /// Rebuilds the mobile game from the player's APK as a game folder in
    /// the data folder (`apk-base/<apk>/`, `c2_mods::mobile_base`), for
    /// those with only the phone game; `apk_base_done` makes it the base.
    fn base_from_apk(&mut self, apk: PathBuf) {
        let stem = apk
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mobile".into());
        let out = crate::config::data_dir().join("apk-base").join(&stem);
        let to = out.clone();
        let step = Step::Task(Box::new(move |job| {
            job.log(format!("Reading {}", apk.display()));
            // A rebuild starts clean (our own folder in the data folder).
            if to.is_dir() {
                std::fs::remove_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            }
            let b = c2_mods::mobile_base::build(&apk, &to).map_err(|e| e.to_string())?;
            job.log(format!(
                "{} areas, {} animals, {} weapons",
                b.areas.len(),
                b.animals.len(),
                b.weapons.len()
            ));
            for n in b.notes {
                job.log(format!("  {n}"));
            }
            Ok(())
        }));
        self.apk_base = Some(out);
        self.job = Some(Job::start("Building the game from your APK", vec![step]));
    }

    /// After building from an APK: the game is the base.
    fn apk_base_done(&mut self) {
        let Some(out) = self.apk_base.clone() else {
            return;
        };
        match self.job.as_ref().and_then(Job::result) {
            None => return,
            Some(true) if self.apk_for_recipe => {
                let dirs = self.game_folders().0;
                if let Some(p) = &mut self.recipe_page {
                    p.game = Some(out.clone());
                    p.check_base(dirs);
                }
            }
            Some(true) => {
                self.set_base(out.clone());
                self.base_text = out.display().to_string();
            }
            Some(false) => {}
        }
        self.apk_base = None;
        self.apk_for_recipe = false;
    }

    /// After an import: its folders become add-ons, its values go to the
    /// balance editor.
    fn import_done(&mut self) {
        let Some(out) = &self.importing else { return };
        match self.job.as_ref().and_then(Job::result) {
            None => return,
            Some(false) => {}
            Some(true) => {
                let mut dirs: Vec<PathBuf> = std::fs::read_dir(out)
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| e.path())
                            .filter(|p| p.is_dir())
                            .collect()
                    })
                    .unwrap_or_default();
                dirs.sort();
                let values = out.join(c2_mods::mobile::MOBILE_VALUES);
                self.cfg.mobile_values = values.exists().then_some(values);
                let survive = out.join(c2_mods::mobile::MOBILE_SURVIVAL);
                if survive.exists() {
                    self.cfg.mobile_survival = Some(survive);
                }
                self.balance.mobile = cfg_mobile(&self.cfg);
                let n = dirs.len();
                self.add_addons(dirs);
                self.note = Some((format!("{n} imported add-ons added (Add-ons tab)."), TEXT));
            }
        }
        self.importing = None;
    }

    fn shows_3ds(&self) -> bool {
        self.mode == Mode::ThreeDs || (self.mode == Mode::Full && self.cfg.show_3ds == Some(true))
    }

    fn shows_wiiu(&self) -> bool {
        self.mode == Mode::WiiU || (self.mode == Mode::Full && self.cfg.show_wiiu != Some(false))
    }

    fn save(&mut self) {
        self.cfg.remember_builds();
        if let Err(e) = self.cfg.store() {
            self.note = Some((format!("Settings not saved: {e}"), RED));
        }
    }

    fn busy(&self) -> bool {
        self.job.as_ref().is_some_and(|j| j.result().is_none())
    }

    fn set_base(&mut self, p: PathBuf) {
        match describe_game(&p) {
            Some(kind) => {
                self.note = Some((format!("{kind} found."), TEXT));
                self.base_text = p.display().to_string();
                self.cfg.base = Some(p);
                self.save();
            }
            None => {
                self.note = Some((
                    format!(
                        "{}: no HUNTDAT folder: that isn't a game folder.",
                        p.display()
                    ),
                    RED,
                ));
            }
        }
    }

    /// Folders are added as they are; archives are unpacked into the data
    /// folder first.
    fn add_addons(&mut self, paths: Vec<PathBuf>) {
        let mut unpack = Vec::new();
        for p in paths {
            let dest = if is_archive(&p) {
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "addon".into());
                let to = crate::config::data_dir().join("addons").join(&stem);
                unpack.push(Step::Unpack {
                    archive: p.clone(),
                    to: to.clone(),
                });
                to
            } else if p.is_dir() {
                p
            } else {
                self.note = Some((
                    format!(
                        "{}: not a folder or an archive (.zip, .rar, .7z)",
                        p.display()
                    ),
                    RED,
                ));
                continue;
            };
            if !self.cfg.addons.iter().any(|a| a.path == dest) {
                self.cfg.addons.push(AddOn {
                    path: dest,
                    enabled: true,
                });
            }
        }
        if !unpack.is_empty() {
            self.job = Some(Job::start("Unpacking add-ons", unpack));
        }
        self.save();
    }

    /// The base and its overlays, then the add-ons that are on.
    fn game_folders(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let bases = match &self.cfg.base {
            Some(b) if describe_game(b).is_some() => std::iter::once(b.clone())
                .chain(self.cfg.overlays.iter().cloned())
                .collect(),
            _ => Vec::new(),
        };
        let mods = self.cfg.enabled_addons();
        (bases, mods)
    }

    /// The recipe in use, if it is on.
    fn active_recipe(&self) -> Option<&RecipeUse> {
        self.cfg.recipe.as_ref().filter(|r| r.on)
    }

    /// A recipe's game folders: its own game, or the Game tab's with its
    /// overlays.
    fn recipe_game(&self, r: &RecipeUse) -> Vec<PathBuf> {
        match &r.game {
            Some(g) => std::iter::once(g.clone())
                .chain(r.overlays.clone().unwrap_or_default())
                .collect(),
            None => self.game_folders().0,
        }
    }

    /// [`Self::recipe_game`] to play or build with: a recipe with its own
    /// game that combines standalones and names no overlay of its own takes
    /// the Game tab's overlays over that game (combining needs the Modder's
    /// Edition; the recipe page only finds the base game). Reads the recipe.
    fn recipe_game_with_overlays(&self, r: &RecipeUse) -> Vec<PathBuf> {
        let mut g = self.recipe_game(r);
        if let Ok(rc) = c2_mods::recipe::Recipe::load(&r.path) {
            let apk = c2_mods::recipe::load_base(&r.path, &rc)
                .is_some_and(|base| !base.base.apk.is_empty());
            if apk {
                // A phone game's own converted rules must not be replaced
                // by an unrelated desktop overlay from the Game tab.
                g.truncate(1);
            } else if !rc.overlays.is_empty() {
                g.truncate(1);
            } else if r.overlays.is_none()
                && r.game.is_some()
                && (r.game == self.cfg.base || !rc.standalones.is_empty())
            {
                g.extend(self.cfg.overlays.iter().cloned());
            }
        }
        g
    }

    /// Where a recipe's base may be, when it is not the Game tab's: the
    /// Standalones tab's games (those on first), then the search folders
    /// (looked through off the UI thread, `c2_mods::recipe::dir_and_subs`).
    fn base_candidates(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.cfg.enabled_standalones();
        out.extend(self.cfg.standalones.iter().map(|a| a.path.clone()));
        out.extend(self.cfg.search_dirs());
        out.push(crate::config::data_dir().join("apk-base"));
        let mut seen = std::collections::HashSet::new();
        out.retain(|p| seen.insert(p.clone()));
        out
    }

    /// The recipe page for `path` (a dropped or opened `.toml`, or an
    /// instance): the folders it was last given, the rest looked for in
    /// the search folders.
    fn open_recipe(&mut self, path: PathBuf) {
        let dirs = self.game_folders().0;
        let candidates = self.base_candidates();
        match crate::recipe_page::Page::open(&path, self.cfg.remembered(&path), dirs, candidates) {
            Ok(mut p) => {
                p.scan(self.cfg.search_dirs());
                self.recipe_page = Some(p);
                if self.tab != Tab::Instances {
                    self.tab = Tab::AddOns;
                }
            }
            Err(e) => self.note = Some((format!("Not a recipe: {e}"), RED)),
        }
    }

    /// The recipe page's recipe in use (its choices taken in), and played.
    fn use_recipe(&mut self, play: bool) {
        let Some(p) = self.recipe_page.take() else {
            return;
        };
        let mut u = p.to_use();
        // Freeze the resolved base/overlays before the Game tab changes.
        u.overlays = self
            .cfg
            .remembered(&u.path)
            .and_then(|r| r.overlays.clone());
        let games = self.recipe_game_with_overlays(&u);
        u.game = games.first().cloned();
        u.overlays = Some(games.into_iter().skip(1).collect());
        let s = &p.recipe.settings;
        self.cfg.remember_builds();
        self.cfg.apply(s);
        self.cfg.pictures = s.pictures;
        self.cfg.tuning = s.tuning;
        self.cfg.restore_builds(&u.path, &p.recipe.recipe.id);
        self.balance.edits = s.balance.clone().unwrap_or_default();
        let _ = self.balance.store();
        self.cfg.remember(&u);
        self.note = Some((
            format!("Recipe '{}' in use: play and the builds go by it.", u.id()),
            TEXT,
        ));
        self.cfg.recipe = Some(u);
        self.save();
        if play {
            self.play();
        }
    }

    /// "Save as instance": the recipe in use with the choices made now, or
    /// the setup (game, add-ons, standalones) as a new recipe, saved under
    /// the recipes folder by default.
    fn save_instance(&mut self) {
        let dir = crate::config::recipes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let current = self.active_recipe().cloned();
        let name = current.as_ref().map_or("my-instance".into(), |u| u.id());
        let Some(out) = rfd::FileDialog::new()
            .set_directory(&dir)
            .set_file_name(format!("{name}.toml"))
            .add_filter("Recipe", &["toml"])
            .save_file()
        else {
            return;
        };
        let stem = out
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let Some(u) = current else {
            self.saving = Some(crate::instances::from_setup(
                &self.cfg,
                out,
                crate::instances::id_from(&stem),
            ));
            self.note = Some(("Saving the instance (hashing its files)...".into(), TEXT));
            return;
        };
        let mut r = match c2_mods::recipe::Recipe::load(&u.path) {
            Ok(r) => r,
            Err(e) => {
                self.note = Some((format!("Not saved: {e}"), RED));
                return;
            }
        };
        // A recipe's patches are found beside it.
        if !r.patches.is_empty() && out.parent() != u.path.parent() {
            self.note = Some((
                "Not saved: the recipe has patches beside it; save it in the same folder.".into(),
                RED,
            ));
            return;
        }
        if out != u.path {
            r.recipe.id = crate::instances::id_from(&stem);
            r.recipe.name = stem;
        }
        r.settings = self.cfg.settings(&r.settings);
        r.settings.balance = self.balance.file().map(|_| self.balance.edits.clone());
        self.write_instance(r, RecipeUse { path: out, ..u });
    }

    fn write_instance(&mut self, r: c2_mods::recipe::Recipe, u: RecipeUse) {
        match std::fs::write(&u.path, r.to_toml()) {
            Ok(()) => {
                self.note = Some((format!("Instance saved: {}", u.path.display()), TEXT));
                self.cfg.remember_builds();
                self.cfg.remember(&u);
                self.cfg.restore_builds(&u.path, &r.recipe.id);
                self.cfg.recipe = Some(u);
                self.save();
                self.instances.refresh(&self.cfg);
            }
            Err(e) => self.note = Some((format!("Not saved: {e}"), RED)),
        }
    }

    /// A "Save as instance" of the setup that finished.
    fn saving_done(&mut self) {
        let Some(rx) = &self.saving else {
            return;
        };
        let Ok(made) = rx.try_recv() else {
            return;
        };
        self.saving = None;
        match made {
            Ok((mut r, u)) => {
                r.settings.balance = self.balance.file().map(|_| self.balance.edits.clone());
                self.write_instance(r, u);
            }
            Err(e) => self.note = Some((format!("Not saved: {e}"), RED)),
        }
    }

    /// The Instances tab: the recipe page while one is open, else the list.
    fn instances_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Instances");
        if self.recipe_edit.is_some() {
            self.recipe_edit_ui(ui);
            return;
        }
        if self.recipe_page.is_some() {
            self.recipe_ui(ui);
            return;
        }
        let busy = self.busy() || self.saving.is_some();
        match self.instances.ui(ui, &self.cfg, self.mode, busy) {
            crate::instances::Ask::Nothing => {}
            crate::instances::Ask::Open(p) => self.open_recipe(p),
            crate::instances::Ask::Start(p, destination) => {
                self.open_recipe(p);
                if let Some(page) = &mut self.recipe_page {
                    page.next = Some(destination);
                }
            }
            crate::instances::Ask::Edit(p) => self.edit_recipe(&p),
            crate::instances::Ask::Clone(p) => self.clone_recipe(&p),
            crate::instances::Ask::Save => self.save_instance(),
        }
    }

    fn edit_recipe(&mut self, path: &Path) {
        match crate::recipe_edit::Editor::open(path) {
            Ok(e) => self.recipe_edit = Some(e),
            Err(e) => self.note = Some((format!("Not a recipe: {e}"), RED)),
        }
    }

    /// "Clone...": the recipe saved under a new name (with the folders
    /// given for it), then opened to edit.
    fn clone_recipe(&mut self, from: &Path) {
        let Some(to) = crate::recipe_edit::pick_copy_path(from) else {
            return;
        };
        let made = c2_mods::recipe::Recipe::load(from)
            .map_err(|e| e.to_string())
            .and_then(|r| {
                let c = crate::recipe_edit::renamed_for(&r, &to);
                if let Some(p) = crate::recipe_edit::save_problem(&c, from, &to) {
                    return Err(p);
                }
                let old = std::fs::read_to_string(from).unwrap_or_default();
                std::fs::write(&to, crate::recipe_edit::with_header(&old, &c.to_toml()))
                    .map_err(|e| e.to_string())
            });
        match made {
            Ok(()) => {
                self.recipe_saved(from, &to, &[]);
                self.note = Some((format!("Cloned to {}", to.display()), TEXT));
                self.edit_recipe(&to);
            }
            Err(e) => self.note = Some((format!("Not cloned: {e}"), RED)),
        }
    }

    /// A recipe written to `to` from the one at `from` (the same path when
    /// saved over it): the folders given for it (and `added`) are kept for
    /// `to`, and the list shows it.
    fn recipe_saved(&mut self, from: &Path, to: &Path, added: &[(String, PathBuf)]) {
        let mut u = self.cfg.remembered(from).cloned().unwrap_or(RecipeUse {
            path: to.to_path_buf(),
            game: None,
            overlays: None,
            folders: Default::default(),
            on: false,
        });
        u.path = to.to_path_buf();
        // Only entries the recipe still has.
        if let Ok(r) = c2_mods::recipe::Recipe::load(to) {
            let ids: std::collections::HashSet<String> = c2_mods::recipe::entries(&r)
                .map(|(_, m, _)| m.id.clone())
                .collect();
            u.folders.retain(|id, _| ids.contains(id));
            for (id, dir) in added {
                if ids.contains(id) {
                    u.folders.insert(id.clone(), dir.clone());
                }
            }
        }
        if let Some(cur) = &mut self.cfg.recipe {
            if cur.path == to {
                cur.folders = u.folders.clone();
            }
        }
        u.on = false;
        self.cfg.remember(&u);
        self.save();
        self.instances.refresh(&self.cfg);
    }

    /// The recipe editor; add-ons added there are read against the
    /// recipe's game (as last given), else the Game tab's.
    fn recipe_edit_ui(&mut self, ui: &mut egui::Ui) {
        let Some(e) = &self.recipe_edit else {
            return;
        };
        let game = match self.cfg.remembered(&e.path) {
            Some(u) => self.recipe_game_with_overlays(&u.clone()),
            None => self.game_folders().0,
        };
        let Some(e) = &mut self.recipe_edit else {
            return;
        };
        e.poll();
        if e.working() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        let mut ask = crate::recipe_edit::Ask::Nothing;
        panel(ui, |ui| ask = e.ui(ui, &game));
        match ask {
            crate::recipe_edit::Ask::Nothing => {}
            crate::recipe_edit::Ask::Close => self.recipe_edit = None,
            crate::recipe_edit::Ask::Saved { from, to } => {
                let added = std::mem::take(&mut e.added);
                self.recipe_saved(&from, &to, &added);
            }
        }
    }

    /// `--balance FILE`: the balance editor's changes, over the phone
    /// game's rules where they are taken (`balance-tuned.toml` in the data
    /// folder then).
    fn balance_args(&self) -> Vec<String> {
        let edits = self.balance.file().map(|f| (f, self.balance.edits.clone()));
        let tuned = self.phone_rules().map(|b| match &edits {
            Some((_, e)) => b.with(e),
            None => b,
        });
        let file = match tuned {
            Some(b) if !b.is_empty() => {
                let f = crate::config::data_dir().join("balance-tuned.toml");
                std::fs::write(&f, b.to_toml()).ok().map(|()| f)
            }
            _ => edits.map(|(f, _)| f),
        };
        match file {
            Some(f) => vec!["--balance".into(), f.display().to_string()],
            None => Vec::new(),
        }
    }

    /// The phone game's rules chosen for a PC game (`cfg.tuning`), from
    /// the APK import's values; `None` on a game built from the APK (its
    /// rules are the phone's already) or with none chosen.
    fn phone_rules(&self) -> Option<c2_mods::balance::Balance> {
        let t = self.cfg.tuning;
        let (s, c, a) = (
            t.shotgun.mobile(false),
            t.scoring.mobile(false),
            t.animals.mobile(false),
        );
        if !(s || c || a) || self.game_is_phone() {
            return None;
        }
        Some(self.balance.mobile.as_ref()?.phone_tuning(s, c, a))
    }

    /// The game is one built from the APK (the recipe's or the Game tab's).
    fn game_is_phone(&self) -> bool {
        match self.active_recipe() {
            Some(r) => self
                .recipe_game(r)
                .first()
                .is_some_and(|g| crate::config::is_phone_game(g)),
            None => self
                .cfg
                .base
                .as_deref()
                .is_some_and(crate::config::is_phone_game),
        }
    }

    /// PC or phone rules (shotgun, scoring, animals' health).
    fn tuning_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use c2_mods::recipe::Source;
        let phone = self.game_is_phone();
        let have = self.balance.mobile.is_some() && !phone;
        let mut changed = false;
        let t = &mut self.cfg.tuning;
        ui.label(RichText::new("PC or phone rules").color(GOLD));
        for (name, s) in [
            ("Shotguns", &mut t.shotgun),
            ("Scores and prices", &mut t.scoring),
            ("Animals' health", &mut t.animals),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{name}:"));
                ui.add_enabled_ui(have, |ui| {
                    changed |= ui.radio_value(s, Source::Auto, "Auto (PC)").changed();
                    changed |= ui.radio_value(s, Source::Pc, "PC").changed();
                    changed |= ui.radio_value(s, Source::Mobile, "Phone").changed();
                });
            });
        }
        help(
            ui,
            if phone {
                "This game is built from the APK: its rules are the phone's already."
            } else if have {
                "The phone's values, from your APK import, under any changes you make below. \
                 Applies to playing on PC, the 3DS and the Wii U."
            } else {
                "The phone's rules come with importing your APK (Add-ons tab)."
            },
        );
        changed
    }

    /// `--survival-modes ..`, and `--mobile-survival FILE` once an APK
    /// import has made the file.
    fn survival_args(&self) -> Vec<String> {
        let mut v = vec!["--survival-modes".into(), self.cfg.survival_modes.arg()];
        if let Some(f) = self.cfg.mobile_survival.as_ref().filter(|f| f.exists()) {
            v.extend(["--mobile-survival".into(), f.display().to_string()]);
        }
        v
    }

    /// `c2-cook`'s survival arguments, and the phone's pictures where
    /// chosen (`--phone-game`, `--phone-pictures`; the 3DS and Wii U).
    fn cook_extras(&self) -> Vec<String> {
        let mut v = self.survival_args();
        let Some(phone) = self.phone_game() else {
            return v;
        };
        let p = self.cfg.pictures;
        let want: Vec<&str> = [
            ("weapons", p.weapons),
            ("animals", p.animals),
            ("scope", p.scope),
        ]
        .iter()
        .filter(|(_, s)| s.mobile(true))
        .map(|(k, _)| *k)
        .collect();
        if !want.is_empty() {
            v.extend([
                "--phone-game".to_string(),
                phone.display().to_string(),
                "--phone-pictures".into(),
                want.join(","),
            ]);
        }
        v
    }

    /// The game built from the player's APK whose pictures the consoles
    /// can take: the recipe's or the Game tab's own game if it is one, else
    /// one built before (Ice Age's for an Ice Age game, else Dinosaur
    /// Hunter's).
    fn phone_game(&self) -> Option<PathBuf> {
        let own = self
            .active_recipe()
            .and_then(|r| r.game.clone())
            .into_iter()
            .chain(self.cfg.base.clone());
        let built = crate::config::phone_games();
        let ice = |p: &PathBuf| p.to_string_lossy().to_ascii_lowercase().contains("ice");
        let wants_ice = self
            .cfg
            .base
            .as_deref()
            .is_some_and(c2_mods::ice_age::is_ice_age_dir);
        own.into_iter()
            .find(|g| crate::config::is_phone_game(g))
            .or_else(|| built.iter().find(|g| ice(g) == wants_ice).cloned())
            .or_else(|| built.first().cloned())
    }

    /// The 3DS and Wii U hunt pictures: the phone's or the game's own.
    fn pictures_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use c2_mods::recipe::Source;
        let mut changed = false;
        let phone = self.phone_game();
        let p = &mut self.cfg.pictures;
        for (name, s) in [
            ("Weapon pictures", &mut p.weapons),
            ("Animal pictures and call icons", &mut p.animals),
            ("Sniper scope", &mut p.scope),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{name}:"));
                ui.add_enabled_ui(phone.is_some(), |ui| {
                    changed |= ui.radio_value(s, Source::Auto, "Auto (phone)").changed();
                    changed |= ui.radio_value(s, Source::Mobile, "Phone").changed();
                });
                changed |= ui.radio_value(s, Source::Pc, "PC").changed();
            });
        }
        help(
            ui,
            &match &phone {
                Some(g) => format!(
                    "The phone's pictures come from {} (matched by model: animals and weapons \
                     both games have). The ammo counter stays as it is. On a game built from the \
                     APK, its pictures are the phone's already.",
                    g.display()
                ),
                None => "The phone's pictures need a game built from your APK (\"Use my APK as \
                         the game...\" on the Game tab, or an Android recipe)."
                    .into(),
            },
        );
        changed
    }

    /// Survival choice (PC and 3DS alike).
    fn survival_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let have = self
            .cfg
            .mobile_survival
            .as_ref()
            .is_some_and(|f| f.exists());
        let mut changed = false;
        let m = &mut self.cfg.survival_modes;
        ui.horizontal(|ui| {
            ui.label("Survival:");
            changed |= ui
                .add_enabled(
                    have,
                    egui::Checkbox::new(&mut m.classic, "Classic (Android)"),
                )
                .changed();
            changed |= ui.checkbox(&mut m.modded, "Modded").changed();
        });
        help(
            ui,
            if have {
                "Classic: the mobile game's own Survive mode, read from your APK (its waves on \
                 Mount Ravan, straight in with the shotgun; on Ice Age, Ice Age animals stand \
                 in). Modded: a Modder's Edition game's own survival, where its data has one. \
                 With both, the game asks which when you pick Survival."
            } else {
                "Modded: a Modder's Edition game's own survival, where its data has one. The \
                 mobile game's classic Survive mode comes with importing your Carnivores 2 \
                 (Dinosaur Hunter) APK (Add-ons tab)."
            },
        );
        changed
    }

    /// `play <base> --overlay .. --mod .. [--balance ..]`
    fn play_args(&self) -> Vec<String> {
        let mut args = vec!["play".to_string()];
        if let Some(r) = self.active_recipe() {
            let game = self.recipe_game_with_overlays(r);
            args.extend(game.first().map(|g| g.display().to_string()));
            for o in game.iter().skip(1) {
                args.extend(["--overlay".to_string(), o.display().to_string()]);
            }
            args.extend(["--recipe".to_string(), r.path.display().to_string()]);
            args.extend(r.mods_args());
        } else {
            if let Some(b) = &self.cfg.base {
                args.push(b.display().to_string());
            }
            for o in &self.cfg.overlays {
                args.push("--overlay".into());
                args.push(o.display().to_string());
            }
            for s in self.cfg.enabled_standalones() {
                args.push("--standalone".into());
                args.push(s.display().to_string());
            }
            for a in self.cfg.enabled_addons() {
                args.push("--mod".into());
                args.push(a.display().to_string());
            }
        }
        args.extend(self.balance_args());
        args.extend(self.survival_args());
        let art = match self.cfg.menu_style {
            MenuStyle::Original => self.cfg.base.clone(),
            MenuStyle::IceAge => self.cfg.ice_age.clone(),
            MenuStyle::Mod => None,
            MenuStyle::Mobile => {
                args.push("--mobile-menu".into());
                None
            }
        };
        if let Some(a) = art {
            args.push("--menu-art".into());
            args.push(a.display().to_string());
        }
        args
    }

    /// Menu style choice (PC: four; the 3DS: the title art).
    fn menu_style_ui(&mut self, ui: &mut egui::Ui, three_ds: bool) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Menu:");
            if three_ds {
                let t = &mut self.cfg.three_ds;
                changed |= ui
                    .radio_value(&mut t.ice_age_title, false, "Carnivores 2")
                    .changed();
                changed |= ui
                    .radio_value(&mut t.ice_age_title, true, "Ice Age")
                    .changed();
            } else {
                for (s, name) in [
                    (MenuStyle::Original, "Original"),
                    (MenuStyle::Mod, "Mod"),
                    (MenuStyle::IceAge, "Ice Age"),
                    (MenuStyle::Mobile, "Experimental mobile layout"),
                ] {
                    changed |= ui.radio_value(&mut self.cfg.menu_style, s, name).changed();
                }
            }
        });
        let wants_ia = if three_ds {
            self.cfg.three_ds.ice_age_title
        } else {
            self.cfg.menu_style == MenuStyle::IceAge
        };
        if wants_ia {
            ui.horizontal(|ui| {
                let shown = self
                    .cfg
                    .ice_age
                    .as_ref()
                    .map_or("Ice Age folder: not set".to_string(), |p| {
                        format!("Ice Age folder: {}", p.display())
                    });
                let ok = self
                    .cfg
                    .ice_age
                    .as_deref()
                    .is_some_and(|p| describe_game(p).is_some());
                ui.label(RichText::new(shown).color(if ok { SAND } else { RED }));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.ice_age = Some(p);
                        changed = true;
                    }
                }
            });
            help(
                ui,
                "Your own Carnivores: Ice Age folder (the one with HUNTDAT); its menu art only.",
            );
        }
        changed
    }

    fn play(&mut self) {
        let Some(pc) = find_tool("c2-pc", self.cfg.tools.as_deref()) else {
            self.note = Some((
                "c2-pc not found: set the tools folder in Settings.".into(),
                RED,
            ));
            return;
        };
        let steps = vec![Step::Launch {
            program: pc,
            args: self.play_args(),
        }];
        self.job = Some(Job::start("Starting the game", steps));
    }

    /// The same game in a VR headset (`c2-vr`, README M17): it checks the
    /// OpenXR runtime and the game, its findings in the log. A first draft:
    /// it shows a test scene in the headset, not the game yet; run and
    /// waited for rather than launched, so its report lands in the log.
    fn play_vr(&mut self) {
        let Some(vr) = find_tool("c2-vr", self.cfg.tools.as_deref()) else {
            self.note = Some((
                "c2-vr not found: set the tools folder in Settings.".into(),
                RED,
            ));
            return;
        };
        let steps = vec![Step::Run {
            program: vr,
            args: self.play_args(),
        }];
        self.job = Some(Job::start("Starting the game in VR", steps));
    }

    fn build_3ds(&mut self) {
        let tools = self.cfg.tools.clone();
        let (Some(cook), Some(app)) = (
            find_tool("c2-cook", tools.as_deref()),
            find_tool("c2-3ds.3dsx", tools.as_deref()),
        ) else {
            self.note = Some((
                "c2-cook or c2-3ds.3dsx not found: set the tools folder in Settings.".into(),
                RED,
            ));
            return;
        };
        let recipe = self.active_recipe().cloned();
        let recipe_game = recipe.as_ref().map(|r| self.recipe_game_with_overlays(r));
        let base = match &recipe_game {
            Some(g) => g.first().cloned(),
            None => self.cfg.base.clone(),
        };
        let (Some(base), Some(out)) = (base, self.cfg.three_ds.out.clone()) else {
            self.note = Some(("Set the game folder and the SD card first.".into(), RED));
            return;
        };
        let t = &self.cfg.three_ds;
        let name = install_name(&t.name);
        let dir = out.join("3ds").join(&name);
        let mut steps = Vec::new();
        let title_art = t.ice_age_title.then(|| self.cfg.ice_age.clone()).flatten();
        let survival = self.cook_extras();
        let cook_args = |extra: Vec<String>, pack: &str| -> Vec<String> {
            let mut a = vec!["--game".to_string(), base.display().to_string()];
            a.extend(extra);
            a.extend(survival.clone());
            if let Some(ia) = &title_art {
                a.extend(["--menu-art".to_string(), ia.display().to_string()]);
            }
            a.extend([
                "--target".into(),
                t.target.arg().into(),
                "--out".into(),
                dir.join(pack).display().to_string(),
            ]);
            a
        };
        let addons = self.cfg.enabled_addons();
        let modded = !addons.is_empty() || !self.cfg.overlays.is_empty();
        // The balance goes with the game it was made for: the add-ons pack,
        // or the original game's when there are no add-ons.
        if t.vanilla_pack {
            let extra = if modded {
                Vec::new()
            } else {
                self.balance_args()
            };
            steps.push(Step::Run {
                program: cook.clone(),
                args: cook_args(extra, "c2rs.c2pk"),
            });
        }
        if let (Some(r), Some(game)) = (&recipe, &recipe_game) {
            // The recipe's pack in place of the add-ons and standalone ones.
            if t.addons_pack {
                let id = install_name(&r.id());
                let input = match crate::console::RecipeInput::new(
                    r,
                    game,
                    &crate::config::data_dir().join("recipes"),
                ) {
                    Ok(input) => input,
                    Err(e) => {
                        self.note = Some((e, RED));
                        return;
                    }
                };
                match input.prepare(find_tool("c2-pc", tools.as_deref())) {
                    Ok(prepare) => steps.extend(prepare),
                    Err(e) => {
                        self.note = Some((e, RED));
                        return;
                    }
                }
                let mut extra = input.extra.clone();
                extra.extend(self.balance_args());
                let mut a = cook_args(extra, &format!("{id}.c2pk"));
                a[1] = input.game.display().to_string();
                steps.push(Step::Run {
                    program: cook.clone(),
                    args: a,
                });
            }
        } else if t.addons_pack && modded {
            let mut extra = self.balance_args();
            for o in &self.cfg.overlays {
                extra.push("--overlay".into());
                extra.push(o.display().to_string());
            }
            for a in addons {
                extra.push("--mod".into());
                extra.push(a.display().to_string());
            }
            steps.push(Step::Run {
                program: cook.clone(),
                args: cook_args(extra, "modded.c2pk"),
            });
        }
        // A pack per standalone, each its own game (as on PC played alone).
        if t.standalone_packs && recipe.is_none() {
            for s in self.cfg.enabled_standalones() {
                let Some(root) = c2_mods::pack::find_game_root(&s) else {
                    continue;
                };
                let pack = format!(
                    "{}.c2pk",
                    c2_mods::manifest::slug(
                        &s.file_name()
                            .map_or_else(|| "game".into(), |n| n.to_string_lossy().into_owned())
                    )
                );
                let mut a = cook_args(Vec::new(), &pack);
                if let Some(i) = a.iter().position(|x| x == "--game") {
                    a[i + 1] = root.display().to_string();
                }
                steps.push(Step::Run {
                    program: cook.clone(),
                    args: a,
                });
            }
        }
        steps.push(Step::Copy {
            from: app,
            to: dir.join(format!("{name}.3dsx")),
        });
        if t.cia {
            let (Some(elf), Some(makerom), Some(bannertool)) = (
                find_tool("c2-3ds.elf", tools.as_deref()),
                find_tool("makerom", tools.as_deref()),
                find_tool("bannertool", tools.as_deref()),
            ) else {
                self.note = Some((
                    "For a .cia: c2-3ds.elf, makerom and bannertool are needed (see the 3DS tab)."
                        .into(),
                    RED,
                ));
                return;
            };
            steps.extend(crate::cia::steps(&crate::cia::Cia {
                name: &name,
                target: t.target,
                elf,
                makerom,
                bannertool,
                icon: t.icon.as_deref(),
                banner: t.banner.as_deref(),
                sound: t.banner_sound.as_deref(),
                art_from: match &t.art_from {
                    crate::config::ArtFrom::Base => Some(base.as_path()),
                    crate::config::ArtFrom::Game(g) => Some(g.as_path()),
                    crate::config::ArtFrom::Plain => None,
                },
                work: crate::config::data_dir().join("cia").join(&name),
                out: out.join("cia").join(format!("{name}.cia")),
            }));
        }
        self.job = Some(Job::start(
            format!("Building the 3DS install '{name}'"),
            steps,
        ));
    }

    /// The base game's files and the Wii U app onto the SD card
    /// (`crate::wiiu`).
    /// The Wii U installs the tab's choices make: (folder, title, `c2-cook`
    /// arguments). An `.rpx` is one install only (the Homebrew Launcher's).
    fn wiiu_installs(&self) -> Vec<(String, String, Vec<String>)> {
        if let Some(r) = self.active_recipe() {
            if !self.cfg.wiiu.addons {
                return Vec::new();
            }
            let games = self.recipe_game_with_overlays(r);
            let Ok(input) = crate::console::RecipeInput::new(
                r,
                &games,
                &crate::config::data_dir().join("recipes"),
            ) else {
                return Vec::new();
            };
            let mut args = input.cook_args();
            args.extend(self.balance_args());
            args.extend(self.cook_extras());
            let title = c2_mods::recipe::Recipe::load(&r.path)
                .map(|r| r.recipe.name)
                .unwrap_or_else(|_| r.id());
            return vec![(install_name(&r.id()), title, args)];
        }
        let Some(base) = self.cfg.base.clone() else {
            return Vec::new();
        };
        let w = &self.cfg.wiiu;
        let game = |root: &std::path::Path| vec!["--game".to_string(), root.display().to_string()];
        let survival = self.cook_extras();
        let addons = self.cfg.enabled_addons();
        let modded = !addons.is_empty() || !self.cfg.overlays.is_empty();
        let (folder, title) = crate::config::base_install(&base);
        let mut v = Vec::new();
        if w.original {
            let mut a = game(&base);
            a.extend(survival.clone());
            if !modded {
                a.extend(self.balance_args());
            }
            v.push((folder.to_string(), title.to_string(), a));
        }
        if w.addons && modded {
            let mut a = game(&base);
            for o in &self.cfg.overlays {
                a.extend(["--overlay".to_string(), o.display().to_string()]);
            }
            for m in addons {
                a.extend(["--mod".to_string(), m.display().to_string()]);
            }
            a.extend(self.balance_args());
            a.extend(survival.clone());
            v.push((format!("{folder}-addons"), format!("{title} Add-ons"), a));
        }
        if w.standalones {
            for s in self.cfg.enabled_standalones() {
                let Some(root) = c2_mods::pack::find_game_root(&s) else {
                    continue;
                };
                let title = s
                    .file_name()
                    .map_or_else(|| "Carnivores".into(), |n| n.to_string_lossy().into_owned());
                let mut a = game(&root);
                a.extend(survival.clone());
                v.push((install_name(&c2_mods::manifest::slug(&title)), title, a));
            }
        }
        if w.format == crate::config::WiiUFormat::Rpx {
            v.truncate(1);
        }
        v
    }

    fn build_wiiu(&mut self) {
        let tools = self.cfg.tools.clone();
        let (Some(cook), Some(app)) = (
            find_tool("c2-cook", tools.as_deref()),
            find_tool(crate::wiiu::APP, tools.as_deref()),
        ) else {
            self.note = Some((
                format!(
                    "c2-cook or {} not found: set the tools folder in Settings.",
                    crate::wiiu::APP
                ),
                RED,
            ));
            return;
        };
        let Some(out) = self.cfg.wiiu.out.clone() else {
            return;
        };
        let wuhbtool = if self.cfg.wiiu.format == crate::config::WiiUFormat::Wuhb {
            match find_tool(crate::wiiu::WUHB_TOOL, tools.as_deref()) {
                Some(tool) => Some(tool),
                None => {
                    self.note = Some((
                        "WUHB output needs wuhbtool from devkitPro wut-tools (install it or set the tools folder in Settings).".into(),
                        RED,
                    ));
                    return;
                }
            }
        } else {
            None
        };
        let mut steps = Vec::new();
        if let Some(r) = self.active_recipe().filter(|_| self.cfg.wiiu.addons) {
            let games = self.recipe_game_with_overlays(r);
            let input = match crate::console::RecipeInput::new(
                r,
                &games,
                &crate::config::data_dir().join("recipes"),
            ) {
                Ok(input) => input,
                Err(e) => {
                    self.note = Some((e, RED));
                    return;
                }
            };
            match input.prepare(find_tool("c2-pc", tools.as_deref())) {
                Ok(prepare) => steps.extend(prepare),
                Err(e) => {
                    self.note = Some((e, RED));
                    return;
                }
            }
        }
        for (name, title, cook_args) in self.wiiu_installs() {
            let art = crate::wiiu::art_source(&cook_args);
            let own = self.cfg.wiiu.art.get(&name).cloned().unwrap_or_default();
            let cook = cook.clone();
            let app = app.clone();
            let wuhbtool = wuhbtool.clone();
            let out = out.clone();
            // Read generated menu art only after the recipe merge has finished.
            steps.push(Step::Task(Box::new(move |job| {
                for step in crate::wiiu::steps(
                    &crate::wiiu::Install {
                        name: &name,
                        title: &title,
                        cook_args,
                        art_from: art.as_deref(),
                        own_art: [own.icon.as_deref(), own.tv.as_deref(), own.drc.as_deref()],
                    },
                    &crate::wiiu::Tools {
                        cook: &cook,
                        app: &app,
                        wuhbtool: wuhbtool.as_deref(),
                    },
                    &out,
                    &crate::config::data_dir().join("wiiu").join(&name),
                ) {
                    job.run(step)?;
                }
                Ok(())
            })));
        }
        self.job = Some(Job::start("Building the Wii U installs", steps));
    }

    // ------------------------------------------------------------ tabs

    fn game_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "The game");
        help(
            ui,
            "Your Carnivores 2 folder (the one with HUNTDAT in it). Drop it on this window, \
             browse to it, or paste its path.",
        );
        ui.add_space(6.0);
        panel(ui, |ui| {
            ui.label(RichText::new("Base game").color(GOLD));
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.base_text)
                        .desired_width(ui.available_width() - 90.0)
                        .hint_text("/path/to/Carnivores 2"),
                );
                if r.lost_focus() && !self.base_text.trim().is_empty() {
                    let p = PathBuf::from(self.base_text.trim());
                    if Some(&p) != self.cfg.base.as_ref() {
                        self.set_base(p);
                    }
                }
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.set_base(p);
                    }
                }
            });
            match self.cfg.base.as_deref().map(|b| (b, describe_game(b))) {
                Some((_, Some(kind))) => {
                    ui.label(RichText::new(format!("OK: {kind}")).color(GREEN.gamma_multiply(2.0)));
                }
                Some((_, None)) => {
                    ui.label(RichText::new("No HUNTDAT folder there.").color(RED));
                }
                None => help(ui, "Not set yet."),
            }
        });
        ui.add_space(8.0);
        panel(ui, |ui| {
            ui.label(RichText::new("Base overlays").color(GOLD));
            help(
                ui,
                "Game folders copied over the base first. Most add-ons are made for the Modder's \
                 Edition: add its Carnivores 2 folder here.",
            );
            let mut remove = None;
            for (i, o) in self.cfg.overlays.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(o.display().to_string());
                    if describe_game(o).is_none() {
                        ui.label(RichText::new("(no HUNTDAT)").color(RED));
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.cfg.overlays.remove(i);
                self.save();
            }
            if ui.button("Add overlay folder...").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.cfg.overlays.push(p);
                    self.save();
                }
            }
        });
        ui.add_space(8.0);
        panel(ui, |ui| {
            ui.label(RichText::new("Only have the phone game?").color(GOLD));
            help(
                ui,
                "Carnivores: Dinosaur Hunter or Ice Age for Android: pick your APK and the game \
                 is built from it (its areas, animals, weapons, sounds and the phone's menus) \
                 and set as the base game. It all stays on this computer.",
            );
            let busy = self.busy();
            if ui
                .add_enabled(!busy, egui::Button::new("Use my APK as the game..."))
                .clicked()
            {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Android app", &["apk"])
                    .pick_file()
                {
                    self.base_from_apk(p);
                }
            }
        });
    }

    fn addons_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Add-ons");
        help(
            ui,
            "Drop add-on folders or archives (.zip, .rar, .7z) on this window. They load top to \
             bottom: where two change the same thing, the lower one wins.",
        );
        if self.mode == Mode::WiiU {
            help(
                ui,
                "The Wii U version doesn't load add-ons yet: the list is kept for when it does.",
            );
        }
        ui.add_space(6.0);
        self.recipe_ui(ui);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy(),
                    egui::Button::new("Import from the mobile game (APK)..."),
                )
                .on_hover_text(
                    "Your own copy of Carnivores: Dinosaur Hunter or Ice Age for Android: its \
                     animals and weapons this game doesn't have become add-ons (for the \
                     Modder's Edition: add it as a base overlay first).",
                )
                .clicked()
            {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Android app", &["apk"])
                    .pick_file()
                {
                    self.import_apk(p);
                }
            }
            if ui
                .add_enabled(!self.busy(), egui::Button::new("Import Ice Age..."))
                .on_hover_text(
                    "Your own Carnivores: Ice Age folder: its animals and areas become add-ons \
                     listed beside Carnivores 2's (for the Modder's Edition: add it as a base \
                     overlay first).",
                )
                .clicked()
            {
                let start = self.cfg.ice_age.clone();
                let mut d = rfd::FileDialog::new();
                if let Some(s) = &start {
                    d = d.set_directory(s);
                }
                if let Some(p) = d.pick_folder() {
                    self.import_ice_age(p);
                }
            }
            if ui.button("Add folder...").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.add_addons(vec![p]);
                }
            }
            if ui.button("Add archives...").clicked() {
                if let Some(ps) = rfd::FileDialog::new()
                    .add_filter("Archives", &["zip", "rar", "7z"])
                    .pick_files()
                {
                    self.add_addons(ps);
                }
            }
        });
        ui.add_space(6.0);
        let n = self.cfg.addons.len();
        let mut changed = false;
        let mut action: Option<(usize, i32)> = None;
        panel(ui, |ui| {
            if n == 0 {
                help(ui, "No add-ons yet.");
            }
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for (i, a) in self.cfg.addons.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut a.enabled, "").changed();
                            let name =
                                RichText::new(a.name()).color(if a.enabled { TEXT } else { DIM });
                            ui.label(name).on_hover_text(a.path.display().to_string());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Remove").clicked() {
                                        action = Some((i, 0));
                                    }
                                    if ui
                                        .add_enabled(i + 1 < n, egui::Button::new("v").small())
                                        .clicked()
                                    {
                                        action = Some((i, 1));
                                    }
                                    if ui
                                        .add_enabled(i > 0, egui::Button::new("^").small())
                                        .clicked()
                                    {
                                        action = Some((i, -1));
                                    }
                                },
                            );
                        });
                    }
                });
        });
        if let Some((i, d)) = action {
            match d {
                0 => {
                    self.cfg.addons.remove(i);
                }
                -1 => self.cfg.addons.swap(i, i - 1),
                _ => self.cfg.addons.swap(i, i + 1),
            }
            changed = true;
        }
        if changed {
            self.save();
        }
        ui.add_space(12.0);
        self.standalones_ui(ui);
    }

    /// The recipe page while one is set up, else the recipe in use (or a
    /// button to open one).
    fn recipe_ui(&mut self, ui: &mut egui::Ui) {
        let dirs = self.game_folders().0;
        let busy = self.busy();
        if let Some(p) = &mut self.recipe_page {
            let mut ask = crate::recipe_page::Ask::Nothing;
            let play = self.mode == Mode::Full;
            panel(ui, |ui| ask = p.ui(ui, &dirs, busy, play));
            match ask {
                crate::recipe_page::Ask::Nothing => {}
                crate::recipe_page::Ask::Use => self.use_recipe(false),
                crate::recipe_page::Ask::Play => self.use_recipe(true),
                crate::recipe_page::Ask::Close => self.recipe_page = None,
                crate::recipe_page::Ask::ChooseApk => {
                    if let Some(apk) = rfd::FileDialog::new()
                        .add_filter("Android app", &["apk"])
                        .pick_file()
                    {
                        self.base_from_apk(apk);
                        self.apk_for_recipe = true;
                    }
                }
            }
            return;
        }
        let mut open = None;
        let mut stop = false;
        let mut changed = false;
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                match &mut self.cfg.recipe {
                    Some(r) => {
                        changed |= ui
                            .checkbox(&mut r.on, RichText::new("Recipe").color(SAND).strong())
                            .on_hover_text(
                                "On: play and the builds use the recipe instead of the lists                                  below.",
                            )
                            .changed();
                        ui.label(r.id()).on_hover_text(r.path.display().to_string());
                        if ui.button("Edit").clicked() {
                            open = Some(r.path.clone());
                        }
                        if ui.button("Stop using").clicked() {
                            stop = true;
                        }
                    }
                    None => {
                        ui.label(RichText::new("Recipe").color(SAND).strong());
                        help(ui, "none: drop a recipe (.toml) on the window, or");
                    }
                }
                if ui.button("Open a recipe...").clicked() {
                    open = rfd::FileDialog::new()
                        .add_filter("Recipe", &["toml"])
                        .pick_file();
                }
            });
            if self.active_recipe().is_some() {
                help(
                    ui,
                    "While the recipe is on, the add-on list and the standalone games below                      aren't used.",
                );
            }
        });
        if stop {
            self.cfg.recipe = None;
            changed = true;
        }
        if changed {
            self.save();
        }
        if let Some(p) = open {
            self.open_recipe(p);
        }
    }

    /// Standalone games: whole games made from Carnivores 2, combined beside
    /// the base on PC; a 3DS pack each.
    fn standalones_ui(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Standalone games");
        help(
            ui,
            "Whole games made from Carnivores 2 (the folder you extracted). On PC each one's \
             areas, animals and weapons are added beside the base game's, its animals on its \
             own maps; most need the Modder's Edition as a base overlay. On the 3DS each one \
             is a pack of its own.",
        );
        ui.add_space(6.0);
        if ui.button("Add game folder...").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                if c2_mods::pack::find_game_root(&p).is_some() {
                    if !self.cfg.standalones.iter().any(|s| s.path == p) {
                        self.cfg.standalones.push(crate::config::AddOn {
                            path: p,
                            enabled: true,
                        });
                        self.save();
                    }
                } else {
                    self.note = Some((
                        "No game (HUNTDAT with _RES.TXT) in that folder.".into(),
                        RED,
                    ));
                }
            }
        }
        ui.add_space(6.0);
        let mut changed = false;
        let mut remove = None;
        panel(ui, |ui| {
            if self.cfg.standalones.is_empty() {
                help(ui, "No standalone games yet.");
            }
            for (i, a) in self.cfg.standalones.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut a.enabled, "").changed();
                    let name = RichText::new(a.name()).color(if a.enabled { TEXT } else { DIM });
                    ui.label(name).on_hover_text(a.path.display().to_string());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = remove {
            self.cfg.standalones.remove(i);
            changed = true;
        }
        if changed {
            self.save();
        }
    }

    fn balance_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Balance");
        help(
            ui,
            "Prices and scores of the game as set up (base, overlays and the add-ons that are \
             on). Change any of them, to taste or to make add-ons fit together; the game's own \
             files are never changed. Applies to playing on PC and to the 3DS add-ons pack (or \
             the original game pack when there are no add-ons).",
        );
        ui.add_space(6.0);
        if panel(ui, |ui| self.tuning_ui(ui)) {
            self.save();
        }
        ui.add_space(6.0);
        let (bases, mods) = self.game_folders();
        if let Some(e) = self.balance.ui(ui, &bases, &mods) {
            self.note = Some((e, RED));
        }
    }

    fn play_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Play on PC");
        let ready = self
            .cfg
            .base
            .as_deref()
            .is_some_and(|b| describe_game(b).is_some());
        panel(ui, |ui| {
            match &self.cfg.base {
                Some(b) => ui.label(format!("Game: {}", b.display())),
                None => {
                    ui.label(RichText::new("Set the game folder first (The game tab).").color(RED))
                }
            };
            ui.label(format!("Overlays: {}", self.cfg.overlays.len()));
            let on = self.cfg.enabled_addons().len();
            ui.label(format!("Add-ons on: {on} of {}", self.cfg.addons.len()));
            if self.balance.file().is_some() {
                ui.label(RichText::new("Your balance changes apply.").color(GOLD));
            }
            help(
                ui,
                "With add-ons, the combined game is built first (a few seconds; kept until the \
                 list changes).",
            );
            ui.add_space(4.0);
            if self.menu_style_ui(ui, false) | self.survival_ui(ui) {
                self.save();
            }
        });
        ui.add_space(12.0);
        let busy = self.busy();
        ui.horizontal(|ui| {
            if big_button(ui, "PLAY", ready && !busy) {
                self.play();
            }
            ui.add_space(12.0);
            if big_button(ui, "PLAY IN VR", ready && !busy) {
                self.play_vr();
            }
        });
        help(
            ui,
            "PLAY IN VR: the same game in a headset, through OpenXR (start SteamVR, Monado or \
             WiVRn first). A first draft: for now it checks your headset runtime and the game, \
             then shows a test scene in the headset (floor grid, boxes, your controllers); \
             the left menu button quits. Its report is in the log below.",
        );
    }

    fn instance_build_heading(&mut self, ui: &mut egui::Ui) {
        if let Some(r) = self.active_recipe() {
            ui.label(RichText::new(format!("Instance: {}", r.id())).color(GOLD));
            help(ui, "Rebuilds from the saved recipe using the current tools and app. Output choices are remembered for this instance.");
        }
        if ui.button("Choose a saved instance…").clicked() {
            self.tab = Tab::Instances;
            self.recipe_page = None;
        }
    }

    fn three_ds_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Build for 3DS");
        self.instance_build_heading(ui);
        help(
            ui,
            "Makes an install for the SD card: the app and its packs in 3ds/<name>/. Several \
             installs with different names can sit side by side.",
        );
        ui.add_space(6.0);
        let mut changed = false;
        let recipe_id = self.active_recipe().map(|r| install_name(&r.id()));
        panel(ui, |ui| {
            let t = &mut self.cfg.three_ds;
            ui.horizontal(|ui| {
                ui.label("Console:");
                changed |= ui
                    .radio_value(&mut t.target, Target::Old, "Old 3DS / 2DS")
                    .changed();
                changed |= ui
                    .radio_value(&mut t.target, Target::New, "New 3DS")
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Install name:");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut t.name).desired_width(200.0))
                    .changed();
                ui.label(RichText::new(format!("-> 3ds/{}/", install_name(&t.name))).color(DIM));
            });
            ui.horizontal(|ui| {
                ui.label("SD card (or a folder):");
                let shown = t
                    .out
                    .as_ref()
                    .map_or("not set".to_string(), |p| p.display().to_string());
                ui.label(RichText::new(shown).color(SAND));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        t.out = Some(p);
                        changed = true;
                    }
                }
            });
            ui.add_space(4.0);
            changed |= ui
                .checkbox(&mut t.vanilla_pack, "Original game pack (c2rs.c2pk)")
                .changed();
            match &recipe_id {
                Some(id) => {
                    changed |= ui
                        .checkbox(
                            &mut t.addons_pack,
                            format!("The recipe's pack ({id}.c2pk; recipe on, Add-ons tab)"),
                        )
                        .changed();
                }
                None => {
                    changed |= ui
                        .checkbox(
                            &mut t.addons_pack,
                            "Add-ons pack (modded.c2pk: base overlays and the add-ons that are on)",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut t.standalone_packs,
                            "A pack per standalone game that is on (each played on its own)",
                        )
                        .changed();
                }
            }
            help(
                ui,
                "The app asks which pack at start; each keeps its own save.",
            );
        });
        changed |= panel(ui, |ui| {
            self.menu_style_ui(ui, true) | self.survival_ui(ui) | self.pictures_ui(ui)
        });
        ui.add_space(8.0);
        changed |= self.cia_panel(ui);
        if changed {
            self.save();
        }
        ui.add_space(12.0);
        let t = &self.cfg.three_ds;
        let game = match self.active_recipe() {
            Some(r) => self.recipe_game(r).first().cloned(),
            None => self.cfg.base.clone(),
        };
        let ready = game.as_deref().is_some_and(|b| describe_game(b).is_some())
            && t.out.is_some()
            && (t.vanilla_pack
                || t.addons_pack
                || (self.active_recipe().is_none() && t.standalone_packs));
        if big_button(ui, "BUILD", ready && !self.busy()) {
            self.build_3ds();
        }
    }

    fn wiiu_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Build for Wii U");
        self.instance_build_heading(ui);
        help(
            ui,
            "Puts each game on the SD card as its own Wii U Menu entry, in wiiu/apps/<name>/: the \
             original game, the game with your overlays and add-ons, and each standalone game. Its \
             icon and start screens are cut from the game's own menu picture. Choose WUHB for \
             Aroma's Wii U Menu or RPX for the Homebrew Launcher (one game only).",
        );
        ui.add_space(6.0);
        let mut changed = false;
        let recipe_id = self.active_recipe().map(|r| install_name(&r.id()));
        let game = self
            .active_recipe()
            .and_then(|r| self.recipe_game(r).first().cloned())
            .or_else(|| self.cfg.base.clone());
        let kind = game.as_deref().and_then(describe_game);
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("SD card (or a folder):");
                let shown = self
                    .cfg
                    .wiiu
                    .out
                    .as_ref()
                    .map_or("not set".to_string(), |p| p.display().to_string());
                ui.label(RichText::new(shown).color(SAND));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.wiiu.out = Some(p);
                        changed = true;
                    }
                }
            });
        });
        ui.add_space(8.0);
        panel(ui, |ui| {
            ui.label(RichText::new("What goes on").color(GOLD));
            if kind.is_none() {
                ui.label(RichText::new("set the game folder first (The game tab)").color(RED));
            }
            let on = self.cfg.enabled_addons().len();
            let overlays = self.cfg.overlays.len();
            let standalones = self.cfg.enabled_standalones().len();
            let w = &mut self.cfg.wiiu;
            if let Some(id) = &recipe_id {
                changed |= ui
                    .checkbox(&mut w.addons, format!("Saved instance ({id})"))
                    .changed();
            } else {
                changed |= ui.checkbox(&mut w.original, "The original game").changed();
                changed |= ui
                    .checkbox(
                        &mut w.addons,
                        format!("The game with {overlays} overlays and {on} add-ons"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut w.standalones,
                        format!("Each standalone game ({standalones} on)"),
                    )
                    .changed();
            }
            help(ui, "The balance file goes with the add-ons install (or the original game without add-ons).");
            ui.add_space(4.0);
            changed |= self.pictures_ui(ui);
            let names: Vec<String> = self
                .wiiu_installs()
                .into_iter()
                .map(|(n, t, _)| {
                    let folder = if self.cfg.wiiu.format == crate::config::WiiUFormat::Rpx {
                        crate::wiiu::SD_FOLDER
                    } else {
                        &n
                    };
                    format!("{t} -> wiiu/apps/{folder}/")
                })
                .collect();
            for n in names {
                ui.label(RichText::new(n).color(DIM));
            }
        });
        ui.add_space(8.0);
        changed |= panel(ui, |ui| self.wiiu_art_ui(ui));
        ui.add_space(8.0);
        panel(ui, |ui| {
            ui.label(RichText::new("The app").color(GOLD));
            match find_tool(crate::wiiu::APP, self.cfg.tools.as_deref()) {
                Some(p) => {
                    ui.label(
                        RichText::new(format!("{}: {}", crate::wiiu::APP, p.display())).color(TEXT),
                    );
                }
                None => {
                    ui.label(
                        RichText::new(format!(
                            "{}: not found (build wiiu-ver/perf-probe, or set the tools folder \
                             in Settings)",
                            crate::wiiu::APP
                        ))
                        .color(RED),
                    );
                }
            }
            ui.horizontal(|ui| {
                ui.label("Output:");
                changed |= ui
                    .radio_value(
                        &mut self.cfg.wiiu.format,
                        crate::config::WiiUFormat::Wuhb,
                        "WUHB (Aroma / Wii U Menu)",
                    )
                    .changed();
                changed |= ui
                    .radio_value(
                        &mut self.cfg.wiiu.format,
                        crate::config::WiiUFormat::Rpx,
                        "RPX (Homebrew Launcher)",
                    )
                    .changed();
            });
            if self.cfg.wiiu.format == crate::config::WiiUFormat::Wuhb {
                match find_tool(crate::wiiu::WUHB_TOOL, self.cfg.tools.as_deref()) {
                    Some(p) => {
                        ui.label(
                            RichText::new(format!("WUHB packager: {}", p.display())).color(TEXT),
                        );
                    }
                    None => {
                        ui.label(RichText::new("wuhbtool not found (install devkitPro wut-tools or set the tools folder in Settings)").color(RED));
                    }
                }
                help(
                    ui,
                    "Each WUHB holds the app and the name of its folder; the game's files go beside it.",
                );
            }
        });
        if changed {
            self.save();
        }
        ui.add_space(12.0);
        let ready =
            kind.is_some() && self.cfg.wiiu.out.is_some() && !self.wiiu_installs().is_empty();
        if big_button(ui, "BUILD", ready && !self.busy()) {
            self.build_wiiu();
        }
    }

    /// Each Wii U install's own art (owner, 2026-11-01): icon, and the TV's
    /// and GamePad's start screens (shown while it loads); any not picked
    /// is cut from the game's menu picture.
    fn wiiu_art_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::cia::{WIIU_DRC, WIIU_ICON, WIIU_TV};
        let mut changed = false;
        ui.label(RichText::new("Wii U Menu art (your own)").color(GOLD));
        help(
            ui,
            "Per install: the icon, and the start screens on the TV and the GamePad while it              loads. Not picked: cut from the game's own menu picture.",
        );
        for (name, title, _) in self.wiiu_installs() {
            let art = self.cfg.wiiu.art.entry(name.clone()).or_default();
            egui::CollapsingHeader::new(format!("{title} ({name})"))
                .id_salt(format!("wiiu-art-{name}"))
                .show(ui, |ui| {
                    for (label, size, slot) in [
                        ("Icon", WIIU_ICON, &mut art.icon),
                        ("TV start screen", WIIU_TV, &mut art.tv),
                        ("GamePad start screen", WIIU_DRC, &mut art.drc),
                    ] {
                        changed |= art_picker(ui, label, size, slot);
                    }
                });
        }
        self.cfg.wiiu.art.retain(|_, a| !a.is_empty());
        changed
    }

    /// The `.cia` options: the tools (with where to get them) and the
    /// user's home-menu art.
    fn cia_panel(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::cia::{png_size, unique_id, BANNER, BANNERTOOL_URL, ICON, MAKEROM_URL};
        let tools = self.cfg.tools.clone();
        let mut changed = false;
        panel(ui, |ui| {
            let t = &mut self.cfg.three_ds;
            changed |= ui
                .checkbox(
                    &mut t.cia,
                    "Also make a .cia (installed with FBI, it shows on the home menu)",
                )
                .changed();
            if !t.cia {
                return;
            }
            let name = install_name(&t.name);
            help(
                ui,
                &format!(
                    "-> cia/{name}.cia, title ID 00040000{:05X}00. It uses the packs in 3ds/{name}/.",
                    unique_id(&name)
                ),
            );
            ui.add_space(4.0);
            for (tool, url) in [("makerom", MAKEROM_URL), ("bannertool", BANNERTOOL_URL)] {
                ui.horizontal(|ui| match find_tool(tool, tools.as_deref()) {
                    Some(p) => {
                        ui.label(RichText::new(format!("{tool}: {}", p.display())).color(TEXT));
                    }
                    None => {
                        ui.label(
                            RichText::new(format!("{tool}: not found. Download it from"))
                                .color(RED),
                        );
                        ui.hyperlink_to(url, url);
                    }
                });
            }
            if find_tool("c2-3ds.elf", tools.as_deref()).is_none() {
                ui.label(
                    RichText::new("c2-3ds.elf: not found (it comes with c2-3ds.3dsx)").color(RED),
                );
            }
            help(
                ui,
                "These are separate downloads (not ours): put makerom and bannertool beside this \
                 launcher or in the tools folder (Settings). On Linux and macOS, make them \
                 executable (chmod +x).",
            );
            ui.add_space(6.0);
            ui.label(RichText::new("Home menu art (your own)").color(GOLD));
            for (label, size, slot, filter) in [
                ("Icon", Some(ICON), &mut t.icon, &["png"][..]),
                ("Banner", Some(BANNER), &mut t.banner, &["png"][..]),
                (
                    "Banner sound",
                    None,
                    &mut t.banner_sound,
                    &["wav", "ogg"][..],
                ),
            ] {
                ui.horizontal(|ui| {
                    let want = match size {
                        Some((w, h)) => format!("{w}x{h} PNG"),
                        None => "WAV or OGG, up to 3 s".into(),
                    };
                    ui.label(format!("{label} ({want}):"));
                    match slot.as_ref() {
                        Some(p) => {
                            ui.label(RichText::new(p.display().to_string()).color(SAND));
                            if let Some(want) = size {
                                match png_size(p) {
                                    Some(s) if s == want => {}
                                    Some((w, h)) => {
                                        ui.label(RichText::new(format!("is {w}x{h}")).color(RED));
                                    }
                                    None => {
                                        ui.label(RichText::new("not a PNG").color(RED));
                                    }
                                }
                            }
                        }
                        None => help(ui, "from the game below"),
                    }
                    if ui.button("Browse...").clicked() {
                        if let Some(p) =
                            rfd::FileDialog::new().add_filter(label, filter).pick_file()
                        {
                            *slot = Some(p);
                            changed = true;
                        }
                    }
                    if slot.is_some() && ui.small_button("Clear").clicked() {
                        *slot = None;
                        changed = true;
                    }
                });
            }
            // Not picked: from a game's own menu pictures.
            use crate::config::ArtFrom;
            let label = |a: &ArtFrom| match a {
                ArtFrom::Base => "the base game".to_string(),
                ArtFrom::Plain => "plain placeholders".to_string(),
                ArtFrom::Game(g) => g.file_name().map_or_else(
                    || g.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                ),
            };
            let mut choices = vec![ArtFrom::Base];
            choices.extend(
                self.cfg
                    .standalones
                    .iter()
                    .map(|s| ArtFrom::Game(s.path.clone())),
            );
            choices.push(ArtFrom::Plain);
            let t = &mut self.cfg.three_ds;
            ui.horizontal(|ui| {
                ui.label("Icon and banner not picked: from");
                egui::ComboBox::from_id_salt("art_from")
                    .selected_text(label(&t.art_from))
                    .show_ui(ui, |ui| {
                        for c in choices {
                            let l = label(&c);
                            changed |= ui.selectable_value(&mut t.art_from, c, l).changed();
                        }
                    });
            });
            help(
                ui,
                "Made from that game's main menu picture on this computer: the banner its top \
                 (logo and dinosaur), the icon its dinosaur.",
            );
        });
        changed
    }

    fn windows_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Windows package");
        help(ui, "Build a saved instance as a portable folder with c2-pc.exe and its game data. Select Build Windows on Instances first. Supply the Windows release c2-pc.exe in the tools folder (Settings); on Linux, c2-pc is also needed to merge the instance.");
        ui.label(self.cfg.windows_out.as_ref().map_or_else(
            || "Output folder: not set".into(),
            |p| p.display().to_string(),
        ));
        if ui.button("Choose output folder…").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                self.cfg.windows_out = Some(p);
                self.save();
            }
        }
        let ready =
            !self.busy() && self.active_recipe().is_some() && self.cfg.windows_out.is_some();
        if ui
            .add_enabled(ready, egui::Button::new("Build Windows package"))
            .clicked()
        {
            self.build_windows();
        }
        help(ui, "The output contains a folder named after the instance. Rebuilding replaces that generated folder, so keep personal files elsewhere. Copy the entire folder to Windows and double-click c2-pc.exe. Game data is supplied by you, separately from the engine download.");
    }

    fn build_windows(&mut self) {
        let result = (|| -> Result<Vec<Step>, String> {
            let r = self
                .active_recipe()
                .ok_or("Choose a saved instance first.")?;
            let pc = find_tool("c2-pc", self.cfg.tools.as_deref())
                .ok_or("c2-pc not found in the tools folder.")?;
            let exe = find_tool("c2-pc.exe", self.cfg.tools.as_deref()).ok_or("Windows c2-pc.exe not found. Download a Windows engine build or build it on Windows, then set the tools folder in Settings.")?;
            let games = self.recipe_game_with_overlays(r);
            let base = games.first().ok_or("Set the instance's base game first.")?;
            let work = crate::config::data_dir()
                .join("windows")
                .join(install_name(&r.id()));
            let mut args = vec![
                "recipe".into(),
                "build".into(),
                r.path.display().to_string(),
                "--base".into(),
                base.display().to_string(),
            ];
            for overlay in games.iter().skip(1) {
                args.extend(["--overlay".into(), overlay.display().to_string()]);
            }
            args.extend(r.mods_args());
            args.extend(["--out".into(), work.display().to_string()]);
            let out = self
                .cfg
                .windows_out
                .as_ref()
                .ok_or("Choose an output folder.")?
                .join(install_name(&r.id()));
            let mut launch = self.balance_args();
            launch.extend(self.survival_args());
            match self.cfg.menu_style {
                MenuStyle::Mobile => launch.push("--mobile-menu".into()),
                MenuStyle::Original | MenuStyle::IceAge => {
                    let art = if self.cfg.menu_style == MenuStyle::IceAge {
                        &self.cfg.ice_age
                    } else {
                        &self.cfg.base
                    };
                    if let Some(p) = art {
                        launch.extend(["--menu-art".into(), p.display().to_string()]);
                    }
                }
                MenuStyle::Mod => {}
            }
            crate::windows::validate_exe(&exe)?;
            Ok(vec![
                Step::Run { program: pc, args },
                Step::Task(Box::new(move |job| {
                    crate::windows::package(&work, &exe, &out, launch)
                        .map_err(|e| e.to_string())?;
                    job.log(format!("Windows package ready: {}", out.display()));
                    Ok(())
                })),
            ])
        })();
        match result {
            Ok(steps) => self.job = Some(Job::start("Building Windows package", steps)),
            Err(e) => self.note = Some((e, RED)),
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Settings");
        panel(ui, |ui| {
            ui.label(RichText::new("Engine options").color(GOLD));
            help(
                ui,
                "One file with the balance (prices, scores, and PC or mobile for each entry where \
                 they differ), the survival choice, the menu style, the PC game's settings and the \
                 3DS target: keep it, share it, or bring it to another machine. No folders, no game \
                 data.",
            );
            ui.horizontal(|ui| {
                if ui.button("Export options...").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_file_name("c2rs.c2rs-options.toml")
                        .add_filter("c2rs options", &["toml"])
                        .save_file()
                    {
                        let text = crate::options::Options::current(&self.cfg, &self.balance.edits)
                            .to_toml();
                        self.note = Some(match std::fs::write(&p, text) {
                            Ok(()) => (format!("Options written to {}", p.display()), TEXT),
                            Err(e) => (format!("Not written: {e}"), RED),
                        });
                    }
                }
                if ui.button("Import options...").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("c2rs options", &["toml"])
                        .pick_file()
                    {
                        let r = std::fs::read_to_string(&p)
                            .map_err(|e| e.to_string())
                            .and_then(|t| crate::options::Options::from_toml(&t))
                            .and_then(|o| o.apply(&mut self.cfg, &mut self.balance.edits));
                        self.note = Some(match r {
                            Ok(done) => {
                                self.save();
                                let _ = self.balance.store();
                                (format!("Imported: {}.", done.join(", ")), TEXT)
                            }
                            Err(e) => (format!("Not imported: {e}"), RED),
                        });
                    }
                }
            });
        });
        ui.add_space(8.0);
        panel(ui, |ui| {
            ui.label(RichText::new("Tools folder").color(GOLD));
            help(
                ui,
                "Where c2-pc, c2-vr, c2-cook and c2-3ds.3dsx (and for .cia builds c2-3ds.elf, makerom \
                 and bannertool) are, if not beside the launcher.",
            );
            ui.horizontal(|ui| {
                let shown = self
                    .cfg
                    .tools
                    .as_ref()
                    .map_or("beside the launcher".to_string(), |p| {
                        p.display().to_string()
                    });
                ui.label(RichText::new(shown).color(SAND));
                if ui.button("Browse...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.tools = Some(p);
                        self.save();
                    }
                }
                if self.cfg.tools.is_some() && ui.button("Reset").clicked() {
                    self.cfg.tools = None;
                    self.save();
                }
            });
            for tool in [
                "c2-pc",
                "c2-vr",
                "c2-cook",
                "c2-3ds.3dsx",
                "c2-3ds.elf",
                "makerom",
                "bannertool",
                crate::wiiu::APP,
            ] {
                if (["c2-3ds.3dsx", "c2-3ds.elf", "makerom", "bannertool"].contains(&tool)
                    && !self.shows_3ds())
                    || (tool == crate::wiiu::APP && !self.shows_wiiu())
                {
                    continue;
                }
                let found = find_tool(tool, self.cfg.tools.as_deref());
                let (text, col) = match &found {
                    Some(p) => (format!("{tool}: {}", p.display()), TEXT),
                    None => (format!("{tool}: not found"), RED),
                };
                ui.label(RichText::new(text).color(col));
            }
        });
        ui.add_space(8.0);
        self.search_dirs_ui(ui);
        if self.mode == Mode::Full {
            ui.add_space(8.0);
            panel(ui, |ui| {
                let mut wiiu = self.cfg.show_wiiu != Some(false);
                if ui.checkbox(&mut wiiu, "Show the Wii U section").changed() {
                    self.cfg.show_wiiu = Some(wiiu);
                    self.save();
                }
                let mut on = self.cfg.show_3ds == Some(true);
                if ui.checkbox(&mut on, "Show the 3DS section").changed() {
                    self.cfg.show_3ds = Some(on);
                    self.save();
                }
            });
        }
    }

    /// Where instances' downloads are looked for.
    fn search_dirs_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        panel(ui, |ui| {
            ui.label(RichText::new("Search folders").color(GOLD));
            help(
                ui,
                "Where an instance's downloads are looked for (each folder and the folders \
                 and .zips in it, up to 4 folders down). With none added: source, source/mods, addons and \
                 mods beside the recipes folder.",
            );
            let mut remove = None;
            for (i, d) in self.cfg.search_dirs().iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(d.display().to_string()).color(SAND));
                    if !self.cfg.mod_dirs.is_empty() && ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.cfg.mod_dirs.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Add folder...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        if self.cfg.mod_dirs.is_empty() {
                            self.cfg.mod_dirs = self.cfg.search_dirs();
                        }
                        self.cfg.mod_dirs.push(p);
                        changed = true;
                    }
                }
                if !self.cfg.mod_dirs.is_empty() && ui.button("Reset").clicked() {
                    self.cfg.mod_dirs.clear();
                    changed = true;
                }
            });
        });
        if changed {
            self.save();
        }
    }

    /// Full launcher, first start: show the 3DS section or not.
    fn first_launch(&mut self, ctx: &egui::Context) {
        egui::Modal::new(egui::Id::unique("first-launch")).show(ctx, |ui| {
            ui.set_width(420.0);
            heading(ui, "Welcome");
            ui.label(
                "This launcher can also build the game for a Nintendo 3DS (homebrew). Show the \
                 3DS options? You can change this in Settings.",
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("Show 3DS options").color(GOLD))
                    .clicked()
                {
                    self.cfg.show_3ds = Some(true);
                    self.save();
                }
                if ui.button("Keep them hidden").clicked() {
                    self.cfg.show_3ds = Some(false);
                    self.save();
                }
            });
        });
    }

    fn dropped(&mut self, files: Vec<PathBuf>) {
        if files.is_empty() {
            return;
        }
        // A game folder dropped on the game tab (or anywhere while there is
        // none) is the base; everything else is an add-on.
        let first = files[0].clone();
        let ext_is =
            |p: &PathBuf, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
        // A recipe opens its page; while it is open, folders dropped are
        // looked through for its entries, and an APK is its game.
        if ext_is(&first, "toml") {
            self.open_recipe(first);
            return;
        }
        let busy = self.busy();
        if let Some(p) = &mut self.recipe_page {
            if ext_is(&first, "apk") && p.is_apk_base() && !busy {
                self.base_from_apk(first);
                self.apk_for_recipe = true;
                self.tab = Tab::AddOns;
                return;
            }
            if self.tab == Tab::AddOns && files.iter().all(|f| f.is_dir()) {
                p.scan(files);
                return;
            }
        }
        // An APK with no game set (or on the game tab): the game from it.
        let apk = first
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("apk"));
        if apk && (self.tab == Tab::Game || self.cfg.base.is_none()) && !self.busy() {
            self.base_from_apk(first);
            return;
        }
        let is_game = describe_game(&first).is_some();
        if is_game && (self.tab == Tab::Game || self.cfg.base.is_none()) {
            self.set_base(first);
            return;
        }
        self.tab = Tab::AddOns;
        self.add_addons(files);
    }
}

fn log_panel(ui: &mut egui::Ui, job: &Job) {
    let (status, col) = match job.result() {
        None => ("working...", GOLD),
        Some(true) => ("done", TEXT),
        Some(false) => ("failed", RED),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(&job.title).color(SAND).strong());
        ui.label(RichText::new(status).color(col));
        if job.result().is_none() {
            ui.spinner();
        }
    });
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .max_height(160.0)
        .show(ui, |ui| {
            for l in job.lines() {
                ui.label(RichText::new(l).monospace().size(12.0));
            }
        });
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        self.dropped(dropped);
        self.import_done();
        self.apk_base_done();
        self.saving_done();
        if self.saving.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let mut next = None;
        let busy = self.busy();
        if let Some(p) = &mut self.recipe_page {
            p.poll();
            if p.working() {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            } else if p.ready() && !busy {
                next = p.next.take();
            }
        }
        if let Some(destination) = next {
            self.use_recipe(destination == crate::instances::Destination::Play);
            match destination {
                crate::instances::Destination::Play => {}
                crate::instances::Destination::Windows => self.tab = Tab::Windows,
                crate::instances::Destination::ThreeDs => {
                    self.cfg.show_3ds = Some(true);
                    self.tab = Tab::ThreeDs;
                    self.save();
                }
                crate::instances::Destination::WiiU => self.tab = Tab::WiiU,
            }
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Panel::top("tabs")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("CARNIVORES 2")
                            .size(22.0)
                            .color(GOLD)
                            .strong(),
                    );
                    ui.add_space(16.0);
                    let mut tabs = vec![
                        (Tab::Instances, "Instances"),
                        (Tab::Game, "The game"),
                        (Tab::AddOns, "Add-ons"),
                    ];
                    // The Wii U reads the game's files as they are: no balance yet.
                    if self.mode != Mode::WiiU {
                        tabs.push((Tab::Balance, "Balance"));
                    }
                    if self.mode == Mode::Full {
                        tabs.push((Tab::Play, "Play"));
                        tabs.push((Tab::Windows, "Windows"));
                    }
                    if self.shows_3ds() {
                        tabs.push((Tab::ThreeDs, "3DS"));
                    }
                    if self.shows_wiiu() {
                        tabs.push((Tab::WiiU, "Wii U"));
                    }
                    tabs.push((Tab::Settings, "Settings"));
                    for (t, name) in tabs {
                        let col = if self.tab == t { GOLD } else { TEXT };
                        if ui
                            .selectable_label(
                                self.tab == t,
                                RichText::new(name).size(16.0).color(col),
                            )
                            .clicked()
                        {
                            self.tab = t;
                        }
                    }
                });
            });
        egui::Panel::bottom("log")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(ui, |ui| {
                if let Some((n, c)) = &self.note {
                    ui.label(RichText::new(n).color(*c));
                }
                match &self.job {
                    Some(j) => log_panel(ui, j),
                    None => help(ui, "Drop the game folder or add-ons on this window."),
                }
            });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                Tab::Instances => self.instances_tab(ui),
                Tab::Game => self.game_tab(ui),
                Tab::AddOns => self.addons_tab(ui),
                Tab::Balance => self.balance_tab(ui),
                Tab::Play => self.play_tab(ui),
                Tab::ThreeDs => self.three_ds_tab(ui),
                Tab::WiiU => self.wiiu_tab(ui),
                Tab::Windows => self.windows_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            });
        });
        if self.mode == Mode::Full && self.cfg.show_3ds.is_none() {
            self.first_launch(&ctx);
        }
        // A tab that went away (3DS hidden) falls back.
        if (self.tab == Tab::ThreeDs && !self.shows_3ds())
            || (self.tab == Tab::WiiU && !self.shows_wiiu())
        {
            self.tab = Tab::Game;
        }
    }
}

/// The mobile values file the config names, if it reads.
fn cfg_mobile(cfg: &Config) -> Option<c2_mods::balance::Balance> {
    cfg.mobile_values
        .as_deref()
        .and_then(|p| c2_mods::balance::Balance::load(p).ok())
}
