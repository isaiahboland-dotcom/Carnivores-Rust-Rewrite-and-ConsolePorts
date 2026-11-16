//! A recipe's `[settings.hunt]` (the launcher's Advanced tab, "Hardcore
//! hunt"): an opt-in ruleset written into the built game as the `hunt`
//! lines of `HUNTDAT/_RULES.TXT` ([`c2_formats::hardcore`]). Each rule is
//! independent; absent or off, nothing differs from the game. Design:
//! `docs/HARDCORE-HUNT.md`.
//!
//! ```toml
//! [settings.hunt]
//! enabled = true
//! no_night_vision = true
//! no_radar = true
//! no_cover_scent = true            # the flashlight is always kept
//! max_firearms = 2
//! ammo_drops = 3                   # placed from the hunt's seed
//! hesitation_s = 10                # a carnivore circles before it attacks
//! back_angle = 90                  # degrees: past this, the back is turned
//! extraction = { arrive_s = 5, hover_s = 30, cooldown_s = 30, min_distance = 48 }
//! experimental_knife = true        # the knife is unfinished: off unless set here
//! knife = { dodge = true }
//! second_predator = { window_s = 30, min_price = 300 }
//!
//! [[settings.hunt.predators]]      # unlisted animals: by size
//! name = "Velociraptor"
//! reaction = "circle_lunge"        # or "circle_eat", "ignore"
//! ```

use serde::{Deserialize, Serialize};

use c2_formats::hardcore as fmt;

