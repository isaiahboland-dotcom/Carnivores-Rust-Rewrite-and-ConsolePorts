//! Carried weapons, ammunition, animation slots and view-model poses.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use super::{arms, MeeWeapon, WeaponInfo};
use crate::dino::PI;
use c2_formats::{bits::Bits, car::Car};

/// Weapon `.CAR` animation and sound slots.
pub mod weapon_anim {
    pub const DRAW: usize = 0;
    pub const FIRE: usize = 1;
    pub const HIDE: usize = 2;
    pub const RELOAD: usize = 3;
    /// The dialect's: a part reload, racking, the firing mode switch, and
    /// the poses with nothing chambered (ready, drawing, hiding).
    pub const PART: usize = 4;
    pub const RACK: usize = 5;
    pub const MODE: usize = 6;
    pub const EMPTY: usize = 7;
    pub const DRAW_EMPTY: usize = 8;
    pub const HIDE_EMPTY: usize = 9;
    pub const SLOTS: usize = 10;
}

/// A weapon's animations per [`weapon_anim`] slot: lengths in ms (0: none)
/// and the model sound each plays (the original: the slot's own; the
/// dialect: the animation's `Anifx`; -1: none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponAnims {
    pub ms: [i32; weapon_anim::SLOTS],
    pub fx: [i32; weapon_anim::SLOTS],
}

impl Default for WeaponAnims {
    fn default() -> Self {
        Self {
            ms: [0; weapon_anim::SLOTS],
            fx: [-1; weapon_anim::SLOTS],
        }
    }
}

impl WeaponAnims {
    /// From the weapon's view model.
    pub fn from_car(info: &WeaponInfo, car: &Car) -> Self {
        let mut a = Self::default();
        for slot in 0..weapon_anim::SLOTS {
            let Some(i) = info.anim_of(slot) else {
                continue;
            };
            let Some(an) = car.animations.get(i) else {
                continue;
            };
            a.ms[slot] = crate::model::car_anim_duration_ms(an) as i32;
            a.fx[slot] = if info.mee.is_some() {
                car.anim_sounds
                    .and_then(|t| t.get(i).copied())
                    .unwrap_or(-1)
            } else {
                slot as i32
            };
        }
        a
    }

    /// The original's four, all `ms` long, each with its own sound (tests).
    pub fn uniform(ms: i32) -> Self {
        let mut a = Self::default();
        for slot in 0..=weapon_anim::RELOAD {
            a.ms[slot] = ms;
            a.fx[slot] = slot as i32;
        }
        a
    }
}

/// A dialect round in flight (`TBullet`).
#[derive(Debug, Clone, PartialEq)]
pub struct Bullet {
    pub pos: [f32; 3],
    /// Units per 17 ms.
    pub vel: [f32; 3],
    /// Its speed once it crosses the water's surface (`ldif`), and the fall
    /// so far (`fallTotal`).
    pub land_vel: [f32; 3],
    pub fall_total: f32,
    /// `aqState`: fired above water (0) or under it (1); crossed (2).
    pub aq_state: u8,
    /// A retrievable round that stopped (`state = 1`): lying there to be
    /// picked up.
    pub stuck: bool,
    /// Its model's animation time (`bModel`).
    pub ftime: i32,
    pub weapon: usize,
    /// A tranquilizer dart (multi ammo).
    pub dart: bool,
}

/// `Weapon.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponState {
    Hidden = 0,
    Drawing = 1,
    /// Ready; `ftime > 0` while the fire animation plays.
    Ready = 2,
    Hiding = 3,
    Reloading = 4,
    /// The dialect's: a part reload, racking, switching the firing mode.
    PartReload = 5,
    Racking = 6,
    Switching = 7,
}

