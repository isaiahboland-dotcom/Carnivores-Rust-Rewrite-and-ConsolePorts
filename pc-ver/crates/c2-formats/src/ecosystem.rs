//! Spawn ecosystems in `HUNTDAT/_WORLD.TXT` ([`crate::world`]): which
//! animals may appear where, and how many (not in the original game).
//!
//! Two lines before the first `area` cap the whole map; `eco` lines after a
//! `zone` are that zone's rule. Animal names are the rest of the line, as
//! the menus name them (case ignored; `Raptor #2`: the second so named).
//!
//! ```text
//! max_animals 40              ; living animals on the map
//! species_max 4 Velociraptor  ; living animals of one kind
//! area area1
//! zone marsh circle 512 300 40
//! eco no_spawn 1              ; nothing appears here
//! zone ridge polygon 100 100 200 100 150 220
//! eco cap 6                   ; living animals inside at once
//! eco contain hard            ; hard or soft: who appears here stays here
//! eco allow Stegosaurus       ; only the listed animals appear here
//! eco weight 3 Stegosaurus    ; how readily, against 1 elsewhere
//! eco avoid 1                 ; wandering and fleeing animals keep out
//! at night eco cap 2          ; at dawn, by day or at night: this instead
//! at night max_animals 20
//! safety respawn 60           ; cells from the hunter (also initial, extra)
//! safety unseen 1             ; replacements only out of his sight
//! safety species 80 Tyrannosaurus Rex
//! ```
//!
//! An `at <time>` line overrides one value for that time of day
//! ([`crate::rules::TIMES`]); what it does not name is inherited.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use crate::rules::TIMES;

/// Accepted `max_animals`, `species_max` and zone `cap`.
pub const CAP_RANGE: (u32, u32) = (0, 255);
/// Accepted weight: 0 (never) to 10 times as readily as elsewhere.
pub const WEIGHT_RANGE: (f32, f32) = (0.0, 10.0);
/// Accepted distance from the hunter, cells.
pub const DISTANCE_RANGE: (u32, u32) = (0, 512);
/// The original's distance from the hunter for a new animal, cells
/// (`PlaceCharacters`, `Characters_AddSecondaryOne`).
pub const DEFAULT_DISTANCE: u32 = 40;

/// The index of time of day `name` in [`TIMES`], case ignored.
pub fn time_index(name: &str) -> Option<usize> {
    TIMES
        .iter()
        .position(|t| t.eq_ignore_ascii_case(name.trim()))
}

/// Whether the animals that appear in a zone keep to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Contain {
    /// They roam as anywhere else.
    #[default]
    Free,
    /// They mostly wander inside, and may leave when chasing or fleeing.
    Soft,
    /// They never cross the zone's edge.
    Hard,
}

impl Contain {
    pub const NAMES: [(Contain, &'static str); 3] = [
        (Contain::Free, "none"),
        (Contain::Soft, "soft"),
        (Contain::Hard, "hard"),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(c, _)| *c == self)
            .map_or("none", |(_, n)| n)
    }

    /// `none` (or nothing), `soft` or `hard`, case ignored.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Some(Contain::Free);
        }
        Self::NAMES
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(s))
            .map(|(c, _)| *c)
    }
}

/// One zone's rule. The default changes nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZoneRule {
    /// No animal is placed in the zone (those there already stay).
    pub no_spawn: bool,
    /// Most living animals inside at once; none: no limit.
    pub cap: Option<u32>,
    pub contain: Contain,
    /// The only animals placed in the zone; empty: any.
    pub allow: Vec<String>,
    /// How readily each animal is placed in the zone, against 1 elsewhere.
    pub weights: Vec<(String, f32)>,
    /// Wandering and fleeing animals keep out of the zone.
    pub avoid: bool,
    /// What differs at each time of day ([`TIMES`]).
    pub at: [ZoneOverride; 3],
}

