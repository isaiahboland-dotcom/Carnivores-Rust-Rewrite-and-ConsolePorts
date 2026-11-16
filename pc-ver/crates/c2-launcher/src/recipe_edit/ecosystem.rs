//! The recipe editor's Advanced page: the spawn ecosystem
//! (`[settings.world.ecosystem]`, `c2_mods::ecosystem`). Caps on the whole
//! map, and a rule for each zone drawn in the map editor
//! ([`super::world`]): nothing spawns there, a cap, containment, the
//! animals allowed and how readily each appears.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::ecosystem::{Contain, CAP_RANGE, DEFAULT_DISTANCE, DISTANCE_RANGE, WEIGHT_RANGE};
use c2_mods::ecosystem::{
    Ecosystem, Safety, SpeciesCap, SpeciesDistance, TimeOverride, Weight, ZoneOverride, ZoneRule,
    NO_SPAWN_TAG,
};
use c2_mods::recipe::Recipe;
use c2_mods::world::World;
use eframe::egui::{self, RichText};

use super::instance::Extras;
use super::pick::names_menu;
use super::rules::with_missing;
use crate::app::help;
use crate::theme;

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The rule of zone `id`, or one that changes nothing.
pub(super) fn rule_of(e: &Ecosystem, id: &str) -> ZoneRule {
    e.zones
        .iter()
        .find(|z| same(&z.zone, id))
        .cloned()
        .unwrap_or_else(|| ZoneRule {
            zone: id.trim().into(),
            ..ZoneRule::default()
        })
}

/// Whether `rule` changes nothing.
fn is_blank(rule: &ZoneRule) -> bool {
    *rule
        == ZoneRule {
            zone: rule.zone.clone(),
            ..ZoneRule::default()
        }
}

/// Stores `rule` as its zone's (the first for that zone; any others go),
/// or takes the zone's rule out when it changes nothing.
pub(super) fn set_rule(e: &mut Ecosystem, rule: ZoneRule) {
    let id = rule.zone.clone();
    let at = e.zones.iter().position(|z| same(&z.zone, &id));
    e.zones.retain(|z| !same(&z.zone, &id));
    if !is_blank(&rule) {
        let i = at.unwrap_or(e.zones.len()).min(e.zones.len());
        e.zones.insert(i, rule);
    }
}

/// The rules for zones that are no longer drawn, taken out. How many went.
pub(super) fn drop_orphans(w: &mut World) -> usize {
    let ids: Vec<String> = w.zones.iter().map(|z| z.id.clone()).collect();
    let Some(e) = &mut w.ecosystem else { return 0 };
    let before = e.zones.len();
    e.zones.retain(|r| ids.iter().any(|id| same(id, &r.zone)));
    before - e.zones.len()
}

/// The validator's findings under `prefix`, without it.
fn faults(w: &World, animals: &[String], prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    c2_mods::ecosystem::check(w, Some(animals), &mut |key, message| {
        if let Some(rest) = key.strip_prefix(prefix) {
            let what = rest.trim_start_matches('.');
            out.push(if what.is_empty() {
                message
            } else {
                format!("{what}: {message}")
            });
        }
    });
    out
}

fn animal_combo(ui: &mut egui::Ui, salt: (&str, usize, usize), name: &mut String, all: &[String]) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(if name.is_empty() {
            "choose an animal".to_string()
        } else {
            name.clone()
        })
        .show_ui(ui, |ui| {
            for (n, here) in with_missing(all, std::slice::from_ref(name)) {
                let text = if here {
                    n.clone()
                } else {
                    format!("{n} (not in this game)")
                };
                ui.selectable_value(name, n, text);
            }
        });
}

/// A switch with a count: `None` when off.
fn cap_ui(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>, start: u32) {
    ui.horizontal(|ui| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *value = on.then_some(start);
        }
        if let Some(n) = value {
            ui.add(egui::DragValue::new(n).range(CAP_RANGE.0..=CAP_RANGE.1));
        }
    });
}

