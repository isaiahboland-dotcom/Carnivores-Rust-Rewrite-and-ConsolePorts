//! The hardcore hunt ruleset (not in the original game): the `hunt` lines of
//! `HUNTDAT/_RULES.TXT` ([`crate::rules`]), written by the launcher from a
//! recipe's `[settings.hunt]`. Every rule is independent; with no `hunt`
//! line nothing differs from the game. Design: `docs/HARDCORE-HUNT.md`.
//!
//! ```text
//! hunt no_night_vision
//! hunt no_radar
//! hunt no_cover_scent
//! hunt max_firearms 2
//! hunt ammo_drops 3
//! hunt extraction 5 30 30 48       ; arrive s, hover s, cooldown s, zone gap (cells)
//! hunt knife dodge                 ; `hunt knife`: without the dodge
//! hunt hesitation 10 90            ; seconds, back angle (degrees)
//! hunt second_predator 30 300      ; window s, menu price above which it kills
//! hunt predator circle_lunge Velociraptor
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

/// Extraction defaults: seconds to arrive, to hover, to cool down, and the
/// least distance between the drop-off and pickup spots (cells).
pub const ARRIVE_S: u32 = 5;
pub const HOVER_S: u32 = 30;
pub const COOLDOWN_S: u32 = 30;
pub const ZONE_GAP: u32 = 48;
/// Hesitation defaults: seconds circling, and the angle (degrees) from the
/// hunter's view direction past which his back is turned.
pub const HESITATION_S: u32 = 10;
pub const BACK_ANGLE: u32 = 90;
/// Second predator defaults: the encounter window (s) and the menu price
/// above which it kills.
pub const WINDOW_S: u32 = 30;
pub const MIN_PRICE: u32 = 300;

/// Accepted ranges; values are clamped.
pub const SECONDS_RANGE: (u32, u32) = (1, 600);
pub const ZONE_GAP_RANGE: (u32, u32) = (8, 400);
pub const BACK_ANGLE_RANGE: (u32, u32) = (30, 170);
pub const FIREARMS_RANGE: (u32, u32) = (0, 32);
pub const DROPS_RANGE: (u32, u32) = (0, 32);

/// The pickup ship's timings and the zones' spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extraction {
    pub arrive_s: u32,
    pub hover_s: u32,
    pub cooldown_s: u32,
    /// Cells between the drop-off and pickup spots, at least.
    pub zone_gap: u32,
}

impl Default for Extraction {
    fn default() -> Self {
        Self {
            arrive_s: ARRIVE_S,
            hover_s: HOVER_S,
            cooldown_s: COOLDOWN_S,
            zone_gap: ZONE_GAP,
        }
    }
}

/// What a carnivore that reaches the hunter does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Circles; lunges when the hunter's back is turned.
    CircleLunge,
    /// Circles; makes its normal kill when the hunter's back is turned.
    CircleEat,
    /// The original attack, no hesitation.
    Ignore,
}

impl Reaction {
    pub const ALL: [Reaction; 3] = [Reaction::CircleLunge, Reaction::CircleEat, Reaction::Ignore];

    pub fn key(self) -> &'static str {
        match self {
            Reaction::CircleLunge => "circle_lunge",
            Reaction::CircleEat => "circle_eat",
            Reaction::Ignore => "ignore",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.key().eq_ignore_ascii_case(k))
    }
}

/// The ruleset. `Default`: every rule off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hunt {
    pub no_night_vision: bool,
    pub no_radar: bool,
    pub no_cover_scent: bool,
    /// At most this many weapons carried.
    pub max_firearms: Option<u32>,
    /// Ammunition drops placed per hunt (0: none).
    pub ammo_drops: u32,
    pub extraction: Option<Extraction>,
    /// The knife, offered when every firearm is empty; and its dodge.
    pub knife: bool,
    pub knife_dodge: bool,
    /// Seconds a carnivore circles the hunter (0: no hesitation).
    pub hesitation_s: u32,
    /// Degrees from the view direction past which the back is turned.
    pub back_angle: u32,
    /// The encounter window (s) and the price above which a second
    /// predator attacks at once.
    pub second_predator: Option<(u32, u32)>,
    /// Per-species reactions, by resource name; unlisted: by size.
    pub predators: Vec<(String, Reaction)>,
}

fn clamp(v: u32, r: (u32, u32)) -> u32 {
    v.clamp(r.0, r.1)
}

impl Hunt {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The reaction set for the animal called `name`, if the rules list it.
    pub fn reaction(&self, name: &str) -> Option<Reaction> {
        let name = name.trim();
        self.predators
            .iter()
            .find(|(n, _)| n.trim().eq_ignore_ascii_case(name))
            .map(|&(_, r)| r)
    }

    /// The back angle in force (the default when the line gave none).
    pub fn back_angle(&self) -> u32 {
        if self.back_angle == 0 {
            BACK_ANGLE
        } else {
            self.back_angle
        }
    }

