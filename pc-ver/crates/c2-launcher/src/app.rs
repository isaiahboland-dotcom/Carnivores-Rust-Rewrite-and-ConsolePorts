//! The launcher window: a tab per job (game folders, add-ons, balance,
//! play, 3DS, Wii U, settings), the job log at the bottom.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::config::{describe_game, install_name, AddOn, Config, MenuStyle, RecipeUse, Target};
use crate::jobs::{is_archive, Job, Step};
use crate::theme::*;
use crate::{find_tool, Mode};

mod builds;
mod game;
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
    base_text: String,
    note: Option<(String, Color32)>,
    balance: crate::balance::Editor,
    recipe_page: Option<crate::recipe_page::Page>,
    recipe_edit: Option<crate::recipe_edit::Editor>,
    instances: crate::instances::List,
    /// Each standalone's areas, animals and weapons, read once for "Pick parts".
    parts: std::collections::HashMap<PathBuf, Option<c2_mods::standalone::Parts>>,
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
            base_text: cfg.base.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            note: (!pending.is_empty()).then(|| (format!("Unfinished install transactions: {}. Inspect data (new output) and previous (backup); close other launchers before recovery.", pending.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")), RED)),
            balance,
            recipe_page: None,
            recipe_edit: None,
            instances: Default::default(),
            parts: Default::default(),
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
                Ok(crate::jobs::Completion::Balance(values)) => self.ui.balance.loaded(*values),
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
                Ok(crate::jobs::Completion::Saved(saved)) => {
                    let (r, u) = *saved;
                    self.saving_done(r, u);
                }
                Err(e) => {
                    self.ui.balance.failed(&e);
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

    fn shows_3ds(&self) -> bool {
        self.ui.mode == Mode::ThreeDs
            || (self.ui.mode == Mode::Full && self.cfg.show_3ds == Some(true))
    }

    fn shows_wiiu(&self) -> bool {
        self.ui.mode == Mode::WiiU
            || (self.ui.mode == Mode::Full && self.cfg.show_wiiu != Some(false))
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
        if ui.button("Choose a saved instance…").clicked() {
            self.ui.tab = Tab::Instances;
            self.ui.recipe_page = None;
        }
    }

    fn dropped(&mut self, files: Vec<PathBuf>) {
        if files.is_empty() || !self.can_start_operation() {
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
        if let Some(p) = &mut self.ui.recipe_page {
            if ext_is(&first, "apk") && p.is_apk_base() && !busy {
                self.base_from_apk(first, true);
                self.ui.tab = Tab::AddOns;
                return;
            }
            if self.ui.tab == Tab::AddOns && files.iter().all(|f| f.is_dir()) {
                p.scan(files);
                p.dispatch(&mut self.job);
                return;
            }
        }
        // An APK with no game set (or on the game tab): the game from it.
        let apk = first
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("apk"));
        if apk && (self.ui.tab == Tab::Game || self.cfg.base.is_none()) && !self.busy() {
            self.base_from_apk(first, false);
            return;
        }
        let is_game = describe_game(&first).is_some();
        if is_game && (self.ui.tab == Tab::Game || self.cfg.base.is_none()) {
            self.set_base(first);
            return;
        }
        self.ui.tab = Tab::AddOns;
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
            match destination {
                crate::instances::Destination::Play => {}
                crate::instances::Destination::Windows => self.ui.tab = Tab::Windows,
                crate::instances::Destination::ThreeDs => {
                    self.cfg.show_3ds = Some(true);
                    self.ui.tab = Tab::ThreeDs;
                    self.save();
                }
                crate::instances::Destination::WiiU => self.ui.tab = Tab::WiiU,
            }
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Panel::top("tabs")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
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
                    if self.ui.mode != Mode::WiiU {
                        tabs.push((Tab::Balance, "Balance"));
                    }
                    if self.ui.mode == Mode::Full {
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
                        let col = if self.ui.tab == t { GOLD } else { TEXT };
                        if ui
                            .selectable_label(
                                self.ui.tab == t,
                                RichText::new(name).size(16.0).color(col),
                            )
                            .clicked()
                        {
                            self.ui.tab = t;
                        }
                    }
                });
            });
        egui::Panel::bottom("log")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(ui, |ui| {
                if let Some((n, c)) = &self.ui.note {
                    ui.label(RichText::new(n).color(*c));
                }
                match self.job.current() {
                    Some(j) => log_panel(ui, j),
                    None => help(ui, "Drop the game folder or add-ons on this window."),
                }
            });
        let enabled = !self.busy();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_enabled_ui(enabled, |ui| match self.ui.tab {
                        Tab::Instances => self.instances_tab(ui),
                        Tab::Game => self.game_tab(ui),
                        Tab::AddOns => self.addons_tab(ui),
                        Tab::Balance => self.balance_tab(ui),
                        Tab::Play => self.play_tab(ui),
                        Tab::ThreeDs => self.three_ds_tab(ui),
                        Tab::WiiU => self.wiiu_tab(ui),
                        Tab::Windows => self.windows_tab(ui),
                        Tab::Settings => self.settings_tab(ui),
                    })
                });
        });
        if self.ui.mode == Mode::Full && self.cfg.show_3ds.is_none() && !self.busy() {
            self.first_launch(&ctx);
        }
        // A tab that went away (3DS hidden) falls back.
        if (self.ui.tab == Tab::ThreeDs && !self.shows_3ds())
            || (self.ui.tab == Tab::WiiU && !self.shows_wiiu())
        {
            self.ui.tab = Tab::Game;
        }
    }
}

