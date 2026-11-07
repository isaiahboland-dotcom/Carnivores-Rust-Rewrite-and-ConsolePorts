//! The game's instance rules (`HUNTDAT/_RULES.TXT`, see
//! [`c2_formats::rules`]), read when the menus load the game: the times of
//! day offered, the settings the instance fixes (applied over the player's
//! own and hidden from the options) and the spawn rules a hunt applies.
//!
//! The settings file (`c2rs.cfg`) is shared by every game, so a locked
//! setting never overwrites the player's own value there.

use std::path::Path;
use std::sync::RwLock;

use c2_formats::rules::{Lock, Rules};

use crate::area_view::find_file;
use crate::settings::PcSettings;

static RULES: RwLock<Option<Rules>> = RwLock::new(None);

/// Reads the rules of the game whose data folder is `huntdat` (none:
/// nothing differs from the game) and makes them the ones in force.
pub fn load(huntdat: &Path) -> Rules {
    let r = read(huntdat);
    if !r.is_empty() {
        tracing::info!(
            "instance rules: {}",
            r.to_text().lines().skip(1).collect::<Vec<_>>().join("; ")
        );
    }
    if let Ok(mut g) = RULES.write() {
        *g = Some(r.clone());
    }
    r
}

/// The rules file in `huntdat`, without making it the one in force.
pub fn read(huntdat: &Path) -> Rules {
    find_file(
        huntdat,
        c2_formats::rules::PATH.trim_start_matches("HUNTDAT/"),
    )
    .and_then(|p| std::fs::read(p).ok())
    .map(|b| Rules::parse(&b))
    .unwrap_or_default()
}

/// The rules in force (empty before a game is loaded).
pub fn get() -> Rules {
    RULES
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// The settings field a lock fixes.
fn field(s: &mut PcSettings, l: Lock) -> &mut i32 {
    match l {
        Lock::DynamicTime => &mut s.dynamic_time,
        Lock::AdvancedObserver => &mut s.advanced_observer,
        Lock::MultiAmmo => &mut s.multi_ammo,
        Lock::MobileRadar => &mut s.map_mobile,
        Lock::Teleport => &mut s.teleport,
        Lock::AllHuntable => &mut s.all_huntable,
        Lock::SecretDinos => &mut s.secret_dinos,
        Lock::SurvivalBudget => &mut s.survival_budget,
    }
}

/// `s` with the locked settings at their instance values.
pub fn apply(mut s: PcSettings, r: &Rules) -> PcSettings {
    for &(l, v) in &r.locks {
        *field(&mut s, l) = match l {
            Lock::SecretDinos => v.clamp(0, 255),
            Lock::SurvivalBudget => v.max(0),
            _ => i32::from(v != 0),
        };
    }
    s
}

/// `s` with the locked settings back at the player's own (`own`, as the
/// settings file has them): what is stored.
pub fn unapply(mut s: PcSettings, own: &PcSettings, r: &Rules) -> PcSettings {
    let mut own = *own;
    for &(l, _) in &r.locks {
        *field(&mut s, l) = *field(&mut own, l);
    }
    s
}

/// The lock behind an options row, by its label.
pub fn lock_of(label: &str) -> Option<Lock> {
    Some(match label {
        "Dynamic time" => Lock::DynamicTime,
        "Advanced observer mode" => Lock::AdvancedObserver,
        "Multi ammo" => Lock::MultiAmmo,
        "Mobile radar" => Lock::MobileRadar,
        "Teleport (map)" => Lock::Teleport,
        "All dinos huntable" => Lock::AllHuntable,
        "Total number of secret dinos on map" => Lock::SecretDinos,
        "Survival starting budget" => Lock::SurvivalBudget,
        _ => return None,
    })
}

/// Whether the options row labelled `label` is hidden (its setting fixed).
pub fn hides(label: &str) -> bool {
    lock_of(label).is_some_and(|l| get().is_locked(l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_apply_over_and_store_back_the_players_own() {
        let r = Rules::parse(b"lock teleport 1\nlock secret_dinos 900\nlock dynamic_time 0\n");
        let own = PcSettings {
            dynamic_time: 1,
            ..PcSettings::default()
        };
        let s = apply(own, &r);
        assert_eq!((s.teleport, s.secret_dinos, s.dynamic_time), (1, 255, 0));
        let mut changed = s;
        changed.blood = 0;
        let stored = unapply(changed, &own, &r);
        assert_eq!(stored.teleport, own.teleport);
        assert_eq!(stored.dynamic_time, 1);
        assert_eq!(stored.blood, 0, "unlocked changes are kept");
    }

    #[test]
    fn every_lock_has_an_options_row() {
        for l in Lock::ALL {
            let label = [
                "Dynamic time",
                "Advanced observer mode",
                "Multi ammo",
                "Mobile radar",
                "Teleport (map)",
                "All dinos huntable",
                "Total number of secret dinos on map",
                "Survival starting budget",
            ]
            .into_iter()
            .find(|n| lock_of(n) == Some(l));
            assert!(label.is_some(), "{l:?}");
        }
    }
}
