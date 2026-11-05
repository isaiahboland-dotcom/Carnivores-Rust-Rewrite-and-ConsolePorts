//! The launcher window (owner, 2026-11-05): a home screen of instances and
//! Settings; editing an instance shows its Base game, Add-ons and Balance
//! pages; Build asks for the platform and shows only its page. The job log
//! is at the bottom.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::config::{describe_game, install_name, AddOn, Config, MenuStyle, RecipeUse, Target};
use crate::jobs::{is_archive, Job, Step};
use crate::theme::*;
use crate::{find_tool, Mode};

mod builds;
mod edit;
mod home;
mod menu;
mod play;
mod recipes;
mod screen;
mod settings;
mod setup;
pub(crate) mod standalones;
mod three_ds;
mod tuning;
mod wiiu;
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    /// The home screen.
    Instances,
    Settings,
    /// Editing an instance.
    BaseGame,
    AddOns,
    Balance,
    /// Building an instance: the platform, then its page.
    Build,
    Windows,
    ThreeDs,
    WiiU,
}

impl Tab {
    fn editing(self) -> bool {
        matches!(self, Tab::BaseGame | Tab::AddOns | Tab::Balance)
    }

    fn building(self) -> bool {
        matches!(self, Tab::Build | Tab::Windows | Tab::ThreeDs | Tab::WiiU)
    }
}

/// Configuration, presentation state, and one owner of asynchronous work.
pub struct Launcher {
    cfg: Config,
    ui: View,
    job: crate::jobs::Handle,
}

/// Presentation data only; workers and completion receivers belong to jobs.
struct View {
    mode: Mode,
    tab: Tab,
    note: Option<(String, Color32)>,
    balance: crate::balance::Editor,
    recipe_page: Option<crate::recipe_page::Page>,
    recipe_edit: Option<crate::recipe_edit::Editor>,
    instances: crate::instances::List,
    /// The instance whose platform is being picked after Build.
    build_for: Option<PathBuf>,
    /// Fix missing, while it runs.
    fixing: Option<home::Fixing>,
    dialog: Option<home::Dialog>,
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
    screen::scroll_style(ctx);
}

pub(crate) use screen::steam_game_mode;

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

/// A build's BUILD button, with what keeps it greyed out listed under it
/// (owner's report 2026-11-05: greyed out with no reason given).
fn build_button(ui: &mut egui::Ui, blockers: &[String], busy: bool) -> bool {
    let go = big_button(ui, "BUILD", blockers.is_empty() && !busy);
    if busy {
        help(ui, "Wait for the current operation to finish.");
    }
    for b in blockers {
        ui.label(RichText::new(format!("Not ready: {b}")).color(RED));
    }
    go
}

/// Why `game` can't be built from, if it can't.
fn game_blocker(game: Option<&Path>, instance: bool) -> Option<String> {
    match game {
        None if instance => {
            Some("the instance has no game: Edit it and choose its base game".into())
        }
        None => Some("pick an instance to build on the Instances page".into()),
        Some(g) if describe_game(g).is_none() => {
            Some(format!("no game (a HUNTDAT folder) in {}", g.display()))
        }
        Some(_) => None,
    }
}

