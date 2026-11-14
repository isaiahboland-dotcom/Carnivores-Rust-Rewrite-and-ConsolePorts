//! Missions in `HUNTDAT/_WORLD.TXT` ([`crate::world`]): fixed challenges on
//! an area (not in the original game). A mission pre-sets the time of day,
//! the animals, the weapons and the equipment, names the player starts the
//! hunter may enter by and the extraction points he may leave by, and asks
//! for kills before he leaves.
//!
//! A `mission <id>` line inside an area starts a mission; the lines after
//! it are its own until the next entry. Names are the rest of the line, as
//! the menus name them (case ignored).
//!
//! ```text
//! area area1
//! spawn beach 100 200
//! extraction pad 300 400
//! mission rex-run
//! title Rex run
//! time night               ; dawn, day (default) or night
//! entry beach              ; a spawn id of this area
//! exit pad                 ; an extraction id of this area
//! animal Tyrannosaurus Rex
//! weapon Shotgun
//! equipment Radar
//! kill 1 Tyrannosaurus Rex ; count, then the animal
//! kill 3                   ; no name: any of the mission's animals
//! limit 600                ; seconds; absent: no time limit
//! requires first-steps     ; a mission to complete first
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use crate::rules::TIMES;
use crate::world::valid_id;

/// Accepted kills per requirement.
pub const KILL_RANGE: (u32, u32) = (1, 99);
/// Accepted time limit, seconds.
pub const LIMIT_RANGE: (u32, u32) = (30, 86_400);
/// The time of day when a mission names none: day.
pub const DEFAULT_TIME: u8 = 1;

/// A kill requirement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kill {
    /// The animal; empty: any of the mission's animals.
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub id: String,
    /// The name shown; empty: the id.
    pub title: String,
    /// Index into [`TIMES`].
    pub time: u8,
    /// Ids of the area's player starts the hunt may begin at.
    pub entries: Vec<String>,
    /// Ids of the area's extraction points the hunter may leave by.
    pub exits: Vec<String>,
    pub animals: Vec<String>,
    pub weapons: Vec<String>,
    pub equipment: Vec<String>,
    pub kills: Vec<Kill>,
    /// Seconds; 0: no limit.
    pub time_limit: u32,
    /// Missions to complete before this one is offered.
    pub requires: Vec<String>,
}

impl Default for Mission {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            time: DEFAULT_TIME,
            entries: Vec::new(),
            exits: Vec::new(),
            animals: Vec::new(),
            weapons: Vec::new(),
            equipment: Vec::new(),
            kills: Vec::new(),
            time_limit: 0,
            requires: Vec::new(),
        }
    }
}

fn push_id(list: &mut Vec<String>, id: &str) {
    if valid_id(id) && !list.iter().any(|x| x.eq_ignore_ascii_case(id)) {
        list.push(id.into());
    }
}

