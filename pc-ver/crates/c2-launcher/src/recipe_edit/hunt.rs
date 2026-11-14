//! The recipe editor's Advanced page: "Hardcore hunt"
//! (`[settings.hunt]`, `c2_mods::hunt`): an opt-in ruleset for hunts, each
//! rule on its own. Regular hunts and Hardcore survival are not changed.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::hardcore as fmt;
use c2_mods::hunt::{Extraction, Hunt, Knife, Predator, SecondPredator};
use c2_mods::recipe::Recipe;
use eframe::egui::{self, RichText};

use super::instance::Extras;
use crate::app::{help, panel};
use crate::theme::*;

/// The reactions, as the page names them.
const REACTIONS: [(&str, &str); 4] = [
    ("", "By size (default)"),
    ("circle_lunge", "Circle and lunge"),
    ("circle_eat", "Circle and eat"),
    ("ignore", "Ignore: the original attack"),
];

fn reaction_label(key: &str) -> &'static str {
    REACTIONS
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key.trim()))
        .map_or(REACTIONS[0].1, |&(_, l)| l)
}

/// A number with a default: the default until changed.
fn number(ui: &mut egui::Ui, v: &mut Option<u32>, default: u32, range: (u32, u32), unit: &str) {
    let mut x = v.unwrap_or(default);
    if ui
        .add(
            egui::DragValue::new(&mut x)
                .range(range.0..=range.1)
                .suffix(unit),
        )
        .changed()
    {
        *v = (x != default).then_some(x);
    }
}

/// A rule that is on or off, with its settings shown while on.
fn rule<T: Default>(
    ui: &mut egui::Ui,
    v: &mut Option<T>,
    label: &str,
    hint: &str,
    body: impl FnOnce(&mut egui::Ui, &mut T),
) {
    let mut on = v.is_some();
    if ui.checkbox(&mut on, label).on_hover_text(hint).changed() {
        *v = on.then(T::default);
    }
    if let Some(x) = v {
        ui.indent(label, |ui| body(ui, x));
    }
}

/// Sets `name`'s reaction (`""`: back to the default, the entry removed).
fn set_reaction(h: &mut Hunt, name: &str, key: &str) {
    h.predators.retain(|p| p.name != name);
    if !key.is_empty() {
        h.predators.push(Predator {
            name: name.into(),
            reaction: key.into(),
        });
    }
}

impl Extras {
    /// The Advanced page's hardcore hunt ruleset: whether it changed.
    pub(super) fn hunt_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) -> bool {
        let mut h = r.settings.hunt.clone().unwrap_or_default();
        let before = h.clone();
        ui.label(
            RichText::new("Hardcore hunt (experimental)")
                .color(SAND)
                .strong(),
        );
        ui.label(
            RichText::new(
                "PC only so far: on Nintendo 3DS, Wii U and VR these hunts are regular hunts. \
                 Play Vanilla leaves the ruleset out.",
            )
            .color(GOLD),
        );
        help(
            ui,
            "An opt-in ruleset for this instance's hunts. Each rule works on its own. \
             Hardcore survival is a different mode and is not changed.",
        );
        ui.checkbox(&mut h.enabled, "Hardcore hunt rules");
        if h.enabled {
            ui.add_space(4.0);
            rules_ui(ui, &mut h);
            if h.hesitation_s.is_some() {
                self.predators_ui(ui, r, &mut h, game, folders, handle);
            }
        }
        if h == before {
            return false;
        }
        r.settings.hunt = (h != Hunt::default()).then_some(h);
        true
    }

    /// Each animal's reaction, in a table.
    fn predators_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &Recipe,
        h: &mut Hunt,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        if values.animals.is_empty() {
            return;
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Reaction per animal").color(TEXT));
        help(
            ui,
            "By size: carnivores circle and lunge, those too big to lunge (the T-Rex) circle \
             and eat, and other animals attack as the original.",
        );
        panel(ui, |ui| {
            egui::Grid::new("hunt-predators")
                .striped(true)
                .num_columns(2)
                .show(ui, |ui| {
                    for a in &values.animals {
                        ui.label(&a.name);
                        let now = h
                            .predators
                            .iter()
                            .find(|p| p.name == a.name)
                            .map(|p| p.reaction.clone())
                            .unwrap_or_default();
                        let mut pick = now.clone();
                        egui::ComboBox::from_id_salt(("hunt-reaction", &a.name))
                            .selected_text(reaction_label(&now))
                            .show_ui(ui, |ui| {
                                for (key, label) in REACTIONS {
                                    ui.selectable_value(&mut pick, key.to_string(), label);
                                }
                            });
                        ui.end_row();
                        if pick != now {
                            set_reaction(h, &a.name, &pick);
                        }
                    }
                });
        });
    }
}