/// The pickup ship's timings; absent keys take the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Extraction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrive_s: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_s: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_s: Option<u32>,
    /// Cells between the drop-off and pickup spots, at least.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_distance: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Knife {
    pub dodge: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecondPredator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_s: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_price: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Predator {
    /// The animal's resource name (as `_RES.TXT` gives it).
    pub name: String,
    /// `circle_lunge`, `circle_eat` or `ignore`.
    pub reaction: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hunt {
    pub enabled: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_night_vision: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_radar: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_cover_scent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_firearms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ammo_drops: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hesitation_s: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_angle: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction: Option<Extraction>,
    /// The knife is unfinished (see `docs/HARDCORE-HUNT.md`, "Deferred"):
    /// `knife` has no effect unless this is set too, whatever else is on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub experimental_knife: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knife: Option<Knife>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_predator: Option<SecondPredator>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub predators: Vec<Predator>,
}

impl Hunt {
    /// Whether the section changes the built game.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.to_format().is_empty()
    }

    /// Whether the knife is written: its section and the explicit
    /// experimental switch.
    pub fn knife_on(&self) -> bool {
        self.experimental_knife && self.knife.is_some()
    }

    /// The rules file's form (off: empty). Values are clamped; predators
    /// without a name or a known reaction are left out.
    pub fn to_format(&self) -> fmt::Hunt {
        if !self.enabled {
            return fmt::Hunt::default();
        }
        let c = |v: u32, r: (u32, u32)| v.clamp(r.0, r.1);
        fmt::Hunt {
            no_night_vision: self.no_night_vision,
            no_radar: self.no_radar,
            no_cover_scent: self.no_cover_scent,
            max_firearms: self.max_firearms.map(|n| c(n, fmt::FIREARMS_RANGE)),
            ammo_drops: c(self.ammo_drops.unwrap_or(0), fmt::DROPS_RANGE),
            extraction: self.extraction.map(|e| fmt::Extraction {
                arrive_s: c(e.arrive_s.unwrap_or(fmt::ARRIVE_S), fmt::SECONDS_RANGE),
                hover_s: c(e.hover_s.unwrap_or(fmt::HOVER_S), fmt::SECONDS_RANGE),
                cooldown_s: e
                    .cooldown_s
                    .unwrap_or(fmt::COOLDOWN_S)
                    .min(fmt::SECONDS_RANGE.1),
                zone_gap: c(e.min_distance.unwrap_or(fmt::ZONE_GAP), fmt::ZONE_GAP_RANGE),
            }),
            knife: self.knife_on(),
            knife_dodge: self.knife_on() && self.knife.is_some_and(|k| k.dodge),
            hesitation_s: self.hesitation_s.map_or(0, |s| {
                if s == 0 {
                    0
                } else {
                    c(s, fmt::SECONDS_RANGE)
                }
            }),
            back_angle: if self.hesitation_s.unwrap_or(0) == 0 {
                0
            } else {
                c(
                    self.back_angle.unwrap_or(fmt::BACK_ANGLE),
                    fmt::BACK_ANGLE_RANGE,
                )
            },
            second_predator: self.second_predator.map(|p| {
                (
                    c(p.window_s.unwrap_or(fmt::WINDOW_S), fmt::SECONDS_RANGE),
                    p.min_price.unwrap_or(fmt::MIN_PRICE),
                )
            }),
            predators: self
                .predators
                .iter()
                .filter(|p| !p.name.trim().is_empty())
                .filter_map(|p| {
                    Some((
                        p.name.trim().into(),
                        fmt::Reaction::from_key(p.reaction.trim())?,
                    ))
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_formats::rules as rules_fmt;
    use c2_vfs::Vfs;

    const SECTION: &str = r#"
        enabled = true
        no_night_vision = true
        no_radar = true
        no_cover_scent = true
        max_firearms = 2
        ammo_drops = 3
        hesitation_s = 10
        extraction = { arrive_s = 5, hover_s = 30, cooldown_s = 30 }
        experimental_knife = true
        knife = { dodge = true }
        second_predator = { window_s = 30, min_price = 300 }
        [[predators]]
        name = "Velociraptor"
        reaction = "circle_lunge"
        [[predators]]
        name = "Tyrannosaurus Rex"
        reaction = "circle_eat"
        [[predators]]
        name = "Bad"
        reaction = "leap"
    "#;

    fn section() -> Hunt {
        toml::from_str(SECTION).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn a_recipe_section_becomes_hunt_lines_in_the_rules_file() {
        let h = section();
        let want = h.to_format();
        assert_eq!(want.predators.len(), 2, "an unknown reaction is left out");
        assert_eq!(want.extraction.map(|e| e.zone_gap), Some(fmt::ZONE_GAP));
        assert_eq!((want.hesitation_s, want.back_angle), (10, 90));
        let s = crate::recipe::Settings {
            hunt: Some(h),
            ..Default::default()
        };
        let mut vfs = Vfs::new();
        assert!(s.push_rules(&mut vfs).unwrap_or(false));
        let back = rules_fmt::Rules::parse(&vfs.read(rules_fmt::PATH).unwrap_or_default());
        assert_eq!(back.hunt, want);
    }

    #[test]
    fn the_knife_needs_its_own_experimental_switch() {
        let mut h = section();
        assert!(h.to_format().knife && h.to_format().knife_dodge);
        h.experimental_knife = false;
        let f = h.to_format();
        assert!(
            !f.knife && !f.knife_dodge,
            "a knife section alone is ignored"
        );
        assert!(h.is_active(), "the other rules still apply");
        let only_knife = Hunt {
            enabled: true,
            knife: Some(Knife { dodge: true }),
            ..Hunt::default()
        };
        assert!(!only_knife.is_active());
    }

    #[test]
    fn off_or_empty_writes_nothing() {
        let mut h = section();
        h.enabled = false;
        assert!(!h.is_active() && h.to_format().is_empty());
        let s = crate::recipe::Settings {
            hunt: Some(h),
            ..Default::default()
        };
        let mut vfs = Vfs::new();
        assert!(!s.push_rules(&mut vfs).unwrap_or(true));
        let on_but_empty = Hunt {
            enabled: true,
            ..Hunt::default()
        };
        assert!(!on_but_empty.is_active());
    }

    #[test]
    fn the_section_round_trips_through_a_recipe() {
        let text = format!(
            "[recipe]\nname = \"t\"\n[settings.hunt]\n{}",
            SECTION.replace("[[predators]]", "[[settings.hunt.predators]]")
        );
        let (r, w) = crate::recipe::Recipe::parse_checked(&text).unwrap_or_else(|e| panic!("{e}"));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(r.settings.hunt, Some(section()));
        let back = crate::recipe::Recipe::parse(&r.to_toml()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.settings.hunt, r.settings.hunt);
    }

    #[test]
    fn a_mistyped_key_is_reported() {
        let text = "[recipe]\nname = \"t\"\n[settings.hunt]\nenabled = true\nmax_firearm = 2\n\
                    extraction = { hover = 3 }\n";
        let (_, w) = crate::recipe::Recipe::parse_checked(text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(w.len(), 2, "{w:?}");
    }
}
