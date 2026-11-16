//! A recipe's `[settings.world.ecosystem]`: which animals may appear in
//! which zone, and how many (an advanced setting, not in the original
//! game). Written into `HUNTDAT/_WORLD.TXT` with the rest of
//! `[settings.world]` ([`c2_formats::ecosystem`]). Opt-in: absent or off,
//! nothing is written and animals appear as they always have.
//!
//! The rules steer where the game's own placement puts an animal; they
//! never remove or move one that is already on the map.
//!
//! ```toml
//! [settings.world.ecosystem]
//! enabled = true
//! max_animals = 40             # living animals on a map; unset: no limit
//!
//! [[settings.world.ecosystem.species]]
//! name = "Velociraptor"        # as the balance editor names it
//! max = 4                      # living at once
//!
//! [[settings.world.ecosystem.zones]]
//! zone = "ridge"               # a [[settings.world.zones]] id
//! max = 6                      # living animals inside at once
//! contain = "hard"             # "hard", "soft" or unset
//! allow = ["Stegosaurus"]      # only these appear here; empty: any
//! weights = [{ name = "Stegosaurus", weight = 3.0 }]   # against 1 elsewhere
//!
//! [[settings.world.ecosystem.zones]]
//! zone = "marsh"
//! no_spawn = true              # nothing appears here
//! avoid = true                 # wandering and fleeing animals keep out
//!
//! # By time of day (dawn, day, night): only the keys given differ.
//! [settings.world.ecosystem.night]
//! max_animals = 20
//! species = [{ name = "Velociraptor", max = 8 }]
//! [[settings.world.ecosystem.night.zones]]
//! zone = "ridge"
//! max = 2
//! no_spawn = false
//! allow = []                   # given and empty: any animal
//! weights = [{ name = "Stegosaurus", weight = 0.5 }]
//!
//! # How far from the hunter new animals appear (cells; unset: 40).
//! [settings.world.ecosystem.safety]
//! initial_distance = 40        # at the start of a hunt
//! respawn_distance = 60        # replacements for killed animals
//! extra_distance = 40          # the instance rules' extra animals
//! out_of_view = true           # replacements only where he is not looking
//! species = [{ name = "Tyrannosaurus Rex", distance = 80 }]
//! ```
//!
//! A zone tagged `no_spawn` in the map editor is a no-spawn zone too.

use serde::{Deserialize, Serialize};

use c2_formats::ecosystem::{self as fmt, Contain};
use c2_formats::world as world_fmt;

use crate::world::World;

/// The zone tag that makes a no-spawn zone while the ecosystem is on.
pub const NO_SPAWN_TAG: &str = "no_spawn";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ecosystem {
    pub enabled: bool,
    /// Most living animals on a map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_animals: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub species: Vec<SpeciesCap>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<ZoneRule>,
    /// What differs at dawn, by day and at night.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dawn: Option<TimeOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<TimeOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night: Option<TimeOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety: Option<Safety>,
}

/// What differs at one time of day; what is not given is inherited.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_animals: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub species: Vec<SpeciesCap>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<ZoneOverride>,
}

/// What a zone's rule changes at one time of day.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoneOverride {
    pub zone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_spawn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    /// Given: the allow list instead (empty: any animal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub weights: Vec<Weight>,
}

/// How far from the hunter new animals appear, cells.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Safety {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_distance: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respawn_distance: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_distance: Option<u32>,
    /// Replacements appear only where the hunter is not looking.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub out_of_view: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub species: Vec<SpeciesDistance>,
}

/// A kind's own distance from the hunter, used when larger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeciesDistance {
    pub name: String,
    pub distance: u32,
}

impl Ecosystem {
    /// The overrides by time of day, in [`c2_formats::rules::TIMES`]' order.
    pub fn times(&self) -> [Option<&TimeOverride>; 3] {
        [self.dawn.as_ref(), self.day.as_ref(), self.night.as_ref()]
    }

