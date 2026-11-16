//! `HUNTDAT/_BEHAVIOR.TXT`: Advanced Dinosaur Behavior settings (not in the
//! original game; design in `docs/BEHAVIOR.md`). The launcher writes it from
//! a recipe's `[settings.behavior]`; the engine reads it at the start of a
//! hunt. A game without the file plays the original's behaviour.
//!
//! So far the file holds one part, awareness: an animal half-notices the
//! hunter before it is alarmed, walks over to look, and a predator that
//! loses him goes to where it last knew him to be and searches.
//!
//! One setting per line, `key value`; `;` starts a comment. Keys before the
//! first `animal` line are global; an `animal <name>` line (the name as
//! `_RES.TXT` gives it, case ignored) starts a section whose keys apply to
//! that animal only. Unset keys inherit the global value, then the built-in
//! default.
//!
//! ```text
//! awareness 1            ; the part is on
//! suspicion 160          ; half-notices up to this percent of the alarm threshold
//! investigate_after 2    ; seconds of suspicion before it goes to look
//! search 20              ; seconds a predator searches (and a look may last)
//! animal Velociraptor
//! search 40
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

/// The path the settings are read from.
pub const PATH: &str = "HUNTDAT/_BEHAVIOR.TXT";

/// Built-in values, used where neither the animal nor the file sets one.
pub const DEFAULT_SUSPICION: u32 = 160;
pub const DEFAULT_INVESTIGATE: u32 = 2;
pub const DEFAULT_SEARCH: u32 = 20;

/// Accepted ranges (percent, seconds, seconds); values are clamped. A
/// suspicion of 100 leaves no band below the alarm threshold: the animal
/// never half-notices.
pub const SUSPICION_RANGE: (u32, u32) = (100, 300);
pub const INVESTIGATE_RANGE: (u32, u32) = (0, 30);
pub const SEARCH_RANGE: (u32, u32) = (0, 300);

/// One animal's awareness values: `None` inherits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Awareness {
    /// Percent of the alarm threshold up to which it half-notices.
    pub suspicion: Option<u32>,
    /// Seconds it stays suspicious before it goes to look.
    pub investigate_after: Option<u32>,
    /// Seconds a search (or a look) lasts.
    pub search: Option<u32>,
}

impl Awareness {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Every unset field of `self` taken from `base`.
    pub fn over(self, base: Awareness) -> Awareness {
        Awareness {
            suspicion: self.suspicion.or(base.suspicion),
            investigate_after: self.investigate_after.or(base.investigate_after),
            search: self.search.or(base.search),
        }
    }

    /// Values brought into their accepted ranges.
    pub fn clamped(self) -> Awareness {
        let c = |v: Option<u32>, r: (u32, u32)| v.map(|v| v.clamp(r.0, r.1));
        Awareness {
            suspicion: c(self.suspicion, SUSPICION_RANGE),
            investigate_after: c(self.investigate_after, INVESTIGATE_RANGE),
            search: c(self.search, SEARCH_RANGE),
        }
    }

    /// Every field set: the file's value, else the built-in default
    /// (suspicion, investigate_after, search).
    pub fn resolved(self) -> [u32; 3] {
        [
            self.suspicion.unwrap_or(DEFAULT_SUSPICION),
            self.investigate_after.unwrap_or(DEFAULT_INVESTIGATE),
            self.search.unwrap_or(DEFAULT_SEARCH),
        ]
    }

    fn set(&mut self, key: &str, value: u32) -> bool {
        match key {
            "suspicion" => self.suspicion = Some(value),
            "investigate_after" => self.investigate_after = Some(value),
            "search" => self.search = Some(value),
            _ => return false,
        }
        true
    }

    fn lines(&self, out: &mut String) {
        for (key, v) in [
            ("suspicion", self.suspicion),
            ("investigate_after", self.investigate_after),
            ("search", self.search),
        ] {
            if let Some(v) = v {
                out.push_str(&format!("{key} {v}\n"));
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Behavior {
    /// The awareness part is on.
    pub awareness: bool,
    pub global: Awareness,
    /// Per-animal overrides, by resource name.
    pub animals: Vec<(String, Awareness)>,
}

impl Behavior {
    /// The awareness values for the animal called `name`: its own, then the
    /// global ones, then the built-in defaults.
    pub fn for_animal(&self, name: &str) -> [u32; 3] {
        let name = name.trim();
        let own = self
            .animals
            .iter()
            .find(|(n, _)| n.trim().eq_ignore_ascii_case(name))
            .map(|(_, a)| *a)
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
                    f.animals.push((rest.into(), Awareness::default()));
                    current = Some(f.animals.len() - 1);
                }
                continue;
            }
            let Ok(v) = rest.parse::<u32>() else {
                continue;
            };
            match (key.as_str(), current) {
                ("awareness", None) => f.awareness = v != 0,
                (_, Some(i)) => {
                    f.animals[i].1.set(&key, v);
                }
                (_, None) => {
                    f.global.set(&key, v);
                }
            }
        }
        f.global = f.global.clamped();
        for (_, a) in &mut f.animals {
            *a = a.clamped();
        }
        f
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut s = String::from("; Advanced behavior (from the recipe's [settings.behavior])\n");
        s.push_str(&format!("awareness {}\n", u8::from(self.awareness)));
        self.global.lines(&mut s);
        for (name, a) in self.animals.iter().filter(|(_, a)| !a.is_empty()) {
            s.push_str(&format!("animal {name}\n"));
            a.lines(&mut s);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let f = Behavior {
            awareness: true,
            global: Awareness {
                suspicion: Some(200),
                ..Awareness::default()
            },
            animals: vec![(
                "Velociraptor".into(),
                Awareness {
                    search: Some(40),
                    ..Awareness::default()
                },
            )],
        };
        assert_eq!(Behavior::parse(f.to_text().as_bytes()), f);
        assert_eq!(Behavior::parse(b""), Behavior::default());
    }

    #[test]
    fn animals_inherit_global_then_defaults() {
        let f = Behavior::parse(b"awareness 1\nsearch 30\nanimal Rex\nsuspicion 250\n");
        assert!(f.awareness);
        assert_eq!(f.for_animal("rex"), [250, DEFAULT_INVESTIGATE, 30]);
        assert_eq!(
            f.for_animal("other"),
            [DEFAULT_SUSPICION, DEFAULT_INVESTIGATE, 30]
        );
    }

    #[test]
    fn junk_is_skipped_and_values_clamped() {
        let f = Behavior::parse(b"bogus 1\nsearch x\nsearch 100000 ; long\nsuspicion 5\n");
        assert!(!f.awareness);
        assert_eq!(f.global.search, Some(SEARCH_RANGE.1));
        assert_eq!(f.global.suspicion, Some(SUSPICION_RANGE.0));
    }
}
