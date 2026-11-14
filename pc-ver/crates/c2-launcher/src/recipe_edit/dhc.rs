//! The recipe editor's Advanced page: the DHC OS terminal's rules
//! (`[settings.dhc]`, `c2_mods::dhc`): how secret animals become known, what
//! a licence costs, and what each area offers in the terminal's hunt setup.
//! Whether the terminal is shown is the player's own switch (the launcher's
//! Menu panel, the game's Options); these rules apply when it is.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::dhc::{self as fmt, Discovery};
use c2_mods::dhc::{Animal, Dhc, Island, EQUIPMENT};
use c2_mods::recipe::Recipe;
use eframe::egui::{self, RichText};

use super::instance::Extras;
use super::pick::names_menu;
use super::rules::with_missing;
use crate::app::help;
use crate::theme::*;

fn discovery_label(d: Discovery) -> &'static str {
    match d {
        Discovery::Off => "Known from the start",
        Discovery::Sighting => "When sighted",
        Discovery::Scan => "When scanned (radar or binoculars)",
        Discovery::Kill => "When killed",
    }
}

/// A discovery mode's label; `unset` for a key that is none of them.
fn mode_label(key: &str, unset: &'static str) -> &'static str {
    Discovery::from_key(key).map_or(unset, discovery_label)
}

/// Keeps animal `name`'s own settings (its entry removed when it has none).
pub(super) fn set_animal(d: &mut Dhc, name: &str, discovery: &str, cost: Option<u32>) {
    d.animals
        .retain(|a| !a.name.trim().eq_ignore_ascii_case(name.trim()));
    if !discovery.is_empty() || cost.is_some() {
        d.animals.push(Animal {
            name: name.trim().into(),
            discovery: discovery.into(),
            licence_cost: cost,
        });
    }
}

/// Keeps what area `name` offers (its entry removed when it restricts
/// nothing).
pub(super) fn set_island(d: &mut Dhc, island: Island) {
    let open = island.animals.is_empty()
        && island.weapons.is_empty()
        && island.equipment.is_empty()
        && island.times.is_empty();
    let i = d
        .islands
        .iter()
        .position(|x| x.area.trim().eq_ignore_ascii_case(island.area.trim()));
    match (i, open) {
        (Some(i), true) => {
            d.islands.remove(i);
        }
        (Some(i), false) => d.islands[i] = island,
        (None, true) => {}
        (None, false) => d.islands.push(island),
    }
}

fn defaults_ui(ui: &mut egui::Ui, d: &mut Dhc) {
    egui::Grid::new("dhc-defaults")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("Secret animals become known");
            egui::ComboBox::from_id_salt("dhc-discovery")
                .selected_text(mode_label(&d.discovery, discovery_label(Discovery::Off)))
                .show_ui(ui, |ui| {
                    for m in Discovery::ALL {
                        let key = if m == Discovery::Off { "" } else { m.key() };
                        ui.selectable_value(&mut d.discovery, key.into(), discovery_label(m));
                    }
                });
            ui.end_row();
            ui.label("A licence costs");
            let mut cost = d.licence_cost.unwrap_or(fmt::DEFAULT_LICENCE_COST);
            let (lo, hi) = fmt::LICENCE_RANGE;
            if ui
                .add(
                    egui::DragValue::new(&mut cost)
                        .range(lo..=hi)
                        .suffix(" points"),
                )
                .changed()
            {
                d.licence_cost = (cost != fmt::DEFAULT_LICENCE_COST).then_some(cost);
            }
            ui.end_row();
        });
}