    /// [`Self::times`], to change them.
    pub fn times_mut(&mut self) -> [&mut Option<TimeOverride>; 3] {
        [&mut self.dawn, &mut self.day, &mut self.night]
    }
}

/// Most living animals of one kind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeciesCap {
    pub name: String,
    pub max: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Weight {
    pub name: String,
    pub weight: f64,
}

/// One zone's rule.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoneRule {
    /// The id of a `[[settings.world.zones]]` entry.
    pub zone: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_spawn: bool,
    /// Most living animals inside at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    /// `hard`, `soft`, or empty (`none`): free to roam.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub contain: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub weights: Vec<Weight>,
    /// Wandering and fleeing animals keep out.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub avoid: bool,
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

fn cap(n: u32) -> u32 {
    n.clamp(fmt::CAP_RANGE.0, fmt::CAP_RANGE.1)
}

fn weights(list: &[Weight]) -> Vec<(String, f32)> {
    let mut out: Vec<(String, f32)> = Vec::new();
    for w in list {
        let name = w.name.trim();
        if !name.is_empty() && w.weight.is_finite() && !out.iter().any(|x| same(&x.0, name)) {
            let v = (w.weight as f32).clamp(fmt::WEIGHT_RANGE.0, fmt::WEIGHT_RANGE.1);
            out.push((name.into(), v));
        }
    }
    out
}

fn species_caps(list: &[SpeciesCap]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for s in list {
        let name = s.name.trim();
        if !name.is_empty() && !out.iter().any(|x| same(&x.0, name)) {
            out.push((name.into(), cap(s.max)));
        }
    }
    out
}

impl ZoneRule {
    /// The file's form (entries that can't be used left out).
    fn to_format(&self) -> fmt::ZoneRule {
        fmt::ZoneRule {
            no_spawn: self.no_spawn,
            cap: self.max.map(cap),
            contain: Contain::parse(&self.contain).unwrap_or_default(),
            allow: clean(&self.allow),
            weights: weights(&self.weights),
            avoid: self.avoid,
            at: Default::default(),
        }
    }
}

impl ZoneOverride {
    fn to_format(&self) -> fmt::ZoneOverride {
        fmt::ZoneOverride {
            no_spawn: self.no_spawn,
            cap: self.max.map(cap),
            allow: self.allow.as_deref().map(clean),
            weights: weights(&self.weights),
        }
    }
}

impl Safety {
    fn to_format(&self) -> fmt::Safety {
        let cells = |n: u32| n.clamp(fmt::DISTANCE_RANGE.0, fmt::DISTANCE_RANGE.1);
        let mut species: Vec<(String, u32)> = Vec::new();
        for s in &self.species {
            let name = s.name.trim();
            if !name.is_empty() && !species.iter().any(|x| same(&x.0, name)) {
                species.push((name.into(), cells(s.distance)));
            }
        }
        fmt::Safety {
            initial: self.initial_distance.map(cells),
            respawn: self.respawn_distance.map(cells),
            extra: self.extra_distance.map(cells),
            unseen: self.out_of_view,
            species,
        }
    }
}

impl Ecosystem {
    /// Whether the section changes the build: on, with something in it.
    /// (Zones tagged `no_spawn` count once it is on, see [`to_format`].)
    pub fn is_active(&self) -> bool {
        self.enabled
    }
}

