//! A recipe's `[settings.world]`: zones, player starts and extraction points
//! drawn on the areas' maps (an advanced setting, not in the original game).
//! The recipe is where they are written and shared; the build derives
//! `HUNTDAT/_WORLD.TXT` ([`c2_formats::world`]) from it, because packs on
//! the consoles and Android never see the recipe. Opt-in: absent, off or
//! empty, nothing is written and the build is unchanged.
//!
//! Coordinates are map cells as floats (world units / 256), `[x, z]`.
//!
//! ```toml
//! [settings.world]
//! enabled = true
//! min_extraction_gap = 0.25    # fraction of the map's side; unset: no rule
//!
//! [[settings.world.zones]]
//! id = "marsh"                 # letters, digits, - and _
//! area = "Vengar Peninsula"    # as the balance editor names it
//! shape = "circle"
//! center = [512.0, 300.5]
//! radius = 40.0
//! tags = ["no_spawn"]
//!
//! [[settings.world.zones]]
//! id = "ridge"
//! area = "Vengar Peninsula"
//! shape = "polygon"
//! points = [[100.0, 100.0], [200.0, 100.0], [150.0, 220.0]]
//!
//! [[settings.world.spawns]]
//! id = "beach"
//! area = "Vengar Peninsula"
//! at = [100.0, 200.0]
//!
//! [[settings.world.extractions]]
//! id = "pad"
//! area = "Vengar Peninsula"
//! at = [300.0, 400.0]
//! ```
//!
//! `[[settings.world.missions]]` (fixed challenges that use these points)
//! is in [`crate::mission`]; `[settings.world.ecosystem]` (which animals
//! appear in which zone) is in [`crate::ecosystem`].

use serde::{Deserialize, Serialize};

use c2_formats::map::MAP_SIZE;
use c2_formats::world as fmt;
use c2_formats::zone::Shape;
use c2_vfs::{MemBackend, Vfs};

use crate::ModError;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct World {
    pub enabled: bool,
    /// Least distance between a player start and an extraction point, as a
    /// fraction of the map's side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_extraction_gap: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spawns: Vec<Marker>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extractions: Vec<Marker>,
    /// Fixed challenges ([`crate::mission`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missions: Vec<crate::mission::Mission>,
    /// Spawn rules by zone ([`crate::ecosystem`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<crate::ecosystem::Ecosystem>,
}

/// A zone as the recipe holds it. One plain table (not an enum by shape) so
/// a mistyped key is reported rather than dropped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Zone {
    pub id: String,
    /// The area's name, as the balance editor lists it.
    pub area: String,
    /// `circle` (with `center` and `radius`) or `polygon` (with `points`).
    pub shape: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f64; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A player start or an extraction point.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Marker {
    pub id: String,
    pub area: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
}

fn point(p: [f64; 2]) -> [f32; 2] {
    [p[0] as f32, p[1] as f32]
}

fn on_map(p: [f32; 2]) -> bool {
    let side = MAP_SIZE as f32;
    p.iter().all(|v| (0.0..=side).contains(v))
}

impl Zone {
    /// The zone's shape, or what is wrong with it.
    pub fn shape(&self) -> Result<Shape, String> {
        let s = match self.shape.trim().to_ascii_lowercase().as_str() {
            "circle" => match (self.center, self.radius) {
                (Some(c), Some(r)) => Shape::Circle {
                    center: point(c),
                    radius: r as f32,
                },
                _ => return Err("a circle needs center and radius".into()),
            },
            "polygon" => Shape::Polygon(self.points.iter().copied().map(point).collect()),
            other => return Err(format!("{other:?} is not circle or polygon")),
        };
        s.check().map_err(|e| e.to_string())?;
        Ok(s)
    }
}

impl Marker {
    fn at(&self) -> Option<[f32; 2]> {
        self.at
            .map(point)
            .filter(|p| p.iter().all(|v| v.is_finite()))
    }
}

fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags.iter().map(|t| t.trim().to_ascii_lowercase()) {
        if fmt::valid_id(&t) && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

impl World {
    /// Whether the section changes the build: on, with something in it.
    pub fn is_active(&self) -> bool {
        self.enabled
            && (!self.zones.is_empty()
                || !self.spawns.is_empty()
                || !self.extractions.is_empty()
                || !self.missions.is_empty()
                || self.ecosystem.as_ref().is_some_and(|e| e.is_active()))
    }

    /// The file's form; `maps` gives each area name's map (areas are given
    /// by name only, so the validator can check them). Entries that
    /// can't be used are left out: an area the game lacks, an id that is not
    /// one plain word or is already taken, an unusable shape, a point with
    /// no position ([`check`] reports each).
    pub fn to_format(&self, maps: &[(String, String)]) -> fmt::World {
        let map = |name: &str| {
            let name = name.trim();
            maps.iter()
                .find(|(n, _)| n.trim().eq_ignore_ascii_case(name))
                .map(|(_, m)| m.clone())
        };
        let mut w = fmt::World {
            min_extraction_gap: self
                .min_extraction_gap
                .filter(|g| g.is_finite())
                .map(|g| (g as f32).clamp(fmt::GAP_RANGE.0, fmt::GAP_RANGE.1)),
            caps: Default::default(),
            safety: Default::default(),
            areas: Vec::new(),
        };
        for z in &self.zones {
            let id = z.id.trim();
            let (Some(m), Ok(shape)) = (map(&z.area), z.shape()) else {
                continue;
            };
            if fmt::valid_id(id) && w.zone(id).is_none() {
                w.area_mut(&m).zones.push(fmt::Zone {
                    id: id.into(),
                    shape,
                    tags: clean_tags(&z.tags),
                    rule: Default::default(),
                });
            }
        }
        for (list, spawn) in [(&self.spawns, true), (&self.extractions, false)] {
            let mut seen: Vec<String> = Vec::new();
            for p in list {
                let id = p.id.trim();
                let (Some(m), Some(at)) = (map(&p.area), p.at()) else {
                    continue;
                };
                if !fmt::valid_id(id) || seen.iter().any(|s| s.eq_ignore_ascii_case(id)) {
                    continue;
                }
                seen.push(id.into());
                let marker = fmt::Marker { id: id.into(), at };
                let a = w.area_mut(&m);
                if spawn {
                    a.spawns.push(marker);
                } else {
                    a.extractions.push(marker);
                }
            }
        }
        crate::mission::to_format(self, &mut w, map);
        crate::ecosystem::to_format(self, &mut w);
        w
    }

    /// The layer to put over the built game in `vfs`, when the section is
    /// on and has entries.
    pub fn layer(&self, vfs: &Vfs) -> Result<Option<MemBackend>, ModError> {
        if !self.is_active() {
            return Ok(None);
        }
        let maps = crate::balance::area_maps(vfs)?;
        let mut m = MemBackend::new();
        m.insert(fmt::PATH, self.to_format(&maps).to_text().into_bytes());
        Ok(Some(m))
    }

    /// Puts the layer over `vfs` (nothing when off). True when it did.
    pub fn push(&self, vfs: &mut Vfs) -> Result<bool, ModError> {
        Ok(match self.layer(vfs)? {
            Some(l) => {
                vfs.push("world zones", Box::new(l));
                true
            }
            None => false,
        })
    }
}

/// The ids of `list` at fault: not one plain word, or used before.
pub(crate) fn id_faults<'a>(
    at: &str,
    ids: impl Iterator<Item = &'a str>,
    warn: &mut dyn FnMut(String, String),
) {
    let mut seen: Vec<String> = Vec::new();
    for (i, id) in ids.enumerate() {
        let id = id.trim();
        if !fmt::valid_id(id) {
            warn(
                format!("{at}[{i}].id"),
                format!(
                    "{id:?} is not an id (letters, digits, - and _, at most {}); skipped",
                    fmt::MAX_ID
                ),
            );
        } else if seen.iter().any(|s| s.eq_ignore_ascii_case(id)) {
            warn(
                format!("{at}[{i}].id"),
                format!("{id:?} is used before; this one is skipped"),
            );
        } else {
            seen.push(id.into());
        }
    }
}

