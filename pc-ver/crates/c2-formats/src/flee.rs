//! `HUNTDAT/_FLEE.TXT`: the limits of "bounded fleeing" (not in the original
//! game). The launcher writes it from a recipe's `[settings.flee]`; the
//! engine reads it at the start of a hunt when the player has bounded
//! fleeing switched on. When the file is missing, built-in defaults apply.
//!
//! One setting per line, `key value`; `;` starts a comment. Keys before the
//! first `animal` line are global; an `animal <name>` line (the name as
//! `_RES.TXT` gives it, case ignored) starts a section whose keys apply to
//! that animal only. Unset keys inherit the global value, then the built-in
//! default.
//!
//! ```text
//! max_distance 24        ; cells a flight may cover
//! max_duration 20        ; seconds a flight may last
//! chase_extension 10     ; seconds a still-pursuing hunter adds
//! retrigger_lockout 4    ; seconds a noise cannot re-arm fear
//! animal Parasaurolophus
//! max_distance 40
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

/// The path the settings are read from.
pub const PATH: &str = "HUNTDAT/_FLEE.TXT";

/// Built-in values, used where neither the animal nor the file sets one.
pub const DEFAULT_DISTANCE: u32 = 24;
pub const DEFAULT_DURATION: u32 = 20;
pub const DEFAULT_CHASE: u32 = 10;
pub const DEFAULT_LOCKOUT: u32 = 4;

/// Accepted ranges (cells, seconds, seconds, seconds); values are clamped.
pub const DISTANCE_RANGE: (u32, u32) = (4, 200);
pub const DURATION_RANGE: (u32, u32) = (2, 300);
pub const CHASE_RANGE: (u32, u32) = (0, 120);
pub const LOCKOUT_RANGE: (u32, u32) = (0, 60);

/// Four optional limits: `None` inherits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Cells a flight may cover from where it began.
    pub max_distance: Option<u32>,
    /// Seconds a flight may last.
    pub max_duration: Option<u32>,
    /// Seconds a still-pursuing hunter may extend a flight by.
    pub chase_extension: Option<u32>,
    /// Seconds after a scare during which noises cannot re-arm fear.
    pub retrigger_lockout: Option<u32>,
}

impl Limits {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Every unset field of `self` taken from `base`.
    pub fn over(self, base: Limits) -> Limits {
        Limits {
            max_distance: self.max_distance.or(base.max_distance),
            max_duration: self.max_duration.or(base.max_duration),
            chase_extension: self.chase_extension.or(base.chase_extension),
            retrigger_lockout: self.retrigger_lockout.or(base.retrigger_lockout),
        }
    }

    /// Values brought into their accepted ranges.
    pub fn clamped(self) -> Limits {
        let c = |v: Option<u32>, r: (u32, u32)| v.map(|v| v.clamp(r.0, r.1));
        Limits {
            max_distance: c(self.max_distance, DISTANCE_RANGE),
            max_duration: c(self.max_duration, DURATION_RANGE),
            chase_extension: c(self.chase_extension, CHASE_RANGE),
            retrigger_lockout: c(self.retrigger_lockout, LOCKOUT_RANGE),
        }
    }

    /// Every field set: the file's value, else the built-in default.
    pub fn resolved(self) -> [u32; 4] {
        [
            self.max_distance.unwrap_or(DEFAULT_DISTANCE),
            self.max_duration.unwrap_or(DEFAULT_DURATION),
            self.chase_extension.unwrap_or(DEFAULT_CHASE),
            self.retrigger_lockout.unwrap_or(DEFAULT_LOCKOUT),
        ]
    }

    fn set(&mut self, key: &str, value: u32) -> bool {
        match key {
            "max_distance" => self.max_distance = Some(value),
            "max_duration" => self.max_duration = Some(value),
            "chase_extension" => self.chase_extension = Some(value),
            "retrigger_lockout" => self.retrigger_lockout = Some(value),
            _ => return false,
        }
        true
    }

    fn lines(&self, out: &mut String) {
        for (key, v) in [
            ("max_distance", self.max_distance),
            ("max_duration", self.max_duration),
            ("chase_extension", self.chase_extension),
            ("retrigger_lockout", self.retrigger_lockout),
        ] {
            if let Some(v) = v {
                out.push_str(&format!("{key} {v}\n"));
            }
        }
    }
}

/// What the original game does when an animal is scared, for display in the
/// launcher beside the custom limits (the original has no distance or
/// duration limit). `c2-core` checks the numbers against the AI code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vanilla {
    /// Whether bounded fleeing changes this animal's AI at all (the
    /// original's carnivores keep their own chase and retreat).
    pub affected: bool,
    /// The fear timer a hunter walking up to it sets, ms (min, max); none:
    /// it is alarmed some other way.
    pub alarm_ms: Option<(i32, i32)>,
    /// A call or danger sound sets the fear timer to this, ms (min, max).
    pub call_ms: (i32, i32),
    /// The timer is re-armed to this every frame while the hunter is within
    /// [`NEAR_UNITS`] (ms), so it never runs out while he stays close.
    pub refresh_ms: Option<i32>,
}