fn caps_ui(ui: &mut egui::Ui, e: &mut Ecosystem, animals: &[String]) {
    cap_ui(
        ui,
        "At most this many living animals on a map",
        &mut e.max_animals,
        30,
    );
    let mut gone = None;
    for (i, s) in e.species.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label("At most");
            ui.add(egui::DragValue::new(&mut s.max).range(CAP_RANGE.0..=CAP_RANGE.1));
            animal_combo(ui, ("eco-species", i, 0), &mut s.name, animals);
            ui.label("alive at once");
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                gone = Some(i);
            }
        });
    }
    if let Some(i) = gone {
        e.species.remove(i);
    }
    if ui.small_button("Cap an animal").clicked() {
        e.species.push(SpeciesCap {
            name: String::new(),
            max: 4,
        });
    }
}

fn zone_ui(ui: &mut egui::Ui, k: usize, rule: &mut ZoneRule, tagged: bool, animals: &[String]) {
    ui.checkbox(&mut rule.avoid, "Wandering and fleeing animals keep out")
        .on_hover_text(
            "They do not head into the zone. One already inside may leave, and a chase may \
             still cross it.",
        );
    if tagged {
        ui.label(
            RichText::new(format!(
                "Tagged {NO_SPAWN_TAG} in the map editor: nothing spawns here."
            ))
            .color(theme::dim()),
        );
    } else {
        ui.checkbox(&mut rule.no_spawn, "Nothing spawns here")
            .on_hover_text("Animals already on the map may still walk in.");
    }
    if rule.no_spawn || tagged {
        return;
    }
    cap_ui(
        ui,
        "At most this many living animals inside",
        &mut rule.max,
        6,
    );
    ui.horizontal(|ui| {
        ui.label("Animals that appear here").on_hover_text(
            "Stay: they never cross the zone's edge. Mostly stay: they wander inside and \
                 may leave while chasing or fleeing.",
        );
        let now = Contain::parse(&rule.contain).unwrap_or_default();
        for (c, label) in [
            (Contain::Free, "roam freely"),
            (Contain::Soft, "mostly stay"),
            (Contain::Hard, "stay"),
        ] {
            if ui.radio(now == c, label).clicked() {
                rule.contain = if c == Contain::Free {
                    String::new()
                } else {
                    c.name().into()
                };
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Only these appear here");
        names_menu(ui, ("eco-allow", k), &mut rule.allow, animals, "any animal");
    });
    let offered: Vec<String> = if rule.allow.is_empty() {
        animals.to_vec()
    } else {
        rule.allow.clone()
    };
    let mut gone = None;
    for (j, w) in rule.weights.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            animal_combo(ui, ("eco-weight", k, j), &mut w.name, &offered);
            ui.label("appears");
            ui.add(
                egui::DragValue::new(&mut w.weight)
                    .range(f64::from(WEIGHT_RANGE.0)..=f64::from(WEIGHT_RANGE.1))
                    .speed(0.05)
                    .suffix(" ×"),
            )
            .on_hover_text("Against 1 anywhere else on the map; 0: never here.");
            ui.label("as readily here");
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                gone = Some(j);
            }
        });
    }
    if let Some(j) = gone {
        rule.weights.remove(j);
    }
    if ui.small_button("Weight an animal").clicked() {
        rule.weights.push(Weight {
            name: String::new(),
            weight: 2.0,
        });
    }
}

/// A switch with a distance in cells: `None` when off (the original's).
fn distance_ui(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>) {
    ui.horizontal(|ui| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *value = on.then_some(DEFAULT_DISTANCE);
        }
        match value {
            Some(n) => {
                ui.add(
                    egui::DragValue::new(n)
                        .range(DISTANCE_RANGE.0..=DISTANCE_RANGE.1)
                        .suffix(" cells"),
                );
            }
            None => {
                ui.label(
                    RichText::new(format!("{DEFAULT_DISTANCE} cells, as the original"))
                        .color(theme::dim()),
                );
            }
        }
    });
}

fn safety_ui(ui: &mut egui::Ui, e: &mut Ecosystem, animals: &[String]) {
    let mut s = e.safety.clone().unwrap_or_default();
    ui.label(RichText::new("Distance from the hunter").color(theme::text()));
    distance_ui(ui, "At the start of a hunt", &mut s.initial_distance);
    distance_ui(
        ui,
        "Replacements for killed animals",
        &mut s.respawn_distance,
    );
    distance_ui(ui, "The spawn rules' extra animals", &mut s.extra_distance);
    ui.checkbox(
        &mut s.out_of_view,
        "Replacements appear only where the hunter is not looking",
    );
    let mut gone = None;
    for (i, x) in s.species.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            animal_combo(ui, ("eco-safety", i, 0), &mut x.name, animals);
            ui.label("at least");
            ui.add(
                egui::DragValue::new(&mut x.distance)
                    .range(DISTANCE_RANGE.0..=DISTANCE_RANGE.1)
                    .suffix(" cells"),
            );
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                gone = Some(i);
            }
        });
    }
    if let Some(i) = gone {
        s.species.remove(i);
    }
    if ui.small_button("Keep an animal farther off").clicked() {
        s.species.push(SpeciesDistance {
            name: String::new(),
            distance: 80,
        });
    }
    e.safety = (s != Safety::default()).then_some(s);
}