/// What looks wrong in `w`, as warnings (`crate::validate`). `areas`: the
/// built game's area names (none: names are not checked).
pub(crate) fn check(w: &World, areas: Option<&[String]>, warn: &mut dyn FnMut(String, String)) {
    const AT: &str = "settings.world";
    let side = MAP_SIZE as f32;
    let (lo, hi) = fmt::GAP_RANGE;
    if let Some(g) = w.min_extraction_gap {
        if !g.is_finite() {
            warn(
                format!("{AT}.min_extraction_gap"),
                "not a finite number; ignored".into(),
            );
        } else if !(f64::from(lo)..=f64::from(hi)).contains(&g) {
            warn(
                format!("{AT}.min_extraction_gap"),
                format!(
                    "{g} is outside {lo}..={hi}; {} is used",
                    g.clamp(lo.into(), hi.into())
                ),
            );
        }
    }
    let area = |key: String, name: &str, warn: &mut dyn FnMut(String, String)| {
        let known = |list: &[String]| {
            list.iter()
                .any(|n| n.trim().eq_ignore_ascii_case(name.trim()))
        };
        if name.trim().is_empty() {
            warn(key, "no area is given; skipped".into());
        } else if areas.is_some_and(|l| !known(l)) {
            warn(key, format!("the game has no area named {name:?}; skipped"));
        }
    };

    id_faults(
        &format!("{AT}.zones"),
        w.zones.iter().map(|z| z.id.as_str()),
        warn,
    );
    for (i, z) in w.zones.iter().enumerate() {
        let at = format!("{AT}.zones[{i}]");
        area(format!("{at}.area"), &z.area, warn);
        match z.shape() {
            Err(e) => warn(format!("{at}.shape"), format!("{e}; the zone is skipped")),
            Ok(s) => {
                let b = s.bounds();
                if b[0] < 0.0 || b[1] < 0.0 || b[2] > side || b[3] > side {
                    warn(
                        format!("{at}.shape"),
                        format!("part of the zone is outside the map (0..={MAP_SIZE} cells)"),
                    );
                }
            }
        }
        for (j, t) in z.tags.iter().enumerate() {
            if !fmt::valid_id(t.trim()) {
                warn(
                    format!("{at}.tags[{j}]"),
                    format!("{t:?} is not a tag (letters, digits, - and _); ignored"),
                );
            }
        }
    }
    for (name, list) in [("spawns", &w.spawns), ("extractions", &w.extractions)] {
        let list_at = format!("{AT}.{name}");
        id_faults(&list_at, list.iter().map(|m| m.id.as_str()), warn);
        for (i, m) in list.iter().enumerate() {
            area(format!("{list_at}[{i}].area"), &m.area, warn);
            match m.at() {
                None => warn(
                    format!("{list_at}[{i}].at"),
                    "no position ([x, z] in cells); skipped".into(),
                ),
                Some(p) if !on_map(p) => warn(
                    format!("{list_at}[{i}].at"),
                    format!("outside the map (0..={MAP_SIZE} cells)"),
                ),
                Some(_) => {}
            }
        }
    }
    // The gap rule: each area with both kinds of point needs one pair far
    // enough apart, or no hunt there can satisfy it.
    let Some(gap) = w
        .min_extraction_gap
        .filter(|g| g.is_finite() && *g > 0.0)
        .map(|g| (g as f32).min(hi) * side)
    else {
        return;
    };
    let mut done: Vec<String> = Vec::new();
    for s in &w.spawns {
        let key = s.area.trim().to_ascii_lowercase();
        if key.is_empty() || done.contains(&key) {
            continue;
        }
        done.push(key.clone());
        let here = |m: &&Marker| m.area.trim().eq_ignore_ascii_case(&key);
        let starts: Vec<[f32; 2]> = w
            .spawns
            .iter()
            .filter(here)
            .filter_map(Marker::at)
            .collect();
        let exits: Vec<[f32; 2]> = w
            .extractions
            .iter()
            .filter(here)
            .filter_map(Marker::at)
            .collect();
        let far = |a: &[f32; 2], b: &[f32; 2]| {
            let (dx, dz) = (a[0] - b[0], a[1] - b[1]);
            dx * dx + dz * dz >= gap * gap
        };
        if !exits.is_empty() && !starts.iter().any(|a| exits.iter().any(|b| far(a, b))) {
            warn(
                format!("{AT}.min_extraction_gap"),
                format!(
                    "on {:?} no player start is {gap} cells or more from an extraction point",
                    s.area.trim()
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECTION: &str = r#"
        enabled = true
        min_extraction_gap = 0.25
        [[zones]]
        id = "marsh"
        area = "Ravan"
        shape = "circle"
        center = [512.0, 300.5]
        radius = 40.0
        tags = ["No_Spawn", "no_spawn", "two words"]
        [[zones]]
        id = "ridge"
        area = " ravan"
        shape = "polygon"
        points = [[100.0, 100.0], [200.0, 100.0], [150.0, 220.0]]
        [[zones]]
        id = "gone"
        area = "Atlantis"
        shape = "circle"
        center = [10.0, 10.0]
        radius = 5.0
        [[zones]]
        id = "marsh"
        area = "Ravan"
        shape = "circle"
        center = [9.0, 9.0]
        radius = 5.0
        [[zones]]
        id = "bow"
        area = "Ravan"
        shape = "polygon"
        points = [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]]
        [[spawns]]
        id = "beach"
        area = "Ravan"
        at = [100.0, 200.0]
        [[spawns]]
        id = "nowhere"
        area = "Ravan"
        [[extractions]]
        id = "beach"
        area = "Vengar"
        at = [300.0, 400.0]
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
    fn a_recipe_section_becomes_the_side_file() {
        let f = section().to_format(&maps());
        assert_eq!(f.min_extraction_gap, Some(0.25));
        assert_eq!(f.areas.len(), 2, "{f:?}");
        let ravan = f.area("area2").unwrap_or_else(|| panic!("no area2"));
        let ids: Vec<&str> = ravan.zones.iter().map(|z| z.id.as_str()).collect();
        assert_eq!(ids, ["marsh", "ridge"], "the unusable ones are left out");
        assert_eq!(ravan.zones[0].tags, ["no_spawn"]);
        assert!(ravan.zones[0].shape.contains(512.0, 300.5));
        assert_eq!(ravan.spawns.len(), 1, "a start without a position is out");
        let vengar = f.area("area1").unwrap_or_else(|| panic!("no area1"));
        assert_eq!(vengar.extractions[0].id, "beach");
        assert_eq!(fmt::World::parse(f.to_text().as_bytes()), f);
    }

    #[test]
    fn off_absent_or_empty_writes_nothing() {
        let vfs = Vfs::new();
        let mut w = section();
        w.enabled = false;
        assert!(!w.is_active() && matches!(w.layer(&vfs), Ok(None)));
        let empty = World {
            enabled: true,
            min_extraction_gap: Some(0.5),
            ..World::default()
        };
        assert!(!empty.is_active() && matches!(empty.layer(&vfs), Ok(None)));
        assert!(matches!(World::default().layer(&vfs), Ok(None)));
    }

    #[test]
    fn a_recipe_keeps_its_world_section_through_save_and_load() {
        let text = format!(
            "[recipe]\nname = \"t\"\n[settings.world]{}",
            SECTION.replace("\n        [[", "\n[[settings.world.")
        );
        let (r, warnings) =
            crate::recipe::Recipe::parse_checked(&text).unwrap_or_else(|e| panic!("{e}"));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(r.settings.world, Some(section()));
        let back = crate::recipe::Recipe::parse(&r.to_toml()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.settings.world, r.settings.world);
        // Written as short decimals, not as widened 32-bit floats.
        assert!(r.to_toml().contains("min_extraction_gap = 0.25"));
        assert!(r.to_toml().contains("300.5") && !r.to_toml().contains("300.50"));
    }

    #[test]
    fn mistyped_keys_are_reported() {
        let text = "[recipe]\nname = \"t\"\n[settings.world]\nenabled = true\ngap = 1\n\
                    [[settings.world.zones]]\nid = \"a\"\nradious = 3.0\n\
                    [[settings.world.spawns]]\nid = \"s\"\npos = [1.0, 2.0]\n";
        let (_, w) = crate::recipe::Recipe::parse_checked(text).unwrap_or_else(|e| panic!("{e}"));
        for k in [
            "`settings.world.gap`",
            "`settings.world.zones.0.radious`",
            "`settings.world.spawns.0.pos`",
        ] {
            assert!(w.iter().any(|x| x.contains(k)), "{k}: {w:?}");
        }
    }

    fn keys(w: &World, areas: Option<&[String]>) -> Vec<String> {
        let mut out = Vec::new();
        check(w, areas, &mut |k, _| out.push(k));
        out
    }

    #[test]
    fn check_names_each_fault_by_its_key() {
        let areas: Vec<String> = maps().into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            keys(&section(), Some(&areas)),
            [
                "settings.world.zones[3].id",
                "settings.world.zones[0].tags[2]",
                "settings.world.zones[2].area",
                "settings.world.zones[4].shape",
                "settings.world.spawns[1].at",
            ]
        );
        // Without the game's names, areas are not checked.
        assert_eq!(keys(&section(), None).len(), 4);
        assert!(keys(&World::default(), Some(&areas)).is_empty());
    }

    #[test]
    fn check_reports_ranges_bad_shapes_and_the_map_edge() {
        let w: World = toml::from_str(
            r#"
            enabled = true
            min_extraction_gap = 3.0
            [[zones]]
            id = "two words"
            area = ""
            shape = "blob"
            [[zones]]
            id = "edge"
            area = "A"
            shape = "circle"
            center = [5.0, 500.0]
            radius = 10.0
            [[zones]]
            id = "half"
            area = "A"
            shape = "circle"
            center = [5.0, 500.0]
            [[extractions]]
            id = "off"
            area = "A"
            at = [2000.0, 5.0]
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let mut out = Vec::new();
        check(&w, None, &mut |k, m| out.push((k, m)));
        let got: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            got,
            [
                "settings.world.min_extraction_gap",
                "settings.world.zones[0].id",
                "settings.world.zones[0].area",
                "settings.world.zones[0].shape",
                "settings.world.zones[1].shape",
                "settings.world.zones[2].shape",
                "settings.world.extractions[0].at",
            ]
        );
        assert!(out[0].1.contains("1 is used"), "{}", out[0].1);
        assert!(out[4].1.contains("outside the map"), "{}", out[4].1);
        assert!(out[5].1.contains("center and radius"), "{}", out[5].1);
    }

    #[test]
    fn the_gap_rule_needs_one_pair_far_enough_apart_per_area() {
        let world = |gap: f64, exit: [f64; 2]| World {
            enabled: true,
            min_extraction_gap: Some(gap),
            spawns: vec![
                Marker {
                    id: "a".into(),
                    area: "Ravan".into(),
                    at: Some([100.0, 100.0]),
                },
                Marker {
                    id: "b".into(),
                    area: "ravan".into(),
                    at: Some([120.0, 100.0]),
                },
                Marker {
                    id: "alone".into(),
                    area: "Vengar".into(),
                    at: Some([1.0, 1.0]),
                },
            ],
            extractions: vec![Marker {
                id: "x".into(),
                area: "Ravan".into(),
                at: Some(exit),
            }],
            ..World::default()
        };
        // A quarter of 1024 cells is 256.
        assert!(keys(&world(0.25, [356.0, 100.0]), None).is_empty());
        assert_eq!(
            keys(&world(0.25, [300.0, 100.0]), None),
            ["settings.world.min_extraction_gap"]
        );
        assert!(keys(&world(0.0, [100.0, 100.0]), None).is_empty());
    }
}
