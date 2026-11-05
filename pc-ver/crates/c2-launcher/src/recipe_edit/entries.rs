//! The editor's entry lists: pinned overlays (on the Base game page),
//! standalone games and add-ons (the Add-ons page). Each entry shows its
//! credit and download page (editable), the folder the instance takes it
//! from (Browse to change it; Play checks it against the hashes), and for
//! add-ons the load order.

use super::*;
use crate::app::{help, panel};

impl Editor {
    /// The Add-ons page.
    pub fn addons_ui(&mut self, ui: &mut egui::Ui) {
        help(
            ui,
            "Add-ons are new animals, maps and weapons loaded over the base game, in load order \
             (later ones win where two change the same thing). Standalone games can be added \
             too: their animals, maps and weapons are combined beside the base game, and \
             \"Pick parts\" keeps only the ones you want. Drop folders on the window to add them \
             as add-ons. Folders shown in red aren't there any more: Browse to the new one, or \
             use Fix missing on the Instances page.",
        );
        ui.add_space(6.0);
        if self.draft.game.is_none() {
            ui.label(
                RichText::new(
                    "Choose the base game first (Base game page): add-ons are read against it.",
                )
                .color(RED),
            );
            ui.add_space(4.0);
        }
        self.list_ui(ui, Kind::Standalone);
        ui.add_space(6.0);
        self.list_ui(ui, Kind::AddOn);
    }

    /// One list of entries, with its "Add" button.
    pub(super) fn list_ui(&mut self, ui: &mut egui::Ui, kind: Kind) {
        let mut remove: Option<String> = None;
        let mut shift: Option<(String, isize)> = None;
        let mut browse: Option<String> = None;
        let mut add = false;
        let order = add_on_order(&self.recipe);
        let can_add = kind != Kind::AddOn || self.draft.game.is_some();
        let r = &mut self.recipe;
        let folders = &self.draft.folders;
        let extras = &mut self.extras;
        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(kind.label()).color(SAND).strong());
                if ui
                    .add_enabled(can_add, egui::Button::new("Add folders..."))
                    .on_hover_text("Each folder picked is pinned (its files hashed) and added.")
                    .clicked()
                {
                    add = true;
                }
            });
            if kind == Kind::Standalone {
                if let Some(c) = &mut r.combine {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Ambient animals:");
                        ui.radio_value(
                            &mut c.ambients,
                            "tied".to_string(),
                            "on their own game's maps",
                        );
                        ui.radio_value(&mut c.ambients, "random".to_string(), "on every map");
                    });
                }
            }
            let mut list: Vec<&mut RecipeMod> = match kind {
                Kind::Overlay => r.overlays.iter_mut().collect(),
                Kind::Standalone => r.standalones.iter_mut().collect(),
                Kind::AddOn => {
                    let mut v: Vec<&mut RecipeMod> = r.mods.iter_mut().collect();
                    v.sort_by_key(|m| order.iter().position(|o| *o == m.id).unwrap_or(usize::MAX));
                    v
                }
            };
            if list.is_empty() {
                ui.label(RichText::new("none").color(DIM));
                return;
            }
            let n = list.len();
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .id_salt(("recipe-edit-rows", kind as u8))
                .show(ui, |ui| {
                    egui::Grid::new(("recipe-edit", kind as u8))
                        .striped(true)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            for h in ["id", "by", "download page", "folder", ""] {
                                ui.label(RichText::new(h).color(DIM));
                            }
                            ui.end_row();
                            for (i, m) in list.iter_mut().enumerate() {
                                ui.label(&m.id)
                                    .on_hover_text(format!("{} files", m.files.len()));
                                ui.add(
                                    egui::TextEdit::singleline(&mut m.credit).desired_width(120.0),
                                );
                                ui.add(egui::TextEdit::singleline(&mut m.url).desired_width(200.0));
                                folder_cell(ui, folders.get(&m.id));
                                ui.horizontal(|ui| {
                                    if ui.small_button("Browse...").clicked() {
                                        browse = Some(m.id.clone());
                                    }
                                    if kind == Kind::AddOn {
                                        if ui
                                            .add_enabled(i > 0, egui::Button::new("⏶").small())
                                            .on_hover_text("Load earlier")
                                            .clicked()
                                        {
                                            shift = Some((m.id.clone(), -1));
                                        }
                                        if ui
                                            .add_enabled(i + 1 < n, egui::Button::new("⏷").small())
                                            .on_hover_text("Load later")
                                            .clicked()
                                        {
                                            shift = Some((m.id.clone(), 1));
                                        }
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        remove = Some(m.id.clone());
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    if kind == Kind::Standalone {
                        extras.parts_ui(ui, &mut list, folders);
                    }
                });
        });
        if let Some(id) = remove {
            self.remove(&id);
        }
        if let Some((id, by)) = shift {
            move_add_on(&mut self.recipe, &id, by);
        }
        if let Some(id) = browse {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.draft.folders.insert(id, dir);
            }
        }
        if add {
            if let Some(dirs) = rfd::FileDialog::new().pick_folders() {
                self.queue_pins(kind, dirs);
            }
        }
    }
}

/// An entry's folder: its name (the path on hover), red when unset or gone.
fn folder_cell(ui: &mut egui::Ui, folder: Option<&PathBuf>) {
    match folder {
        None => {
            ui.label(RichText::new("no folder").color(RED));
        }
        Some(f) => {
            let name = f.file_name().map_or_else(
                || f.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let col = if f.is_dir() { SAND } else { RED };
            ui.label(RichText::new(name).color(col))
                .on_hover_text(f.display().to_string());
        }
    }
}
