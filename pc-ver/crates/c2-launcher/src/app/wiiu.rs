//! Wii U build options.
use super::*;

impl Launcher {
    pub(super) fn wiiu_tab(&mut self, ui: &mut egui::Ui) {
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
            .and_then(|r| self.recipe_game(r).first().cloned());
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
        let instance = self.active_recipe().is_some();
        let mut blockers: Vec<String> = game_blocker(game.as_deref(), instance)
            .into_iter()
            .collect();
        if self.cfg.wiiu.out.is_none() {
            blockers.push("choose the SD card (or a folder) above".into());
        }
        if blockers.is_empty() && self.wiiu_installs().is_empty() {
            blockers.push(if instance {
                "tick \"Saved instance\" under What goes on".into()
            } else {
                "tick at least one install under What goes on".into()
            });
        }
        if build_button(ui, &blockers, self.busy()) {
            self.build_wiiu();
        }
    }

    /// Each Wii U install's own art (owner, 2026-11-01): icon, and the TV's
    /// and GamePad's start screens (shown while it loads); any not picked
    /// is cut from the game's menu picture.
    pub(super) fn wiiu_art_ui(&mut self, ui: &mut egui::Ui) -> bool {
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
}
