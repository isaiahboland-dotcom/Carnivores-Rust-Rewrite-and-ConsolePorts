//! The home screen's actions (owner, 2026-11-05): Play, Build (a platform
//! picked first), and an instance's Options: Edit, Clone, Fix missing and
//! Delete. Names and confirmations are asked in the window itself (no file
//! dialogs: they are awkward on a Steam Deck in game mode).
use super::*;
use crate::instances::{Ask, Destination};

/// A question on top of the home screen.
pub(super) enum Dialog {
    /// A name for a new instance.
    New(String),
    /// A name for a copy of the instance.
    Clone(PathBuf, String),
    Delete(PathBuf),
}

/// Where Fix missing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fixing {
    /// The folders it was last given being checked.
    Checking,
    /// The search folders being looked through for the rest.
    Searching,
}

impl Launcher {
    pub(super) fn instances_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Instances");
        if self.ui.recipe_page.is_some() {
            self.recipe_ui(ui);
            return;
        }
        let busy = self.busy();
        match self.ui.instances.ui(ui, &self.cfg, self.ui.mode, busy) {
            Ask::Nothing => {}
            Ask::Play(p) => self.start(p, Destination::Play),
            Ask::PlayVr(p) => self.start(p, Destination::PlayVr),
            Ask::Build(p) => {
                // One platform: straight to it.
                match Destination::builds(self.ui.mode) {
                    [(only, _)] => self.start(p, *only),
                    _ => {
                        self.ui.build_for = Some(p);
                        self.ui.tab = Tab::Build;
                    }
                }
            }
            Ask::Edit(p) => self.edit_recipe(&p),
            Ask::Clone(p) => {
                let name = Recipe::load(&p)
                    .map(|r| format!("{} copy", crate::instances::display_name(&r)))
                    .unwrap_or_default();
                self.ui.dialog = Some(Dialog::Clone(p, name));
            }
            Ask::Delete(p) => self.ui.dialog = Some(Dialog::Delete(p)),
            Ask::FixMissing(p) => self.fix_missing(p),
            Ask::New => self.ui.dialog = Some(Dialog::New(String::new())),
            Ask::Import => self.import_recipe(),
            Ask::Export(p) => self.export_recipe(&p),
            Ask::FromOldSetup => self.save_instance(),
        }
    }

    /// Checks the instance's downloads and base game, then goes on to
    /// `to` once everything is found and matches.
    pub(super) fn start(&mut self, path: PathBuf, to: Destination) {
        self.open_recipe(path);
        if let Some(page) = &mut self.ui.recipe_page {
            page.next = Some(to);
        }
    }

    /// "What platform are you building this for?"
    pub(super) fn build_pick_tab(&mut self, ui: &mut egui::Ui) {
        if self.ui.recipe_page.is_some() {
            heading(ui, "Build");
            self.recipe_ui(ui);
            return;
        }
        let Some(path) = self.ui.build_for.clone() else {
            self.go_home();
            return;
        };
        let name = Recipe::load(&path)
            .map(|r| crate::instances::display_name(&r))
            .unwrap_or_default();
        heading(ui, &format!("Build {name}"));
        ui.label(RichText::new("What platform are you building this for?").size(18.0));
        ui.add_space(10.0);
        let mut pick = None;
        // Android takes only a game built from a mobile APK.
        let android_problem = self
            .cfg
            .remembered(&path)
            .and_then(|r| self.android_base_problem(r));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
            for &(d, label) in Destination::builds(self.ui.mode) {
                let warn = d == Destination::Android && android_problem.is_some();
                let text = if warn {
                    format!("{label}\n(needs an APK base)")
                } else {
                    label.to_string()
                };
                let b = egui::Button::new(RichText::new(text).size(22.0).color(GOLD).strong())
                    .fill(GREEN)
                    .min_size(egui::vec2(200.0, 120.0));
                let mut resp = ui.add_enabled(!self.busy(), b);
                if let Some(p) = android_problem.as_ref().filter(|_| warn) {
                    resp = resp.on_hover_text(p);
                }
                if resp.clicked() {
                    pick = Some(d);
                }
            }
        });
        if let Some(p) = &android_problem {
            if Destination::builds(self.ui.mode)
                .iter()
                .any(|(d, _)| *d == Destination::Android)
            {
                ui.add_space(6.0);
                help(ui, &format!("Android: {p}"));
            }
        }
        if let Some(d) = pick {
            self.start(path, d);
        }
    }

    /// Back to the instances.
    pub(super) fn go_home(&mut self) {
        self.ui.recipe_page = None;
        self.ui.fixing = None;
        self.ui.build_for = None;
        self.ui.tab = Tab::Instances;
    }

    /// Fix missing: the folders the instance was given checked, then the
    /// search folders looked through for its base game and the downloads
    /// still missing; what is found is kept for the instance.
    pub(super) fn fix_missing(&mut self, path: PathBuf) {
        self.open_recipe(path);
        if self.ui.recipe_page.is_some() {
            self.ui.fixing = Some(Fixing::Checking);
        }
    }

    /// Moves Fix missing on once the page's checks finish.
    pub(super) fn fix_step(&mut self) {
        let Some(stage) = self.ui.fixing else {
            return;
        };
        let busy = self.busy();
        let Some(p) = &mut self.ui.recipe_page else {
            self.ui.fixing = None;
            return;
        };
        if p.working() || busy {
            return;
        }
        if stage == Fixing::Checking && !p.missing().is_empty() {
            p.scan(self.cfg.search_dirs());
            p.dispatch(&mut self.job);
            self.ui.fixing = Some(Fixing::Searching);
            return;
        }
        let missing: Vec<String> = p.missing().iter().map(|r| r.entry.id.clone()).collect();
        let base_ok = !p.base_differs();
        let mut u = p.to_use();
        u.on = false;
        if let Some(cur) = self.cfg.remembered(&u.path) {
            u.overlays = cur.overlays.clone();
        }
        if let Some(cur) = self.cfg.recipe.as_mut().filter(|r| r.path == u.path) {
            cur.game = u.game.clone();
            cur.folders = u.folders.clone();
        }
        self.cfg.remember(&u);
        self.save();
        self.ui.instances.refresh(&self.cfg);
        self.ui.fixing = None;
        self.ui.recipe_page = None;
        self.ui.note = Some(match (missing.is_empty(), base_ok) {
            (true, true) => ("Fix missing: everything was found.".into(), TEXT),
            (true, false) => (
                "Fix missing: the downloads were found, but the base game isn't the one the \
                 instance expects: Edit it."
                    .into(),
                RED,
            ),
            (false, _) => (
                format!(
                    "Fix missing: still missing {}. Put them in a search folder (Settings) or \
                     give their folders with Edit.",
                    missing.join(", ")
                ),
                RED,
            ),
        });
    }

    /// The name and confirmation dialogs.
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ui.dialog.take() else {
            return;
        };
        let mut done = false;
        egui::Modal::new(egui::Id::unique("instance-dialog")).show(ctx, |ui| {
            ui.set_width(420.0);
            match &mut d {
                Dialog::New(name) | Dialog::Clone(_, name) => {
                    ui.label(RichText::new("Instance name").color(SAND).strong());
                    let r = ui.text_edit_singleline(name);
                    r.request_focus();
                    let id = crate::instances::id_from(name);
                    let to = match &d {
                        Dialog::Clone(from, _) => from.with_file_name(format!("{id}.toml")),
                        _ => crate::config::recipes_dir().join(format!("{id}.toml")),
                    };
                    let taken = to.exists();
                    if taken {
                        ui.label(
                            RichText::new(format!("{} exists already.", to.display())).color(RED),
                        );
                    } else {
                        help(ui, &format!("Saved as {}", to.display()));
                    }
                    ui.horizontal(|ui| {
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let ok = ui
                            .add_enabled(
                                !taken,
                                egui::Button::new(RichText::new("OK").color(GOLD)).fill(GREEN),
                            )
                            .clicked();
                        if (ok || enter) && !taken {
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            d = Dialog::Delete(PathBuf::new());
                            done = true;
                        }
                    });
                }
                Dialog::Delete(p) => {
                    let name = Recipe::load(p.as_path())
                        .map(|r| crate::instances::display_name(&r))
                        .unwrap_or_else(|_| p.display().to_string());
                    ui.label(
                        RichText::new(format!("Delete '{name}'?"))
                            .color(SAND)
                            .strong(),
                    );
                    help(
                        ui,
                        &format!(
                            "Its recipe file ({}) is deleted. Your game and mod folders are not \
                             touched.",
                            p.display()
                        ),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Delete").color(RED)).clicked() {
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            *p = PathBuf::new();
                            done = true;
                        }
                    });
                }
            }
        });
        if !done {
            self.ui.dialog = Some(d);
            return;
        }
        match d {
            Dialog::New(name) => self.new_instance(&name),
            Dialog::Clone(from, name) => self.clone_recipe(&from, &name),
            Dialog::Delete(p) if !p.as_os_str().is_empty() => self.delete_instance(&p),
            Dialog::Delete(_) => {}
        }
    }

    /// A new, empty instance, opened to edit.
    fn new_instance(&mut self, name: &str) {
        let id = crate::instances::id_from(name);
        let to = crate::config::recipes_dir().join(format!("{id}.toml"));
        let mut r = Recipe::default();
        r.recipe.id = id;
        r.recipe.name = name.trim().to_string();
        r.recipe.version = "1".into();
        let made = std::fs::create_dir_all(crate::config::recipes_dir())
            .and_then(|()| std::fs::write(&to, r.to_toml()));
        match made {
            Ok(()) => {
                self.ui.instances.refresh(&self.cfg);
                self.edit_recipe(&to);
            }
            Err(e) => self.ui.note = Some((format!("Not created: {e}"), RED)),
        }
    }

    /// Import recipe: a shared recipe copied into the recipes folder (under
    /// a free name), with its patches and base game hash list.
    fn import_recipe(&mut self) {
        let Some(from) = rfd::FileDialog::new()
            .add_filter("Recipe", &["toml"])
            .pick_file()
        else {
            return;
        };
        let dir = crate::config::recipes_dir();
        let stem = from
            .file_stem()
            .map_or("recipe".into(), |s| s.to_string_lossy().into_owned());
        let to = (1..)
            .map(|n| match n {
                1 => dir.join(format!("{stem}.toml")),
                n => dir.join(format!("{stem}-{n}.toml")),
            })
            .find(|p| !p.exists())
            .unwrap_or_else(|| dir.join("imported.toml"));
        self.ui.note = Some(match crate::instances::copy_recipe(&from, &to) {
            Ok(()) => {
                self.ui.instances.refresh(&self.cfg);
                (
                    format!(
                        "Imported as {}: Fix missing (Options) finds its downloads.",
                        to.display()
                    ),
                    TEXT,
                )
            }
            Err(e) => (format!("Not imported: {e}"), RED),
        });
    }

    /// Export: the recipe written where the player picks, to share.
    fn export_recipe(&mut self, from: &Path) {
        let name = from.file_name().map(|n| n.to_string_lossy().into_owned());
        let mut d = rfd::FileDialog::new().add_filter("Recipe", &["toml"]);
        if let Some(n) = name {
            d = d.set_file_name(n);
        }
        let Some(to) = d.save_file() else {
            return;
        };
        if to == from {
            return;
        }
        self.ui.note = Some(match crate::instances::copy_recipe(from, &to) {
            Ok(()) => (format!("Exported to {}", to.display()), TEXT),
            Err(e) => (format!("Not exported: {e}"), RED),
        });
    }

    /// The recipe file deleted, and what the launcher kept for it.
    fn delete_instance(&mut self, path: &Path) {
        if let Err(e) = std::fs::remove_file(path) {
            self.ui.note = Some((format!("Not deleted: {e}"), RED));
            return;
        }
        self.cfg.remembered.retain(|u| u.path != path);
        self.cfg.instance_builds.remove(path);
        self.cfg.playtime.remove(path);
        if self.cfg.recipe.as_ref().is_some_and(|u| u.path == path) {
            self.cfg.recipe = None;
        }
        self.save();
        self.ui.instances.refresh(&self.cfg);
        self.ui.note = Some((format!("Deleted {}", path.display()), TEXT));
    }
}

use c2_mods::recipe::Recipe;
