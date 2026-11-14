//! `HUNTDAT/_WORLD.TXT`: zones, player starts and extraction points on the
//! areas' maps (not in the original game). The build writes it from a
//! recipe's `[settings.world]`; it is never edited by hand. When the file is
//! missing, hunts play as they always have.
//!
//! One entry per line; `;` starts a comment. An `area <map>` line (the map's
//! name: `area3`, or a `.c2map`'s stem; the rest of the line, case ignored)
//! starts that area's entries. Coordinates are map cells as floats (world
//! units / 256), x then z. Ids and tags are single words of letters, digits,
//! `-` and `_`. A `tags` line belongs to the zone before it. A `mission`
//! line starts a mission ([`crate::mission`]), whose own lines follow it.
//!
//! ```text
//! min_extraction_gap 0.25      ; fraction of the map's side
//! area area1
//! zone marsh circle 512 300.5 40
//! tags no_spawn
//! zone ridge polygon 100 100 200 100 150 220
//! spawn beach 100 200
//! extraction pad 300 400
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use crate::mission::Mission;
use crate::zone::{Point, Shape};

/// The path the entries are read from.
pub const PATH: &str = "HUNTDAT/_WORLD.TXT";

/// Accepted `min_extraction_gap`, as a fraction of the map's side.
pub const GAP_RANGE: (f32, f32) = (0.0, 1.0);
/// Longest id or tag, characters.
pub const MAX_ID: usize = 32;

/// Whether `s` can be an id or a tag: one word of ASCII letters, digits,
/// `-` and `_`, at most [`MAX_ID`] long.
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_ID
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub id: String,
    pub shape: Shape,
    pub tags: Vec<String>,
}

impl Zone {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }
}

/// A named point: a player start or an extraction point.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub id: String,
    pub at: Point,
}

/// One area's entries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Area {
    /// The map's name (`area3`, a `.c2map`'s stem).
    pub map: String,
    pub zones: Vec<Zone>,
    pub spawns: Vec<Marker>,
    pub extractions: Vec<Marker>,
    pub missions: Vec<Mission>,
}

impl Area {
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
            && self.spawns.is_empty()
            && self.extractions.is_empty()
            && self.missions.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct World {
    /// Least distance between a player start and an extraction point, as a
    /// fraction of the map's side; none: no rule.
    pub min_extraction_gap: Option<f32>,
    pub areas: Vec<Area>,
}

fn coords(words: &[&str]) -> Option<Vec<f32>> {
    words
        .iter()
        .map(|w| w.parse::<f32>().ok().filter(|v| v.is_finite()))
        .collect()
}

fn shape(kind: &str, words: &[&str]) -> Option<Shape> {
    let v = coords(words)?;
    let s = match kind.to_ascii_lowercase().as_str() {
        "circle" if v.len() == 3 => Shape::Circle {
            center: [v[0], v[1]],
            radius: v[2],
        },
        "polygon" if v.len() % 2 == 0 => {
            Shape::Polygon((0..v.len() / 2).map(|i| [v[2 * i], v[2 * i + 1]]).collect())
        }
        _ => return None,
    };
    s.check().ok().map(|()| s)
}

fn marker(rest: &str) -> Option<Marker> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let [id, x, z] = words[..] else { return None };
    let v = coords(&[x, z])?;
    valid_id(id).then(|| Marker {
        id: id.into(),
        at: [v[0], v[1]],
    })
}

impl World {
    pub fn is_empty(&self) -> bool {
        self.min_extraction_gap.is_none() && self.areas.iter().all(Area::is_empty)
    }

    /// The entries of the area whose map is `map` (case ignored).
    pub fn area(&self, map: &str) -> Option<&Area> {
        let map = map.trim();
        self.areas
            .iter()
            .find(|a| a.map.trim().eq_ignore_ascii_case(map))
    }

    /// The area for `map`, added when new.
    pub fn area_mut(&mut self, map: &str) -> &mut Area {
        let map = map.trim();
        let i = match self
            .areas
            .iter()
            .position(|a| a.map.eq_ignore_ascii_case(map))
        {
            Some(i) => i,
            None => {
                self.areas.push(Area {
                    map: map.into(),
                    ..Area::default()
                });
                self.areas.len() - 1
            }
        };
        &mut self.areas[i]
    }

    /// The zone called `id` (case ignored) and the map it is on.
    pub fn zone(&self, id: &str) -> Option<(&str, &Zone)> {
        self.areas.iter().find_map(|a| {
            let z = a.zones.iter().find(|z| z.id.eq_ignore_ascii_case(id))?;
            Some((a.map.as_str(), z))
        })
    }

