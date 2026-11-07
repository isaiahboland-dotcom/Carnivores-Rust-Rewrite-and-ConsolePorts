//! The recipe editor's Advanced page, instance rules
//! (`[settings.rules]`, `c2_mods::rules`): the times of day offered,
//! settings fixed by the instance (and hidden from the in-game settings),
//! and spawn rules: at most so many picked animals per area, extra
//! unpicked animals on chosen areas, and animals kept off the hunt menu.
//! Nothing set: the game is as it was.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::rules::{Lock, TIMES};
use c2_mods::recipe::Recipe;
use c2_mods::rules::{AreaRule, LockValue, Rules, SecretRule};
use eframe::egui::{self, RichText};

use super::instance::Extras;
use crate::app::help;
use crate::theme::*;

/// Each lockable setting's name in the launcher, its hint, and the value
/// it starts at when locked.
const LOCKS: [(Lock, &str, &str, u32); 8] = [
    (
        Lock::DynamicTime,
        "Dynamic time",
        "The one-hour day/night cycle.",
        0,
    ),
    (
        Lock::AdvancedObserver,
        "Advanced observer mode",
        "Observing: unlimited instant teleports, flying and stopping time.",
        0,
    ),
    (
        Lock::MultiAmmo,
        "Multi ammo",
        "With the tranquilizer: half lethal rounds, half darts.",
        0,
    ),
    (
        Lock::MobileRadar,
        "Mobile radar",
        "The map as the mobile game draws it.",
        0,
    ),
    (
        Lock::Teleport,
        "Teleport (map)",
        "Teleporting by the map.",
        0,
    ),
    (
        Lock::AllHuntable,
        "All dinos huntable",
        "Every huntable animal in the hunt menu, roaming add-ons too.",
        0,
    ),
    (
        Lock::SecretDinos,
        "Secret dinos per map",
        "Unpicked huntable animals per map (0: none).",
        10,
    ),
    (
        Lock::SurvivalBudget,
        "Survival starting budget",
        "Starting survival points (0: from the menu's prices).",
        0,
    ),
];

/// Whether time `t` is offered by `r` (none listed: all three).
fn offered(r: &Rules, t: &str) -> bool {
    r.times.is_empty() || r.times.iter().any(|x| x.eq_ignore_ascii_case(t))
}

/// The times of day offered: at least one stays.
pub(super) fn times_ui(ui: &mut egui::Ui, r: &mut Rules) {
    ui.label(RichText::new("Times of day").color(SAND).strong());
    help(
        ui,
        "The times of day the hunt menu offers. At least one stays. Survival keeps its own.",
    );
    let on: Vec<bool> = TIMES.iter().map(|t| offered(r, t)).collect();
    let count = on.iter().filter(|&&b| b).count();
    let mut next = on.clone();
    ui.horizontal(|ui| {
        for (i, label) in ["Dawn", "Day", "Night"].into_iter().enumerate() {
            let last = on[i] && count == 1;
            ui.add_enabled(!last, egui::Checkbox::new(&mut next[i], label))
                .on_disabled_hover_text("One time of day must stay.");
        }
    });
    if next != on {
        r.times = if next.iter().all(|&b| b) {
            Vec::new()
        } else {
            (0..3)
                .filter(|&i| next[i])
                .map(|i| TIMES[i].to_string())
                .collect()
        };
    }
}