/// The override of zone `id` in `o`, or one that changes nothing.
pub(super) fn override_of(o: &TimeOverride, id: &str) -> ZoneOverride {
    o.zones
        .iter()
        .find(|z| same(&z.zone, id))
        .cloned()
        .unwrap_or_else(|| ZoneOverride {
            zone: id.trim().into(),
            ..ZoneOverride::default()
        })
}

/// Stores `z` as its zone's override in `o`, or takes it out when it
/// changes nothing.
pub(super) fn set_override(o: &mut TimeOverride, z: ZoneOverride) {
    let blank = z
        == ZoneOverride {
            zone: z.zone.clone(),
            ..ZoneOverride::default()
        };
    let id = z.zone.clone();
    let at = o.zones.iter().position(|x| same(&x.zone, &id));
    o.zones.retain(|x| !same(&x.zone, &id));
    if !blank {
        let i = at.unwrap_or(o.zones.len()).min(o.zones.len());
        o.zones.insert(i, z);
    }
}

fn time_ui(ui: &mut egui::Ui, t: usize, slot: &mut Option<TimeOverride>, zones: &[String]) {
    let mut o = slot.clone().unwrap_or_default();
    cap_ui(
        ui,
        "At most this many living animals on a map",
        &mut o.max_animals,
        20,
    );
    for (k, id) in zones.iter().enumerate() {
        let mut z = override_of(&o, id);
        let was = z.clone();
        ui.horizontal(|ui| {
            ui.label(RichText::new(id).color(theme::text()));
            let text = match z.no_spawn {
                None => "spawning as its rule",
                Some(true) => "nothing spawns",
                Some(false) => "animals spawn",
            };
            egui::ComboBox::from_id_salt(("eco-time-spawn", t, k))
                .selected_text(text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut z.no_spawn, None, "spawning as its rule");
                    ui.selectable_value(&mut z.no_spawn, Some(true), "nothing spawns");
                    ui.selectable_value(&mut z.no_spawn, Some(false), "animals spawn");
                });
            let mut on = z.max.is_some();
            if ui.checkbox(&mut on, "at most").changed() {
                z.max = on.then_some(4);
            }
            if let Some(n) = &mut z.max {
                ui.add(egui::DragValue::new(n).range(CAP_RANGE.0..=CAP_RANGE.1));
                ui.label("inside");
            }
        });
        if z != was {
            set_override(&mut o, z);
        }
    }
    *slot = (o != TimeOverride::default()).then_some(o);
}

