//! A recipe's `[[settings.world.missions]]`: fixed challenges on an area
//! (an advanced setting, not in the original game). A mission pre-sets the
//! time of day, the animals on the map, the weapons and the equipment; the
//! hunter enters by one of its player starts, makes its kills and leaves by
//! one of its extraction points. Written into `HUNTDAT/_WORLD.TXT` with the
//! rest of `[settings.world]` ([`c2_formats::mission`]).
//!
//! Missions are offered from the DHC OS terminal only: with the original
//! menus they are hidden. The lists are the mission's whole loadout (an
//! empty `equipment`: none carried).
//!
//! ```toml
//! [[settings.world.missions]]
//! id = "rex-run"                   # letters, digits, - and _
//! name = "Rex run"
//! area = "Vengar Peninsula"        # as the balance editor names it
//! time = "night"                   # dawn, day (default) or night
//! entries = ["beach"]              # ids of [[settings.world.spawns]] there
//! exits = ["pad"]                  # ids of [[settings.world.extractions]]
//! animals = ["Tyrannosaurus Rex", "Velociraptor"]
//! weapons = ["Shotgun"]
//! equipment = ["Radar"]
//! time_limit = 600                 # seconds; unset: none
//! requires = ["first-steps"]       # missions to complete first
//! kills = [
//!     { name = "Tyrannosaurus Rex", count = 1 },
//!     { count = 3 },               # no name: any of the mission's animals
//! ]
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::mission as fmt;
use c2_formats::rules::TIMES;
use c2_formats::world::{self as world_fmt, valid_id};

use crate::validate::Names;
use crate::world::{Marker, World};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kill {
    /// The animal; empty: any of the mission's animals.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mission {
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The area's name, as the balance editor lists it.
    pub area: String,
    /// `dawn`, `day` or `night`; empty: day.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub time: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exits: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub weapons: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub equipment: Vec<String>,
    /// Seconds; unset: no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_limit: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kills: Vec<Kill>,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn clean(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in list.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if !out.iter().any(|o| same(o, s)) {
            out.push(s.into());
        }
    }
    out
}

/// The `ids` that name one of `markers` (by the marker's own spelling).
fn known(ids: &[String], markers: &[world_fmt::Marker]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if let Some(m) = markers.iter().find(|m| same(&m.id, id)) {
            if !out.contains(&m.id) {
                out.push(m.id.clone());
            }
        }
    }
    out
}

impl Mission {
    fn time(&self) -> Option<u8> {
        if self.time.trim().is_empty() {
            return Some(fmt::DEFAULT_TIME);
        }
        TIMES
            .iter()
            .position(|t| same(t, &self.time))
            .map(|t| t as u8)
    }
}

/// Puts `w`'s missions in `out` (which holds the zones and points already);
/// `map` gives an area name's map. Left out: a mission on an area the game
/// lacks, with an unusable or repeated id, or without a usable entry and
/// exit ([`check`] reports each).
pub(crate) fn to_format(
    w: &World,
    out: &mut world_fmt::World,
    map: impl Fn(&str) -> Option<String>,
) {
    for m in &w.missions {
        let id = m.id.trim();
        let Some(area) = map(&m.area) else { continue };
        if !valid_id(id) || out.mission(id).is_some() {
            continue;
        }
        let a = out.area_mut(&area);
        let (entries, exits) = (
            known(&m.entries, &a.spawns),
            known(&m.exits, &a.extractions),
        );
        if entries.is_empty() || exits.is_empty() {
            continue;
        }
        a.missions.push(fmt::Mission {
            id: id.into(),
            title: m.name.trim().into(),
            time: m.time().unwrap_or(fmt::DEFAULT_TIME),
            entries,
            exits,
            animals: clean(&m.animals),
            weapons: clean(&m.weapons),
            equipment: clean(&m.equipment),
            kills: m
                .kills
                .iter()
                .filter(|k| k.count > 0)
                .map(|k| fmt::Kill {
                    name: k.name.trim().into(),
                    count: k.count.clamp(fmt::KILL_RANGE.0, fmt::KILL_RANGE.1),
                })
                .collect(),
            time_limit: m
                .time_limit
                .filter(|s| *s > 0)
                .map_or(0, |s| s.clamp(fmt::LIMIT_RANGE.0, fmt::LIMIT_RANGE.1)),
            requires: clean(&m.requires),
        });
    }
    // A requirement on a mission that was left out can never be met.
    let ids: Vec<String> = out
        .areas
        .iter()
        .flat_map(|a| a.missions.iter().map(|m| m.id.clone()))
        .collect();
    for a in &mut out.areas {
        for m in &mut a.missions {
            let own = m.id.clone();
            m.requires
                .retain(|r| !same(r, &own) && ids.iter().any(|i| same(i, r)));
        }
    }
}