/// The settings fixed by the instance.
pub(super) fn locks_ui(ui: &mut egui::Ui, r: &mut Rules) {
    ui.label(RichText::new("Locked settings").color(SAND).strong());
    help(
        ui,
        "A locked setting is fixed at the value here and removed from the in-game settings, \
         so the instance plays as you meant it. Unlocked, players choose. The 3DS and Wii U \
         lock the ones they have.",
    );
    egui::Grid::new("rules-locks")
        .striped(true)
        .num_columns(3)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (lock, name, hint, start) in LOCKS {
                match r.locks.field(lock) {
                    LockValue::Switch(v) => {
                        let mut locked = v.is_some();
                        if ui.checkbox(&mut locked, name).on_hover_text(hint).changed() {
                            *v = locked.then_some(start != 0);
                        }
                        if let Some(b) = v {
                            ui.horizontal(|ui| {
                                ui.radio_value(b, true, "On");
                                ui.radio_value(b, false, "Off");
                            });
                        } else {
                            ui.label(RichText::new("player's choice").color(DIM));
                        }
                    }
                    LockValue::Number(v) => {
                        let mut locked = v.is_some();
                        if ui.checkbox(&mut locked, name).on_hover_text(hint).changed() {
                            *v = locked.then_some(start);
                        }
                        if let Some(n) = v {
                            let max = if lock == Lock::SecretDinos {
                                255
                            } else {
                                1_000_000
                            };
                            ui.add(egui::DragValue::new(n).range(0..=max));
                        } else {
                            ui.label(RichText::new("player's choice").color(DIM));
                        }
                    }
                }
                ui.end_row();
            }
        });
}

/// `names` with any of `kept` it lacks (marked as not in this game).
fn with_missing(names: &[String], kept: &[String]) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = names.iter().map(|n| (n.clone(), true)).collect();
    for k in kept {
        if !k.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(k)) {
            v.push((k.clone(), false));
        }
    }
    v
}

impl Extras {
    /// Spawn rules over the instance's areas and animals.
    pub(super) fn spawn_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        ui.label(RichText::new("Spawn rules").color(SAND).strong());
        help(
            ui,
            "Secret animals are ones the player never picked that still roam the map. Add some \
             per area, cap the picked animals per area, and keep animals off the hunt menu: \
             for a hardcore instance, leave only herbivores to pick and give each area its own \
             carnivores to track down.",
        );
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        let areas: Vec<String> = values.areas.iter().map(|a| a.name.clone()).collect();
        let animals: Vec<String> = values.animals.iter().map(|a| a.name.clone()).collect();
        let rules = &mut r.settings.rules;
        ui.add_space(4.0);
        area_caps_ui(ui, rules, &areas);
        ui.add_space(6.0);
        secrets_ui(ui, rules, &areas, &animals);
        ui.add_space(6.0);
        hidden_ui(ui, rules, &animals);
    }
}

/// Per area: at most so many picked animals.
fn area_caps_ui(ui: &mut egui::Ui, r: &mut Rules, areas: &[String]) {
    ui.label(RichText::new("Picked animals per area").color(TEXT));
    let kept: Vec<String> = r.areas.iter().map(|a| a.area.clone()).collect();
    egui::Grid::new("rules-area-caps")
        .striped(true)
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (name, here) in with_missing(areas, &kept) {
                let i = r
                    .areas
                    .iter()
                    .position(|a| a.area.eq_ignore_ascii_case(&name));
                let mut capped = i.and_then(|i| r.areas[i].max_huntable).is_some();
                let label = if here {
                    RichText::new(&name).color(TEXT)
                } else {
                    RichText::new(format!("{name} (not in this game)")).color(DIM)
                };
                let changed = ui
                    .checkbox(&mut capped, label)
                    .on_hover_text("At most this many of the picked animals spawn here.")
                    .changed();
                match (i, capped) {
                    (Some(i), true) => {
                        if let Some(n) = &mut r.areas[i].max_huntable {
                            ui.add(egui::DragValue::new(n).range(0..=64).suffix(" at most"));
                        }
                    }
                    (None, true) if changed => {
                        r.areas.push(AreaRule {
                            area: name.clone(),
                            max_huntable: Some(4),
                        });
                        ui.label("");
                    }
                    _ => {
                        ui.label(RichText::new("as the game places them").color(DIM));
                    }
                }
                if changed && !capped {
                    r.areas.retain(|a| !a.area.eq_ignore_ascii_case(&name));
                }
                ui.end_row();
            }
        });
}

