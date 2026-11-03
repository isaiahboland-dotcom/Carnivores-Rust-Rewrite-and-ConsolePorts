//! Weapon definitions read from the resource table.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use super::{weapon_anim, MeeWeapon};
use c2_formats::res_txt::WeaponDef;

/// `TWeapInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponInfo {
    pub name: String,
    pub file: String,
    pub pic: String,
    pub power: f32,
    pub prec: f32,
    pub loud: f32,
    pub rate: f32,
    pub shots: i32,
    /// Rounds per magazine (0 = no reload animation between shots).
    pub reload: i32,
    /// Extra pellets (`trace - 1`).
    pub trace_c: i32,
    pub optic: bool,
    /// Bullet drop (crossbow).
    pub fall: bool,
    /// The Modder's Edition's weapon keys, when the weapon has any.
    pub mee: Option<MeeWeapon>,
}

impl WeaponInfo {
    pub fn from_def(d: &WeaponDef) -> Self {
        Self {
            name: d.name.clone().unwrap_or_default(),
            file: d.file.clone().unwrap_or_default(),
            pic: d.pic.clone().unwrap_or_default(),
            power: d.power.unwrap_or(0.0),
            prec: d.prec.unwrap_or(0.0),
            loud: d.loud.unwrap_or(0.0),
            rate: d.rate.unwrap_or(0.0),
            shots: d.shots.unwrap_or(0),
            reload: d.reload.unwrap_or(0),
            trace_c: d.trace.unwrap_or(0) - 1,
            optic: d.optic.unwrap_or(0.0) != 0.0,
            fall: d.fall.unwrap_or(0) != 0,
            mee: MeeWeapon::from_def(d),
        }
        .read_optic(d)
    }

    /// The original reads `optic` as a whole number (`optic = 0.5`: none).
    fn read_optic(mut self, d: &WeaponDef) -> Self {
        if self.mee.is_none() {
            self.optic = d.optic.unwrap_or(0.0).trunc() != 0.0;
        }
        self
    }

    /// The model animation for a `weapon_anim` slot: the original's four
    /// in order; a dialect weapon's as its keys say.
    pub fn anim_of(&self, slot: usize) -> Option<usize> {
        match &self.mee {
            Some(m) => m.anims.get(slot).copied().flatten(),
            None => (slot <= weapon_anim::RELOAD).then_some(slot),
        }
    }

    /// The dialect's ammo model (rounds chambered, reloads and racking by
    /// hand): a weapon with the Modder's Edition's keys.
    pub fn chambers(&self) -> bool {
        self.mee.is_some()
    }
}