    /// The mission called `id` (case ignored) and the map it is on.
    pub fn mission(&self, id: &str) -> Option<(&str, &Mission)> {
        self.areas.iter().find_map(|a| {
            let m = a.missions.iter().find(|m| m.id.eq_ignore_ascii_case(id))?;
            Some((a.map.as_str(), m))
        })
    }

    /// Reads the file. Unknown keys, unusable shapes, entries before the
    /// first `area` line and repeated ids (the first is kept) are skipped,
    /// so a newer file still loads.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut w = Self::default();
        let mut current: Option<String> = None;
        // Whether the last entry line added a zone (for `tags`).
        let mut zone_open = false;
        // Whether the last entry line started a mission (for its own lines).
        let mut mission_open = false;
        let taken = |list: &[&Vec<Marker>], id: &str| {
            list.iter()
                .any(|l| l.iter().any(|m| m.id.eq_ignore_ascii_case(id)))
        };
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let Some((key, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let (key, rest) = (key.to_ascii_lowercase(), rest.trim());
            if mission_open {
                let open = current
                    .as_deref()
                    .and_then(|m| w.area_mut(m).missions.last_mut());
                if open.is_some_and(|m| m.line(&key, rest)) {
                    continue;
                }
            }
            if key == "tags" {
                let zone = current
                    .as_deref()
                    .filter(|_| zone_open)
                    .and_then(|m| w.area_mut(m).zones.last_mut());
                if let Some(z) = zone {
                    for t in rest.split_whitespace().filter(|t| valid_id(t)) {
                        if !z.has_tag(t) {
                            z.tags.push(t.to_ascii_lowercase());
                        }
                    }
                }
                continue;
            }
            zone_open = false;
            if matches!(
                key.as_str(),
                "area" | "zone" | "spawn" | "extraction" | "mission"
            ) {
                mission_open = false;
            }
            match key.as_str() {
                "mission" => {
                    let Some(map) = current.as_deref() else {
                        continue;
                    };
                    if valid_id(rest) && w.mission(rest).is_none() {
                        w.area_mut(map).missions.push(Mission::new(rest));
                        mission_open = true;
                    }
                }
                "min_extraction_gap" => {
                    if let Some(v) = rest.parse::<f32>().ok().filter(|v| v.is_finite()) {
                        w.min_extraction_gap = Some(v.clamp(GAP_RANGE.0, GAP_RANGE.1));
                    }
                }
                "area" => current = (!rest.is_empty()).then(|| rest.into()),
                "zone" => {
                    let words: Vec<&str> = rest.split_whitespace().collect();
                    let (Some(map), [id, kind, nums @ ..]) = (current.as_deref(), &words[..])
                    else {
                        continue;
                    };
                    if !valid_id(id) || w.zone(id).is_some() {
                        continue;
                    }
                    if let Some(shape) = shape(kind, nums) {
                        w.area_mut(map).zones.push(Zone {
                            id: (*id).into(),
                            shape,
                            tags: Vec::new(),
                        });
                        zone_open = true;
                    }
                }
                "spawn" | "extraction" => {
                    let (Some(map), Some(m)) = (current.as_deref(), marker(rest)) else {
                        continue;
                    };
                    let spawn = key == "spawn";
                    let all: Vec<&Vec<Marker>> = w
                        .areas
                        .iter()
                        .map(|a| if spawn { &a.spawns } else { &a.extractions })
                        .collect();
                    if taken(&all, &m.id) {
                        continue;
                    }
                    let a = w.area_mut(map);
                    if spawn {
                        a.spawns.push(m);
                    } else {
                        a.extractions.push(m);
                    }
                }
                _ => {}
            }
        }
        w.areas.retain(|a| !a.is_empty());
        w
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut s = String::from("; World zones (from the recipe's [settings.world])\n");
        if let Some(g) = self.min_extraction_gap {
            s.push_str(&format!("min_extraction_gap {g}\n"));
        }
        for a in self.areas.iter().filter(|a| !a.is_empty()) {
            s.push_str(&format!("area {}\n", a.map));
            for z in &a.zones {
                match &z.shape {
                    Shape::Circle { center, radius } => s.push_str(&format!(
                        "zone {} circle {} {} {radius}\n",
                        z.id, center[0], center[1]
                    )),
                    Shape::Polygon(points) => {
                        s.push_str(&format!("zone {} polygon", z.id));
                        for p in points {
                            s.push_str(&format!(" {} {}", p[0], p[1]));
                        }
                        s.push('\n');
                    }
                }
                if !z.tags.is_empty() {
                    s.push_str(&format!("tags {}\n", z.tags.join(" ")));
                }
            }
            for (key, list) in [("spawn", &a.spawns), ("extraction", &a.extractions)] {
                for m in list {
                    s.push_str(&format!("{key} {} {} {}\n", m.id, m.at[0], m.at[1]));
                }
            }
            for m in &a.missions {
                m.write(&mut s);
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn sample() -> World {
        World {
            min_extraction_gap: Some(0.25),
            areas: vec![
                Area {
                    map: "area1".into(),
                    zones: vec![
                        Zone {
                            id: "marsh".into(),
                            shape: Shape::Circle {
                                center: [512.0, 300.5],
                                radius: 40.0,
                            },
                            tags: vec!["no_spawn".into(), "wet".into()],
                        },
                        Zone {
                            id: "ridge".into(),
                            shape: Shape::Polygon(vec![
                                [100.0, 100.0],
                                [200.0, 100.0],
                                [150.1, 220.0],
                            ]),
                            tags: Vec::new(),
                        },
                    ],
                    spawns: vec![Marker {
                        id: "beach".into(),
                        at: [100.0, 200.0],
                    }],
                    extractions: vec![Marker {
                        id: "pad".into(),
                        at: [300.25, 400.0],
                    }],
                    missions: Vec::new(),
                },
                Area {
                    map: "Lost Valley".into(),
                    zones: Vec::new(),
                    spawns: vec![Marker {
                        id: "gate".into(),
                        at: [10.0, 20.0],
                    }],
                    extractions: Vec::new(),
                    missions: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn round_trips_through_text() {
        let w = sample();
        assert_eq!(World::parse(w.to_text().as_bytes()), w);
        assert_eq!(World::parse(b""), World::default());
        assert!(World::default().is_empty() && !w.is_empty());
    }

    #[test]
    fn areas_and_zones_are_found_whatever_the_case() {
        let w = sample();
        assert_eq!(w.area("AREA1").map(|a| a.zones.len()), Some(2));
        assert_eq!(w.area(" lost valley ").map(|a| a.spawns.len()), Some(1));
        assert!(w.area("area2").is_none());
        let (map, z) = w.zone("Marsh").unwrap_or_else(|| panic!("no zone"));
        assert_eq!(map, "area1");
        assert!(z.has_tag("NO_SPAWN") && !z.has_tag("dry"));
        assert!(z.shape.contains(512.0, 320.0));
    }

    #[test]
    fn junk_is_skipped_and_the_gap_clamped() {
        let w = World::parse(
            b"min_extraction_gap 7 ; too far\n\
              zone early circle 1 1 5\n\
              bogus 1\n\
              area area2\n\
              tags orphan\n\
              zone a circle 10 10 5\n\
              zone a circle 20 20 5 ; the id again\n\
              zone bad! circle 10 10 5\n\
              zone bow polygon 0 0 10 10 10 0 0 10\n\
              zone odd polygon 0 0 10 10 10\n\
              zone nan circle x 10 5\n\
              zone tiny circle 10 10 0.1\n\
              spawn s 1 2\n\
              spawn s 3 4\n\
              spawn far 1\n\
              tags late\n\
              extraction s 5 6\n\
              area area3\n\
              zone A circle 1 1 5\n",
        );
        assert_eq!(w.min_extraction_gap, Some(GAP_RANGE.1));
        assert_eq!(w.areas.len(), 1, "area3 kept nothing: {w:?}");
        let a = &w.areas[0];
        assert_eq!(a.zones.len(), 1);
        assert_eq!(a.zones[0].id, "a");
        assert!(a.zones[0].tags.is_empty(), "tags only follow their zone");
        assert_eq!(a.spawns.len(), 1);
        assert_eq!(a.spawns[0].at, [1.0, 2.0]);
        // A start and an extraction point may share a name.
        assert_eq!(a.extractions.len(), 1);
    }

    #[test]
    fn ids_and_tags_are_single_plain_words() {
        for ok in ["a", "north-marsh", "Zone_2", &"x".repeat(MAX_ID)] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "two words", "caf\u{e9}", "a;b", &"x".repeat(MAX_ID + 1)] {
            assert!(!valid_id(bad), "{bad}");
        }
    }
}
