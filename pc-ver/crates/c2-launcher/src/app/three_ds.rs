//! 3DS build options.
use super::*;

impl Launcher {
    pub(super) fn three_ds_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Build for 3DS");
        self.instance_build_heading(ui);
        help(
            ui,
            "Makes an install for the SD card: the app and its packs in 3ds/<name>/. Several \
             installs with different names can sit side by side.",
        );
        ui.add_space(6.0);
        let mut changed = false;
        let recipe_id = self.active_recipe().map(|r| install_name(&r.id()));
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
            match &recipe_id {
                Some(id) => {
                    changed |= ui
                        .checkbox(
                            &mut t.addons_pack,
                            format!("The recipe's pack ({id}.c2pk; recipe on, Add-ons tab)"),
                        )
                        .changed();
                }
                None => {
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
                }
            }
            help(
                ui,
                "The app asks which pack at start; each keeps its own save.",
            );
        });
        changed |= panel(ui, |ui| {
            self.menu_style_ui(ui, true) | self.survival_ui(ui) | self.pictures_ui(ui)
        });
        ui.add_space(8.0);
        changed |= self.cia_panel(ui);
        if changed {
            self.save();
        }
        ui.add_space(12.0);
        let t = &self.cfg.three_ds;
        let game = match self.active_recipe() {
            Some(r) => self.recipe_game(r).first().cloned(),
            None => self.cfg.base.clone(),
        };
        let ready = game.as_deref().is_some_and(|b| describe_game(b).is_some())
            && t.out.is_some()
            && (t.vanilla_pack
                || t.addons_pack
                || (self.active_recipe().is_none() && t.standalone_packs));
        if big_button(ui, "BUILD", ready && !self.busy()) {
            self.build_3ds();
        }
    }

    /// The `.cia` options: the tools (with where to get them) and the
    /// user's home-menu art.
    pub(super) fn cia_panel(&mut self, ui: &mut egui::Ui) -> bool {
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
}