/// The missions that can never be offered: each needs, directly or through
/// others, a mission that needs it back.
fn in_cycle(missions: &[Mission]) -> Vec<usize> {
    let find = |id: &str| missions.iter().position(|m| same(&m.id, id));
    let reaches = |from: usize, to: usize| {
        let mut seen = vec![false; missions.len()];
        let mut todo = vec![from];
        while let Some(i) = todo.pop() {
            for r in missions[i].requires.iter().filter_map(|r| find(r)) {
                if r == to {
                    return true;
                }
                if !std::mem::replace(&mut seen[r], true) {
                    todo.push(r);
                }
            }
        }
        false
    };
    (0..missions.len()).filter(|&i| reaches(i, i)).collect()
}

/// What looks wrong in `w`'s missions, as warnings (`crate::validate`).
/// `names`: the built game's (none: names are not checked).
pub fn check(w: &World, names: Option<&Names>, warn: &mut dyn FnMut(String, String)) {
    const AT: &str = "settings.world.missions";
    if w.missions.is_empty() {
        return;
    }
    if !w.enabled {
        warn(
            "settings.world.enabled".into(),
            "missions are set but [settings.world] is off; they are not built into the game".into(),
        );
    }
    crate::world::id_faults(AT, w.missions.iter().map(|m| m.id.as_str()), warn);
    let listed = |list: &[String], name: &str| list.iter().any(|n| same(n, name));
    let on = |list: &[Marker], area: &str, id: &str| {
        list.iter()
            .any(|p| same(&p.area, area) && same(&p.id, id) && p.at.is_some())
    };
    for (i, m) in w.missions.iter().enumerate() {
        let at = format!("{AT}[{i}]");
        if m.area.trim().is_empty() {
            warn(format!("{at}.area"), "no area is given; skipped".into());
        } else if names.is_some_and(|n| !listed(&n.areas, &m.area)) {
            warn(
                format!("{at}.area"),
                format!("the game has no area named {:?}; skipped", m.area),
            );
        }
        if m.time().is_none() {
            warn(
                format!("{at}.time"),
                format!("{:?} is not dawn, day or night; day is used", m.time),
            );
        }
        for (key, ids, points, what) in [
            ("entries", &m.entries, &w.spawns, "player start"),
            ("exits", &m.exits, &w.extractions, "extraction point"),
        ] {
            for (j, id) in ids.iter().enumerate() {
                if !on(points, &m.area, id) {
                    warn(
                        format!("{at}.{key}[{j}]"),
                        format!("{:?} has no {what} called {id:?}; ignored", m.area.trim()),
                    );
                }
            }
            if !ids.iter().any(|id| on(points, &m.area, id)) {
                warn(
                    format!("{at}.{key}"),
                    format!("no usable {what}; the mission is skipped"),
                );
            }
        }
        for (key, list, game, what) in [
            ("animals", &m.animals, names.map(|n| &n.animals), "animal"),
            ("weapons", &m.weapons, names.map(|n| &n.weapons), "weapon"),
        ] {
            if list.is_empty() {
                warn(format!("{at}.{key}"), format!("no {what} is listed"));
            }
            for (j, n) in list.iter().enumerate() {
                if game.is_some_and(|g| !listed(g, n)) {
                    warn(
                        format!("{at}.{key}[{j}]"),
                        format!("the game has no {what} named {n:?}"),
                    );
                }
            }
        }
        for (j, e) in m.equipment.iter().enumerate() {
            if !crate::dhc::EQUIPMENT.iter().any(|x| same(x, e)) {
                warn(
                    format!("{at}.equipment[{j}]"),
                    format!("{e:?} is not an equipment name; ignored"),
                );
            }
        }
        if !m.kills.iter().any(|k| k.count > 0) {
            warn(
                format!("{at}.kills"),
                "no kill is required: reaching an exit completes the mission".into(),
            );
        }
        let (lo, hi) = fmt::KILL_RANGE;
        for (j, k) in m.kills.iter().enumerate() {
            if k.count == 0 {
                warn(
                    format!("{at}.kills[{j}].count"),
                    "0 kills; this requirement is skipped".into(),
                );
            } else if k.count > hi {
                warn(
                    format!("{at}.kills[{j}].count"),
                    format!("{} is outside {lo}..={hi}; {hi} is used", k.count),
                );
            }
            if !k.name.trim().is_empty() && !m.animals.is_empty() && !listed(&m.animals, &k.name) {
                warn(
                    format!("{at}.kills[{j}].name"),
                    format!(
                        "{:?} is not among the mission's animals; it can't be met",
                        k.name
                    ),
                );
            }
        }
        if let Some(s) = m.time_limit.filter(|s| *s > 0) {
            let (lo, hi) = fmt::LIMIT_RANGE;
            if !(lo..=hi).contains(&s) {
                warn(
                    format!("{at}.time_limit"),
                    format!(
                        "{s} is outside {lo}..={hi} seconds; {} is used",
                        s.clamp(lo, hi)
                    ),
                );
            }
        }
        for (j, r) in m.requires.iter().enumerate() {
            if same(r, &m.id) {
                warn(
                    format!("{at}.requires[{j}]"),
                    "a mission can't require itself; ignored".into(),
                );
            } else if !w.missions.iter().any(|o| same(&o.id, r)) {
                warn(
                    format!("{at}.requires[{j}]"),
                    format!("no mission is called {r:?}; ignored"),
                );
            }
        }
        gap(w, m, &at, warn);
    }
    for i in in_cycle(&w.missions) {
        warn(
            format!("{AT}[{i}].requires"),
            "it requires a mission that requires it back; it can never be offered".into(),
        );
    }
    let open = |m: &Mission| {
        !m.requires
            .iter()
            .any(|r| !same(r, &m.id) && w.missions.iter().any(|o| same(&o.id, r)))
    };
    if !w.missions.iter().any(open) {
        warn(
            AT.into(),
            "every mission requires another: none is offered at the start".into(),
        );
    }
}

