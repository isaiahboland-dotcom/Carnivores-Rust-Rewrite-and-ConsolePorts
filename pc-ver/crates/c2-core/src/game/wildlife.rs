//! Animal replacement, hearing, detection and target identity.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;

use super::Hunt;
use crate::area::Area;
use crate::dino::eco::{Deferred, Spawn};
use crate::dino::{ai, Character};
use crate::trace::trace_look;

impl Hunt {
    /// `Characters_AddSecondaryOne`: replace a killed ambient animal. With
    /// an ecosystem, a replacement that finds no room or no spot is tried
    /// again later ([`Self::retry_replacements`]).
    pub(super) fn add_secondary_one(&mut self, area: &Area, ctype: usize) {
        if self.replace_one(area, ctype) == Some(false) {
            if let Some(e) = &mut self.herd.eco {
                e.defer(Deferred { ctype, group: None });
            }
        }
    }

    /// One try at [`Self::add_secondary_one`]. `None`: nothing is replaced
    /// in this hunt; `Some(false)`: no room or no spot this time.
    fn replace_one(&mut self, area: &Area, ctype: usize) -> Option<bool> {
        if self.survival.is_some() || self.herd.chars.len() > 64 {
            return None;
        }
        if !self.herd.eco_room(ctype) {
            return Some(false);
        }
        let (px, pz) = (self.player.x, self.player.z);
        for _ in 0..128 {
            let mut p = [
                px + self.rng.si_rand(20040) as f32,
                0.0,
                pz + self.rng.si_rand(20040) as f32,
            ];
            p[1] = area.land_height(p[0], p[2]);
            if crate::dino::check_place_collision_p(area, &mut p) {
                continue;
            }
            if !self
                .herd
                .hunter_clear(Spawn::Respawn, Some(ctype), &p, (px, pz))
                || !self.herd.eco_spot(ctype, &p, &mut self.rng)
            {
                continue;
            }
            let ctype = self.herd.select_variant(ctype, &mut self.rng)?;
            let mut c = Character {
                ctype,
                pos: p,
                tgx: p[0],
                tgz: p[2],
                home_zone: self.herd.eco_home(&p),
                ..Default::default()
            };
            if let Some(sp) = self.herd.species.get(ctype) {
                c.reset(sp, &mut self.rng, self.opts.opt_agres);
            }
            self.herd.chars.push(c);
            return Some(true);
        }
        Some(false)
    }

    /// The ecosystem's replacements that found no room or no spot are
    /// tried again every few seconds; those that fail wait on.
    pub(super) fn retry_replacements(&mut self, area: &Area, dt: i32) {
        let Some(due) = self.herd.eco.as_mut().map(|e| e.due(dt)) else {
            return;
        };
        let hunter = (self.player.x, self.player.z);
        let agres = self.opts.opt_agres;
        for d in due {
            let done = match d.group {
                Some(_) => self.herd.replace_in(area, d, hunter, &mut self.rng, agres),
                None => self.replace_one(area, d.ctype),
            };
            if done == Some(false) {
                if let Some(e) = &mut self.herd.eco {
                    e.defer(d);
                }
            }
        }
    }

    /// `MakeNoise`: startle everything within `range`. Dialect species hear
    /// `2 * hearK` times as far, only start an alarm if calm, and remember
    /// the shot (`heardShot`).
    pub fn make_noise(&mut self, pos: [f32; 3], range: f32) {
        let flee = self.herd.flee.as_deref();
        for c in &mut self.herd.chars {
            if c.health == 0 {
                continue;
            }
            let l = ((c.pos[0] - pos[0]).powi(2)
                + (c.pos[1] - pos[1]).powi(2)
                + (c.pos[2] - pos[2]).powi(2))
            .sqrt();
            let sp = self.herd.species.get(c.ctype);
            if let Some(sp) = sp.filter(|s| s.mee.is_some()) {
                if l > range * sp.info.hear_k * 2.0 {
                    continue;
                }
                if c.ai == ai::TREX {
                    if c.state == 0 {
                        c.state = 2;
                        c.aware_hunter = true;
                        c.heard_shot = true;
                    }
                    continue;
                }
                // Fish and Mosasaurus do not hear shots (`Aquatic`).
                if crate::dino::mee::is_fish(c.ai) {
                    continue;
                }
                let ticks = ((10.0 + (range - l) / 256.0) as i32) * 1024;
                if !c.scare(ticks, flee.map(|t| t.limits(c.ctype))) {
                    continue;
                }
                if c.state == 0 {
                    c.state = 2;
                }
                c.no_find_cnt = 0;
                c.aware_hunter = true;
                c.heard_shot = true;
                continue;
            }
            if l > range {
                continue;
            }
            if c.ai == ai::TREX {
                if c.state == 0 {
                    c.state = 2;
                }
            } else {
                let ticks = ((10.0 + (range - l) / 256.0) as i32) * 1024;
                if !c.scare(ticks, flee.map(|t| t.limits(c.ctype))) {
                    continue;
                }
                c.state = 2;
                c.no_find_cnt = 0;
                // Read only by predation's hunter-first priority.
                c.aware_hunter = true;
            }
        }
        // Awareness (a setting): an alarmed animal in earshot takes the
        // hunter to be where the noise came from.
        if self.herd.awareness.is_some() {
            for c in &mut self.herd.chars {
                let l = ((c.pos[0] - pos[0]).powi(2)
                    + (c.pos[1] - pos[1]).powi(2)
                    + (c.pos[2] - pos[2]).powi(2))
                .sqrt();
                let sp = self.herd.species.get(c.ctype);
                let k = sp
                    .filter(|s| s.mee.is_some())
                    .map_or(1.0, |s| s.info.hear_k * 2.0);
                if c.health != 0 && c.state != 0 && l <= range * k {
                    c.aware.heard(pos[0], pos[2]);
                }
            }
        }
    }

