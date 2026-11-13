//! A Play Vanilla session (`c2-pc play --recipe R --classic`): the opt-in
//! enhancements among the player's settings read their defaults, the
//! original game's behaviour, while comfort and display settings keep the
//! player's values (`docs/RECIPE-SETTINGS.md`).
//!
//! Like the instance's locks (`instance_rules`), this is applied in memory
//! only: `c2rs.cfg` keeps the player's own values, and the options menu
//! hides the rows it fixes.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::settings::PcSettings;

static CLASSIC: AtomicBool = AtomicBool::new(false);

/// Makes this a classic session (before the settings are first loaded).
pub fn enable() {
    CLASSIC.store(true, Ordering::Relaxed);
}

/// Whether this is a classic session.
pub fn on() -> bool {
    CLASSIC.load(Ordering::Relaxed)
}

/// The `c2rs.cfg` keys a classic session reads at their defaults, in
/// [`off_fields`] order (the tests check them against the stored form).
#[cfg(test)]
const OFF_KEYS: [&str; 9] = [
    "teleport",
    "advanced_observer",
    "multi_ammo",
    "bounded_flee",
    "all_huntable",
    "radar_water",
    "hardcore_survival",
    "dynamic_time",
    "map_zoom",
];

/// The options rows of those settings (hidden in a classic session).
const OFF_LABELS: [&str; 9] = [
    "Teleport (map)",
    "Advanced observer mode",
    "Multi ammo",
    "Bounded fleeing",
    "All dinos huntable",
    "Water animals on radar",
    "Enable hardcore survival mode",
    "Dynamic time",
    "Map zoom / pan",
];

fn off_fields(s: &mut PcSettings) -> [&mut i32; 9] {
    [
        &mut s.teleport,
        &mut s.advanced_observer,
        &mut s.multi_ammo,
        &mut s.bounded_flee,
        &mut s.all_huntable,
        &mut s.radar_water,
        &mut s.hardcore_survival,
        &mut s.dynamic_time,
        &mut s.map_zoom,
    ]
}

/// `s` with the classic-off settings taken from `from`.
fn take_off(mut s: PcSettings, mut from: PcSettings) -> PcSettings {
    for (d, v) in off_fields(&mut s).into_iter().zip(off_fields(&mut from)) {
        *d = *v;
    }
    s
}

/// `s` as a classic session plays it: the enhancements at their defaults.
pub fn apply(s: PcSettings) -> PcSettings {
    take_off(s, PcSettings::default())
}

/// `s` with those settings back at the player's own (`own`, as the
/// settings file has them): what is stored.
pub fn unapply(s: PcSettings, own: &PcSettings) -> PcSettings {
    take_off(s, *own)
}

/// Whether the options row labelled `label` is hidden in this session.
pub fn hides(label: &str) -> bool {
    on() && OFF_LABELS.contains(&label)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each `key = value` line of the stored form.
    fn values(s: &PcSettings) -> Vec<(String, String)> {
        s.to_text()
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect()
    }

    /// Settings with every value changed from its default.
    fn changed() -> PcSettings {
        let text: String = values(&PcSettings::default())
            .into_iter()
            .map(|(k, v)| {
                let v = match v.as_str() {
                    "0" => "1",
                    "1" => "0",
                    _ => "7",
                };
                format!("{k} = {v}\n")
            })
            .collect();
        PcSettings::parse(&text)
    }

    #[test]
    fn every_setting_is_either_off_or_the_players() {
        let own = changed();
        let (d, o) = (values(&PcSettings::default()), values(&own));
        assert!(d.iter().zip(&o).all(|(a, b)| a.1 != b.1), "{o:?}");
        let classic = values(&apply(own));
        for ((key, def), ((_, mine), (_, got))) in d.iter().zip(o.iter().zip(&classic)) {
            let want = if OFF_KEYS.contains(&key.as_str()) {
                def
            } else {
                mine
            };
            assert_eq!(got, want, "{key}");
        }
        for k in OFF_KEYS {
            assert!(d.iter().any(|(key, _)| key == k), "{k} is not a setting");
        }
    }

    #[test]
    fn the_players_own_values_are_what_is_stored() {
        let own = changed();
        let mut played = apply(own);
        played.blood = 1 - own.blood;
        let stored = unapply(played, &own);
        assert_eq!(stored.teleport, own.teleport);
        assert_eq!(stored.dynamic_time, own.dynamic_time);
        assert_eq!(
            stored.blood, played.blood,
            "a change made in the session is kept"
        );
    }

    /// Score, rank and trophies live in the instance's `trophy0N.sav`
    /// (format round trip: `c2_formats::sav` tests), and the rank is
    /// recomputed from the score at a hunt's end: a classic session reads
    /// and writes the same files. (No other test reads the classic flag.)
    #[test]
    fn a_classic_session_keeps_the_instances_saves() {
        let (dir, slot) = (crate::save::saves_dir(), crate::save::slot_path(3));
        enable();
        assert!(on());
        assert_eq!(crate::save::saves_dir(), dir);
        assert_eq!(crate::save::slot_path(3), slot);
        assert!(hides("Teleport (map)") && !hides("Blood"));
    }

    #[test]
    fn every_off_setting_has_an_options_row() {
        let rows = include_str!("menu/options.rs");
        for l in OFF_LABELS {
            assert!(rows.contains(&format!("\"{l}\"")), "{l}");
        }
        assert_eq!(OFF_KEYS.len(), OFF_LABELS.len());
    }
}