impl Mission {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }

    /// The name to show.
    pub fn label(&self) -> &str {
        if self.title.is_empty() {
            &self.id
        } else {
            &self.title
        }
    }

    /// The kills asked of animal `name` alone (requirements for any animal
    /// not counted).
    pub fn kills_of(&self, name: &str) -> u32 {
        self.kills
            .iter()
            .filter(|k| k.name.trim().eq_ignore_ascii_case(name.trim()))
            .map(|k| k.count)
            .sum()
    }

    /// Reads one of the mission's own lines. False: `key` is not one (bad
    /// values of a known key are skipped, and still true).
    pub(crate) fn line(&mut self, key: &str, rest: &str) -> bool {
        match key {
            "title" => self.title = rest.into(),
            "time" => {
                if let Some(t) = TIMES.iter().position(|t| t.eq_ignore_ascii_case(rest)) {
                    self.time = t as u8;
                }
            }
            "entry" => push_id(&mut self.entries, rest),
            "exit" => push_id(&mut self.exits, rest),
            "requires" => push_id(&mut self.requires, rest),
            "animal" if !rest.is_empty() => self.animals.push(rest.into()),
            "weapon" if !rest.is_empty() => self.weapons.push(rest.into()),
            "equipment" if !rest.is_empty() => self.equipment.push(rest.into()),
            "animal" | "weapon" | "equipment" => {}
            "kill" => {
                let (count, name) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                if let Some(n) = count.parse::<u32>().ok().filter(|n| *n > 0) {
                    self.kills.push(Kill {
                        name: name.trim().into(),
                        count: n.clamp(KILL_RANGE.0, KILL_RANGE.1),
                    });
                }
            }
            "limit" => {
                if let Some(s) = rest.parse::<u32>().ok().filter(|s| *s > 0) {
                    self.time_limit = s.clamp(LIMIT_RANGE.0, LIMIT_RANGE.1);
                }
            }
            _ => return false,
        }
        true
    }

    /// The mission's lines.
    pub(crate) fn write(&self, s: &mut String) {
        s.push_str(&format!("mission {}\n", self.id));
        if !self.title.is_empty() {
            s.push_str(&format!("title {}\n", self.title));
        }
        if let Some(t) = TIMES.get(usize::from(self.time)) {
            if self.time != DEFAULT_TIME {
                s.push_str(&format!("time {t}\n"));
            }
        }
        for (key, list) in [
            ("entry", &self.entries),
            ("exit", &self.exits),
            ("animal", &self.animals),
            ("weapon", &self.weapons),
            ("equipment", &self.equipment),
        ] {
            for n in list {
                s.push_str(&format!("{key} {n}\n"));
            }
        }
        for k in &self.kills {
            s.push_str(format!("kill {} {}", k.count, k.name).trim_end());
            s.push('\n');
        }
        if self.time_limit > 0 {
            s.push_str(&format!("limit {}\n", self.time_limit));
        }
        for r in &self.requires {
            s.push_str(&format!("requires {r}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;
    use alloc::vec;

    fn sample() -> Mission {
        Mission {
            id: "rex-run".into(),
            title: "Rex run".into(),
            time: 2,
            entries: vec!["beach".into(), "cove".into()],
            exits: vec!["pad".into()],
            animals: vec!["Tyrannosaurus Rex".into(), "Velociraptor".into()],
            weapons: vec!["Shotgun".into()],
            equipment: vec!["Radar".into()],
            kills: vec![
                Kill {
                    name: "Tyrannosaurus Rex".into(),
                    count: 1,
                },
                Kill {
                    name: String::new(),
                    count: 3,
                },
            ],
            time_limit: 600,
            requires: vec!["first".into()],
        }
    }

    #[test]
    fn missions_round_trip_inside_their_area() {
        let mut w = World::default();
        w.area_mut("area1").missions.push(sample());
        w.area_mut("area1").missions.push(Mission::new("first"));
        w.area_mut("area2").missions.push(Mission::new("other"));
        let text = w.to_text();
        assert_eq!(World::parse(text.as_bytes()), w);
        assert_eq!(w.mission("REX-RUN").map(|(m, _)| m), Some("area1"));
        assert_eq!(sample().kills_of("tyrannosaurus rex"), 1);
        assert_eq!(Mission::new("x").label(), "x");
    }

    #[test]
    fn bad_lines_are_skipped_and_values_clamped() {
        let text = "mission early\n\
                    area area1\n\
                    mission a\n\
                    time dusk\n\
                    kill 0 Moa\n\
                    kill 500 Moa\n\
                    kill many Moa\n\
                    limit 5\n\
                    entry two words\n\
                    entry beach\n\
                    entry BEACH\n\
                    future thing\n\
                    exit pad\n\
                    mission a\n\
                    title Second of the name\n\
                    mission not an id\n\
                    title Lost\n\
                    spawn beach 1 2\n\
                    title After a point\n";
        let w = World::parse(text.as_bytes());
        let a = &w.areas[0];
        assert_eq!(a.missions.len(), 1, "{:?}", a.missions);
        let m = &a.missions[0];
        // A repeated or unusable `mission` line takes its own lines with it.
        assert_eq!(m.title, "");
        assert_eq!(m.time, DEFAULT_TIME);
        assert_eq!(
            m.kills,
            vec![Kill {
                name: "Moa".into(),
                count: KILL_RANGE.1
            }]
        );
        assert_eq!(m.time_limit, LIMIT_RANGE.0);
        assert_eq!(m.entries, vec![String::from("beach")]);
        // An unknown line does not end the mission.
        assert_eq!(m.exits, vec![String::from("pad")]);
        assert_eq!(a.spawns.len(), 1);
    }
}