    /// `CheckAfraid`: can each big animal see or smell the hunter? `dt` is
    /// the time since the last check, ms (awareness's timers).
    pub(super) fn check_afraid(&mut self, area: &Area, dt: i32) {
        if self.health <= 0 || self.opts.trophy || self.opts.observer || self.player.underwater {
            return;
        }
        let hunter = (self.player.x, self.player.z);
        for i in 0..self.herd.chars.len() {
            let c = &self.herd.chars[i];
            let Some(sp) = self.herd.species.get(c.ctype) else {
                continue;
            };
            // The dialect keeps checking alarmed animals (renewing
            // `AfraidTime`), and asks the AI's table which ones look.
            let mee = sp.mee.is_some();
            let skip = if mee {
                !crate::dino::mee::ai_info(c.ai).sniffer
                    || (c.ai == ai::TREX && (c.afraid_time != 0 || c.state == 1))
            } else {
                // A predator chasing an animal still notices the hunter
                // (predation, a setting).
                let hunting = matches!(c.predation.goal, crate::dino::predation::Goal::Hunt(_));
                c.ai < 10 || (!hunting && (c.afraid_time != 0 || c.state == 1))
            };
            if c.health == 0 {
                continue;
            }
            let noticed = if skip {
                None
            } else {
                self.notice(area, i, false)
            };
            if let Some((k_res, k_look, k_smell)) = noticed.filter(|n| n.0 < 1.0) {
                let c = &mut self.herd.chars[i];
                if !mee && matches!(c.predation.goal, crate::dino::predation::Goal::Hunt(_)) {
                    // Noticed while chasing an animal: predation decides.
                    c.aware_hunter = true;
                } else {
                    c.afraid_time = (1.0 / (k_res + 0.1) * 10.0 * 1000.0) as i32;
                    if !mee || c.state == 0 {
                        c.state = 2;
                    }
                    c.aware_hunter = true;
                    if c.ai == ai::TREX && k_look > k_smell {
                        c.state = 3;
                    }
                    c.no_find_cnt = 0;
                }
            }
            // Awareness (a setting): what the animal makes of that.
            if self.herd.awareness.is_some() {
                let c = &self.herd.chars[i];
                let tracks = self
                    .herd
                    .awareness
                    .as_deref()
                    .is_some_and(|r| r.tracks(c.ctype));
                // An alarmed predator is tested again without the crouch.
                let k = if c.state != 0 && tracks {
                    self.notice(area, i, true)
                } else {
                    noticed
                };
                if let Some(r) = self.herd.awareness.as_deref() {
                    r.sense(&mut self.herd.chars[i], k.map(|n| n.0), hunter, dt);
                }
            }
        }
    }

