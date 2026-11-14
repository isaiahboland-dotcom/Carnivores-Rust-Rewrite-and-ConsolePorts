//! The recipe editor's Advanced page: the mission editor
//! (`[[settings.world.missions]]`, `c2_mods::mission`). A mission is a fixed
//! challenge: an area, a time of day, the animals on the map, the weapons
//! and equipment carried, the player starts the hunter may enter by, the
//! extraction points he may leave by, and the kills asked of him. Entries
//! and exits are the points drawn in the map editor ([`super::world`]).
//!
//! Missions are listed by the DHC OS terminal only, so the panel warns when
//! the terminal is off.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::mission as fmt;
use c2_formats::rules::TIMES;
use c2_formats::world::valid_id;
use c2_mods::dhc::EQUIPMENT;
use c2_mods::mission::{Kill, Mission};
use c2_mods::recipe::Recipe;
use c2_mods::validate::Names;
use c2_mods::world::{Marker, World};
use eframe::egui::{self, RichText};

use super::instance::Extras;
use super::pick::names_menu;
use super::rules::with_missing;
use super::world_edit::same_area;
use crate::app::help;
use crate::theme::*;

/// What the terminal's state means for a recipe with missions.
pub(super) fn terminal_warning(missions: usize, terminal_on: bool) -> Option<&'static str> {
    (missions > 0 && !terminal_on).then_some(
        "The DHC OS terminal is off. Missions are listed by the terminal only: starting this \
         instance without it can hide its missions. Turn it on in the launcher's Menu panel \
         or the game's Options > Game.",
    )
}

/// The ids of `list`'s points on `area`.
fn ids_on(list: &[Marker], area: &str) -> Vec<String> {
    list.iter()
        .filter(|m| same_area(&m.area, area))
        .map(|m| m.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

/// An id no mission of `w` has yet.
fn free_id(w: &World) -> String {
    (1..)
        .map(|n| format!("mission-{n}"))
        .find(|id| {
            !w.missions
                .iter()
                .any(|m| m.id.trim().eq_ignore_ascii_case(id))
        })
        .unwrap_or_default()
}

/// A new mission on `area`, with every start and extraction point drawn
/// there as its entries and exits.
pub(super) fn new_mission(w: &World, area: &str) -> Mission {
    Mission {
        id: free_id(w),
        area: area.into(),
        entries: ids_on(&w.spawns, area),
        exits: ids_on(&w.extractions, area),
        kills: vec![Kill {
            name: String::new(),
            count: 1,
        }],
        ..Mission::default()
    }
}

/// Mission `i` again under a free id, placed after it.
pub(super) fn duplicate(w: &mut World, i: usize) {
    let Some(mut m) = w.missions.get(i).cloned() else {
        return;
    };
    m.id = free_id(w);
    if !m.name.is_empty() {
        m.name.push_str(" (copy)");
    }
    w.missions.insert(i + 1, m);
}

/// Takes mission `i` out, and out of the others' requirements.
pub(super) fn remove(w: &mut World, i: usize) {
    if i >= w.missions.len() {
        return;
    }
    let gone = w.missions.remove(i).id;
    if !w
        .missions
        .iter()
        .any(|m| m.id.trim().eq_ignore_ascii_case(gone.trim()))
    {
        for m in &mut w.missions {
            m.requires
                .retain(|r| !r.trim().eq_ignore_ascii_case(gone.trim()));
        }
    }
}

/// The validator's findings for mission `i`, without their key's prefix.
fn faults(w: &World, names: &Names, i: usize) -> Vec<String> {
    let prefix = format!("settings.world.missions[{i}]");
    let mut out = Vec::new();
    c2_mods::mission::check(w, Some(names), &mut |key, message| {
        if let Some(rest) = key.strip_prefix(&prefix) {
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

fn time_ui(ui: &mut egui::Ui, m: &mut Mission) {
    let now = TIMES
        .iter()
        .position(|t| t.eq_ignore_ascii_case(m.time.trim()))
        .unwrap_or(usize::from(fmt::DEFAULT_TIME));
    for (i, label) in ["Dawn", "Day", "Night"].into_iter().enumerate() {
        if ui.radio(now == i, label).clicked() {
            m.time = if i == usize::from(fmt::DEFAULT_TIME) {
                String::new()
            } else {
                TIMES[i].to_string()
            };
        }
    }
}

fn kills_ui(ui: &mut egui::Ui, k: usize, m: &mut Mission, animals: &[String]) {
    const ANY: &str = "Any of its animals";
    ui.label(RichText::new("Kills required before leaving").color(TEXT));
    // A requirement names one of the mission's own animals.
    let offered: Vec<String> = if m.animals.is_empty() {
        animals.to_vec()
    } else {
        m.animals.clone()
    };
    let mut gone = None;
    for (j, kill) in m.kills.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let (lo, hi) = fmt::KILL_RANGE;
            ui.add(
                egui::DragValue::new(&mut kill.count)
                    .range(lo..=hi)
                    .suffix(" ×"),
            );
            let shown = if kill.name.is_empty() {
                ANY
            } else {
                kill.name.as_str()
            };
            egui::ComboBox::from_id_salt(("mission-kill", k, j))
                .selected_text(shown.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut kill.name, String::new(), ANY);
                    for (n, here) in with_missing(&offered, std::slice::from_ref(&kill.name)) {
                        let text = if here {
                            n.clone()
                        } else {
                            format!("{n} (not in this mission)")
                        };
                        ui.selectable_value(&mut kill.name, n, text);
                    }
                });
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                gone = Some(j);
            }
        });
    }
    if let Some(j) = gone {
        m.kills.remove(j);
    }
    if ui.small_button("Add a kill requirement").clicked() {
        m.kills.push(Kill {
            name: String::new(),
            count: 1,
        });
    }
}