/// What a zone's rule changes at one time of day; unset: as the rule.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZoneOverride {
    pub no_spawn: Option<bool>,
    pub cap: Option<u32>,
    /// The allow list instead (empty: any animal).
    pub allow: Option<Vec<String>>,
    /// Weights by animal; the rule's others are kept.
    pub weights: Vec<(String, f32)>,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// `<number> <name>`: the number and the rest of the line.
fn counted<T: core::str::FromStr>(rest: &str) -> Option<(T, &str)> {
    let (n, name) = rest.split_once(char::is_whitespace)?;
    let name = name.trim();
    (!name.is_empty()).then_some(())?;
    Some((n.parse().ok()?, name))
}

fn cap(s: &str) -> Option<u32> {
    s.trim()
        .parse::<u32>()
        .ok()
        .map(|n| n.clamp(CAP_RANGE.0, CAP_RANGE.1))
}

impl ZoneRule {
    /// Whether the rule changes anything.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The weight given to `name` (1 when it has none).
    pub fn weight(&self, name: &str) -> f32 {
        self.weights
            .iter()
            .find(|(n, _)| same(n, name))
            .map_or(1.0, |(_, w)| *w)
    }

    /// Reads the rest of one `eco` line; bad values and unknown words are
    /// skipped.
    pub(crate) fn line(&mut self, rest: &str) {
        let (key, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let rest = rest.trim();
        match key.to_ascii_lowercase().as_str() {
            "no_spawn" => self.no_spawn = rest != "0",
            "cap" => self.cap = cap(rest).or(self.cap),
            "contain" => self.contain = Contain::parse(rest).unwrap_or(self.contain),
            "allow" if !rest.is_empty() && !self.allow.iter().any(|n| same(n, rest)) => {
                self.allow.push(rest.into());
            }
            "avoid" => self.avoid = rest != "0",
            "weight" => push_weight(&mut self.weights, rest),
            _ => {}
        }
    }

    /// The rule in force at time of day `time` (an index into [`TIMES`]):
    /// the overrides merged in, and none left.
    pub fn at_time(&self, time: usize) -> ZoneRule {
        let mut r = ZoneRule {
            at: Default::default(),
            ..self.clone()
        };
        let Some(o) = self.at.get(time) else { return r };
        r.no_spawn = o.no_spawn.unwrap_or(r.no_spawn);
        r.cap = o.cap.or(r.cap);
        if let Some(a) = &o.allow {
            r.allow = a.clone();
        }
        for (n, w) in &o.weights {
            match r.weights.iter_mut().find(|(x, _)| same(x, n)) {
                Some(x) => x.1 = *w,
                None => r.weights.push((n.clone(), *w)),
            }
        }
        r
    }

    /// The rule's lines.
    pub(crate) fn write(&self, s: &mut String) {
        if self.no_spawn {
            s.push_str("eco no_spawn 1\n");
        }
        if let Some(c) = self.cap {
            s.push_str(&format!("eco cap {c}\n"));
        }
        if self.contain != Contain::Free {
            s.push_str(&format!("eco contain {}\n", self.contain.name()));
        }
        for n in &self.allow {
            s.push_str(&format!("eco allow {n}\n"));
        }
        for (n, w) in &self.weights {
            s.push_str(&format!("eco weight {w} {n}\n"));
        }
        if self.avoid {
            s.push_str("eco avoid 1\n");
        }
        for (t, o) in TIMES.iter().zip(&self.at) {
            o.write(t, s);
        }
    }
}

fn push_weight(list: &mut Vec<(String, f32)>, rest: &str) {
    let Some((w, name)) = counted::<f32>(rest).filter(|(w, _)| w.is_finite()) else {
        return;
    };
    if !list.iter().any(|(n, _)| same(n, name)) {
        list.push((name.into(), w.clamp(WEIGHT_RANGE.0, WEIGHT_RANGE.1)));
    }
}

impl ZoneOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Reads the rest of one `at <time> eco` line.
    pub(crate) fn line(&mut self, rest: &str) {
        let (key, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let rest = rest.trim();
        match key.to_ascii_lowercase().as_str() {
            "no_spawn" => self.no_spawn = Some(rest != "0"),
            "cap" => self.cap = cap(rest).or(self.cap),
            "allow_any" => self.allow = Some(Vec::new()),
            "allow" if !rest.is_empty() => {
                let list = self.allow.get_or_insert_with(Vec::new);
                if !list.iter().any(|n| same(n, rest)) {
                    list.push(rest.into());
                }
            }
            "weight" => push_weight(&mut self.weights, rest),
            _ => {}
        }
    }

    fn write(&self, time: &str, s: &mut String) {
        if let Some(n) = self.no_spawn {
            s.push_str(&format!("at {time} eco no_spawn {}\n", u8::from(n)));
        }
        if let Some(c) = self.cap {
            s.push_str(&format!("at {time} eco cap {c}\n"));
        }
        match &self.allow {
            Some(a) if a.is_empty() => s.push_str(&format!("at {time} eco allow_any 1\n")),
            Some(a) => {
                for n in a {
                    s.push_str(&format!("at {time} eco allow {n}\n"));
                }
            }
            None => {}
        }
        for (n, w) in &self.weights {
            s.push_str(&format!("at {time} eco weight {w} {n}\n"));
        }
    }
}

/// Caps on a map: the most living animals, in all and of one kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// Most living animals at once; none: no limit.
    pub max_animals: Option<u32>,
    /// Most living animals of one kind: its name and the count.
    pub species: Vec<(String, u32)>,
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        self.max_animals.is_none() && self.species.is_empty()
    }

    /// Reads one of the caps' lines. False: `key` is not one.
    fn line(&mut self, key: &str, rest: &str) -> bool {
        match key {
            "max_animals" => self.max_animals = cap(rest).or(self.max_animals),
            "species_max" => {
                if let Some((n, name)) = counted::<u32>(rest) {
                    if !self.species.iter().any(|(s, _)| same(s, name)) {
                        self.species
                            .push((name.into(), n.clamp(CAP_RANGE.0, CAP_RANGE.1)));
                    }
                }
            }
            _ => return false,
        }
        true
    }

    fn write(&self, prefix: &str, s: &mut String) {
        if let Some(m) = self.max_animals {
            s.push_str(&format!("{prefix}max_animals {m}\n"));
        }
        for (name, n) in &self.species {
            s.push_str(&format!("{prefix}species_max {n} {name}\n"));
        }
    }
}

