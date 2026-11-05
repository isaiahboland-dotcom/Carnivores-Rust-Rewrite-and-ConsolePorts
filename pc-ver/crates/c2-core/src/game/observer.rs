//! Advanced observer mode (a setting; not in the original): on top of the
//! observer's hunt, unlimited instant teleports (see `teleport.rs`),
//! flying, and stopping time for camera shots.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{GameEvent, Hunt, HuntInput, MAX_HEALTH};
use crate::area::Area;

impl Hunt {
    /// Advanced observer mode is on for this hunt.
    pub fn free_observer(&self) -> bool {
        self.opts.observer && self.opts.free_observer && !self.opts.trophy
    }

    /// Flying on or off; whether it is on now.
    pub fn toggle_fly(&mut self) -> bool {
        self.flying = self.free_observer() && !self.flying;
        self.flying
    }

    /// Time stopped or going again; whether it is stopped now.
    pub fn toggle_time_stop(&mut self) -> bool {
        self.time_stopped = self.free_observer() && !self.time_stopped;
        self.time_stopped
    }

    /// One tick with time stopped: only the camera (on foot or flying),
    /// the map, binoculars, teleports and leaving go on.
    pub(super) fn stopped_tick(
        &mut self,
        area: &Area,
        input: &HuntInput,
        dt: i32,
        out: &mut Vec<GameEvent>,
    ) {
        self.move_hunter(area, input, dt, out);
        self.binoculars(input, dt);
        if input.toggle_map && !self.bin_mode {
            self.map_mode = !self.map_mode;
        }
        self.exit_step(dt);
        self.teleport_step(area, dt, out);
        self.health = MAX_HEALTH;
    }

    /// The hunter's own movement this tick; true if he flew (and the
    /// walking side of the tick, gravity, swimming and so on, is skipped).
    pub(super) fn fly_step(&mut self, area: &Area, input: &HuntInput, dt: i32) -> bool {
        if !(self.flying && self.free_observer()) {
            return false;
        }
        self.player.fly(area, &input.controls, dt as u32);
        true
    }

    fn move_hunter(&mut self, area: &Area, input: &HuntInput, dt: i32, out: &mut Vec<GameEvent>) {
        if self.fly_step(area, input, dt) {
            return;
        }
        let mut pev = Vec::new();
        self.player.tick(area, &input.controls, dt as u32, &mut pev);
        out.extend(pev.into_iter().map(GameEvent::Player));
    }
}