impl Launcher {
    pub fn new(cc: &eframe::CreationContext<'_>, mode: Mode) -> Self {
        style(&cc.egui_ctx);
        let cfg = Config::load();
        let pending = crate::publish::pending(&crate::config::data_dir());
        let mut balance = crate::balance::Editor::load();
        balance.mobile = cfg_mobile(&cfg);
        let ui = View {
            mode,
            tab: Tab::Instances,
            note: (!pending.is_empty()).then(|| (format!("Unfinished install transactions: {}. Inspect data (new output) and previous (backup); close other launchers before recovery.", pending.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")), RED)),
            balance,
            recipe_page: None,
            recipe_edit: None,
            instances: Default::default(),
            build_for: None,
            fixing: None,
            dialog: None,
        };
        Self {
            cfg,
            ui,
            job: Default::default(),
        }
    }

    fn poll_job(&mut self) {
        if let Some(result) = self.job.poll() {
            match result {
                Ok(crate::jobs::Completion::Imported(out)) => self.import_done(out),
                Ok(crate::jobs::Completion::BaseMatch(m)) => {
                    if let Some(editor) = &mut self.ui.recipe_edit {
                        let (game, id) = *m;
                        editor.base_matched(game, id);
                    }
                }
                Ok(crate::jobs::Completion::RecipeBalance(values)) => {
                    if let Some(editor) = &mut self.ui.recipe_edit {
                        editor.values_read(*values);
                    }
                }
                Ok(crate::jobs::Completion::ApkBase { out, for_recipe }) => {
                    self.apk_base_done(out, for_recipe)
                }
                Ok(crate::jobs::Completion::Pinned(pinned)) => {
                    if let Some(editor) = &mut self.ui.recipe_edit {
                        let (kind, entry, dir) = *pinned;
                        editor.pinned(kind, entry, dir);
                    }
                }
                Ok(crate::jobs::Completion::Recipe(messages)) => {
                    if let Some(page) = &mut self.ui.recipe_page {
                        page.apply(messages);
                    }
                }
                Ok(crate::jobs::Completion::Addons(paths)) => self.add_addons(paths),
                Ok(crate::jobs::Completion::Played(p)) => {
                    let (instance, secs, error) = *p;
                    self.played(instance, secs, error);
                }
                Ok(crate::jobs::Completion::Saved(saved)) => {
                    let (r, u) = *saved;
                    self.saving_done(r, u);
                }
                Err(e) => {
                    if let Some(editor) = &mut self.ui.recipe_edit {
                        editor.failed(&e);
                    }
                    if let Some(page) = &mut self.ui.recipe_page {
                        page.failed(&e);
                    }
                    self.ui.note = Some((e, RED));
                }
            }
        }
    }

    fn start_job(&mut self, title: impl Into<String>, steps: Vec<Step>) {
        if let Err(e) = self.job.start(title, steps) {
            self.ui.note = Some((e, RED));
        }
    }

    /// This launcher builds for the 3DS (the full one, or the 3DS builder).
    fn shows_3ds(&self) -> bool {
        self.ui.mode != Mode::WiiU
    }

    fn shows_wiiu(&self) -> bool {
        self.ui.mode != Mode::ThreeDs
    }

    fn save(&mut self) {
        self.cfg.remember_builds();
        if let Err(e) = self.cfg.store() {
            self.ui.note = Some((format!("Settings not saved: {e}"), RED));
        }
    }

    fn busy(&self) -> bool {
        self.job.busy()
    }

    fn can_start_operation(&mut self) -> bool {
        if self.busy() {
            self.ui.note = Some(("Wait for the current operation to finish.".into(), RED));
            false
        } else {
            true
        }
    }

    fn instance_build_heading(&mut self, ui: &mut egui::Ui) {
        if let Some(r) = self.active_recipe() {
            ui.label(RichText::new(format!("Instance: {}", r.id())).color(GOLD));
            help(ui, "Rebuilds from the saved recipe using the current tools and app. Output choices are remembered for this instance.");
        }
    }

    fn dropped(&mut self, files: Vec<PathBuf>) {
        if files.is_empty() {
            return;
        }
        let first = files[0].clone();
        let ext_is =
            |p: &PathBuf, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
        if self.ui.recipe_edit.is_some() {
            self.dropped_on_editor(files);
            return;
        }
        if !self.can_start_operation() {
            return;
        }
        // A recipe opens its page; while it is open, folders dropped are
        // looked through for its entries, and an APK is its game.
        if ext_is(&first, "toml") {
            self.open_recipe(first);
            return;
        }
        if let Some(p) = &mut self.ui.recipe_page {
            if ext_is(&first, "apk") && p.is_apk_base() {
                self.base_from_apk(first, true);
                return;
            }
            if files.iter().all(|f| f.is_dir()) {
                p.scan(files);
                p.dispatch(&mut self.job);
                return;
            }
        }
        self.ui.note = Some((
            "To add a game or add-ons, open an instance with Edit (or make a new one) and drop \
             them there."
                .into(),
            TEXT,
        ));
    }

    /// Dropped while editing: an APK or a game folder on the Base game page
    /// is the base game; folders elsewhere are added as add-ons.
    fn dropped_on_editor(&mut self, files: Vec<PathBuf>) {
        let busy = self.busy();
        let Some(e) = &mut self.ui.recipe_edit else {
            return;
        };
        if busy || e.working() {
            self.ui.note = Some(("Wait for the current operation to finish.".into(), RED));
            return;
        }
        let first = files[0].clone();
        if first
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("apk"))
        {
            self.ui.tab = Tab::BaseGame;
            self.base_from_apk(first, true);
            return;
        }
        if self.ui.tab == Tab::BaseGame && describe_game(&first).is_some() {
            e.set_game(first);
            return;
        }
        let (dirs, other): (Vec<PathBuf>, Vec<PathBuf>) =
            files.into_iter().partition(|f| f.is_dir());
        if !other.is_empty() {
            self.ui.note = Some((
                "Only folders can be added here: unpack archives (.zip, .rar, .7z) first.".into(),
                RED,
            ));
        }
        if !dirs.is_empty() {
            e.queue_pins(crate::recipe_edit::Kind::AddOn, dirs);
            self.ui.tab = Tab::AddOns;
        }
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
        screen::fit_screen(&ctx, self.cfg.ui_scale);
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        self.poll_job();
        self.dropped(dropped);
        self.fix_step();
        let mut next = None;
        let busy = self.busy();
        if let Some(p) = &mut self.ui.recipe_page {
            p.dispatch(&mut self.job);
            if p.working() {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            } else if p.ready() && !busy {
                next = p.next.take();
            }
        }
        if let Some(destination) = next {
            self.use_recipe(destination == crate::instances::Destination::Play);
            if destination == crate::instances::Destination::PlayVr {
                self.play_vr();
            }
            self.ui.tab = match destination {
                crate::instances::Destination::Play | crate::instances::Destination::PlayVr => {
                    Tab::Instances
                }
                crate::instances::Destination::Windows => Tab::Windows,
                crate::instances::Destination::ThreeDs => Tab::ThreeDs,
                crate::instances::Destination::WiiU => Tab::WiiU,
            };
        }
        // Editing shows only the editor's pages; anything else leaves it.
        if self.ui.recipe_edit.is_some() && !self.ui.tab.editing() {
            self.ui.tab = Tab::BaseGame;
        } else if self.ui.recipe_edit.is_none() && self.ui.tab.editing() {
            self.ui.tab = Tab::Instances;
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        egui::Panel::top("tabs")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("log")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(ui, |ui| {
                if let Some((n, c)) = &self.ui.note {
                    ui.label(RichText::new(n).color(*c));
                }
                match self.job.current() {
                    Some(j) => log_panel(ui, j),
                    None => help(
                        ui,
                        "Drop game folders, APKs or add-ons on an instance's Edit pages.",
                    ),
                }
            });
        if self.ui.tab.editing() {
            egui::Panel::bottom("edit-bar")
                .frame(egui::Frame::new().fill(PANEL_HI).inner_margin(8.0))
                .show(ui, |ui| self.edit_bar(ui));
        } else if self.ui.tab.building() {
            egui::Panel::bottom("build-bar")
                .frame(egui::Frame::new().fill(PANEL_HI).inner_margin(8.0))
                .show(ui, |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Cancel").size(16.0))
                                .min_size(egui::vec2(120.0, 30.0)),
                        )
                        .on_hover_text("Back to the instances.")
                        .clicked()
                    {
                        self.go_home();
                    }
                });
        }
        // The editor's own jobs (reading its game, pinning) leave it usable.
        let editor_job = self
            .ui
            .recipe_edit
            .as_ref()
            .is_some_and(|e| e.reading() || e.working());
        let enabled = !self.busy() || editor_job;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_enabled_ui(enabled, |ui| self.page(ui));
                });
        });
        self.dialogs(&ctx);
    }
}