/// Units between target and animal for one leg of a flight: the original's.
pub const LEG_UNITS: i32 = 2048;
/// How close the hunter must be to keep a refreshed fear timer armed.
pub const NEAR_UNITS: i32 = 6000;
/// The original's AI numbers.
const MOSH: i32 = 1;
const GALL: i32 = 2;
const DIMET: i32 = 5;
const PARA: i32 = 10;
const ANKY: i32 = 11;
const STEGO: i32 = 12;
const CHASM: i32 = 14;

/// The original's scare timers for the animal running AI `ai`.
pub fn vanilla(ai: i32) -> Vanilla {
    let call_ms = (10 * 1024, 14 * 1024);
    match ai {
        PARA | ANKY | STEGO => Vanilla {
            affected: true,
            alarm_ms: Some((6 * 1024, 13 * 1024)),
            call_ms,
            refresh_ms: None,
        },
        CHASM => Vanilla {
            affected: true,
            alarm_ms: None,
            call_ms,
            refresh_ms: Some(8000),
        },
        MOSH | GALL | DIMET => Vanilla {
            affected: true,
            alarm_ms: Some((5 * 1024, 9 * 1024)),
            call_ms,
            refresh_ms: None,
        },
        // The raptors, Allosaurus, Spinosaurus, Ceratosaurus and T-Rex keep
        // their own chase and retreat.
        13 | 15 | 16 | 17 | 18 => Vanilla {
            affected: false,
            alarm_ms: None,
            call_ms,
            refresh_ms: None,
        },
        // The Modder's Edition Engine's hunting AI: the proximity refresh.
        _ => Vanilla {
            affected: true,
            alarm_ms: None,
            call_ms,
            refresh_ms: Some(8000),
        },
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flee {
    pub global: Limits,
    /// Per-animal overrides, by resource name.
    pub animals: Vec<(String, Limits)>,
}

impl Flee {
    /// The limits for the animal called `name`: its own, then the global
    /// ones, then the built-in defaults.
    pub fn for_animal(&self, name: &str) -> [u32; 4] {
        let name = name.trim();
        let own = self
            .animals
            .iter()
            .find(|(n, _)| n.trim().eq_ignore_ascii_case(name))
            .map(|(_, l)| *l)
            .unwrap_or_default();
        own.over(self.global).resolved()
    }

    /// Reads the file. Unknown keys and bad values are skipped, so a newer
    /// file still loads; values are clamped into their ranges.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut f = Self::default();
        let mut current: Option<usize> = None;
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let Some((key, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let (key, rest) = (key.to_ascii_lowercase(), rest.trim());
            if key == "animal" {
                if rest.is_empty() {
                    current = None;
                } else {
                    f.animals.push((rest.into(), Limits::default()));
                    current = Some(f.animals.len() - 1);
                }
                continue;
            }
            let Ok(v) = rest.parse::<u32>() else {
                continue;
            };
            match current {
                Some(i) => f.animals[i].1.set(&key, v),
                None => f.global.set(&key, v),
            };
        }
        f.global = f.global.clamped();
        for (_, l) in &mut f.animals {
            *l = l.clamped();
        }
        f
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut s = String::from("; Bounded fleeing (from the recipe's [settings.flee])\n");
        self.global.lines(&mut s);
        for (name, l) in self.animals.iter().filter(|(_, l)| !l.is_empty()) {
            s.push_str(&format!("animal {name}\n"));
            l.lines(&mut s);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let f = Flee {
            global: Limits {
                max_distance: Some(30),
                chase_extension: Some(5),
                ..Limits::default()
            },
            animals: vec![(
                "Parasaurolophus".into(),
                Limits {
                    max_duration: Some(40),
                    ..Limits::default()
                },
            )],
        };
        assert_eq!(Flee::parse(f.to_text().as_bytes()), f);
    }

    #[test]
    fn animals_inherit_global_then_defaults() {
        let f = Flee::parse(b"max_distance 30\nanimal Rex\nmax_duration 60\nanimal Hadro\n");
        assert_eq!(
            f.for_animal("rex"),
            [30, 60, DEFAULT_CHASE, DEFAULT_LOCKOUT]
        );
        assert_eq!(f.for_animal("hadro")[0], 30);
        assert_eq!(f.for_animal("other")[1], DEFAULT_DURATION);
    }

    #[test]
    fn junk_is_skipped_and_values_clamped() {
        let f =
            Flee::parse(b"bogus 1\nmax_distance x\nmax_distance 100000 ; big\nmax_duration 0\n");
        assert_eq!(f.global.max_distance, Some(DISTANCE_RANGE.1));
        assert_eq!(f.global.max_duration, Some(DURATION_RANGE.0));
    }
}
