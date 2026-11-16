//! Extraction: the called ship flies to the pickup zone, hovers, and takes
//! the hunter out if he is in the zone at any moment while it hovers. The
//! cooldown starts when it leaves.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use super::super::{GameEvent, Hunt};
use crate::area::Area;
use crate::dino::find_vector_alpha;
use crate::ship::{ship_sound, ShipEvent};
use crate::CELL;

/// The pickup zone's radius.
pub const ZONE_RADIUS: f32 = 6.0 * CELL;
/// The ship's `state` while extraction flies it (drawn, engine heard).
pub const SHIP_STATE: i32 = 4;
/// How far off the ship comes from and leaves to, and for how long it is
/// seen leaving.
const APPROACH: f32 = 90.0 * CELL;
const LEAVE_MS: i32 = 6_000;
/// The ship hovers this high over the zone's ground.
const HOVER_Y: f32 = 1400.0;

/// Where the pickup stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Pickup {
    /// The ship can be called.
    #[default]
    Idle,
    /// Called: ms until it is over the zone.
    Arriving(i32),
    /// Over the zone: ms until it leaves.
    Hovering(i32),
    /// Gone: ms until it can be called again.
    Cooldown(i32),
}

impl Hunt {
    /// Calls the ship to the pickup zone; why not, if it can't come.
    pub fn call_ship(&mut self, out: &mut Vec<GameEvent>) -> Result<(), String> {
        let Some(h) = self.hardcore.as_deref() else {
            return Err("No ship to call".into());
        };
        let (Some(e), Some(_)) = (h.rules.extraction, h.zone) else {
            return Err("No ship to call".into());
        };
        if self.death.is_some() || self.outcome.is_some() {
            return Err("No ship to call".into());
        }
        match h.pickup {
            Pickup::Idle => {}
            Pickup::Arriving(_) => return Err("The ship is on its way".into()),
            Pickup::Hovering(_) => return Err("The ship is at the pickup zone".into()),
            Pickup::Cooldown(ms) => {
                return Err(format!("Ship ready in {}s", (ms + 999) / 1000));
            }
        }
        // It is out collecting a trophy.
        if self.ship.state != -1 || !self.ship.tasks.is_empty() {
            return Err("The ship is busy".into());
        }
        let ms = e.arrive_s as i32 * 1000;
        if let Some(h) = self.hardcore.as_deref_mut() {
            h.pickup = Pickup::Arriving(ms);
        }
        out.push(GameEvent::Ship(ShipEvent::Sound {
            slot: ship_sound::CALL,
            at: None,
        }));
        out.push(GameEvent::Message(format!(
            "Ship called: at the pickup zone in {}s",
            e.arrive_s
        )));
        Ok(())
    }

    /// Escape at the pickup zone calls the ship instead of opening the exit
    /// prompt. `None` when Escape keeps its usual meaning (outside the zone,
    /// no extraction rules, a prompt or pause already up, dead). Inside the
    /// zone, a ship that cannot come gives its reason rather than the
    /// forfeit prompt.
    pub fn escape_calls_ship(&mut self, out: &mut Vec<GameEvent>) -> Option<Result<(), String>> {
        let h = self.hardcore.as_deref()?;
        if h.rules.extraction.is_none()
            || h.zone.is_none()
            || self.paused
            || self.exit_prompt
            || self.exit_time != 0
            || self.death.is_some()
            || self.outcome.is_some()
            || self.health <= 0
            || !self.in_pickup_zone()
        {
            return None;
        }
        Some(self.call_ship(out))
    }

    /// Whether extraction is flying the ship (the trophy pickups wait).
    pub fn extraction_flies_ship(&self) -> bool {
        self.ship.state == SHIP_STATE
    }

    /// Whether the hunter stands in the pickup zone.
    pub fn in_pickup_zone(&self) -> bool {
        self.hardcore
            .as_deref()
            .and_then(|h| h.zone)
            .is_some_and(|(x, z)| {
                (self.player.x - x).powi(2) + (self.player.z - z).powi(2)
                    <= ZONE_RADIUS * ZONE_RADIUS
            })
    }