/// What the editor offers a mission's lists from.
struct Offer<'a> {
    names: &'a Names,
    /// Every mission's id.
    ids: &'a [String],
    spawns: &'a [Marker],
    extractions: &'a [Marker],
}

fn mission_ui(ui: &mut egui::Ui, k: usize, m: &mut Mission, o: &Offer) {
    let equipment: Vec<String> = EQUIPMENT.iter().map(|e| e.to_string()).collect();
    egui::Grid::new(("mission-grid", k))
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("Id");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut m.id).desired_width(160.0));
                if !valid_id(m.id.trim()) {
                    ui.label(RichText::new("letters, digits, - and _ only").color(RED));
                }
            });
            ui.end_row();
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut m.name)
                    .desired_width(260.0)
                    .hint_text("shown in the mission list"),
            );
            ui.end_row();
            ui.label("Area");
            egui::ComboBox::from_id_salt(("mission-area", k))
                .selected_text(m.area.clone())
                .show_ui(ui, |ui| {
                    let kept = std::slice::from_ref(&m.area).to_vec();
                    for (a, here) in with_missing(&o.names.areas, &kept) {
                        let text = if here {
                            a.clone()
                        } else {
                            format!("{a} (not in this game)")
                        };
                        ui.selectable_value(&mut m.area, a, text);
                    }
                });
            ui.end_row();
            ui.label("Time of day");
            ui.horizontal(|ui| time_ui(ui, m));
            ui.end_row();
            ui.label("Entry points")
                .on_hover_text("Player starts drawn on this area in the map editor.");
            names_menu(
                ui,
                ("mission-entries", k),
                &mut m.entries,
                &ids_on(o.spawns, &m.area),
                "none: draw a player start on this area",
            );
            ui.end_row();
            ui.label("Exit points")
                .on_hover_text("Extraction points drawn on this area in the map editor.");
            names_menu(
                ui,
                ("mission-exits", k),
                &mut m.exits,
                &ids_on(o.extractions, &m.area),
                "none: draw an extraction point on this area",
            );
            ui.end_row();
            ui.label("Animals on the map");
            names_menu(
                ui,
                ("mission-animals", k),
                &mut m.animals,
                &o.names.animals,
                "none",
            );
            ui.end_row();
            ui.label("Weapons");
            names_menu(
                ui,
                ("mission-weapons", k),
                &mut m.weapons,
                &o.names.weapons,
                "none",
            );
            ui.end_row();
            ui.label("Equipment");
            names_menu(
                ui,
                ("mission-equipment", k),
                &mut m.equipment,
                &equipment,
                "none",
            );
            ui.end_row();
            ui.label("Time limit");
            ui.horizontal(|ui| {
                let mut on = m.time_limit.is_some();
                if ui.checkbox(&mut on, "").changed() {
                    m.time_limit = on.then_some(600);
                }
                match &mut m.time_limit {
                    Some(s) => {
                        let (lo, hi) = fmt::LIMIT_RANGE;
                        ui.add(egui::DragValue::new(s).range(lo..=hi).suffix(" s"));
                        ui.label(RichText::new(format!("{}:{:02}", *s / 60, *s % 60)).color(DIM));
                    }
                    None => {
                        ui.label(RichText::new("none").color(DIM));
                    }
                }
            });
            ui.end_row();
            ui.label("Unlocked by")
                .on_hover_text("Missions to complete before this one is offered.");
            let others: Vec<String> = o
                .ids
                .iter()
                .filter(|id| !id.trim().eq_ignore_ascii_case(m.id.trim()))
                .cloned()
                .collect();
            names_menu(
                ui,
                ("mission-requires", k),
                &mut m.requires,
                &others,
                "nothing: offered from the start",
            );
            ui.end_row();
        });
    kills_ui(ui, k, m, &o.names.animals);
}