impl Extras {
    /// The Advanced page's spawn ecosystem panel.
    pub(super) fn ecosystem_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let mut w = r.settings.world.clone().unwrap_or_default();
        let before = w.clone();
        let mut e = w.ecosystem.clone().unwrap_or_default();
        ui.label(
            RichText::new("Spawn ecosystem (experimental)")
                .color(theme::sand())
                .strong(),
        );
        help(
            ui,
            "Which animals appear in which zone of a map, and how many. The rules steer where \
             the game places animals at the start of a hunt and when it replaces them; nothing \
             already on the map is removed or moved. Zones are the ones drawn in the map editor \
             above. PC and Android for now. Play Vanilla leaves it out.",
        );
        if ui
            .checkbox(&mut e.enabled, "Build the ecosystem into the game")
            .changed()
            && e.enabled
        {
            w.enabled = true;
        }
        if e.enabled && !w.enabled {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        "World zones are not built into the game, so this is not either.",
                    )
                    .color(theme::red()),
                );
                if ui.small_button("Build them").clicked() {
                    w.enabled = true;
                }
            });
        }
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        let animals: Vec<String> = values.animals.iter().map(|i| i.name.clone()).collect();
        caps_ui(ui, &mut e, &animals);
        w.ecosystem = Some(e.clone());
        for f in faults(&w, &animals, "settings.world.ecosystem.species")
            .into_iter()
            .chain(faults(&w, &animals, "settings.world.ecosystem.max_animals"))
        {
            ui.label(RichText::new(f).color(theme::gold()));
        }
        ui.add_space(4.0);
        if w.zones.is_empty() {
            help(
                ui,
                "No zones yet: draw one in the map editor above to give it a rule.",
            );
        }
        for k in 0..w.zones.len() {
            let (id, area) = (w.zones[k].id.trim().to_string(), w.zones[k].area.clone());
            let tagged = w.zones[k].tags.iter().any(|t| same(t, NO_SPAWN_TAG));
            let mut rule = rule_of(&e, &id);
            let was = rule.clone();
            let ruled = !is_blank(&rule) || tagged;
            let title = format!("{id}  ({area}){}", if ruled { "  ●" } else { "" });
            egui::CollapsingHeader::new(RichText::new(title).color(theme::text()))
                .id_salt(("eco-zone", k))
                .show(ui, |ui| {
                    zone_ui(ui, k, &mut rule, tagged, &animals);
                    if let Some(i) = e.zones.iter().position(|z| same(&z.zone, &id)) {
                        let prefix = format!("settings.world.ecosystem.zones[{i}]");
                        for f in faults(&w, &animals, &prefix) {
                            ui.label(RichText::new(f).color(theme::gold()));
                        }
                    }
                });
            if rule != was {
                set_rule(&mut e, rule);
            }
        }
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("Spawn safety").color(theme::text()))
            .id_salt("eco-safety")
            .show(ui, |ui| {
                safety_ui(ui, &mut e, &animals);
                w.ecosystem = Some(e.clone());
                for f in faults(&w, &animals, "settings.world.ecosystem.safety") {
                    ui.label(RichText::new(f).color(theme::gold()));
                }
            });
        let ids: Vec<String> = w.zones.iter().map(|z| z.id.trim().to_string()).collect();
        for (t, label) in ["At dawn", "By day", "At night"].into_iter().enumerate() {
            let set = e.times()[t].is_some();
            let title = format!("{label}{}", if set { "  ●" } else { "" });
            egui::CollapsingHeader::new(RichText::new(title).color(theme::text()))
                .id_salt(("eco-time", t))
                .show(ui, |ui| {
                    help(
                        ui,
                        "Only what is set here differs at this time of day; the rest is as \
                         above. Caps per animal, allow lists and weights by time of day are \
                         written in the recipe file (see MODDING.md).",
                    );
                    time_ui(ui, t, e.times_mut()[t], &ids);
                });
        }
        w.ecosystem = (e != Ecosystem::default()).then_some(e);
        let orphans = w.ecosystem.as_ref().map_or(0, |e| {
            e.zones
                .iter()
                .filter(|r| !w.zones.iter().any(|z| same(&z.id, &r.zone)))
                .count()
        });
        if orphans > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{orphans} rule(s) are for zones that are no longer drawn."
                    ))
                    .color(theme::gold()),
                );
                if ui.small_button("Remove them").clicked() {
                    drop_orphans(&mut w);
                }
            });
        }
        if w != before {
            r.settings.world = (w != World::default()).then_some(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_mods::world::Zone;

    fn rule(zone: &str, max: Option<u32>) -> ZoneRule {
        ZoneRule {
            zone: zone.into(),
            max,
            ..ZoneRule::default()
        }
    }

    #[test]
    fn a_zones_rule_is_stored_once_and_goes_when_it_changes_nothing() {
        let mut e = Ecosystem::default();
        assert!(is_blank(&rule_of(&e, " marsh ")));
        assert_eq!(rule_of(&e, " marsh ").zone, "marsh");
        set_rule(&mut e, rule("marsh", Some(4)));
        set_rule(&mut e, rule("ridge", Some(2)));
        set_rule(&mut e, rule("MARSH", Some(7)));
        assert_eq!(e.zones.len(), 2);
        assert_eq!(rule_of(&e, "marsh").max, Some(7));
        // Nothing set: no rule is kept for it.
        set_rule(&mut e, rule("marsh", None));
        assert_eq!(e.zones, [rule("ridge", Some(2))]);
        set_rule(&mut e, rule("nowhere", None));
        assert_eq!(e.zones.len(), 1);
    }

    #[test]
    fn a_second_rule_for_a_zone_goes_when_the_first_is_edited() {
        let mut e = Ecosystem {
            zones: vec![rule("a", Some(1)), rule("b", Some(2)), rule("A", Some(3))],
            ..Ecosystem::default()
        };
        set_rule(&mut e, rule("a", Some(9)));
        assert_eq!(e.zones, [rule("a", Some(9)), rule("b", Some(2))]);
    }

    #[test]
    fn rules_for_zones_no_longer_drawn_are_dropped() {
        let mut w = World {
            zones: vec![Zone {
                id: "Ridge".into(),
                ..Zone::default()
            }],
            ecosystem: Some(Ecosystem {
                zones: vec![rule("ridge", Some(2)), rule("gone", Some(1))],
                ..Ecosystem::default()
            }),
            ..World::default()
        };
        assert_eq!(drop_orphans(&mut w), 1);
        assert_eq!(drop_orphans(&mut w), 0);
        assert_eq!(
            w.ecosystem.map(|e| e.zones),
            Some(vec![rule("ridge", Some(2))])
        );
    }

    #[test]
    fn faults_are_shown_without_their_keys_prefix() {
        let w = World {
            enabled: true,
            ecosystem: Some(Ecosystem {
                enabled: true,
                species: vec![SpeciesCap {
                    name: "Dodo".into(),
                    max: 2,
                }],
                ..Ecosystem::default()
            }),
            ..World::default()
        };
        let found = faults(&w, &["Rex".to_string()], "settings.world.ecosystem.species");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("[0].name: "), "{found:?}");
    }

    #[test]
    fn a_time_override_is_kept_only_while_it_changes_something() {
        let mut o = TimeOverride::default();
        let mut z = override_of(&o, " ridge ");
        assert_eq!(z.zone, "ridge");
        z.no_spawn = Some(false);
        set_override(&mut o, z.clone());
        z.max = Some(2);
        z.zone = "RIDGE".into();
        set_override(&mut o, z);
        assert_eq!(o.zones.len(), 1);
        assert_eq!(override_of(&o, "ridge").max, Some(2));
        set_override(&mut o, override_of(&TimeOverride::default(), "ridge"));
        assert_eq!(o, TimeOverride::default());
    }

    /// Every part of the panel is laid out in a headless frame without a
    /// clash of ids or a panic, and drawing changes nothing.
    #[test]
    fn the_filled_panel_is_laid_out_unchanged() {
        let animals = vec!["Rex".to_string(), "Stego".to_string()];
        let mut e = Ecosystem {
            enabled: true,
            max_animals: Some(30),
            species: vec![SpeciesCap {
                name: "Rex".into(),
                max: 1,
            }],
            safety: Some(Safety {
                respawn_distance: Some(60),
                out_of_view: true,
                species: vec![SpeciesDistance {
                    name: "Rex".into(),
                    distance: 90,
                }],
                ..Safety::default()
            }),
            night: Some(TimeOverride {
                max_animals: Some(10),
                zones: vec![ZoneOverride {
                    zone: "ridge".into(),
                    no_spawn: Some(true),
                    max: Some(2),
                    ..ZoneOverride::default()
                }],
                ..TimeOverride::default()
            }),
            ..Ecosystem::default()
        };
        let mut full = ZoneRule {
            zone: "ridge".into(),
            max: Some(4),
            contain: "soft".into(),
            allow: vec!["Stego".into()],
            weights: vec![Weight {
                name: "Stego".into(),
                weight: 2.0,
            }],
            avoid: true,
            ..ZoneRule::default()
        };
        let (was_e, was_rule) = (e.clone(), full.clone());
        let ids = vec!["ridge".to_string(), "marsh".to_string()];
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            caps_ui(ui, &mut e, &animals);
            zone_ui(ui, 0, &mut full, false, &animals);
            zone_ui(ui, 1, &mut rule("marsh", None), true, &animals);
            safety_ui(ui, &mut e, &animals);
            for t in 0..3 {
                ui.push_id(t, |ui| time_ui(ui, t, e.times_mut()[t], &ids));
            }
        });
        out.textures_delta.clear();
        assert_eq!((e, full), (was_e, was_rule));
    }
}