/// Extra unpicked animals: which, how many, on which areas.
fn secrets_ui(ui: &mut egui::Ui, r: &mut Rules, areas: &[String], animals: &[String]) {
    ui.label(RichText::new("Secret animals").color(TEXT));
    help(
        ui,
        "Placed at random, at least 40 cells from the hunter, whatever the player picked, on \
         top of the game's own secret dinos.",
    );
    let mut remove = None;
    for (k, s) in r.secrets.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            let names = with_missing(animals, std::slice::from_ref(&s.animal));
            egui::ComboBox::from_id_salt(("rules-secret-animal", k))
                .selected_text(s.animal.as_str())
                .show_ui(ui, |ui| {
                    for (n, here) in names {
                        let text = if here {
                            n.clone()
                        } else {
                            format!("{n} (not in this game)")
                        };
                        ui.selectable_value(&mut s.animal, n, text);
                    }
                });
            ui.add(
                egui::DragValue::new(&mut s.count)
                    .range(1..=32)
                    .prefix("× "),
            );
            let shown = if s.areas.is_empty() {
                "every area".to_string()
            } else {
                s.areas.join(", ")
            };
            ui.menu_button(format!("on {shown}"), |ui| {
                let mut every = s.areas.is_empty();
                if ui.checkbox(&mut every, "Every area").changed() {
                    s.areas.clear();
                    if !every {
                        s.areas.extend(areas.first().cloned());
                    }
                }
                ui.separator();
                for (a, here) in with_missing(areas, &s.areas) {
                    let mut on = s.areas.iter().any(|x| x.eq_ignore_ascii_case(&a));
                    let text = if here {
                        a.clone()
                    } else {
                        format!("{a} (not in this game)")
                    };
                    if ui.checkbox(&mut on, text).changed() {
                        if on {
                            s.areas.push(a.clone());
                        } else {
                            s.areas.retain(|x| !x.eq_ignore_ascii_case(&a));
                        }
                    }
                }
            });
            if ui.small_button("Remove").clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        r.secrets.remove(k);
    }
    if !animals.is_empty() && ui.button("Add a secret animal").clicked() {
        r.secrets.push(SecretRule {
            animal: animals[0].clone(),
            count: 1,
            areas: Vec::new(),
        });
    }
}

/// Animals kept off the hunt menu.
fn hidden_ui(ui: &mut egui::Ui, r: &mut Rules, animals: &[String]) {
    ui.label(RichText::new("Not in the hunt menu").color(TEXT));
    help(
        ui,
        "Ticked animals stay in the game but can't be picked; they only appear as secret \
         animals.",
    );
    ui.horizontal_wrapped(|ui| {
        for (n, here) in with_missing(animals, &r.hidden) {
            let mut on = r.hidden.iter().any(|h| h.eq_ignore_ascii_case(&n));
            let text = if here {
                RichText::new(&n).color(TEXT)
            } else {
                RichText::new(format!("{n} (not in this game)")).color(DIM)
            };
            if ui.checkbox(&mut on, text).changed() {
                if on {
                    r.hidden.push(n.clone());
                } else {
                    r.hidden.retain(|h| !h.eq_ignore_ascii_case(&n));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_left_from_another_game_are_still_listed() {
        let names = ["Rex".to_string(), "Raptor".to_string()];
        let v = with_missing(&names, &["raptor".into(), "Spino".into()]);
        assert_eq!(
            v,
            [
                ("Rex".into(), true),
                ("Raptor".into(), true),
                ("Spino".into(), false)
            ]
        );
    }

    #[test]
    fn every_time_offered_is_kept_as_none_listed() {
        let mut r = Rules {
            times: vec!["day".into()],
            ..Rules::default()
        };
        assert!(offered(&r, "day") && !offered(&r, "night"));
        r.times.clear();
        assert!(TIMES.iter().all(|t| offered(&r, t)));
    }
}