/// The caps on the whole map, whatever the area.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caps {
    /// Most living animals at once; none: no limit.
    pub max_animals: Option<u32>,
    /// Most living animals of one kind: its name and the count.
    pub species: Vec<(String, u32)>,
    /// What differs at each time of day ([`TIMES`]).
    pub at: [Limits; 3],
}

impl Caps {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn base(&self) -> Limits {
        Limits {
            max_animals: self.max_animals,
            species: self.species.clone(),
        }
    }

    /// The caps in force at time of day `time` (an index into [`TIMES`]).
    pub fn at_time(&self, time: usize) -> Limits {
        let mut l = self.base();
        let Some(o) = self.at.get(time) else { return l };
        l.max_animals = o.max_animals.or(l.max_animals);
        for (n, max) in &o.species {
            match l.species.iter_mut().find(|(x, _)| same(x, n)) {
                Some(x) => x.1 = *max,
                None => l.species.push((n.clone(), *max)),
            }
        }
        l
    }

    /// Reads one of the caps' lines. False: `key` is not one.
    pub(crate) fn line(&mut self, key: &str, rest: &str) -> bool {
        let mut l = self.base();
        let known = l.line(key, rest);
        self.max_animals = l.max_animals;
        self.species = l.species;
        known
    }

    /// Reads an `at <time>` caps line. False: not one.
    pub(crate) fn line_at(&mut self, time: usize, key: &str, rest: &str) -> bool {
        self.at.get_mut(time).is_some_and(|l| l.line(key, rest))
    }

    pub(crate) fn write(&self, s: &mut String) {
        self.base().write("", s);
        for (t, l) in TIMES.iter().zip(&self.at) {
            l.write(&format!("at {t} "), s);
        }
    }
}