fn animals_ui(ui: &mut egui::Ui, d: &mut Dhc, animals: &[String]) {
    let kept: Vec<String> = d.animals.iter().map(|a| a.name.clone()).collect();
    let default_cost = d.licence_cost.unwrap_or(fmt::DEFAULT_LICENCE_COST);
    egui::Grid::new("dhc-animals")
        .striped(true)
        .num_columns(3)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (name, here) in with_missing(animals, &kept) {
                let was = d
                    .animals
                    .iter()
                    .find(|a| a.name.trim().eq_ignore_ascii_case(name.trim()))
                    .map_or((String::new(), None), |a| {
                        (a.discovery.clone(), a.licence_cost)
                    });
                let mut now = was.clone();
                if here {
                    ui.label(RichText::new(&name).color(TEXT));
                } else {
                    ui.label(RichText::new(format!("{name} (not in this game)")).color(DIM));
                }
                egui::ComboBox::from_id_salt(("dhc-animal-discovery", &name))
                    .selected_text(mode_label(&now.0, "As the default"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut now.0, String::new(), "As the default");
                        for m in Discovery::ALL {
                            ui.selectable_value(&mut now.0, m.key().into(), discovery_label(m));
                        }
                    });
                ui.horizontal(|ui| {
                    let mut own = now.1.is_some();
                    if ui.checkbox(&mut own, "Own licence cost").changed() {
                        now.1 = own.then_some(default_cost);
                    }
                    if let Some(c) = &mut now.1 {
                        let (lo, hi) = fmt::LICENCE_RANGE;
                        ui.add(egui::DragValue::new(c).range(lo..=hi).suffix(" points"));
                    }
                });
                ui.end_row();
                if now != was {
                    set_animal(d, &name, &now.0, now.1);
                }
            }
        });
}

fn islands_ui(
    ui: &mut egui::Ui,
    d: &mut Dhc,
    areas: &[String],
    animals: &[String],
    weapons: &[String],
) {
    let kept: Vec<String> = d.islands.iter().map(|i| i.area.clone()).collect();
    let equipment: Vec<String> = EQUIPMENT.iter().map(|e| e.to_string()).collect();
    let times: Vec<String> = fmt::TIMES.iter().map(|t| t.to_string()).collect();
    egui::Grid::new("dhc-islands")
        .striped(true)
        .num_columns(5)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (area, here) in with_missing(areas, &kept) {
                let was = d
                    .islands
                    .iter()
                    .find(|i| i.area.trim().eq_ignore_ascii_case(area.trim()))
                    .cloned()
                    .unwrap_or_else(|| Island {
                        area: area.clone(),
                        ..Island::default()
                    });
                let mut now = was.clone();
                if here {
                    ui.label(RichText::new(&area).color(TEXT));
                } else {
                    ui.label(RichText::new(format!("{area} (not in this game)")).color(DIM));
                }
                names_menu(
                    ui,
                    ("dhc-island-animals", &area),
                    &mut now.animals,
                    animals,
                    "every animal",
                );
                names_menu(
                    ui,
                    ("dhc-island-weapons", &area),
                    &mut now.weapons,
                    weapons,
                    "every weapon",
                );
                names_menu(
                    ui,
                    ("dhc-island-equipment", &area),
                    &mut now.equipment,
                    &equipment,
                    "all equipment",
                );
                names_menu(
                    ui,
                    ("dhc-island-times", &area),
                    &mut now.times,
                    &times,
                    "every time of day",
                );
                ui.end_row();
                if now != was {
                    // All three times ticked is the same as none listed.
                    if fmt::TIMES
                        .iter()
                        .all(|t| now.times.iter().any(|x| x.eq_ignore_ascii_case(t)))
                    {
                        now.times.clear();
                    }
                    set_island(d, now);
                }
            }
        });
}