    /// The original's detection values for character `i`: the result (below
    /// 1: it notices the hunter), and its sight and smell parts. `None`:
    /// the hunter is out of range. `plain` leaves the stealth crouch out,
    /// taking the hunter as standing.
    pub(super) fn notice(&mut self, area: &Area, i: usize, plain: bool) -> Option<(f32, f32, f32)> {
        let p = &self.player;
        let ppos = [p.x, p.y, p.z];
        let head_y = p.head_y;
        let wlook = self.wind.nv();
        let kmask = if self.opts.camo { 1.5 } else { 1.0 };
        let kscent = if self.opts.scent { 1.5 } else { 1.0 };
        // Crouched (`kstand`'s test) with stealth crouch on.
        let stealthy = !plain && self.opts.stealth && head_y <= 180.0;
        let c = &self.herd.chars[i];
        let sp = self.herd.species.get(c.ctype)?;
        let mut rlook = [ppos[0] - c.pos[0], ppos[1] - c.pos[1], ppos[2] - c.pos[2]];
        let rl = (rlook[0] * rlook[0] + rlook[1] * rlook[1] + rlook[2] * rlook[2]).sqrt();
        let mut kr = rl / 256.0 / (32.0 + self.opts.view_r as f32 / 2.0);
        if rl > 0.0 {
            rlook = [rlook[0] / rl, rlook[1] / rl, rlook[2] / rl];
        }
        kr *= 2.5 / (1.5 + self.opts.opt_sens as f32 / 128.0);
        kr /= self.opts.notice_k.clamp(0.1, 1.0);
        if kr > 3.0 {
            return None;
        }
        let kwind = (wlook[0] * rlook[0] + wlook[2] * rlook[2]) * self.wind.speed / 10.0;
        let klook = -(c.lookx * rlook[0] + c.lookz * rlook[2]);
        let kstand = if plain || head_y > 180.0 { 0.7 } else { 1.2 };

        let mut k_look = kr * ((klook + 3.0) / 3.0) * kstand * kmask;
        if klook > 0.3 {
            k_look *= 2.0;
        }
        if klook > 0.8 {
            k_look *= 2.0;
        }
        k_look /= sp.info.look_k.max(1e-3);
        let eye = [c.pos[0], c.pos[1] + 220.0, c.pos[2]];
        if k_look < 1.0 {
            let (blocked, n) = trace_look(area, eye, [ppos[0], ppos[1] + head_y / 2.0, ppos[2]]);
            self.objects_on_look = n;
            if blocked {
                k_look *= 1.3;
            }
        }
        if k_look < 1.0 {
            let (blocked, n) = trace_look(area, eye, [ppos[0], ppos[1] + head_y, ppos[2]]);
            self.objects_on_look = n;
            if blocked {
                k_look = 2.0;
            }
        }
        k_look *= 1.0 + self.objects_on_look as f32 / 6.0;
        if stealthy {
            k_look *= 2.0;
        }

        let mut k_smell = kr * ((kwind + 2.0) / 2.0) * ((klook + 3.0) / 3.0) * kscent;
        if kwind > 0.0 {
            k_smell *= 2.0;
        }
        k_smell /= sp.info.smell_k.max(1e-3);

        let k_res = k_look.min(k_smell);
        Some((
            if k_res < 1.0 { k_res.min(kr) } else { k_res },
            k_look,
            k_smell,
        ))
    }

    /// `ScanLifeForms`: the nearest visible animal close to the view centre.
    /// Whether `c` is one of the hunted species: in Modder's Edition data,
    /// picked by its resolved menu identity. Legacy tables without explicit
    /// identity retain their radar/call rules; original data uses AI (`TargetDino`).
    pub fn is_target(&self, c: &crate::dino::Character) -> bool {
        let Some(sp) = self.herd.species.get(c.ctype) else {
            return false;
        };
        if sp.tuning.menu_slot.is_some() {
            return self
                .call_bit(c)
                .is_some_and(|b| self.opts.targets.has(b as usize));
        }
        sp.tuning.radar.unwrap_or_else(|| {
            self.call_bit(c)
                .is_some_and(|b| self.opts.targets.has(b as usize))
        })
    }

    /// The target bit (call id) `c` is picked by: `10 + callNo` in Modder's
    /// Edition data, its AI in the original's. `None` for a dialect ambient,
    /// flyer or fish: its `callNo` is 0 when not given, which would make it
    /// the first menu animal (green on the radar when that one is hunted).
    pub fn call_bit(&self, c: &crate::dino::Character) -> Option<i32> {
        let sp = self.herd.species.get(c.ctype)?;
        if let Some(slot) = sp.tuning.menu_slot {
            return (slot >= 0).then_some(10 + slot);
        }
        match &sp.mee {
            Some(m) if (10..=29).contains(&m.ai) => Some(10 + m.call_no),
            Some(_) => None,
            None => Some(sp.info.ai),
        }
    }

    /// A huntable animal (a menu animal, picked or not), not an ambient,
    /// flyer or fish.
    /// Whether `c` lives in the water (the dialect's fish and Mosasaurus
    /// AIs): never a radar target of its own accord.
    pub fn is_swimmer(&self, c: &crate::dino::Character) -> bool {
        self.herd
            .species
            .get(c.ctype)
            .and_then(|s| s.mee.as_ref())
            .is_some_and(|m| crate::dino::mee::is_fish(m.ai))
    }

    pub fn is_huntable(&self, c: &crate::dino::Character) -> bool {
        match self.herd.species.get(c.ctype).and_then(|s| s.mee.as_ref()) {
            Some(m) => (10..=29).contains(&m.ai),
            None => c.ai >= 10,
        }
    }
}
