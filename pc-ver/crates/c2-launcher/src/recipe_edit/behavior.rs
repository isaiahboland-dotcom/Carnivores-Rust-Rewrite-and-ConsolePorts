//! The recipe editor's Advanced page: "Awareness"
//! (`[settings.behavior.awareness]`, `c2_mods::behavior`), a part of
//! Advanced Dinosaur Behavior and AI. Off by default; globally or per
//! animal.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::behavior as fmt;
use c2_mods::behavior::{Animal, Awareness, Behavior};
use c2_mods::recipe::Recipe;
use eframe::egui::{self, RichText};

use super::flee::limit_ui;
use super::instance::Extras;
use crate::app::{help, panel};
use crate::theme;

const HEADS: [&str; 3] = ["Suspicion", "Looks after", "Search"];

/// The three values of one row; `base` is what an unset one inherits.
fn values_row(ui: &mut egui::Ui, v: [&mut Option<u32>; 3], base: [u32; 3]) {
    let [suspicion, look, search] = v;
    limit_ui(ui, suspicion, fmt::SUSPICION_RANGE, base[0], " %");
    limit_ui(ui, look, fmt::INVESTIGATE_RANGE, base[1], " s");
    limit_ui(ui, search, fmt::SEARCH_RANGE, base[2], " s");
}

fn animal_mut<'a>(a: &'a mut Awareness, name: &str) -> &'a mut Animal {
    let i = match a.animals.iter().position(|x| x.name == name) {
        Some(i) => i,
        None => {
            a.animals.push(Animal {
                name: name.into(),
                ..Animal::default()
            });
            a.animals.len() - 1
        }
    };
    &mut a.animals[i]
}

impl Extras {
    /// The Advanced page's awareness settings: whether one changed.
    pub(super) fn behavior_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) -> bool {
        let mut b = r.settings.behavior.clone().unwrap_or_default();
        let before = b.clone();
        let mut a = b.awareness.clone().unwrap_or_default();
        ui.label(
            RichText::new("Advanced Dinosaur Behavior and AI: Awareness (experimental)")
                .color(theme::sand())
                .strong(),
        );
        ui.label(
            RichText::new("PC and Android only for now; other platforms play the original.")
                .color(theme::gold()),
        );
        help(
            ui,
            "An animal that almost notices you grows suspicious and, after a while, walks over \
             to look (a herbivore stops well short). A predator that loses you goes to where it \
             last sensed you and searches round it, then gives up. Crouching hides you only \
             until you are noticed: it never ends a chase or a search. Off: an alarmed animal \
             always knows where you are, as in the original.",
        );
        ui.checkbox(&mut a.enabled, "Awareness");
        if a.enabled {
            ui.add_space(4.0);
            self.awareness_values(ui, r, &mut a, game, folders, handle);
        }
        b.awareness = (a != Awareness::default()).then_some(a);
        if b == before {
            return false;
        }
        r.settings.behavior = (b != Behavior::default()).then_some(b);
        true
    }

    fn awareness_values(
        &mut self,
        ui: &mut egui::Ui,
        r: &Recipe,
        a: &mut Awareness,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let defaults = fmt::Awareness::default().resolved();
        help(
            ui,
            "Suspicion: how far past its alarm threshold an animal still half-notices you \
             (100 %: never). Looks after: seconds of suspicion before it comes to look. \
             Search: seconds a predator searches, and the longest a look lasts.",
        );
        egui::Grid::new("aware-global")
            .num_columns(4)
            .show(ui, |ui| {
                ui.label("");
                for h in HEADS {
                    ui.label(RichText::new(h).color(theme::sand()));
                }
                ui.end_row();
                ui.label("All animals");
                values_row(
                    ui,
                    [&mut a.suspicion, &mut a.investigate_after, &mut a.search],
                    defaults,
                );
                ui.end_row();
            });
        if ui.button("Reset").clicked() {
            *a = Awareness {
                enabled: true,
                ..Awareness::default()
            };
        }
        let base = a.values().over(fmt::Awareness::default()).resolved();
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        if values.animals.is_empty() {
            return;
        }
        ui.add_space(4.0);
        egui::CollapsingHeader::new("Per animal")
            .id_salt("aware-animals")
            .show(ui, |ui| {
                help(
                    ui,
                    "Unset values inherit the one above. Ambient animals, fish, flyers and \
                     brachiosaurs take no part.",
                );
                panel(ui, |ui| {
                    egui::Grid::new("aware-animal-grid")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Animal").color(theme::sand()));
                            for h in HEADS {
                                ui.label(RichText::new(h).color(theme::sand()));
                            }
                            ui.end_row();
                            for x in &values.animals {
                                ui.label(&x.name);
                                let mut own = a
                                    .animals
                                    .iter()
                                    .find(|y| y.name == x.name)
                                    .cloned()
                                    .unwrap_or_default();
                                let was = own.clone();
                                values_row(
                                    ui,
                                    [
                                        &mut own.suspicion,
                                        &mut own.investigate_after,
                                        &mut own.search,
                                    ],
                                    base,
                                );
                                ui.end_row();
                                if own != was {
                                    let slot = animal_mut(a, &x.name);
                                    (slot.suspicion, slot.investigate_after, slot.search) =
                                        (own.suspicion, own.investigate_after, own.search);
                                }
                            }
                        });
                });
            });
        a.animals.retain(|x| !x.values().is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_a_value_adds_the_animal_and_clearing_drops_it() {
        let mut a = Awareness::default();
        animal_mut(&mut a, "Raptor").search = Some(40);
        assert_eq!(a.animals.len(), 1);
        animal_mut(&mut a, "Raptor").search = None;
        a.animals.retain(|x| !x.values().is_empty());
        assert!(a.animals.is_empty());
    }
}