/// How far from the hunter a new animal must be. Unset values are the
/// original's ([`DEFAULT_DISTANCE`], seen or not).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Safety {
    /// At the start of a hunt, cells.
    pub initial: Option<u32>,
    /// A replacement for a killed animal, cells.
    pub respawn: Option<u32>,
    /// The instance rules' extra animals, cells.
    pub extra: Option<u32>,
    /// Replacements appear only where the hunter is not looking.
    pub unseen: bool,
    /// A kind's own distance, when larger: its name and the cells.
    pub species: Vec<(String, u32)>,
}

impl Safety {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Reads the rest of one `safety` line.
    pub(crate) fn line(&mut self, rest: &str) {
        let (key, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let rest = rest.trim();
        let cells = |s: &str| {
            s.parse::<u32>()
                .ok()
                .map(|n| n.clamp(DISTANCE_RANGE.0, DISTANCE_RANGE.1))
        };
        match key.to_ascii_lowercase().as_str() {
            "initial" => self.initial = cells(rest).or(self.initial),
            "respawn" => self.respawn = cells(rest).or(self.respawn),
            "extra" => self.extra = cells(rest).or(self.extra),
            "unseen" => self.unseen = rest != "0",
            "species" => {
                if let Some((n, name)) = counted::<u32>(rest) {
                    if !self.species.iter().any(|(s, _)| same(s, name)) {
                        self.species
                            .push((name.into(), n.clamp(DISTANCE_RANGE.0, DISTANCE_RANGE.1)));
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn write(&self, s: &mut String) {
        for (key, v) in [
            ("initial", self.initial),
            ("respawn", self.respawn),
            ("extra", self.extra),
        ] {
            if let Some(v) = v {
                s.push_str(&format!("safety {key} {v}\n"));
            }
        }
        if self.unseen {
            s.push_str("safety unseen 1\n");
        }
        for (name, n) in &self.species {
            s.push_str(&format!("safety species {n} {name}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn a_rule_round_trips_through_its_lines() {
        let r = ZoneRule {
            no_spawn: true,
            cap: Some(6),
            contain: Contain::Hard,
            allow: vec!["Stegosaurus".into(), "Tyrannosaurus Rex".into()],
            weights: vec![
                ("Tyrannosaurus Rex".into(), 0.25),
                ("Raptor #2".into(), 3.0),
            ],
            avoid: true,
            at: Default::default(),
        };
        let mut text = String::new();
        r.write(&mut text);
        let mut back = ZoneRule::default();
        for l in text.lines() {
            back.line(l.strip_prefix("eco ").unwrap_or(l));
        }
        assert_eq!(back, r);
        assert!(!r.is_empty() && ZoneRule::default().is_empty());
        assert_eq!(r.weight("tyrannosaurus rex"), 0.25);
        assert_eq!(r.weight("Stegosaurus"), 1.0);
    }

    #[test]
    fn bad_values_are_skipped_and_numbers_clamped() {
        let mut r = ZoneRule::default();
        for l in [
            "cap many",
            "cap 9000",
            "contain firmly",
            "contain SOFT",
            "allow",
            "allow Stego",
            "allow stego",
            "weight x Stego",
            "weight 99 Stego",
            "weight 2 stego",
            "weight 3",
            "bogus 1",
        ] {
            r.line(l);
        }
        assert_eq!(r.cap, Some(CAP_RANGE.1));
        assert_eq!(r.contain, Contain::Soft);
        assert_eq!(r.allow, ["Stego"]);
        assert_eq!(r.weights, [("Stego".into(), WEIGHT_RANGE.1)]);
        assert!(!r.no_spawn);
    }

    #[test]
    fn caps_read_their_own_lines_only() {
        let mut c = Caps::default();
        assert!(c.line("max_animals", "40"));
        assert!(c.line("species_max", "4 Velociraptor"));
        assert!(c.line("species_max", "9 velociraptor"), "repeated: skipped");
        assert!(c.line("species_max", "4"), "no name: skipped");
        assert!(!c.line("zone", "a circle 1 1 5"));
        assert_eq!(c.max_animals, Some(40));
        assert_eq!(c.species, [("Velociraptor".into(), 4)]);
        let mut text = String::new();
        c.write(&mut text);
        assert_eq!(text, "max_animals 40\nspecies_max 4 Velociraptor\n");
    }

    #[test]
    fn containment_names_parse_whatever_the_case() {
        for (c, n) in Contain::NAMES {
            assert_eq!(Contain::parse(&n.to_ascii_uppercase()), Some(c));
            assert_eq!(c.name(), n);
        }
        assert_eq!(Contain::parse(" "), Some(Contain::Free));
        assert_eq!(Contain::parse("tight"), None);
    }

    #[test]
    fn overrides_merge_over_the_rule_key_by_key() {
        let mut r = ZoneRule {
            cap: Some(6),
            allow: vec!["Stego".into()],
            weights: vec![("Stego".into(), 2.0), ("Rex".into(), 0.5)],
            ..ZoneRule::default()
        };
        // Night: no cap line, so the rule's; a weight replaced, one added.
        for l in [
            "no_spawn 0",
            "weight 4 stego",
            "weight 3 Para",
            "allow_any 1",
        ] {
            r.at[2].line(l);
        }
        r.at[0].line("cap 1");
        r.at[0].line("no_spawn 1");
        let night = r.at_time(2);
        assert_eq!((night.cap, night.no_spawn), (Some(6), false));
        assert!(night.allow.is_empty(), "any animal at night");
        assert_eq!(
            night.weights,
            [
                ("Stego".into(), 4.0),
                ("Rex".into(), 0.5),
                ("Para".into(), 3.0)
            ]
        );
        let dawn = r.at_time(0);
        assert_eq!((dawn.cap, dawn.no_spawn), (Some(1), true));
        assert_eq!(dawn.allow, ["Stego"]);
        // By day nothing is overridden; an unknown time is the rule itself.
        let base = ZoneRule {
            at: Default::default(),
            ..r.clone()
        };
        assert_eq!(r.at_time(1), base);
        assert_eq!(r.at_time(9), base);
    }

    #[test]
    fn caps_inherit_then_override_by_time() {
        let mut c = Caps::default();
        c.line("max_animals", "40");
        c.line("species_max", "4 Raptor");
        assert!(c.line_at(2, "species_max", "8 raptor"));
        assert!(c.line_at(2, "species_max", "1 Rex"));
        assert!(c.line_at(0, "max_animals", "10"));
        assert!(!c.line_at(0, "zone", "x") && !c.line_at(7, "max_animals", "1"));
        assert_eq!(c.at_time(1).max_animals, Some(40));
        assert_eq!(c.at_time(0).max_animals, Some(10));
        assert_eq!(c.at_time(0).species, [("Raptor".into(), 4)]);
        assert_eq!(
            c.at_time(2).species,
            [("Raptor".into(), 8), ("Rex".into(), 1)]
        );
        assert_eq!(c.at_time(2).max_animals, Some(40));
    }

    #[test]
    fn safety_lines_round_trip_and_clamp() {
        let mut s = Safety::default();
        for l in [
            "initial 60",
            "respawn 9999",
            "extra x",
            "unseen 1",
            "species 80 Tyrannosaurus Rex",
            "species 10 tyrannosaurus rex",
            "bogus 3",
        ] {
            s.line(l);
        }
        assert_eq!(
            (s.initial, s.respawn, s.extra, s.unseen),
            (Some(60), Some(DISTANCE_RANGE.1), None, true)
        );
        assert_eq!(s.species, [("Tyrannosaurus Rex".into(), 80)]);
        let mut text = String::new();
        s.write(&mut text);
        let mut back = Safety::default();
        for l in text.lines() {
            back.line(l.strip_prefix("safety ").unwrap_or(l));
        }
        assert_eq!(back, s);
        assert!(Safety::default().is_empty() && !s.is_empty());
    }
}
