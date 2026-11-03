//! Scoring, ship collection and trophy-room management.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{GameEvent, Hunt, HuntKill};
use crate::area::Area;
use crate::ship::ShipEvent;
use c2_formats::sav::TrophyItem;

impl Hunt {
    /// `RemoveCharacter`, fixing up every index that refers past it.
    pub(super) fn remove_character(&mut self, index: usize) {
        if index >= self.herd.chars.len() {
            return;
        }
        self.herd.chars.remove(index);
        self.ship.character_removed(index);
        if let Some(d) = &mut self.herd.demo_index {
            if *d > index {
                *d -= 1;
            }
        }
        if let Some(d) = &mut self.death {
            if d.body > index {
                d.body -= 1;
            }
            d.killer = d.killer.and_then(|k| {
                if k == index {
                    None
                } else {
                    Some(k - usize::from(k > index))
                }
            });
        }
    }

    /// `AddShipTask`: call the ship for a trophy and record it.
    pub(super) fn add_ship_task(&mut self, area: &Area, index: usize, out: &mut Vec<GameEvent>) {
        let Some(c) = self.herd.chars.get(index) else {
            return;
        };
        if self.survival.is_some() {
            // No ship, trophy or score in a survival; the kill is recorded.
            let p = &self.player;
            let range =
                ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2))
                    .sqrt()
                    / 64.0;
            self.kills.push(HuntKill {
                ctype: c.ctype,
                score: 0,
                range,
                weapon: self.weapons.current,
            });
            return;
        }
        let water = area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2]);
        if water < 100.0 && !self.opts.tranq {
            self.ship.tasks.push_back(index);
            let slot = crate::ship::ship_sound::CALL;
            out.push(GameEvent::Ship(ShipEvent::Sound { slot, at: None }));
        }
        // The first free stand; in a dialect game the first free spot made
        // for this animal (none: no trophy, as `AddShipTask` there).
        let spots = &self.herd.trophy_spots;
        let t = if spots.is_empty() {
            Some(
                self.room
                    .bodies
                    .iter()
                    .take(23)
                    .position(|b| b.ctype == 0)
                    .unwrap_or(23),
            )
        } else {
            // A spot made for it, else any free one (add-ons written
            // without `tropinfo` would never have a trophy).
            let free = |t: &usize| self.room.bodies[*t].ctype == 0;
            let n = spots.len().min(self.room.bodies.len());
            (0..n)
                .filter(free)
                .find(|&t| spots[t].chars.contains(&c.ctype))
                .or_else(|| (0..n).find(free))
        };
        let base = self
            .herd
            .species
            .get(c.ctype)
            .map_or(0.0, |s| s.info.base_score);
        let mut score = base;
        if self.successes > 1 {
            score *= 1.0 + self.successes as f32 / 10.0;
        }
        // The original halves a kill that isn't a target; the Modder's
        // Edition doesn't (`SubmitDinoScore`: the line is commented out).
        let dialect = self
            .herd
            .species
            .get(c.ctype)
            .is_some_and(|s| s.tuning.mee_ai.is_some());
        if !dialect && !self.is_target(c) {
            score /= 2.0;
        }
        let no_call = self
            .call_bit(c)
            .is_some_and(|b| self.opts.no_call_bonus.has(b as usize));
        // Night and no-call: +10% each (not in the original; owner's requests).
        for (on, k) in [
            (self.opts.tranq, 1.25),
            (area.time == crate::area::TimeOfDay::Night, 1.10),
            (no_call, 1.10),
            (self.opts.radar, 0.70),
            (self.opts.scent, 0.80),
            (self.opts.camo, 0.85),
        ] {
            if on {
                score *= k;
            }
        }
        score *= self.opts.score_factor;
        self.room.score += score as i32;
        let p = &self.player;
        let range =
            ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2)).sqrt()
                / 64.0;
        out.push(GameEvent::Scored {
            ctype: c.ctype,
            score: score as i32,
            range,
            tranq: self.opts.tranq,
        });
        self.kills.push(HuntKill {
            ctype: c.ctype,
            score: score as i32,
            range,
            weapon: self.weapons.current,
        });
        if let (false, Some(t)) = (self.opts.tranq, t) {
            let (y, mo, d, h, mi) = self.clock;
            self.room.bodies[t] = TrophyItem {
                ctype: c.ctype as i32,
                scale: c.scale,
                weapon: self.weapons.current as i32,
                score: score as i32,
                phase: self.real_time & 3,
                time: (h << 10) + mi,
                date: (y << 20) + (mo << 10) + d,
                range,
                ..Default::default()
            };
            self.room.bodies[t].set_area(self.area_no);
            self.trophy_time = 20 * 1000;
            self.trophy_body = Some(t);
            out.push(GameEvent::Trophy {
                slot: t,
                score: score as i32,
            });
        }
    }

    /// Debug helper (screenshots, tools): kill character `index` outright as
    /// if by a vital shot, scoring it and calling the ship.
    pub fn debug_bag(&mut self, area: &Area, index: usize, out: &mut Vec<GameEvent>) {
        let Some(c) = self.herd.chars.get_mut(index) else {
            return;
        };
        if c.health == 0 {
            return;
        }
        c.health = 0;
        let trophy = c.ai >= 10;
        out.push(GameEvent::Killed { index, trophy });
        if trophy {
            self.successes += 1;
            self.add_ship_task(area, index, out);
        }
    }

    /// `ProcessTrophy`: the plaque of the trophy the hunter stands in front
    /// of (2.5 cells ahead of its nose, within 148 units).
    pub(super) fn process_trophy(&mut self) {
        let p = [self.player.x, self.player.y, self.player.z];
        self.trophy_body = None;
        for c in &self.herd.chars {
            let x = c.pos[0] + c.lookx * 256.0 * 2.5;
            let z = c.pos[2] + c.lookz * 256.0 * 2.5;
            let d = ((x - p[0]).powi(2) + (c.pos[1] - p[1]).powi(2) + (z - p[2]).powi(2)).sqrt();
            if d < 148.0 {
                self.trophy_body = Some(c.state as usize);
            }
        }
    }

    /// `RemoveCurrentTrophy` (R in the trophy room): delete the trophy whose
    /// plaque is showing.
    pub fn remove_current_trophy(&mut self) {
        if !self.opts.trophy {
            return;
        }
        let Some(slot) = self.trophy_body else {
            return;
        };
        if self
            .room
            .bodies
            .get(slot)
            .is_none_or(|b| b.ctype == 0 || b.locked())
        {
            return;
        }
        self.room.bodies[slot].ctype = 0;
        if let Some(i) = self
            .herd
            .chars
            .iter()
            .position(|c| c.state as usize == slot)
        {
            self.herd.chars.remove(i);
        }
        self.trophy_time = 0;
        self.trophy_body = None;
    }

    /// Locks or unlocks the trophy whose plaque is showing (locked ones
    /// can't be removed). Whether it is now locked.
    pub fn toggle_lock_current(&mut self) -> Option<bool> {
        let slot = self.trophy_body.filter(|_| self.opts.trophy)?;
        let b = self.room.bodies.get_mut(slot).filter(|b| b.ctype != 0)?;
        b.set_locked(!b.locked());
        Some(b.locked())
    }

    /// Moves the trophy whose plaque is showing to the next free stand
    /// before it (`-1`) or after it (`1`); its stuffed animal goes there.
    pub fn move_current_trophy(&mut self, area: &Area, dir: i32) -> bool {
        let Some(slot) = self.trophy_body.filter(|_| self.opts.trophy) else {
            return false;
        };
        let n = if self.herd.trophy_spots.is_empty() {
            self.room.bodies.len().min(area.landings.len())
        } else {
            self.herd.trophy_spots.len().min(self.room.bodies.len())
        };
        if n < 2 || !matches!(dir, -1 | 1) || slot >= n {
            return false;
        }
        // The phone room is a closed walk around four walls. Its last
        // and first stands are neighbours at the northwest corner.
        let wraps = area.trophy_start.is_some() && self.herd.trophy_spots.is_empty();
        let mut t = slot as i32;
        let mut to = None;
        for _ in 1..n {
            t += dir;
            if wraps {
                t = t.rem_euclid(n as i32);
            } else if t < 0 || t >= n as i32 {
                break;
            }
            if self.room.bodies[t as usize].ctype == 0 {
                to = Some(t as usize);
                break;
            }
        }
        let Some(to) = to else { return false };
        self.room.bodies.swap(slot, to);
        let room = self.room.clone();
        self.herd.place_trophies(area, &room, &mut self.rng);
        self.trophy_body = Some(to);
        true
    }
}