/// The mobile values file the config names, if it reads.
fn cfg_mobile(cfg: &Config) -> Option<c2_mods::balance::Balance> {
    cfg.mobile_values
        .as_deref()
        .and_then(|p| c2_mods::balance::Balance::load(p).ok())
}

#[cfg(test)]
mod operation_tests {
    use super::*;

    fn launcher() -> Launcher {
        Launcher {
            cfg: Config::default(),
            ui: View {
                mode: Mode::Full,
                tab: Tab::Instances,
                base_text: String::new(),
                note: None,
                balance: Default::default(),
                recipe_page: None,
                recipe_edit: None,
                instances: Default::default(),
                parts: Default::default(),
            },
            job: Default::default(),
        }
    }

    #[test]
    fn all_entry_points_keep_the_current_job_and_pending_result() {
        let mut app = launcher();
        app.job
            .start_task("pending", |_| Ok(crate::jobs::Completion::Recipe(vec![])))
            .unwrap();
        app.import_apk("input.apk".into());
        app.import_ice_age("input".into());
        app.base_from_apk("input.apk".into(), true);
        app.add_addons(vec!["input.zip".into()]);
        app.play();
        app.play_vr();
        app.build_3ds();
        app.build_wiiu();
        app.build_windows();
        app.save_instance();
        app.open_recipe("missing.toml".into());
        app.edit_recipe(Path::new("missing.toml"));
        app.clone_recipe(Path::new("missing.toml"));
        app.dropped(vec!["missing.toml".into()]);
        assert_eq!(app.job.current().unwrap().title, "pending");
        assert!(app.busy());
        assert!(app.cfg.base.is_none());
        assert!(app.cfg.recipe.is_none());
        assert!(app.ui.recipe_page.is_none());
        assert!(app.ui.recipe_edit.is_none());
        assert!(app.ui.note.is_some());
    }

    #[test]
    fn all_tabs_render_without_game_data() {
        let mut app = launcher();
        let ctx = egui::Context::default();
        for tab in [
            Tab::Instances,
            Tab::Game,
            Tab::AddOns,
            Tab::Balance,
            Tab::Play,
            Tab::Windows,
            Tab::ThreeDs,
            Tab::WiiU,
            Tab::Settings,
        ] {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| match tab {
                    Tab::Instances => app.instances_tab(ui),
                    Tab::Game => app.game_tab(ui),
                    Tab::AddOns => app.addons_tab(ui),
                    Tab::Balance => app.balance_tab(ui),
                    Tab::Play => app.play_tab(ui),
                    Tab::Windows => app.windows_tab(ui),
                    Tab::ThreeDs => app.three_ds_tab(ui),
                    Tab::WiiU => app.wiiu_tab(ui),
                    Tab::Settings => app.settings_tab(ui),
                });
            });
            output.textures_delta.clear();
        }
    }
}
