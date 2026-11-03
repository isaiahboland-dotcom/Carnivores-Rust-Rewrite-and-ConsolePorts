//! Game and add-on tabs.
use super::*;

impl Launcher {
    // ------------------------------------------------------------ tabs

    pub(super) fn game_tab(&mut self, ui: &mut egui::Ui) {
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
                    egui::TextEdit::singleline(&mut self.ui.base_text)
                        .desired_width(ui.available_width() - 90.0)
                        .hint_text("/path/to/Carnivores 2"),
                );
                if r.lost_focus() && !self.ui.base_text.trim().is_empty() {
                    let p = PathBuf::from(self.ui.base_text.trim());
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
                    self.base_from_apk(p, false);
                }
            }
        });
    }

    pub(super) fn addons_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Add-ons");
        help(
            ui,
            "Drop add-on folders or archives (.zip, .rar, .7z) on this window. They load top to \
             bottom: where two change the same thing, the lower one wins.",
        );
        if self.ui.mode == Mode::WiiU {
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

    /// Standalone games: whole games made from Carnivores 2, combined beside
    /// the base on PC; a 3DS pack each.
    pub(super) fn standalones_ui(&mut self, ui: &mut egui::Ui) {
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
                    self.ui.note = Some((
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
}
