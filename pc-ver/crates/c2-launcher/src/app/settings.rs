//! Launcher settings and first-run preferences.
use super::*;

impl Launcher {
    pub(super) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Settings");
        if panel(ui, |ui| {
            ui.label(RichText::new("Look").color(theme::gold()));
            help(
                ui,
                "The launcher's colours, fonts and frames. Legacy is the launcher's earlier look. \
                 The game's own menus have their own settings.",
            );
            let mut changed = false;
            for (look, name) in [
                (theme::Look::Dhc, "DHC OS (default)"),
                (theme::Look::Legacy, "Legacy"),
            ] {
                changed |= ui.radio_value(&mut self.cfg.look, look, name).changed();
            }
            changed
        }) {
            theme::set_look(self.cfg.look);
            style(ui.ctx());
            self.save();
        }
        ui.add_space(8.0);
        if panel(ui, |ui| {
            ui.label(RichText::new("Window").color(theme::gold()));
            self.ui_scale_ui(ui)
        }) {
            self.save();
        }
        ui.add_space(8.0);
        panel(ui, |ui| {
            ui.label(RichText::new("Tools folder").color(theme::gold()));
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
                ui.label(RichText::new(shown).color(theme::path()));
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
                    Some(p) => (format!("{tool}: {}", p.display()), theme::path()),
                    None => (format!("{tool}: not found"), theme::red()),
                };
                ui.label(RichText::new(text).color(col));
            }
        });
        ui.add_space(8.0);
        self.search_dirs_ui(ui);
        ui.add_space(8.0);
        if panel(ui, |ui| {
            ui.label(RichText::new("Advanced").color(theme::gold()));
            let changed = ui
                .checkbox(
                    &mut self.cfg.show_advanced,
                    "Show the Advanced tab in the instance editor",
                )
                .changed();
            help(
                ui,
                "For modpack makers: the base game's parts, times of day, settings fixed for \
                 players, spawn rules, and experimental options that may change between \
                 versions.",
            );
            changed
        }) {
            self.save();
        }
    }

    /// The survival switch: whether the games' menus (the original menus
    /// and the DHC OS terminal) offer survival. True when it changed.
    pub(super) fn survival_ui(&mut self, ui: &mut egui::Ui) -> bool {
        ui.label(RichText::new("Game modes").color(theme::gold()));
        let phone = self.has_phone_base();
        let changed = if phone {
            let mut on = true;
            ui.add_enabled(false, egui::Checkbox::new(&mut on, "Survival mode"));
            ui.label(
                RichText::new("Always on for a game built from your APK.").color(theme::sand()),
            );
            false
        } else {
            ui.checkbox(&mut self.cfg.survival, "Survival mode")
                .changed()
        };
        help(
            ui,
            "Off by default; each instance has its own. On, the game's menus offer survival (the main menu and the DHC OS \
             terminal) on PC, Windows packages, the 3DS and the Wii U. Rebuild console packs \
             after changing it. A standalone game without Mount Ravan or the shotgun holds its \
             survival on its last area with its third weapon. A game built from your APK always \
             offers it.",
        );
        changed
    }

    /// `--offer-survival` when survival is offered (the switch, or a game
    /// built from the APK): `c2-pc play`, Windows packages and `c2-cook`.
    pub(super) fn survival_args(&self) -> Vec<String> {
        if self.cfg.survival || self.has_phone_base() {
            vec!["--offer-survival".into()]
        } else {
            Vec::new()
        }
    }

    /// Where instances' downloads are looked for.
    pub(super) fn search_dirs_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        panel(ui, |ui| {
            ui.label(RichText::new("Mod and add-on folders").color(theme::gold()));
            help(
                ui,
                "Where Play, the builds and Fix missing look for an instance's base game and \
                 downloads (each folder and the folders and .zips in it, up to 4 folders down). \
                 Add your mods folder and your add-ons folder here, then drop new downloads in \
                 them. With none added: source, source/mods, addons and mods beside the \
                 recipes folder.",
            );
            let mut remove = None;
            for (i, d) in self.cfg.search_dirs().iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(d.display().to_string()).color(theme::path()));
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
}