/// Puts `w`'s ecosystem in `out` (which holds the zones already). Left out:
/// a rule for a zone that is not in `out`, and a second rule for a zone
/// ([`check`] reports each).
pub(crate) fn to_format(w: &World, out: &mut world_fmt::World) {
    let Some(e) = w.ecosystem.as_ref().filter(|e| e.is_active()) else {
        return;
    };
    out.caps.max_animals = e.max_animals.map(cap);
    out.caps.species = species_caps(&e.species);
    if let Some(s) = &e.safety {
        out.safety = s.to_format();
    }
    let mut done: Vec<String> = Vec::new();
    for r in &e.zones {
        let id = r.zone.trim().to_ascii_lowercase();
        if done.contains(&id) {
            continue;
        }
        if let Some(z) = out.zone_mut(&id) {
            z.rule = r.to_format();
            done.push(id);
        }
    }
    for z in out.areas.iter_mut().flat_map(|a| a.zones.iter_mut()) {
        if z.has_tag(NO_SPAWN_TAG) {
            z.rule.no_spawn = true;
        }
    }
    for (t, o) in e.times().into_iter().enumerate() {
        let Some(o) = o else { continue };
        out.caps.at[t] = fmt::Limits {
            max_animals: o.max_animals.map(cap),
            species: species_caps(&o.species),
        };
        let mut done: Vec<String> = Vec::new();
        for r in &o.zones {
            let id = r.zone.trim().to_ascii_lowercase();
            if done.contains(&id) {
                continue;
            }
            if let Some(z) = out.zone_mut(&id) {
                z.rule.at[t] = r.to_format();
                done.push(id);
            }
        }
    }
}