impl Extras {
    /// The Advanced page's DHC OS terminal rules.
    pub(super) fn dhc_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let mut d = r.settings.dhc.clone().unwrap_or_default();
        let before = d.clone();
        ui.label(
            RichText::new("DHC OS terminal rules (experimental)")
                .color(SAND)
                .strong(),
        );
        help(
            ui,
            "Rules for the DinoHunt Corp terminal: how secret animals become known, what a \
             licence costs, and what each area offers in its hunt setup. They apply when the \
             player has the terminal on (Menu panel, or the game's Options > Game); the \
             original menus do not use them yet. PC, Android and 3DS; Play Vanilla leaves \
             them out.",
        );
        ui.checkbox(&mut d.enabled, "Build these rules into the game");
        if d.enabled {
            defaults_ui(ui, &mut d);
            if let Some(values) = self.read_values(ui, r, game, folders, handle) {
                let names = |l: &[c2_mods::balance::Item]| -> Vec<String> {
                    l.iter().map(|i| i.name.clone()).collect()
                };
                let (areas, animals, weapons) = (
                    names(&values.areas),
                    names(&values.animals),
                    names(&values.weapons),
                );
                egui::CollapsingHeader::new("Per animal: discovery and licence cost")
                    .id_salt("dhc-animals-header")
                    .show(ui, |ui| animals_ui(ui, &mut d, &animals));
                egui::CollapsingHeader::new("Per area: what the terminal offers there")
                    .id_salt("dhc-islands-header")
                    .show(ui, |ui| {
                        help(
                            ui,
                            "Tick what an area offers. Nothing ticked of a kind: every one is \
                             offered.",
                        );
                        islands_ui(ui, &mut d, &areas, &animals, &weapons);
                    });
            }
        }
        if d != before {
            r.settings.dhc = (d != Dhc::default()).then_some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_animal_with_nothing_of_its_own_keeps_no_entry() {
        let mut d = Dhc::default();
        set_animal(&mut d, "Rex", "", None);
        assert!(d.animals.is_empty());
        set_animal(&mut d, "Rex", "kill", None);
        set_animal(&mut d, " rex", "kill", Some(500));
        assert_eq!(d.animals.len(), 1);
        assert_eq!(d.animals[0].licence_cost, Some(500));
        set_animal(&mut d, "REX", "", None);
        assert!(d.animals.is_empty());
    }

    #[test]
    fn an_area_that_restricts_nothing_keeps_no_entry() {
        let mut d = Dhc::default();
        let island = |weapons: &[&str]| Island {
            area: "Ravan".into(),
            weapons: weapons.iter().map(|w| w.to_string()).collect(),
            ..Island::default()
        };
        set_island(&mut d, island(&[]));
        assert!(d.islands.is_empty());
        set_island(&mut d, island(&["Pistol"]));
        set_island(&mut d, island(&["Pistol", "Shotgun"]));
        assert_eq!(d.islands.len(), 1);
        assert_eq!(d.islands[0].weapons.len(), 2);
        set_island(&mut d, island(&[]));
        assert!(d.islands.is_empty());
    }

    /// The tables are laid out in a headless frame and change nothing.
    #[test]
    fn the_tables_are_laid_out_without_changing_the_rules() {
        let mut d = Dhc {
            enabled: true,
            ..Dhc::default()
        };
        set_animal(&mut d, "Rex", "kill", Some(500));
        set_animal(&mut d, "Dodo", "scan", None);
        let names = |l: &[&str]| -> Vec<String> { l.iter().map(|s| s.to_string()).collect() };
        let (areas, animals, weapons) = (names(&["Ravan"]), names(&["Rex"]), names(&["Pistol"]));
        let before = d.clone();
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            defaults_ui(ui, &mut d);
            animals_ui(ui, &mut d, &animals);
            islands_ui(ui, &mut d, &areas, &animals, &weapons);
        });
        out.textures_delta.clear();
        assert_eq!(d, before);
    }

    #[test]
    fn every_mode_has_a_label_and_an_unknown_key_reads_as_unset() {
        for m in Discovery::ALL {
            assert_eq!(mode_label(m.key(), "unset"), discovery_label(m));
        }
        assert_eq!(mode_label("", "unset"), "unset");
        assert_eq!(mode_label("smell", "unset"), "unset");
    }
}
