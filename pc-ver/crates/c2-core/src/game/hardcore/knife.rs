//! The knife: offered when every carried firearm is empty. Built into the
//! engine, not a `_RES.TXT` weapon, so saved trophy weapon indices keep
//! their meaning. Its kills give no points and no trophy and do not count
//! as a success. With the knife out the hunter can dodge: a side-step with
//! a short spell in which no strike lands.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::super::{GameEvent, Hunt, HuntInput, WeaponState};
use crate::area::Area;

/// A stab: its length, its reach beyond the animal's radius, the half
/// angle it covers and the health it takes.
pub const SWING_MS: i32 = 450;
const REACH: f32 = 260.0;
const HALF_ANGLE_COS: f32 = 0.5;
const POWER: i32 = 2;
/// The dodge: how long no strike lands, how long the side-step takes, how
/// far it goes and the wait before the next.
pub const DODGE_SAFE_MS: i32 = 600;
const DODGE_MOVE_MS: i32 = 220;
const DODGE_DISTANCE: f32 = 560.0;
pub const DODGE_COOLDOWN_MS: i32 = 3_000;

/// The knife's state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Knife {
    /// Ms left of the stab under way (the frontends draw it).
    pub swing_ms: i32,
    /// Stabs made (the frontends sound each new one).
    pub swings: u32,
    /// Ms left in which no strike lands, and of the side-step.
    pub safe_ms: i32,
    move_ms: i32,
    /// -1 left, 1 right.
    dir: f32,
    /// Ms until the next dodge.
    pub dodge_cooldown: i32,
}

impl Hunt {
    /// Whether the knife is in hand: the rule is on and every carried
    /// firearm is out of rounds (chambered, reserve and spare magazine).
    pub fn knife_ready(&self) -> bool {
        self.hardcore.as_deref().is_some_and(|h| h.rules.knife)
            && self.health > 0
            && !self.opts.observer
            && (0..self.weapons.info.len())
                .all(|i| !self.weapons.carried(i) || self.weapons.rounds(i) == 0)
    }

    /// The knife's state, while it is in hand.
    pub fn knife(&self) -> Option<&Knife> {
        self.knife_ready()
            .then(|| self.hardcore.as_deref().map(|h| &h.knife))
            .flatten()
    }

    /// The trigger with the knife in hand: a fresh pull stabs. True when
    /// the knife took the input (the empty firearm does not click).
    pub(in crate::game) fn knife_input(
        &mut self,
        area: &Area,
        input: &HuntInput,
        out: &mut Vec<GameEvent>,
    ) -> bool {
        if !self.knife_ready() {
            return false;
        }
        // The empty firearm is put away.
        if self.weapons.state == WeaponState::Ready && self.weapons.ftime == 0 {
            self.hide_weapon(out);
        }
        let fresh = input.fire && !self.fire_held;
        self.fire_held = input.fire;
        let idle = self
            .hardcore
            .as_deref()
            .is_some_and(|h| h.knife.swing_ms == 0);
        if fresh && idle && !self.map_mode && !self.bin_mode {
            if let Some(h) = self.hardcore.as_deref_mut() {
                h.knife.swing_ms = SWING_MS;
                h.knife.swings += 1;
            }
            self.stab(area, out);
        }
        true
    }

    /// The nearest living animal in front of the hunter within reach is
    /// hurt; killed, it is no trophy and scores nothing.
    fn stab(&mut self, area: &Area, out: &mut Vec<GameEvent>) {
        let p = &self.player;
        let (s, c) = p.alpha.sin_cos();
        let mut best: Option<(usize, f32)> = None;
        for (i, ch) in self.herd.chars.iter().enumerate() {
            let Some(sp) = self.herd.species.get(ch.ctype) else {
                continue;
            };
            if ch.health == 0 || ch.state_f == 0xff || sp.info.ai == crate::dino::ai::HUNTER {
                continue;
            }
            let (dx, dz) = (ch.pos[0] - p.x, ch.pos[2] - p.z);
            let d = (dx * dx + dz * dz).sqrt();
            let reach = REACH + sp.info.radius * ch.scale;
            let facing = d < 1.0 || (dx * s - dz * c) / d >= HALF_ANGLE_COS;
            if d < reach
                && facing
                && (ch.pos[1] - p.y).abs() < 500.0
                && best.is_none_or(|(_, b)| d < b)
            {
                best = Some((i, d));
            }
        }
        let Some((index, _)) = best else { return };
        let dialect = self
            .herd
            .species
            .get(self.herd.chars[index].ctype)
            .is_some_and(|sp| sp.mee.is_some());
        let ch = &mut self.herd.chars[index];
        let pos = [ch.pos[0], ch.pos[1] + 100.0 * ch.scale, ch.pos[2]];
        ch.damaged = true;
        ch.darted = false;
        ch.health = (ch.health - POWER).max(0);
        if ch.health == 0 {
            // Not `trophy`: no ship, no score, no success.
            out.push(GameEvent::Killed {
                index,
                trophy: false,
            });
        } else {
            super::super::combat::wound(ch, dialect);
        }
        if self.opts.blood {
            let a = self.camera().1;
            self.effects
                .add(area, &mut self.rng, pos, crate::effects::Part::Blood, 10, a);
        }
    }

    /// A side-step (`dir`: negative left, else right) with the knife out,
    /// when the rule allows it and the last one is over. Whether it began.
    pub fn dodge(&mut self, dir: f32) -> bool {
        let ready = self.knife_ready() && !self.player.swim && self.deploying().is_none();
        let Some(h) = self.hardcore.as_deref_mut() else {
            return false;
        };
        if !ready || !h.rules.knife_dodge || h.knife.dodge_cooldown > 0 {
            return false;
        }
        h.knife.safe_ms = DODGE_SAFE_MS;
        h.knife.move_ms = DODGE_MOVE_MS;
        h.knife.dir = if dir < 0.0 { -1.0 } else { 1.0 };
        h.knife.dodge_cooldown = DODGE_COOLDOWN_MS;
        if let Some(hes) = self.herd.hesitation.as_deref_mut() {
            hes.hunter_safe = true;
        }
        true
    }

    /// Whether a strike on the hunter misses now (mid-dodge).
    pub fn dodging(&self) -> bool {
        self.hardcore
            .as_deref()
            .is_some_and(|h| h.knife.safe_ms > 0)
    }

    pub(super) fn knife_step(&mut self, area: &Area, dt: i32) {
        let Some(h) = self.hardcore.as_deref_mut() else {
            return;
        };
        let k = &mut h.knife;
        k.swing_ms = (k.swing_ms - dt).max(0);
        k.dodge_cooldown = (k.dodge_cooldown - dt).max(0);
        if k.move_ms > 0 {
            let step = dt.min(k.move_ms);
            k.move_ms -= step;
            let d = DODGE_DISTANCE * step as f32 / DODGE_MOVE_MS as f32 * k.dir;
            if self.health > 0 {
                self.player.shove(area, d);
            }
        }
        if k.safe_ms > 0 {
            k.safe_ms = (k.safe_ms - dt).max(0);
            if k.safe_ms == 0 {
                if let Some(hes) = self.herd.hesitation.as_deref_mut() {
                    hes.hunter_safe = false;
                    hes.dodged();
                }
            }
        }
    }
}
