//! The launcher window: a tab per job (game folders, add-ons, balance,
//! play, 3DS, settings), the job log at the bottom.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::config::{describe_game, install_name, AddOn, Config, MenuStyle, Target};
use crate::jobs::{is_archive, Job, Step};
use crate::theme::*;
use crate::{find_tool, Mode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Game,
    AddOns,
    Balance,
    Play,
    ThreeDs,
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
            tab: Tab::Game,
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
        self.mode == Mode::ThreeDs || self.cfg.show_3ds == Some(true)
    }

    fn save(&mut self) {
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

    /// `--balance FILE` if the balance editor changed anything.
    fn balance_args(&self) -> Vec<String> {
        match self.balance.file() {
            Some(f) => vec!["--balance".into(), f.display().to_string()],
            None => Vec::new(),
        }
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
        let (Some(base), Some(out)) = (self.cfg.base.clone(), self.cfg.three_ds.out.clone()) else {
            return;
        };
        let t = &self.cfg.three_ds;
        let name = install_name(&t.name);
        let dir = out.join("3ds").join(&name);
        let mut steps = Vec::new();
        let title_art = t.ice_age_title.then(|| self.cfg.ice_age.clone()).flatten();
        let survival = self.survival_args();
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
        if t.addons_pack && modded {
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
        if t.standalone_packs {
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
    }

    fn addons_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Add-ons");
        help(
            ui,
            "Drop add-on folders or archives (.zip, .rar, .7z) on this window. They load top to \
             bottom: where two change the same thing, the lower one wins.",
        );
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
        if big_button(ui, "PLAY", ready && !self.busy()) {
            self.play();
        }
    }

    fn three_ds_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Build for 3DS");
        help(
            ui,
            "Makes an install for the SD card: the app and its packs in 3ds/<name>/. Several \
             installs with different names can sit side by side.",
        );
        ui.add_space(6.0);
        let mut changed = false;
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
            help(
                ui,
                "The app asks which pack at start; each keeps its own save.",
            );
        });
        changed |= panel(ui, |ui| self.menu_style_ui(ui, true) | self.survival_ui(ui));
        ui.add_space(8.0);
        changed |= self.cia_panel(ui);
        if changed {
            self.save();
        }
        ui.add_space(12.0);
        let t = &self.cfg.three_ds;
        let ready = self
            .cfg
            .base
            .as_deref()
            .is_some_and(|b| describe_game(b).is_some())
            && t.out.is_some()
            && (t.vanilla_pack || t.addons_pack || t.standalone_packs);
        if big_button(ui, "BUILD", ready && !self.busy()) {
            self.build_3ds();
        }
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
                "Where c2-pc, c2-cook and c2-3ds.3dsx (and for .cia builds c2-3ds.elf, makerom \
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
                "c2-cook",
                "c2-3ds.3dsx",
                "c2-3ds.elf",
                "makerom",
                "bannertool",
            ] {
                let found = find_tool(tool, self.cfg.tools.as_deref());
                let (text, col) = match &found {
                    Some(p) => (format!("{tool}: {}", p.display()), TEXT),
                    None => (format!("{tool}: not found"), RED),
                };
                ui.label(RichText::new(text).color(col));
            }
        });
        if self.mode == Mode::Full {
            ui.add_space(8.0);
            panel(ui, |ui| {
                let mut on = self.cfg.show_3ds == Some(true);
                if ui.checkbox(&mut on, "Show the 3DS section").changed() {
                    self.cfg.show_3ds = Some(on);
                    self.save();
                }
            });
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
                        (Tab::Game, "The game"),
                        (Tab::AddOns, "Add-ons"),
                        (Tab::Balance, "Balance"),
                    ];
                    if self.mode == Mode::Full {
                        tabs.push((Tab::Play, "Play"));
                    }
                    if self.shows_3ds() {
                        tabs.push((Tab::ThreeDs, "3DS"));
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
                Tab::Game => self.game_tab(ui),
                Tab::AddOns => self.addons_tab(ui),
                Tab::Balance => self.balance_tab(ui),
                Tab::Play => self.play_tab(ui),
                Tab::ThreeDs => self.three_ds_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            });
        });
        if self.mode == Mode::Full && self.cfg.show_3ds.is_none() {
            self.first_launch(&ctx);
        }
        // A tab that went away (3DS hidden) falls back.
        if self.tab == Tab::ThreeDs && !self.shows_3ds() {
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
