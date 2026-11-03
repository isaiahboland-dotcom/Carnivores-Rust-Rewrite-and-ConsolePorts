//! Ambient sounds and hunting calls.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{GameEvent, Hunt, HuntInput, MAX_CALL};
use crate::area::Area;
use crate::dino::ai;

impl Hunt {
    /// Ambient loop by zone and random wildlife sounds (`ProcessControls`).
    pub(super) fn ambience(&mut self, area: &Area, dt: i32, out: &mut Vec<GameEvent>) {
        let [cx, cy, cz] = self.player.eye();
        let (ax, az) = (
            ((cx as i32) >> 9).clamp(0, 511) as usize,
            ((cz as i32) >> 9).clamp(0, 511) as usize,
        );
        let zone = usize::from(area.map.ambient.get(az * 512 + ax).copied().unwrap_or(0));
        let want = if self.player.underwater {
            None
        } else {
            Some(zone)
        };
        if self.current_ambient != Some(want) {
            self.current_ambient = Some(want);
            let volume = match want {
                None => 240,
                Some(z) => area.rsc.ambients.get(z).map_or(0, |a| a.volume),
            };
            out.push(GameEvent::Ambient { zone: want, volume });
        }
        let Some(z) = want else { return };
        let (Some(t), Some(list), Some(amb)) = (
            self.amb_rnd.get_mut(z),
            self.amb_sounds.get(z),
            area.rsc.ambients.get(z),
        ) else {
            return;
        };
        if list.is_empty() {
            return;
        }
        *t -= dt;
        if *t > 0 {
            return;
        }
        let freq = amb.random[0].freq;
        *t = (freq / 2 + self.rng.r_rand(freq)) * 1000;
        let rr = self.rng.rand() as usize % list.len();
        let r = list[rr];
        let pos = [
            cx + self.rng.si_rand(4096) as f32,
            cy + self.rng.si_rand(256) as f32,
            cz + self.rng.si_rand(4096) as f32,
        ];
        out.push(GameEvent::RandomSound {
            sound: r.number.max(0) as usize,
            pos,
            volume: r.volume,
        });
    }

    /// `MakeCall`, `ChangeCall` and the delayed answer (`AnimateProcesses`).
    pub(super) fn calls(&mut self, input: &HuntInput, dt: i32, out: &mut Vec<GameEvent>) {
        self.call_lock = (self.call_lock - dt).max(0);
        self.call_icon_time = (self.call_icon_time - dt).max(0);
        if let Some((t, pos, call)) = &mut self.answer {
            *t -= dt;
            if *t <= 0 {
                let variant = (self.rng.r_rand(128) % 3) as usize;
                out.push(GameEvent::Call {
                    call: *call,
                    variant,
                    pos: Some(*pos),
                });
                self.answer = None;
            }
        }
        let targets = self.opts.targets;
        if input.change_call && !targets.is_empty() {
            if self.call_icon_time > 0 {
                for _ in 0..MAX_CALL {
                    self.target_call += 1;
                    if self.target_call >= MAX_CALL {
                        self.target_call = 10;
                    }
                    if usize::try_from(self.target_call).is_ok_and(|c| targets.has(c)) {
                        break;
                    }
                }
            }
            self.call_icon_time = 2048;
        }
        if !input.call
            || targets.is_empty()
            || self.player.underwater
            || self.opts.observer
            || self.opts.trophy
            || self.call_lock > 0
        {
            return;
        }
        self.call_lock = 1024 * 3;
        self.next_call = (self.next_call + (self.real_time % 2) + 1) % 3;
        out.push(GameEvent::Call {
            call: self.target_call,
            variant: self.next_call as usize,
            pos: None,
        });

        let danger = self
            .herd
            .species
            .iter()
            .find(|s| s.mee.is_none() && s.info.ai == self.target_call)
            .is_some_and(|s| s.info.danger_call);
        let p = [self.player.x, self.player.y, self.player.z];
        let mut best: Option<(usize, f32)> = None;
        let range = self.opts.view_r as f32 * 400.0;
        let slot = self.target_call - 10;
        for i in 0..self.herd.chars.len() {
            if danger && self.herd.chars[i].ai < 10 {
                let t = (10 + self.rng.r_rand(5)) * 1024;
                let c = &mut self.herd.chars[i];
                c.state = 2;
                c.afraid_time = t;
            }
            let c = &self.herd.chars[i];
            let d =
                ((p[0] - c.pos[0]).powi(2) + (p[1] - c.pos[1]).powi(2) + (p[2] - c.pos[2]).powi(2))
                    .sqrt();
            // The dialect's species: calls are by menu slot. A species
            // answers its `callNo` slot's call, flees the ones it fears
            // (`fearCall`), and hears `2 * hearK` times as far.
            let mee = self
                .herd
                .species
                .get(c.ctype)
                .and_then(|s| s.mee.as_ref().map(|m| (m, s.info.hear_k)));
            let (answers, hears) = match mee {
                Some((m, hear_k)) => {
                    let hears = d < range * hear_k * 2.0;
                    let flier = matches!(
                        m.ai,
                        ai::DIMOR | ai::PTERA | crate::dino::mee::mee_ai::BRACH
                    );
                    if c.health != 0 && hears && !flier && m.fear_calls.contains(&slot) {
                        let t = (10 + self.rng.r_rand(5)) * 1024;
                        let c = &mut self.herd.chars[i];
                        c.state = 2;
                        c.afraid_time = t;
                    }
                    (m.call_no == slot, hears)
                }
                // By its species' AI: a patched exe's extra slot runs as
                // another (`Species::stock_ai`).
                None => (self.call_bit(c) == Some(self.target_call), d < range),
            };
            let c = &self.herd.chars[i];
            if !answers || c.health == 0 || c.afraid_time != 0 || c.state != 0 {
                continue;
            }
            if hears {
                if self.rng.r_rand(128) > 32 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
                let (tx, tz) = (
                    p[0] + self.rng.si_rand(1800) as f32,
                    p[2] + self.rng.si_rand(1800) as f32,
                );
                let c = &mut self.herd.chars[i];
                c.tgx = tx;
                c.tgz = tz;
            }
        }
        if let Some((i, _)) = best {
            // The answer comes from a third of the way toward the animal.
            let c = self.herd.chars[i].pos;
            let pos = [
                p[0] + (c[0] - p[0]) / 3.0,
                p[1] + (c[1] - p[1]) / 3.0,
                p[2] + (c[2] - p[2]) / 3.0,
            ];
            self.answer = Some((2000 + self.rng.r_rand(2000), pos, self.target_call));
        }
    }
}