/// What looks wrong in `w`'s ecosystem, as warnings (`crate::validate`).
/// `animals`: the built game's animal names (none: names are not checked).
pub fn check(w: &World, animals: Option<&[String]>, warn: &mut dyn FnMut(String, String)) {
    const AT: &str = "settings.world.ecosystem";
    let Some(e) = &w.ecosystem else { return };
    let (lo, hi) = fmt::CAP_RANGE;
    if e.enabled && !w.enabled {
        warn(
            "settings.world.enabled".into(),
            "the ecosystem is on but [settings.world] is off; it is not built into the game".into(),
        );
    }
    let unset = Ecosystem {
        enabled: false,
        ..Ecosystem::default()
    };
    if !e.enabled && *e != unset {
        warn(
            format!("{AT}.enabled"),
            "rules are set but the ecosystem is off; they are not built into the game".into(),
        );
    }
    let known = |name: &str| animals.is_none_or(|l| l.iter().any(|n| same(n, name)));
    let over = |key: String, n: u32, warn: &mut dyn FnMut(String, String)| {
        if n > hi {
            warn(key, format!("{n} is outside {lo}..={hi}; {hi} is used"));
        }
    };
    if let Some(n) = e.max_animals {
        over(format!("{AT}.max_animals"), n, warn);
        if n == 0 {
            warn(
                format!("{AT}.max_animals"),
                "0 leaves the maps without animals".into(),
            );
        }
    }
    let mut seen: Vec<&str> = Vec::new();
    for (i, s) in e.species.iter().enumerate() {
        let at = format!("{AT}.species[{i}]");
        let name = s.name.trim();
        if name.is_empty() {
            warn(format!("{at}.name"), "no animal is named; skipped".into());
        } else if seen.iter().any(|n| same(n, name)) {
            warn(
                format!("{at}.name"),
                format!("{name:?} is capped before; this one is skipped"),
            );
        } else if !known(name) {
            warn(
                format!("{at}.name"),
                format!("the game has no animal named {name:?}"),
            );
        }
        seen.push(name);
        over(format!("{at}.max"), s.max, warn);
    }
    let mut seen: Vec<&str> = Vec::new();
    for (i, r) in e.zones.iter().enumerate() {
        let at = format!("{AT}.zones[{i}]");
        let id = r.zone.trim();
        if !w.zones.iter().any(|z| same(&z.id, id)) {
            warn(
                format!("{at}.zone"),
                format!("[settings.world] has no zone called {id:?}; skipped"),
            );
        } else if seen.iter().any(|z| same(z, id)) {
            warn(
                format!("{at}.zone"),
                format!("{id:?} has a rule before; this one is skipped"),
            );
        }
        seen.push(id);
        if let Some(n) = r.max {
            over(format!("{at}.max"), n, warn);
        }
        if Contain::parse(&r.contain).is_none() {
            warn(
                format!("{at}.contain"),
                format!("{:?} is not hard, soft or none; none is used", r.contain),
            );
        }
        if r.no_spawn && (r.max.is_some() || !r.allow.is_empty() || !r.weights.is_empty()) {
            warn(
                format!("{at}.no_spawn"),
                "nothing appears in a no-spawn zone; its cap, allow list and weights are unused"
                    .into(),
            );
        }
        for (j, n) in r.allow.iter().enumerate() {
            if !known(n) {
                warn(
                    format!("{at}.allow[{j}]"),
                    format!("the game has no animal named {n:?}"),
                );
            }
        }
        let (wlo, whi) = fmt::WEIGHT_RANGE;
        for (j, x) in r.weights.iter().enumerate() {
            let wat = format!("{at}.weights[{j}]");
            if !known(&x.name) {
                warn(
                    format!("{wat}.name"),
                    format!("the game has no animal named {:?}", x.name),
                );
            } else if !r.allow.is_empty() && !r.allow.iter().any(|a| same(a, &x.name)) {
                warn(
                    format!("{wat}.name"),
                    format!("{:?} is not in allow; it never appears here", x.name.trim()),
                );
            }
            if !x.weight.is_finite() {
                warn(
                    format!("{wat}.weight"),
                    "not a finite number; skipped".into(),
                );
            } else if !(f64::from(wlo)..=f64::from(whi)).contains(&x.weight) {
                warn(
                    format!("{wat}.weight"),
                    format!(
                        "{} is outside {wlo}..={whi}; {} is used",
                        x.weight,
                        x.weight.clamp(wlo.into(), whi.into())
                    ),
                );
            }
        }
    }
    for (t, o) in e.times().into_iter().enumerate() {
        let Some(o) = o else { continue };
        let at = format!("{AT}.{}", c2_formats::rules::TIMES[t]);
        if let Some(n) = o.max_animals {
            over(format!("{at}.max_animals"), n, warn);
        }
        for (i, s) in o.species.iter().enumerate() {
            if !known(&s.name) {
                warn(
                    format!("{at}.species[{i}].name"),
                    format!("the game has no animal named {:?}", s.name.trim()),
                );
            }
            over(format!("{at}.species[{i}].max"), s.max, warn);
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, r) in o.zones.iter().enumerate() {
            let zat = format!("{at}.zones[{i}]");
            let id = r.zone.trim();
            if !w.zones.iter().any(|z| same(&z.id, id)) {
                warn(
                    format!("{zat}.zone"),
                    format!("[settings.world] has no zone called {id:?}; skipped"),
                );
            } else if seen.iter().any(|z| same(z, id)) {
                warn(
                    format!("{zat}.zone"),
                    format!("{id:?} is overridden before; this one is skipped"),
                );
            }
            seen.push(id);
            if let Some(n) = r.max {
                over(format!("{zat}.max"), n, warn);
            }
            let names = r.allow.iter().flatten().map(|n| ("allow", n.as_str()));
            let names = names.chain(r.weights.iter().map(|x| ("weights", x.name.as_str())));
            for (j, (key, n)) in names.enumerate() {
                if !known(n) {
                    warn(
                        format!("{zat}.{key}[{j}]"),
                        format!("the game has no animal named {n:?}"),
                    );
                }
            }
        }
    }
    if let Some(s) = &e.safety {
        let at = format!("{AT}.safety");
        let (dlo, dhi) = fmt::DISTANCE_RANGE;
        let mut far = |key: String, n: u32| {
            if n > dhi {
                warn(
                    key,
                    format!("{n} is outside {dlo}..={dhi} cells; {dhi} is used"),
                );
            }
        };
        for (key, v) in [
            ("initial_distance", s.initial_distance),
            ("respawn_distance", s.respawn_distance),
            ("extra_distance", s.extra_distance),
        ] {
            if let Some(n) = v {
                far(format!("{at}.{key}"), n);
            }
        }
        for (i, x) in s.species.iter().enumerate() {
            far(format!("{at}.species[{i}].distance"), x.distance);
        }
        for (i, x) in s.species.iter().enumerate() {
            if !known(&x.name) {
                warn(
                    format!("{at}.species[{i}].name"),
                    format!("the game has no animal named {:?}", x.name.trim()),
                );
            }
        }
        // Replacements are looked for within 78 cells of the hunter.
        if s.respawn_distance.is_some_and(|d| d > 70) {
            warn(
                format!("{at}.respawn_distance"),
                "replacements are looked for within 78 cells of the hunter; \
                 above 70 few or none will be found"
                    .into(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECTION: &str = r#"
        enabled = true
        [[zones]]
        id = "ridge"
        area = "Ravan"
        shape = "circle"
        center = [100.0, 100.0]
        radius = 30.0
        [[zones]]
        id = "marsh"
        area = "Ravan"
        shape = "circle"
        center = [300.0, 300.0]
        radius = 30.0
        tags = ["no_spawn"]
        [ecosystem]
        enabled = true
        max_animals = 40
        [[ecosystem.species]]
        name = "Velociraptor"
        max = 4
        [[ecosystem.zones]]
        zone = "Ridge"
        max = 6
        contain = "hard"
        allow = ["Stegosaurus", " stegosaurus "]
        weights = [{ name = "Stegosaurus", weight = 3.0 }]
    "#;

    fn section() -> World {
        toml::from_str(SECTION).unwrap_or_else(|e| panic!("{e}"))
    }

    fn maps() -> Vec<(String, String)> {
        vec![("Ravan".into(), "area2".into())]
    }

    fn keys(w: &World, animals: Option<&[String]>) -> Vec<String> {
        let mut out = Vec::new();
        check(w, animals, &mut |k, _| out.push(k));
        out
    }

    #[test]
    fn the_recipe_round_trips_and_builds_the_files_rules() {
        let w = section();
        let text = toml::to_string(&w).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(toml::from_str::<World>(&text).ok(), Some(w.clone()));
        let f = w.to_format(&maps());
        assert_eq!(f.caps.max_animals, Some(40));
        assert_eq!(f.caps.species, [("Velociraptor".into(), 4)]);
        let (_, ridge) = f.zone("ridge").unwrap_or_else(|| panic!("no ridge"));
        assert_eq!(ridge.rule.cap, Some(6));
        assert_eq!(ridge.rule.contain, Contain::Hard);
        assert_eq!(ridge.rule.allow, ["Stegosaurus"]);
        assert_eq!(ridge.rule.weights, [("Stegosaurus".into(), 3.0)]);
        // The tag alone makes a no-spawn zone.
        assert!(f.zone("marsh").is_some_and(|(_, z)| z.rule.no_spawn));
        // And back from the file's text.
        let back = world_fmt::World::parse(f.to_text().as_bytes());
        assert_eq!(back, f);
        assert!(keys(&w, None).is_empty(), "{:?}", keys(&w, None));
    }

    #[test]
    fn off_it_writes_nothing() {
        let mut w = section();
        if let Some(e) = &mut w.ecosystem {
            e.enabled = false;
        }
        let f = w.to_format(&maps());
        assert!(f.caps.is_empty());
        assert!(f
            .areas
            .iter()
            .all(|a| a.zones.iter().all(|z| z.rule.is_empty())));
        assert_eq!(keys(&w, None), ["settings.world.ecosystem.enabled"]);
        w.ecosystem = None;
        assert!(keys(&w, None).is_empty());
    }

    #[test]
    fn caps_alone_make_the_world_section_active() {
        let w: World =
            toml::from_str("enabled = true\n[ecosystem]\nenabled = true\nmax_animals = 9\n")
                .unwrap_or_else(|e| panic!("{e}"));
        assert!(w.is_active());
        assert_eq!(w.to_format(&[]).caps.max_animals, Some(9));
        let off: World = toml::from_str("[ecosystem]\nenabled = true\n").unwrap_or_default();
        assert!(!off.is_active());
        assert_eq!(keys(&off, None), ["settings.world.enabled"]);
    }

    #[test]
    fn faults_are_reported_by_key() {
        let w: World = toml::from_str(
            r#"
            enabled = true
            [[zones]]
            id = "a"
            area = "Ravan"
            shape = "circle"
            center = [100.0, 100.0]
            radius = 30.0
            [ecosystem]
            enabled = true
            max_animals = 0
            [[ecosystem.species]]
            name = "Dodo"
            max = 999
            [[ecosystem.species]]
            name = "dodo"
            max = 1
            [[ecosystem.zones]]
            zone = "nowhere"
            [[ecosystem.zones]]
            zone = "a"
            no_spawn = true
            max = 3
            contain = "tight"
            allow = ["Dodo", "Rex"]
            weights = [{ name = "Stego", weight = 99.0 }, { name = "Dodo", weight = 1.0 }]
            [[ecosystem.zones]]
            zone = "A"
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let animals = ["Rex".to_string(), "Stego".to_string()];
        let at = |s: &str| format!("settings.world.ecosystem.{s}");
        assert_eq!(
            keys(&w, Some(&animals)),
            [
                at("max_animals"),
                at("species[0].name"),
                at("species[0].max"),
                at("species[1].name"),
                at("zones[0].zone"),
                at("zones[1].contain"),
                at("zones[1].no_spawn"),
                at("zones[1].allow[0]"),
                at("zones[1].weights[0].name"),
                at("zones[1].weights[0].weight"),
                at("zones[1].weights[1].name"),
                at("zones[2].zone"),
            ]
        );
        // The second rule for a zone is not the one built.
        let f = w.to_format(&maps());
        assert!(f.zone("a").is_some_and(|(_, z)| z.rule.no_spawn));
    }

    #[test]
    fn time_overrides_and_safety_reach_the_file() {
        let mut w = section();
        let more: Ecosystem = toml::from_str(
            r#"
            [night]
            max_animals = 20
            species = [{ name = "Velociraptor", max = 8 }]
            [[night.zones]]
            zone = "RIDGE"
            max = 2
            no_spawn = false
            allow = []
            weights = [{ name = "Stegosaurus", weight = 0.5 }]
            [[night.zones]]
            zone = "nowhere"
            max = 1
            [safety]
            respawn_distance = 60
            out_of_view = true
            species = [{ name = "Tyrannosaurus Rex", distance = 9000 }]
            "#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let e = w.ecosystem.as_mut().unwrap_or_else(|| panic!("no section"));
        e.night = more.night;
        e.safety = more.safety;
        e.zones[0].avoid = true;
        let text = toml::to_string(&w).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(toml::from_str::<World>(&text).ok(), Some(w.clone()));
        let f = w.to_format(&maps());
        assert_eq!(f.caps.at_time(2).max_animals, Some(20));
        assert_eq!(f.caps.at_time(1).max_animals, Some(40));
        assert_eq!(f.caps.at_time(2).species, [("Velociraptor".into(), 8)]);
        let (_, ridge) = f.zone("ridge").unwrap_or_else(|| panic!("no ridge"));
        assert!(ridge.rule.avoid);
        let night = ridge.rule.at_time(2);
        assert_eq!(night.cap, Some(2));
        assert!(night.allow.is_empty());
        assert_eq!(night.weights, [("Stegosaurus".into(), 0.5)]);
        assert_eq!(ridge.rule.at_time(1).cap, Some(6));
        assert_eq!(f.safety.respawn, Some(60));
        assert!(f.safety.unseen);
        assert_eq!(f.safety.species, [("Tyrannosaurus Rex".into(), 512)]);
        assert_eq!(world_fmt::World::parse(f.to_text().as_bytes()), f);
        let at = |s: &str| format!("settings.world.ecosystem.{s}");
        assert_eq!(
            keys(&w, None),
            [at("night.zones[1].zone"), at("safety.species[0].distance")]
        );
    }
}
