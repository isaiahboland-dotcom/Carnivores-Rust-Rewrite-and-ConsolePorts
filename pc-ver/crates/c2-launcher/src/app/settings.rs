//! Launcher settings and first-run preferences.
use super::*;

impl Launcher {
    pub(super) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Settings");
        if panel(ui, |ui| {
            ui.label(RichText::new("Window").color(GOLD));
            self.ui_scale_ui(ui)
        }) {
            self.save();
        }
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
        ui.add_space(8.0);
        if panel(ui, |ui| {
            ui.label(RichText::new("Advanced").color(GOLD));
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

    /// Where instances' downloads are looked for.
    pub(super) fn search_dirs_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        panel(ui, |ui| {
            ui.label(RichText::new("Mod and add-on folders").color(GOLD));
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
}
