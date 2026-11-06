//! Editing an instance: the Base game, Add-ons and Balance pages
//! (`crate::recipe_edit`), and the experimental Advanced page when Settings
//! shows it, with Save, Save and exit, Discard
//! changes and Cancel at the bottom. While it is edited the instance is the
//! one in use, so the menu and rule choices shown are its own.
use super::*;
use crate::recipe_edit::{BaseAsk, Draft, Editor};

impl Launcher {
    /// Opens the instance at `path` to edit, its folders as last kept.
    pub(super) fn edit_recipe(&mut self, path: &Path) {
        if !self.can_start_operation() {
            return;
        }
        let draft = self
            .cfg
            .remembered(path)
            .cloned()
            .map(|u| {
                // An instance from before kept no game of its own: it took
                // whatever the Game tab had, so it is chosen again here.
                let dirs = if u.game.is_some() {
                    self.recipe_game_with_overlays(&u)
                } else {
                    Vec::new()
                };
                Draft {
                    game: dirs.first().cloned(),
                    overlays: dirs.into_iter().skip(1).collect(),
                    folders: u.folders,
                }
            })
            .unwrap_or_default();
        let mut e = match Editor::open(path, draft) {
            Ok(e) => e,
            Err(err) => {
                self.ui.note = Some((format!("Not a recipe: {err}"), RED));
                return;
            }
        };
        self.take_settings(&e.recipe.settings, path, &e.recipe.recipe.id);
        self.cfg.recipe = Some(e.to_use(true));
        e.settle_settings(self.cfg.settings(&e.recipe.settings));
        self.ui.recipe_edit = Some(e);
        self.ui.recipe_page = None;
        self.ui.tab = Tab::BaseGame;
    }

    /// A recipe's choices taken in by the launcher, its build outputs and
    /// balance restored (the instance becomes the one in use).
    pub(super) fn take_settings(&mut self, s: &c2_mods::recipe::Settings, path: &Path, id: &str) {
        self.cfg.remember_builds();
        self.cfg.apply(s);
        self.cfg.pictures = s.pictures;
        self.cfg.tuning = s.tuning;
        self.cfg.restore_builds(path, id);
        self.set_balance(s.balance.clone().unwrap_or_default());
    }

    /// The balance played with, written only when it changes.
    pub(super) fn set_balance(&mut self, edits: c2_mods::balance::Balance) {
        if self.ui.balance.edits == edits {
            return;
        }
        self.ui.balance.edits = edits;
        if let Err(e) = self.ui.balance.store() {
            self.ui.note = Some((format!("Balance not saved: {e}"), RED));
        }
    }

    /// Keeps the launcher and the editor in step each frame: the instance
    /// in use is the one edited (its draft folders), and the launcher's
    /// menu and rule choices are the recipe's settings.
    fn sync_edit(&mut self) {
        let Some(e) = &mut self.ui.recipe_edit else {
            return;
        };
        e.pump(&mut self.job);
        let u = e.to_use(true);
        if self.cfg.recipe.as_ref() != Some(&u) {
            self.cfg.recipe = Some(u);
        }
        let s = self.cfg.settings(&e.recipe.settings);
        if s != e.recipe.settings {
            e.recipe.settings = s;
        }
    }

    pub(super) fn edit_tab(&mut self, ui: &mut egui::Ui) {
        self.sync_edit();
        if self.ui.tab == Tab::Advanced && !self.cfg.show_advanced {
            self.ui.tab = Tab::BaseGame;
        }
        let busy = self.busy();
        let Some(e) = &mut self.ui.recipe_edit else {
            self.go_home();
            return;
        };
        if e.working() || e.reading() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        match self.ui.tab {
            Tab::BaseGame => {
                heading(ui, "Base game");
                let ask = ui
                    .add_enabled_ui(!e.working(), |ui| e.base_ui(ui, busy))
                    .inner;
                ui.add_space(6.0);
                if panel(ui, |ui| {
                    ui.label(RichText::new("Menu").color(GOLD));
                    self.menu_style_ui(ui, false)
                }) {
                    self.save();
                }
                if let BaseAsk::ChooseApk = ask {
                    if let Some(apk) = rfd::FileDialog::new()
                        .add_filter("Android app", &["apk"])
                        .pick_file()
                    {
                        self.base_from_apk(apk, true);
                    }
                }
            }
            Tab::Advanced => {
                heading(ui, "Advanced");
                panel(ui, |ui| {
                    ui.label(RichText::new("Experimental").color(GOLD));
                    help(
                        ui,
                        "Experimental instance options will be added here. They may change or \
                         go away between versions. Hide this tab in Settings.",
                    );
                });
            }
            Tab::AddOns => {
                heading(ui, "Add-ons");
                ui.add_enabled_ui(!e.working(), |ui| e.addons_ui(ui));
                ui.add_space(6.0);
                self.importers_ui(ui);
            }
            _ => {
                heading(ui, "Balance");
                if panel(ui, |ui| self.tuning_ui(ui)) {
                    self.save();
                }
                ui.add_space(6.0);
                if let Some(e) = &mut self.ui.recipe_edit {
                    panel(ui, |ui| e.balance_ui(ui, &mut self.job));
                }
            }
        }
    }

