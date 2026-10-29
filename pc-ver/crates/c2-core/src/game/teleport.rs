//! Teleporting (not in the original; the mobile game's, as an option):
//! the hunter picks a spot on the map, and after [`DELAY_MS`] is moved
//! there; then he can't for [`COOLDOWN_MS`]. Not in a survival or the
//! trophy room.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    boxed::Box,
    collections::{BinaryHeap, VecDeque},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use super::*;
use crate::CELL;

/// From picking a spot to being there.
pub const DELAY_MS: i32 = 5_000;
/// After a teleport, until the next.
pub const COOLDOWN_MS: i32 = 60_000;

/// A teleport under way or cooling down.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Teleport {
    /// Where to, and the ms until he is there.
    pub pending: Option<((f32, f32), i32)>,
    /// Ms until the next may start.
    pub cooldown: i32,
}

impl Hunt {
    /// Whether teleporting is on for this hunt (the option, not in a
    /// survival or the trophy room).
    pub fn can_teleport(&self) -> bool {
        self.opts.teleport && self.survival.is_none() && !self.opts.trophy
    }

    /// Starts a teleport to world (`x`, `z`) (the nearest dry ground the
    /// hunter can stand on); why not, if it can't start.
    pub fn request_teleport(&mut self, area: &Area, x: f32, z: f32) -> Result<(), String> {
        if !self.can_teleport() || self.death.is_some() {
            return Err("No teleporting here".into());
        }
        if self.teleport.pending.is_some() {
            return Err("Already teleporting".into());
        }
        if self.teleport.cooldown > 0 {
            return Err(format!(
                "Teleport ready in {}s",
                (self.teleport.cooldown + 999) / 1000
            ));
        }
        let at = landing(area, x, z).ok_or("No ground to stand on there")?;
        self.teleport.pending = Some((at, DELAY_MS));
        Ok(())
    }

    /// Calls off a teleport under way.
    pub fn cancel_teleport(&mut self) {
        self.teleport.pending = None;
    }

    pub(super) fn teleport_step(&mut self, area: &Area, dt: i32, out: &mut Vec<GameEvent>) {
        let t = &mut self.teleport;
        t.cooldown = (t.cooldown - dt).max(0);
        let Some((at, ms)) = t.pending else {
            return;
        };
        if self.death.is_some() {
            t.pending = None;
            return;
        }
        if ms > dt {
            t.pending = Some((at, ms - dt));
            return;
        }
        t.pending = None;
        t.cooldown = COOLDOWN_MS;
        let (alpha, beta) = (self.player.alpha, self.player.beta);
        let cell = |v: f32| (v / CELL) as u16;
        self.player = crate::player::Player::new(area, (cell(at.0), cell(at.1)));
        (self.player.alpha, self.player.beta) = (alpha, beta);
        out.push(GameEvent::Message("Teleported".into()));
    }
}

/// The dry cell nearest world (`x`, `z`) within the hunter's reach (up to
/// 16 cells round), as world coordinates of its centre.
fn landing(area: &Area, x: f32, z: f32) -> Option<(f32, f32)> {
    let (cx, cz) = ((x / CELL) as i32, (z / CELL) as i32);
    let walk = crate::collide::WALK_CELLS;
    let dry = |x: i32, z: i32| {
        walk.contains(&x)
            && walk.contains(&z)
            && area.map.flags[crate::area::idx(x as usize, z as usize)]
                & (crate::area::cell_flags::WATER_ANY | crate::area::cell_flags::NO_WAY)
                == 0
    };
    (0..=16).find_map(|r: i32| {
        (-r..=r)
            .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
            .filter(|&(dx, dz)| dx.abs().max(dz.abs()) == r)
            .map(|(dx, dz)| (cx + dx, cz + dz))
            .find(|&(x, z)| dry(x, z))
            .map(|(x, z)| (x as f32 * CELL + CELL / 2.0, z as f32 * CELL + CELL / 2.0))
    })
}
