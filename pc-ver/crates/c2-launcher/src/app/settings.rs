//! Launcher settings and first-run preferences.
use super::*;

impl Launcher {
    pub(super) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Settings");
        panel(ui, |ui| {
            ui.label(RichText::new("Engine options").color(GOLD));
            help(
                ui,
                "One file with the balance (prices, scores, and PC or mobile for each entry where \
                 they differ), the menu style, the PC game's settings and the 3DS target: keep it, share it, or bring it to another machine. No folders, no game \
                 data.",
            );
            ui.horizontal(|ui| {
                if ui.button("Export options...").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_file_name("c2rs.c2rs-options.toml")
                        .add_filter("c2rs options", &["toml"])
                        .save_file()
                    {
                        let text =
                            crate::options::Options::current(&self.cfg, &self.ui.balance.edits)
                                .to_toml();
                        self.ui.note = Some(match std::fs::write(&p, text) {
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
                            .and_then(|o| o.apply(&mut self.cfg, &mut self.ui.balance.edits));
                        self.ui.note = Some(match r {
                            Ok(done) => {
                                self.save();
                                let _ = self.ui.balance.store();
                                (format!("Imported: {}.", done.join(", ")), TEXT)
                            }
                            Err(e) => (format!("Not imported: {e}"), RED),
                        });
                    }
                }
            });
        });
        ui.add_space(8.0);
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