    pub(super) fn extraction_step(&mut self, area: &Area, dt: i32, out: &mut Vec<GameEvent>) {
        let Some(h) = self.hardcore.as_deref() else {
            return;
        };
        let (Some(e), Some(zone)) = (h.rules.extraction, h.zone) else {
            return;
        };
        let hover = [zone.0, area.land_up_h(zone.0, zone.1) + HOVER_Y, zone.1];
        // It comes in from the side away from the map's edge.
        let side = |v: f32| {
            if v - APPROACH < crate::collide::WALK_MIN {
                v + APPROACH
            } else {
                v - APPROACH
            }
        };
        let far = [side(zone.0), hover[1] + 1024.0, side(zone.1)];
        let lerp = |t: f32| {
            [
                far[0] + (hover[0] - far[0]) * t,
                far[1] + (hover[1] - far[1]) * t,
                far[2] + (hover[2] - far[2]) * t,
            ]
        };
        let heading = find_vector_alpha(hover[0] - far[0], hover[2] - far[2]);
        let inside = self.in_pickup_zone() && self.health > 0;
        let arrive_ms = (e.arrive_s as i32 * 1000).max(1);
        let cooldown_ms = e.cooldown_s as i32 * 1000;
        let mut next = h.pickup;
        let mut leaving = h.leaving_ms;
        match h.pickup {
            Pickup::Idle => {}
            Pickup::Arriving(ms) => {
                let ms = ms - dt;
                self.ship.state = SHIP_STATE;
                self.ship.ftime = 0;
                self.ship.alpha = heading;
                self.ship.pos = lerp(1.0 - ms.max(0) as f32 / arrive_ms as f32);
                next = if ms > 0 {
                    Pickup::Arriving(ms)
                } else {
                    out.push(GameEvent::Ship(ShipEvent::Sound {
                        slot: ship_sound::ARRIVE,
                        at: Some(hover),
                    }));
                    out.push(GameEvent::Message(format!(
                        "The ship is at the pickup zone for {}s",
                        e.hover_s
                    )));
                    Pickup::Hovering(e.hover_s as i32 * 1000)
                };
            }
            Pickup::Hovering(ms) => {
                self.ship.state = SHIP_STATE;
                self.ship.pos = hover;
                self.ship.pos[1] += 12.0 * (self.real_time as f32 / 256.0).cos();
                if inside {
                    // Out alive: a normal exit, trophies and points kept.
                    self.leave_alive();
                    out.push(GameEvent::Message("Extracted".into()));
                    return;
                }
                let ms = ms - dt;
                next = if ms > 0 {
                    Pickup::Hovering(ms)
                } else {
                    out.push(GameEvent::Ship(ShipEvent::Sound {
                        slot: ship_sound::DEPART,
                        at: Some(hover),
                    }));
                    out.push(GameEvent::Message(if cooldown_ms > 0 {
                        format!("The ship left. Ready again in {}s", e.cooldown_s)
                    } else {
                        "The ship left".to_string()
                    }));
                    leaving = LEAVE_MS;
                    Pickup::Cooldown(cooldown_ms)
                };
            }
            Pickup::Cooldown(ms) => {
                let ms = ms - dt;
                next = if ms > 0 {
                    Pickup::Cooldown(ms)
                } else {
                    Pickup::Idle
                };
            }
        }
        // Seen flying off, whatever the cooldown.
        if leaving > 0 && !matches!(next, Pickup::Arriving(_) | Pickup::Hovering(_)) {
            leaving = (leaving - dt).max(0);
            if leaving == 0 {
                self.ship.state = -1;
            } else {
                self.ship.state = SHIP_STATE;
                self.ship.alpha = find_vector_alpha(far[0] - hover[0], far[2] - hover[2]);
                self.ship.pos = lerp(leaving as f32 / LEAVE_MS as f32);
            }
        }
        if let Some(h) = self.hardcore.as_deref_mut() {
            h.pickup = next;
            h.leaving_ms = leaving;
        }
    }
}