#[derive(Debug, Clone)]
pub struct Weapons {
    pub info: Vec<WeaponInfo>,
    /// Animation lengths and sounds per weapon.
    pub anims: Vec<WeaponAnims>,
    pub current: usize,
    pub target: usize,
    pub state: WeaponState,
    pub ftime: i32,
    pub shakel: f32,
    /// Rounds left: all of them (the original); the reserve, beside those
    /// chambered and the spare magazine (the dialect's).
    pub shots_left: Vec<i32>,
    pub ammo_mag: Vec<i32>,
    /// The dialect's: rounds chambered (`Chambered`), the spare
    /// magazine's (`MagShotsLeft`), the firing mode (`FiringMode`: 1 full
    /// auto) and the rounds a reload is putting in (`ammoIn`).
    pub chambered: Vec<i32>,
    pub mag_shots: Vec<i32>,
    pub fire_mode: Vec<u8>,
    pub ammo_in: i32,
    /// The HUD's round flash (`FlashP`: 1..=4, 0 off) and the muzzle flash
    /// (`Muzz`, `MuzzGamma`: its turn, rad).
    pub flash: u8,
    pub muzzle: Option<f32>,
    /// The muzzle flash's length (`MUZZ4.CAR`'s animation), ms.
    pub muzzle_ms: i32,
    /// Sniper scope view (`OPTICMODE`).
    pub optic_mode: bool,
    /// What `new` was given, for [`resupply`](Self::resupply).
    pub(super) present: Bits,
    pub(super) double_ammo: bool,
    /// Multi ammo ([`split_tranq`](Self::split_tranq)): whether each
    /// weapon has tranquilizer darts loaded, and the other kind's rounds
    /// put aside. Not in the original.
    pub darts: Vec<bool>,
    pub(super) stash: Vec<super::ammo_kinds::Stash>,
    pub(super) multi: bool,
}

impl Weapons {
    /// `ReInitGame`: full ammo for every weapon in `present` (bit mask).
    pub fn new(
        info: Vec<WeaponInfo>,
        anims: Vec<WeaponAnims>,
        present: Bits,
        double_ammo: bool,
    ) -> Self {
        let n = info.len();
        let mut w = Self {
            info,
            anims,
            current: 0,
            target: 0,
            state: WeaponState::Hidden,
            ftime: 0,
            shakel: 0.0,
            shots_left: vec![0; n],
            ammo_mag: vec![0; n],
            chambered: vec![0; n],
            mag_shots: vec![0; n],
            fire_mode: vec![0; n],
            ammo_in: 0,
            flash: 0,
            muzzle: None,
            muzzle_ms: arms::MUZZLE_MS,
            optic_mode: false,
            present,
            double_ammo,
            darts: vec![false; n],
            stash: vec![Default::default(); n],
            multi: false,
        };
        let mut first = None;
        for i in 0..n.min(Bits::LEN) {
            if present.has(i) {
                w.refill(i, true);
                first.get_or_insert(i);
            }
        }
        w.target = first.unwrap_or(0);
        w.current = w.target;
        w
    }

    /// `refillWeapons`: weapon `i`'s ammo as at the start (`init`), or
    /// topped up (the dialect then leaves what is chambered where it would
    /// be reloaded or racked by hand).
    fn refill(&mut self, i: usize, init: bool) {
        let info = &self.info[i];
        if !info.chambers() {
            self.shots_left[i] = info.shots;
            self.ammo_mag[i] = i32::from(self.double_ammo);
            return;
        }
        let a = self.anims.get(i).copied().unwrap_or_default();
        let no_reload = a.ms[weapon_anim::RELOAD] == 0;
        let no_rack = a.ms[weapon_anim::RACK] == 0;
        self.fire_mode[i] = u8::from(info.mee.as_ref().is_some_and(|m| m.full_auto));
        self.shots_left[i] = info.shots;
        if self.double_ammo {
            if info.reload != 0 || no_reload {
                self.shots_left[i] *= 2;
            } else {
                self.ammo_mag[i] = 1;
                self.mag_shots[i] = info.shots;
            }
        }
        if info.reload != 0 {
            if init || no_reload {
                let n = info.reload.min(self.shots_left[i]).max(0);
                self.chambered[i] = n;
                self.shots_left[i] -= n;
            }
        } else if (init || no_rack) && self.shots_left[i] > 0 {
            self.chambered[i] = 1;
            self.shots_left[i] -= 1;
        }
    }