/// The rules that need no list of animals.
fn rules_ui(ui: &mut egui::Ui, h: &mut Hunt) {
    ui.label(RichText::new("Equipment").color(TEXT));
    ui.checkbox(&mut h.no_night_vision, "No night vision");
    ui.checkbox(&mut h.no_radar, "No radar");
    ui.checkbox(&mut h.no_cover_scent, "No cover scent")
        .on_hover_text("The flashlight is always kept.");
    rule(
        ui,
        &mut h.max_firearms,
        "Limit the weapons carried",
        "The knife does not count.",
        |ui, n| {
            if *n == 0 {
                *n = 2;
            }
            ui.add(
                egui::DragValue::new(n)
                    .range(1..=fmt::FIREARMS_RANGE.1)
                    .suffix(" weapons"),
            );
        },
    );
    ui.add_space(4.0);
    ui.label(RichText::new("Getting out").color(TEXT));
    rule(
        ui,
        &mut h.extraction,
        "Extraction: drop-off and pickup zones",
        "The player picks both spots on a map before the hunt and leaves by calling the \
         ship to the pickup zone. Leaving by the exit prompt forfeits the hunt.",
        |ui, e: &mut Extraction| {
            egui::Grid::new("hunt-extraction")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Ship arrives after");
                    number(ui, &mut e.arrive_s, fmt::ARRIVE_S, fmt::SECONDS_RANGE, " s");
                    ui.end_row();
                    ui.label("Ship hovers for");
                    number(ui, &mut e.hover_s, fmt::HOVER_S, fmt::SECONDS_RANGE, " s");
                    ui.end_row();
                    ui.label("Cooldown after it leaves");
                    number(
                        ui,
                        &mut e.cooldown_s,
                        fmt::COOLDOWN_S,
                        (0, fmt::SECONDS_RANGE.1),
                        " s",
                    );
                    ui.end_row();
                    ui.label("Zones at least this far apart");
                    number(
                        ui,
                        &mut e.min_distance,
                        fmt::ZONE_GAP,
                        fmt::ZONE_GAP_RANGE,
                        " cells",
                    );
                    ui.end_row();
                });
        },
    );
    ui.add_space(4.0);
    ui.label(RichText::new("Supplies and the knife").color(TEXT));
    rule(
        ui,
        &mut h.ammo_drops,
        "Ammunition drops",
        "Placed from the hunt's seed, away from the drop-off. Each adds a load of rounds \
         to every carried weapon.",
        |ui, n| {
            if *n == 0 {
                *n = 3;
            }
            ui.add(
                egui::DragValue::new(n)
                    .range(1..=fmt::DROPS_RANGE.1)
                    .suffix(" per hunt"),
            );
        },
    );
    rule(
        ui,
        &mut h.knife,
        "Knife when every weapon is empty",
        "Knife kills give no points and no trophy.",
        |ui, k: &mut Knife| {
            ui.checkbox(&mut k.dodge, "Dodge: a side-step with the knife out");
        },
    );
    ui.add_space(4.0);
    ui.label(RichText::new("Predators").color(TEXT));
    rule(
        ui,
        &mut h.hesitation_s,
        "Hesitation: a carnivore circles before it attacks",
        "It attacks when the hunter's back is turned to it, or when the time is up.",
        |ui, s| {
            if *s == 0 {
                *s = fmt::HESITATION_S;
            }
            ui.horizontal(|ui| {
                ui.label("Circles for");
                ui.add(
                    egui::DragValue::new(s)
                        .range(fmt::SECONDS_RANGE.0..=fmt::SECONDS_RANGE.1)
                        .suffix(" s"),
                );
            });
        },
    );
    if h.hesitation_s.is_some() {
        ui.indent("hunt-back", |ui| {
            ui.horizontal(|ui| {
                ui.label("Back turned beyond");
                number(
                    ui,
                    &mut h.back_angle,
                    fmt::BACK_ANGLE,
                    fmt::BACK_ANGLE_RANGE,
                    "°",
                );
            });
            rule(
                ui,
                &mut h.second_predator,
                "Second predator attacks at once",
                "A carnivore arriving while another is hesitating, or soon after, does not \
                 wait if its menu price is above the threshold.",
                |ui, p: &mut SecondPredator| {
                    egui::Grid::new("hunt-second")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Window after an encounter starts");
                            number(ui, &mut p.window_s, fmt::WINDOW_S, fmt::SECONDS_RANGE, " s");
                            ui.end_row();
                            ui.label("Menu price above");
                            number(ui, &mut p.min_price, fmt::MIN_PRICE, (0, 1_000_000), "");
                            ui.end_row();
                        });
                },
            );
        });
    } else {
        h.back_angle = None;
        h.second_predator = None;
        h.predators.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reaction_is_listed_once_and_the_default_removes_it() {
        let mut h = Hunt::default();
        set_reaction(&mut h, "Velociraptor", "circle_eat");
        set_reaction(&mut h, "Velociraptor", "ignore");
        assert_eq!(h.predators.len(), 1);
        assert_eq!(h.predators[0].reaction, "ignore");
        set_reaction(&mut h, "Velociraptor", "");
        assert!(h.predators.is_empty());
    }

    #[test]
    fn every_reaction_the_page_offers_is_one_the_game_reads() {
        for (key, label) in REACTIONS.iter().skip(1) {
            assert!(fmt::Reaction::from_key(key).is_some(), "{key}");
            assert_eq!(reaction_label(key), *label);
        }
        assert_eq!(reaction_label("unknown"), REACTIONS[0].1);
    }
}