/// The extraction gap rule: one of the mission's entries must be far enough
/// from one of its exits.
fn gap(w: &World, m: &Mission, at: &str, warn: &mut dyn FnMut(String, String)) {
    let side = c2_formats::map::MAP_SIZE as f64;
    let Some(gap) = w
        .min_extraction_gap
        .filter(|g| g.is_finite() && *g > 0.0)
        .map(|g| g.min(f64::from(world_fmt::GAP_RANGE.1)) * side)
    else {
        return;
    };
    let points = |ids: &[String], list: &[Marker]| -> Vec<[f64; 2]> {
        list.iter()
            .filter(|p| same(&p.area, &m.area) && ids.iter().any(|i| same(i, &p.id)))
            .filter_map(|p| p.at)
            .collect()
    };
    let (starts, ends) = (
        points(&m.entries, &w.spawns),
        points(&m.exits, &w.extractions),
    );
    if starts.is_empty() || ends.is_empty() {
        return;
    }
    let far = |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) >= gap;
    if !starts.iter().any(|a| ends.iter().any(|b| far(a, b))) {
        warn(
            format!("{at}.exits"),
            format!("no exit is {gap} cells or more from an entry (min_extraction_gap)"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECTION: &str = r#"
        enabled = true
        [[spawns]]
        id = "beach"
        area = "Ravan"
        at = [100.0, 200.0]
        [[spawns]]
        id = "far"
        area = "Vengar"
        at = [10.0, 10.0]
        [[extractions]]
        id = "pad"
        area = "Ravan"
        at = [300.0, 400.0]
        [[missions]]
        id = "rex-run"
        name = "Rex run"
        area = "ravan"
        time = "Night"
        entries = ["Beach", "far", "nowhere"]
        exits = ["pad"]
        animals = ["Tyrannosaurus Rex", " Velociraptor ", "velociraptor"]
        weapons = ["Shotgun"]
        equipment = ["Radar"]
        time_limit = 5
        requires = ["first", "rex-run", "gone"]
        kills = [{ name = "Tyrannosaurus Rex", count = 1 }, { count = 300 }, { count = 0 }]
        [[missions]]
        id = "first"
        area = "Ravan"
        entries = ["beach"]
        exits = ["pad"]
        [[missions]]
        id = "gone"
        area = "Atlantis"
        entries = ["beach"]
        exits = ["pad"]
        [[missions]]
        id = "no-exit"
        area = "Ravan"
        entries = ["beach"]
    "#;

    fn section() -> World {
        toml::from_str(SECTION).unwrap_or_else(|e| panic!("{e}"))
    }

    fn maps() -> Vec<(String, String)> {
        vec![
            ("Vengar".to_string(), "area1".to_string()),
            ("Ravan".to_string(), "area2".to_string()),
        ]
    }

    #[test]
    fn missions_are_written_with_what_the_game_can_use() {
        let f = section().to_format(&maps());
        let a = f.area("area2").expect("area2");
        let ids: Vec<&str> = a.missions.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["rex-run", "first"]);
        let m = &a.missions[0];
        assert_eq!((m.title.as_str(), m.time), ("Rex run", 2));
        // Entries of another area and unknown ids are dropped; ids take the
        // point's spelling.
        assert_eq!(m.entries, ["beach"]);
        assert_eq!(m.animals, ["Tyrannosaurus Rex", "Velociraptor"]);
        assert_eq!(m.kills.len(), 2);
        assert_eq!(m.kills[1].count, fmt::KILL_RANGE.1);
        assert_eq!(m.time_limit, fmt::LIMIT_RANGE.0);
        // Itself and the mission that was left out are not requirements.
        assert_eq!(m.requires, ["first"]);
        assert_eq!(a.missions[1].time, fmt::DEFAULT_TIME);
        let again = world_fmt::World::parse(f.to_text().as_bytes());
        assert_eq!(again, f);
    }

    #[test]
    fn missions_alone_make_the_section_active_and_survive_toml() {
        let w = section();
        let text = toml::to_string(&w).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(toml::from_str::<World>(&text).ok(), Some(w));
        let only = World {
            enabled: true,
            missions: vec![Mission::default()],
            ..World::default()
        };
        assert!(only.is_active());
    }

    fn keys(w: &World, names: Option<&Names>) -> Vec<String> {
        let mut out = Vec::new();
        check(w, names, &mut |k, _| out.push(k));
        out
    }

    #[test]
    fn faults_are_reported_by_key() {
        let names = Names {
            areas: vec!["Vengar".into(), "Ravan".into()],
            animals: vec!["Tyrannosaurus Rex".into(), "Velociraptor".into()],
            weapons: vec!["Pistol".into()],
        };
        let mut w = section();
        w.missions[0].time = "dusk".into();
        w.missions[0].equipment.push("Jetpack".into());
        w.missions[0].kills.push(Kill {
            name: "Moa".into(),
            count: 1,
        });
        let k = keys(&w, Some(&names));
        const AT: &str = "settings.world.missions";
        for want in [
            "[0].time",
            "[0].entries[1]",
            "[0].entries[2]",
            "[0].weapons[0]",
            "[0].equipment[1]",
            "[0].time_limit",
            "[0].requires[1]",
            "[0].kills[1].count",
            "[0].kills[2].count",
            "[0].kills[3].name",
            "[1].animals",
            "[1].kills",
            "[2].area",
            "[3].exits",
        ] {
            assert!(k.contains(&format!("{AT}{want}")), "{want}: {k:?}");
        }
        assert!(!k.contains(&format!("{AT}[0].entries[0]")), "{k:?}");
        assert!(!k.contains(&"settings.world.enabled".to_string()));
        w.enabled = false;
        assert!(keys(&w, None).contains(&"settings.world.enabled".to_string()));
    }

    #[test]
    fn requirement_cycles_and_the_gap_rule_are_reported() {
        let mut w = section();
        w.missions.truncate(2);
        w.missions[0].requires = vec!["first".into()];
        w.missions[1].requires = vec!["rex-run".into()];
        let k = keys(&w, None);
        const AT: &str = "settings.world.missions";
        assert!(k.contains(&format!("{AT}[0].requires")), "{k:?}");
        assert!(k.contains(&format!("{AT}[1].requires")), "{k:?}");
        assert!(k.contains(&AT.to_string()), "{k:?}");
        w.missions[1].requires.clear();
        let k = keys(&w, None);
        assert!(!k.contains(&format!("{AT}[0].requires")) && !k.contains(&AT.to_string()));
        // beach to pad is about 283 cells: under half the map's side.
        assert!(!k.contains(&format!("{AT}[0].exits")));
        w.min_extraction_gap = Some(0.5);
        assert!(keys(&w, None).contains(&format!("{AT}[0].exits")));
    }
}