impl Extras {
    /// The Advanced page's mission editor.
    pub(super) fn missions_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let mut w = r.settings.world.clone().unwrap_or_default();
        let before = w.clone();
        ui.label(
            RichText::new("Missions (experimental)")
                .color(SAND)
                .strong(),
        );
        ui.label(
            RichText::new(
                "Editor and data only so far: missions are saved in the recipe and built into \
                 the game, and the game does not offer them yet. They will be listed by the DHC \
                 OS terminal only. Play Vanilla leaves them out.",
            )
            .color(GOLD),
        );
        help(
            ui,
            "A mission is a fixed challenge on one area: its animals, weapons and equipment are \
             set here, the hunter enters by one of its entry points, makes its kills and leaves \
             by one of its exit points. Entry and exit points are the player starts and \
             extraction points drawn in the map editor above.",
        );
        let terminal_on = crate::game_cfg::flag(crate::game_cfg::DHC_TERMINAL);
        if let Some(text) = terminal_warning(w.missions.len(), terminal_on) {
            ui.label(RichText::new(text).color(RED));
        }
        if !w.missions.is_empty() && !w.enabled {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        "World zones are not built into the game, so these are not either.",
                    )
                    .color(RED),
                );
                if ui.small_button("Build them").clicked() {
                    w.enabled = true;
                }
            });
        }
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        let list = |l: &[c2_mods::balance::Item]| -> Vec<String> {
            l.iter().map(|i| i.name.clone()).collect()
        };
        let names = Names {
            areas: list(&values.areas),
            animals: list(&values.animals),
            weapons: list(&values.weapons),
        };
        let ids: Vec<String> = w.missions.iter().map(|m| m.id.trim().to_string()).collect();
        let (spawns, extractions) = (w.spawns.clone(), w.extractions.clone());
        let offer = Offer {
            names: &names,
            ids: &ids,
            spawns: &spawns,
            extractions: &extractions,
        };
        let (mut gone, mut copy) = (None, None);
        for k in 0..w.missions.len() {
            let found = faults(&w, &names, k);
            let m = &mut w.missions[k];
            let label = if m.name.trim().is_empty() {
                m.id.clone()
            } else {
                m.name.clone()
            };
            let title = format!("{label}  ({})", m.area);
            let title = if found.is_empty() {
                RichText::new(title).color(TEXT)
            } else {
                RichText::new(format!("{title}  ⚠ {}", found.len())).color(GOLD)
            };
            egui::CollapsingHeader::new(title)
                .id_salt(("mission", k))
                .show(ui, |ui| {
                    mission_ui(ui, k, m, &offer);
                    for f in &found {
                        ui.label(RichText::new(f).color(GOLD));
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button("Duplicate").clicked() {
                            copy = Some(k);
                        }
                        if ui.small_button("Remove this mission").clicked() {
                            gone = Some(k);
                        }
                    });
                });
        }
        if let Some(k) = copy {
            duplicate(&mut w, k);
        }
        if let Some(k) = gone {
            remove(&mut w, k);
        }
        if names.areas.is_empty() {
            help(ui, "This game has no areas.");
        } else if ui.button("Add a mission").clicked() {
            // On the area the map editor shows, where its points were drawn.
            let area = if names.areas.contains(&self.world.area) {
                self.world.area.clone()
            } else {
                names.areas[0].clone()
            };
            let m = new_mission(&w, &area);
            w.missions.push(m);
            w.enabled = true;
        }
        if w != before {
            r.settings.world = (w != World::default()).then_some(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(id: &str, area: &str) -> Marker {
        Marker {
            id: id.into(),
            area: area.into(),
            at: Some([10.0, 10.0]),
        }
    }

    fn world() -> World {
        World {
            enabled: true,
            spawns: vec![marker("beach", "Ravan"), marker("gate", "Vengar")],
            extractions: vec![marker("pad", " ravan")],
            ..World::default()
        }
    }

    #[test]
    fn the_warning_shows_only_with_missions_and_the_terminal_off() {
        assert!(terminal_warning(0, false).is_none());
        assert!(terminal_warning(2, true).is_none());
        assert!(terminal_warning(1, false).is_some_and(|t| t.contains("hide")));
    }

    #[test]
    fn a_new_mission_takes_the_areas_points_and_a_free_id() {
        let mut w = world();
        let m = new_mission(&w, "Ravan");
        assert_eq!(m.id, "mission-1");
        assert_eq!(
            (m.entries.as_slice(), m.exits.as_slice()),
            (&["beach".to_string()][..], &["pad".to_string()][..])
        );
        assert_eq!(m.kills.len(), 1);
        w.missions.push(m);
        assert_eq!(new_mission(&w, "Vengar").id, "mission-2");
        // Complete as made: nothing for the validator but its empty lists.
        let names = Names {
            areas: vec!["Ravan".into()],
            ..Names::default()
        };
        let f = faults(&w, &names, 0);
        assert!(
            f.iter()
                .all(|x| x.starts_with("animals") || x.starts_with("weapons")),
            "{f:?}"
        );
    }

    /// The panel's rows are laid out in a headless frame without a clash
    /// of ids or a panic, for a mission with every list filled.
    #[test]
    fn a_full_mission_is_laid_out() {
        let mut w = world();
        let mut m = new_mission(&w, "Ravan");
        m.animals = vec!["Rex".into(), "Moa".into()];
        m.kills.push(Kill {
            name: "Moa".into(),
            count: 2,
        });
        m.time_limit = Some(600);
        w.missions = vec![m.clone(), m];
        let names = Names {
            areas: vec!["Ravan".into()],
            animals: vec!["Rex".into()],
            weapons: vec!["Pistol".into()],
        };
        let ids = vec!["mission-1".to_string()];
        let offer = Offer {
            names: &names,
            ids: &ids,
            spawns: &w.spawns.clone(),
            extractions: &w.extractions.clone(),
        };
        let ctx = egui::Context::default();
        let before = w.missions.clone();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            for (k, m) in w.missions.iter_mut().enumerate() {
                mission_ui(ui, k, m, &offer);
            }
        });
        out.textures_delta.clear();
        assert_eq!(w.missions, before, "drawing changes nothing");
    }

    #[test]
    fn duplicating_and_removing_keep_ids_and_requirements_sound() {
        let mut w = world();
        w.missions.push(new_mission(&w, "Ravan"));
        w.missions[0].name = "First".into();
        duplicate(&mut w, 0);
        assert_eq!(w.missions[1].id, "mission-2");
        assert_eq!(w.missions[1].name, "First (copy)");
        w.missions[1].requires = vec!["Mission-1".into()];
        remove(&mut w, 0);
        assert_eq!(w.missions.len(), 1);
        assert!(w.missions[0].requires.is_empty());
        remove(&mut w, 5);
        assert_eq!(w.missions.len(), 1);
    }
}