    /// Reads one `hunt` line's words (after the `hunt` key). Unknown keys
    /// and bad values are skipped; values are clamped.
    pub fn parse_line<'a>(&mut self, mut words: impl Iterator<Item = &'a str>) {
        let Some(key) = words.next() else { return };
        let mut num = |d: u32| {
            words
                .next()
                .and_then(|w| w.parse::<u32>().ok())
                .unwrap_or(d)
        };
        match key.to_ascii_lowercase().as_str() {
            "no_night_vision" => self.no_night_vision = num(1) != 0,
            "no_radar" => self.no_radar = num(1) != 0,
            "no_cover_scent" => self.no_cover_scent = num(1) != 0,
            "max_firearms" => {
                if let Some(Ok(n)) = words.next().map(str::parse::<u32>) {
                    self.max_firearms = Some(clamp(n, FIREARMS_RANGE));
                }
            }
            "ammo_drops" => self.ammo_drops = clamp(num(0), DROPS_RANGE),
            "extraction" => {
                self.extraction = Some(Extraction {
                    arrive_s: clamp(num(ARRIVE_S), SECONDS_RANGE),
                    hover_s: clamp(num(HOVER_S), SECONDS_RANGE),
                    cooldown_s: clamp(num(COOLDOWN_S), (0, SECONDS_RANGE.1)),
                    zone_gap: clamp(num(ZONE_GAP), ZONE_GAP_RANGE),
                });
            }
            "knife" => {
                self.knife = true;
                self.knife_dodge = words
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case("dodge"));
            }
            "hesitation" => {
                self.hesitation_s = clamp(num(HESITATION_S), SECONDS_RANGE);
                self.back_angle = clamp(num(BACK_ANGLE), BACK_ANGLE_RANGE);
            }
            "second_predator" => {
                self.second_predator = Some((clamp(num(WINDOW_S), SECONDS_RANGE), num(MIN_PRICE)));
            }
            "predator" => {
                let Some(r) = words.next().and_then(Reaction::from_key) else {
                    return;
                };
                let name = words.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    self.predators
                        .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
                    self.predators.push((name, r));
                }
            }
            _ => {}
        }
    }

    /// The `hunt` lines.
    pub fn lines(&self, s: &mut String) {
        for (on, key) in [
            (self.no_night_vision, "no_night_vision"),
            (self.no_radar, "no_radar"),
            (self.no_cover_scent, "no_cover_scent"),
        ] {
            if on {
                *s += &format!("hunt {key}\n");
            }
        }
        if let Some(n) = self.max_firearms {
            *s += &format!("hunt max_firearms {n}\n");
        }
        if self.ammo_drops > 0 {
            *s += &format!("hunt ammo_drops {}\n", self.ammo_drops);
        }
        if let Some(e) = self.extraction {
            *s += &format!(
                "hunt extraction {} {} {} {}\n",
                e.arrive_s, e.hover_s, e.cooldown_s, e.zone_gap
            );
        }
        if self.knife {
            *s += if self.knife_dodge {
                "hunt knife dodge\n"
            } else {
                "hunt knife\n"
            };
        }
        if self.hesitation_s > 0 {
            *s += &format!(
                "hunt hesitation {} {}\n",
                self.hesitation_s,
                self.back_angle()
            );
        }
        if let Some((w, p)) = self.second_predator {
            *s += &format!("hunt second_predator {w} {p}\n");
        }
        for (name, r) in &self.predators {
            *s += &format!("hunt predator {} {name}\n", r.key());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Rules;

    fn full() -> Hunt {
        Hunt {
            no_night_vision: true,
            no_radar: true,
            no_cover_scent: true,
            max_firearms: Some(2),
            ammo_drops: 3,
            extraction: Some(Extraction::default()),
            knife: true,
            knife_dodge: true,
            hesitation_s: 10,
            back_angle: 90,
            second_predator: Some((30, 300)),
            predators: vec![
                ("Velociraptor".into(), Reaction::CircleLunge),
                ("Tyrannosaurus Rex".into(), Reaction::CircleEat),
                ("Allosaurus".into(), Reaction::Ignore),
            ],
        }
    }

    #[test]
    fn round_trips_through_the_rules_file() {
        let r = Rules {
            hunt: full(),
            ..Rules::default()
        };
        let back = Rules::parse(r.to_text().as_bytes());
        assert_eq!(back, r);
        assert_eq!(
            back.hunt.reaction("tyrannosaurus rex"),
            Some(Reaction::CircleEat)
        );
        assert_eq!(back.hunt.reaction("Spinosaurus"), None);
    }

    #[test]
    fn no_hunt_lines_is_the_game_as_it_was() {
        let r = Rules::parse(b"times day\nlock teleport 1\n");
        assert!(r.hunt.is_empty());
        assert!(!r.to_text().contains("hunt "));
    }

    #[test]
    fn unknown_and_bad_hunt_lines_are_skipped_and_values_clamped() {
        let r = Rules::parse(
            b"hunt future_rule 3\nhunt\nhunt max_firearms x\nhunt predator leap Rex\n\
              hunt predator ignore\nhunt extraction 0 100000 30 1\nhunt hesitation 5 500\n",
        );
        let h = &r.hunt;
        assert_eq!(h.max_firearms, None);
        assert!(h.predators.is_empty());
        let e = h.extraction.unwrap_or_default();
        assert_eq!((e.arrive_s, e.hover_s, e.zone_gap), (1, 600, 8));
        assert_eq!((h.hesitation_s, h.back_angle), (5, 170));
    }

    #[test]
    fn missing_values_take_the_defaults() {
        let r =
            Rules::parse(b"hunt extraction\nhunt hesitation\nhunt second_predator\nhunt knife\n");
        let h = &r.hunt;
        assert_eq!(h.extraction, Some(Extraction::default()));
        assert_eq!((h.hesitation_s, h.back_angle()), (HESITATION_S, BACK_ANGLE));
        assert_eq!(h.second_predator, Some((WINDOW_S, MIN_PRICE)));
        assert!(h.knife && !h.knife_dodge);
    }
}
