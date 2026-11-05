//! The Add-ons tab's standalone games, each with a "Pick parts" dropdown
//! (owner request 2026-11-05): which of its areas, huntables and weapons
//! are combined on PC (`c2_mods::standalone::Skip`, `c2-pc --skip`).
use super::*;

impl Launcher {
    /// Standalone games: whole games made from Carnivores 2, combined beside
    /// the base on PC; a 3DS pack each.
    pub(super) fn standalones_ui(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Standalone games");
        help(
            ui,
            "Whole games made from Carnivores 2 (the folder you extracted). On PC each one's \
             areas, animals and weapons are added beside the base game's, its animals on its \
             own maps (\"Pick parts\" leaves some out); most need the Modder's Edition as a \
             base overlay. On the 3DS each one is a pack of its own, whole.",
        );
        ui.add_space(6.0);
        if ui.button("Add game folder...").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                if c2_mods::pack::find_game_root(&p).is_some() {
                    if !self.cfg.standalones.iter().any(|s| s.path == p) {
                        self.cfg.standalones.push(crate::config::AddOn {
                            path: p,
                            enabled: true,
                            skip: Default::default(),
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
        let parts = &mut self.ui.parts;
        panel(ui, |ui| {
            if self.cfg.standalones.is_empty() {
                help(ui, "No standalone games yet.");
            } else {
                changed |= super::game::all_on_off(ui, &mut self.cfg.standalones);
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
                let p = parts
                    .entry(a.path.clone())
                    .or_insert_with(|| c2_mods::standalone::parts(&a.path));
                if let Some(p) = p {
                    ui.indent(("parts", i), |ui| {
                        changed |= pick_parts(
                            ui,
                            egui::IdSalt::new(("pick-parts", i, &a.path)),
                            p,
                            &mut a.skip,
                        )
                    });
                }
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

/// The dropdown for one standalone (`salt`: its id): whether a choice
/// changed. Also the recipe editor's, for an instance's standalones.
pub(crate) fn pick_parts(
    ui: &mut egui::Ui,
    salt: egui::IdSalt,
    p: &c2_mods::standalone::Parts,
    s: &mut c2_mods::standalone::Skip,
) -> bool {
    let kept = |all: usize, out: &[u32]| all - out.iter().filter(|&&n| n as usize <= all).count();
    let title = format!(
        "Pick parts: {} of {} areas, {} of {} animals, {} of {} weapons",
        kept(p.areas, &s.areas),
        p.areas,
        kept(p.huntables.len(), &s.huntables),
        p.huntables.len(),
        kept(p.weapons.len(), &s.weapons),
        p.weapons.len()
    );
    let mut changed = false;
    egui::CollapsingHeader::new(RichText::new(title).color(if s.is_empty() { DIM } else { GOLD }))
        .id_salt(salt)
        .show(ui, |ui| {
            let areas: Vec<String> = (1..=p.areas).map(|n| format!("Area {n}")).collect();
            changed |= pick_list(ui, "Areas", &areas, &mut s.areas);
            changed |= pick_list(ui, "Animals", &p.huntables, &mut s.huntables);
            changed |= pick_list(ui, "Weapons", &p.weapons, &mut s.weapons);
            if !s.is_empty() && ui.small_button("Take everything").clicked() {
                *s = Default::default();
                changed = true;
            }
        });
    changed
}

/// A row of checkboxes, one per name; `left_out` holds the unticked ones'
/// numbers (from 1).
fn pick_list(ui: &mut egui::Ui, title: &str, names: &[String], left_out: &mut Vec<u32>) -> bool {
    if names.is_empty() {
        return false;
    }
    let mut changed = false;
    ui.label(RichText::new(title).color(SAND));
    ui.horizontal_wrapped(|ui| {
        for (k, name) in names.iter().enumerate() {
            let n = k as u32 + 1;
            let mut on = !left_out.contains(&n);
            let label = if name.is_empty() {
                format!("#{n}")
            } else {
                name.clone()
            };
            if ui.checkbox(&mut on, label).changed() {
                if on {
                    left_out.retain(|&x| x != n);
                } else {
                    left_out.push(n);
                    left_out.sort_unstable();
                }
                changed = true;
            }
        }
    });
    changed
}