impl Launcher {
    /// The page for the current tab.
    fn page(&mut self, ui: &mut egui::Ui) {
        match self.ui.tab {
            Tab::Instances => self.instances_tab(ui),
            Tab::Settings => self.settings_tab(ui),
            Tab::BaseGame | Tab::AddOns | Tab::Balance => self.edit_tab(ui),
            Tab::Build => self.build_pick_tab(ui),
            Tab::Windows => self.windows_tab(ui),
            Tab::ThreeDs => self.three_ds_tab(ui),
            Tab::WiiU => self.wiiu_tab(ui),
        }
    }

    /// The title and the tabs of the current screen: Instances and
    /// Settings at home, the editor's pages while editing, none while
    /// building (Cancel at the bottom goes back).
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("CARNIVORES 2")
                    .size(22.0)
                    .color(GOLD)
                    .strong(),
            );
            ui.add_space(16.0);
            let tabs: Vec<(Tab, &str)> = if let Some(e) = &self.ui.recipe_edit {
                ui.label(
                    RichText::new(format!(
                        "Editing {}",
                        crate::instances::display_name(&e.recipe)
                    ))
                    .size(16.0)
                    .color(SAND),
                );
                ui.add_space(12.0);
                let mut t = vec![(Tab::BaseGame, "Base game"), (Tab::AddOns, "Add-ons")];
                // The Wii U reads the game's files as they are: no balance yet.
                if self.ui.mode != Mode::WiiU {
                    t.push((Tab::Balance, "Balance"));
                }
                t
            } else if self.ui.tab.building() {
                let name = self.active_recipe().map(|r| r.id()).unwrap_or_default();
                let platform = match self.ui.tab {
                    Tab::Windows => "Windows",
                    Tab::ThreeDs => "3DS",
                    Tab::WiiU => "Wii U",
                    _ => "",
                };
                ui.label(
                    RichText::new(format!("Build {platform}"))
                        .size(16.0)
                        .color(SAND),
                );
                if !name.is_empty() && self.ui.tab != Tab::Build {
                    ui.label(RichText::new(name).color(DIM));
                }
                Vec::new()
            } else {
                vec![(Tab::Instances, "Instances"), (Tab::Settings, "Settings")]
            };
            for (t, name) in tabs {
                let col = if self.ui.tab == t { GOLD } else { TEXT };
                if ui
                    .selectable_label(self.ui.tab == t, RichText::new(name).size(16.0).color(col))
                    .clicked()
                {
                    self.ui.tab = t;
                }
            }
        });
    }
}

/// The mobile values file the config names, if it reads.
fn cfg_mobile(cfg: &Config) -> Option<c2_mods::balance::Balance> {
    cfg.mobile_values
        .as_deref()
        .and_then(|p| c2_mods::balance::Balance::load(p).ok())
}

#[cfg(test)]
mod operation_tests;