    /// The phone game's and Ice Age's animals, imported as add-ons.
    fn importers_ui(&mut self, ui: &mut egui::Ui) {
        let busy = self.busy();
        panel(ui, |ui| {
            ui.label(RichText::new("Import").color(GOLD));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new("Animals from the mobile game (APK)..."),
                    )
                    .on_hover_text(
                        "Your own copy of Carnivores: Dinosaur Hunter or Ice Age for Android: \
                         its animals and weapons this game doesn't have become add-ons.",
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
                    .add_enabled(!busy, egui::Button::new("Carnivores: Ice Age..."))
                    .on_hover_text(
                        "Your own Carnivores: Ice Age folder: its animals and areas become \
                         add-ons.",
                    )
                    .clicked()
                {
                    let mut d = rfd::FileDialog::new();
                    if let Some(s) = &self.cfg.ice_age {
                        d = d.set_directory(s);
                    }
                    if let Some(p) = d.pick_folder() {
                        self.import_ice_age(p);
                    }
                }
            });
        });
    }

    /// Save, Save and exit, Discard changes, Cancel.
    pub(super) fn edit_bar(&mut self, ui: &mut egui::Ui) {
        let Some(e) = &mut self.ui.recipe_edit else {
            return;
        };
        let (changed, working) = (e.changed(), e.working());
        let mut save = false;
        let mut exit = false;
        let mut discard = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    changed && !working,
                    egui::Button::new(RichText::new("Save").color(GOLD).strong()).fill(GREEN),
                )
                .clicked()
            {
                save = true;
            }
            if ui
                .add_enabled(
                    !working,
                    egui::Button::new(RichText::new("Save and exit").color(GOLD).strong())
                        .fill(GREEN),
                )
                .clicked()
            {
                save = changed;
                exit = true;
            }
            if ui
                .add_enabled(changed, egui::Button::new("Discard changes"))
                .on_hover_text("Back to the instance as last saved.")
                .clicked()
            {
                discard = true;
            }
            if ui
                .button("Cancel")
                .on_hover_text("Leave without saving: changes since the last save are lost.")
                .clicked()
            {
                discard = changed;
                exit = true;
            }
            e.note_ui(ui);
        });
        if save && !self.save_edit() {
            return;
        }
        if discard {
            self.discard_edit();
        }
        if exit {
            self.ui.recipe_edit = None;
            self.save();
            self.ui.instances.refresh(&self.cfg);
            self.go_home();
        }
    }

    /// Writes the instance; whether it was saved.
    fn save_edit(&mut self) -> bool {
        self.sync_edit();
        let Some(e) = &mut self.ui.recipe_edit else {
            return false;
        };
        if let Err(err) = e.save() {
            self.ui.note = Some((format!("Not saved: {err}"), RED));
            return false;
        }
        let u = e.to_use(true);
        let balance = e.recipe.settings.balance.clone().unwrap_or_default();
        self.set_balance(balance);
        self.cfg.remember(&u);
        self.cfg.recipe = Some(u);
        self.save();
        self.ui.instances.refresh(&self.cfg);
        true
    }

    /// Back to the instance as last saved (the launcher's choices too).
    pub(super) fn discard_edit(&mut self) {
        let Some(e) = &mut self.ui.recipe_edit else {
            return;
        };
        e.discard();
        let s = e.saved_settings().clone();
        self.cfg.apply(&s);
        self.cfg.pictures = s.pictures;
        self.cfg.tuning = s.tuning;
        self.sync_edit();
    }
}
