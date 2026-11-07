//! The pack's instance rules (`HUNTDAT/_RULES.TXT`, see
//! [`c2_formats::rules`]), read when the menu loads the pack: the times of
//! day offered, the settings the instance fixes (the ones the 3DS has:
//! mobile map, teleport, all dinos huntable, multi ammo, advanced observer,
//! and the survival budget), animals kept off the hunt menu, and the spawn
//! rules a hunt applies.
//!
//! The settings file is shared by every pack on the card, so a locked
//! setting never overwrites the player's own value there.

use std::sync::Mutex;

use c2_formats::rules::{Lock, Rules};
use c2_vfs::Vfs;

use crate::settings::Settings;

static RULES: Mutex<Option<Rules>> = Mutex::new(None);

/// Reads the pack's rules and makes them the ones in force.
pub fn load(vfs: &Vfs) -> Rules {
    let r = c2_core::load::rules(vfs);
    if let Ok(mut g) = RULES.lock() {
        *g = Some(r.clone());
    }
    r
}

/// The rules in force (empty before a pack is loaded).
pub fn get() -> Rules {
    RULES.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// The 3DS setting a lock fixes, if the 3DS has it.
fn field(s: &mut Settings, l: Lock) -> Option<&mut bool> {
    Some(match l {
        Lock::MobileRadar => &mut s.map_mobile,
        Lock::Teleport => &mut s.teleport,
        Lock::AllHuntable => &mut s.all_huntable,
        Lock::MultiAmmo => &mut s.multi_ammo,
        Lock::AdvancedObserver => &mut s.free_observer,
        _ => return None,
    })
}

/// `s` with the locked settings at the instance's values.
pub fn apply(mut s: Settings, r: &Rules) -> Settings {
    for &(l, v) in &r.locks {
        if let Some(f) = field(&mut s, l) {
            *f = v != 0;
        }
    }
    s
}

/// `s` with the locked settings back at the player's own (`own`): what is
/// stored.
pub fn unapply(mut s: Settings, own: &Settings, r: &Rules) -> Settings {
    let mut own = *own;
    for &(l, _) in &r.locks {
        if let (Some(f), Some(o)) = (field(&mut s, l), field(&mut own, l)) {
            *f = *o;
        }
    }
    s
}

/// Whether the setting is fixed (its button hidden).
pub fn locked(l: Lock) -> bool {
    get().is_locked(l)
}

/// The survival's starting points: the instance's, when it fixes more
/// than zero, else the menu's.
pub fn survival_budget(menu: u32) -> u32 {
    match get().lock(Lock::SurvivalBudget) {
        Some(v) if v > 0 => v as u32,
        _ => menu,
    }
}