    /// Every carried weapon back to the ammo it started with (not in the
    /// original: the 3DS "extra ammo" gear, unlimited survival ammo).
    pub fn resupply(&mut self) {
        // Multi ammo: both kinds back to half the full load each.
        let multi = self.multi;
        if multi {
            self.unsplit();
        }
        for i in 0..self.info.len().min(Bits::LEN) {
            if self.present.has(i) {
                if self.info[i].chambers() {
                    // Topped up, keeping what is chambered.
                    let c = self.chambered[i];
                    self.refill(i, false);
                    self.chambered[i] = self.chambered[i].max(c);
                } else {
                    self.refill(i, false);
                }
            }
        }
        if multi {
            self.split_tranq();
        }
    }

    /// All the rounds weapon `i` has: chambered, left and the spare
    /// magazine's (the dialect's "no weapon" test).
    pub fn rounds(&self, i: usize) -> i32 {
        let g = |v: &Vec<i32>| v.get(i).copied().unwrap_or(0);
        let mag = if g(&self.ammo_mag) > 0 && self.info.get(i).is_some_and(WeaponInfo::chambers) {
            g(&self.mag_shots)
        } else {
            0
        };
        g(&self.shots_left) + g(&self.chambered) + mag
    }

    /// The current weapon's animation lengths and sounds.
    pub fn cur_anims(&self) -> WeaponAnims {
        self.anims.get(self.current).copied().unwrap_or_default()
    }

    /// Whether weapon `i` was taken on the hunt.
    pub fn carried(&self, i: usize) -> bool {
        self.present.has(i)
    }

    pub(super) fn anim(&self, slot: usize) -> i32 {
        self.anims.get(self.current).map_or(0, |a| a.ms[slot])
    }

    /// The [`weapon_anim`] slot the view model shows (the dialect's empty
    /// poses with nothing chambered, where the weapon has them).
    pub fn pose_slot(&self) -> Option<usize> {
        use weapon_anim as wa;
        let empty = self.chambered.get(self.current).copied().unwrap_or(0) == 0
            && self
                .info
                .get(self.current)
                .is_some_and(WeaponInfo::chambers);
        let has = |slot| self.anim(slot) > 0;
        Some(match self.state {
            WeaponState::Hidden => return None,
            WeaponState::Drawing if empty && has(wa::DRAW_EMPTY) => wa::DRAW_EMPTY,
            WeaponState::Drawing => wa::DRAW,
            WeaponState::Ready if empty && self.ftime == 0 && has(wa::EMPTY) => wa::EMPTY,
            WeaponState::Ready => wa::FIRE,
            WeaponState::Hiding if empty && has(wa::HIDE_EMPTY) => wa::HIDE_EMPTY,
            WeaponState::Hiding => wa::HIDE,
            WeaponState::Reloading => wa::RELOAD,
            WeaponState::PartReload => wa::PART,
            WeaponState::Racking => wa::RACK,
            WeaponState::Switching => wa::MODE,
        })
    }

    /// The model animation and time to pose the view model with.
    pub fn pose(&self) -> Option<(usize, i32)> {
        let slot = self.pose_slot()?;
        let anim = self
            .info
            .get(self.current)
            .map_or(Some(slot), |i| i.anim_of(slot))
            .unwrap_or(slot);
        Some((anim, self.ftime))
    }

    /// The current weapon's dialect extras, if any.
    pub fn mee(&self) -> Option<&MeeWeapon> {
        self.info.get(self.current).and_then(|i| i.mee.as_ref())
    }

    /// Sway of the aim (`wpnDAlpha`, `wpnDBeta`).
    pub fn sway(&self, real_time: i32) -> (f32, f32) {
        let t = real_time as f32 / 300.0;
        (
            self.shakel * (t + PI / 2.0).sin() / 200.0,
            self.shakel * t.sin() / 400.0,
        )
    }
}
