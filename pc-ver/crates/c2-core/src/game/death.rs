//! Hunter death, synchronized bodies and the death camera.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{GameEvent, Hunt};
use crate::area::Area;
use crate::dino::{ai, delta_func, find_vector_alpha, Character, PI};

/// The death camera (`DemoPoint`).
#[derive(Debug, Clone, Copy)]
pub struct DeathCam {
    /// Character index of the hunter's body.
    pub body: usize,
    /// The animal whose kill animation may synchronize the hunter body.
    pub killer: Option<usize>,
    /// 1 = flying in, then ms since it started circling (+2).
    pub time: i32,
    pub pos: [f32; 3],
    pub alpha: f32,
    pub beta: f32,
    /// How far it circles, and how high above the watched body it looks
    /// (the original: 824 and 256; 1424 and 768 over a T-Rex carrying the
    /// body; the killer's own in the dialect, `CamBaseLand`, `CamYLand`).
    pub base: f32,
    pub lift: f32,
}

/// The death camera's distance and height over `killer`'s kill (`None`:
/// no killer, drowned or the like). The dialect's entry gives its own
/// (`camBase`, `camDemoPoint`, the water ones for a kill in the water);
/// otherwise the original's, and further back from a killer whose model is
/// bigger than those numbers were made for (ours: the original keeps every
/// animal at 824, so a big add-on fills the screen and hides the body).
pub fn death_cam_view(
    killer: Option<(&Character, &crate::dino::Species)>,
    in_water: bool,
    carried: bool,
) -> (f32, f32) {
    let (base, lift) = if carried {
        (1424.0, 768.0)
    } else {
        (824.0, 256.0)
    };
    let Some((c, sp)) = killer else {
        return (base, lift);
    };
    if let Some(m) = &sp.mee {
        let (b, y) = if in_water {
            (m.cam_base_water, m.cam_y_water)
        } else {
            (m.cam_base, m.cam_y)
        };
        if b.is_some() || y.is_some() {
            let scale = c.scale.max(1.0);
            return (b.unwrap_or(824.0) * scale, y.unwrap_or(256.0) * scale);
        }
    }
    let reach = sp.size * c.scale.max(0.1);
    // 1.4: the original T-Rex (reach about 1000) stays at its 1424.
    (base.max(reach * 1.4), lift.max(reach * 0.6))
}

impl Hunt {
    /// `AddDeadBody`: the hunter dies; a body is added and the camera
    /// switches to the death cam.
    pub(super) fn kill_hunter(
        &mut self,
        area: &Area,
        by: Option<usize>,
        kind: i32,
        scream: bool,
        out: &mut Vec<GameEvent>,
    ) {
        if self.death.is_some() {
            return;
        }
        self.health = 0;
        self.bin_mode = false;
        self.weapons.optic_mode = false;
        let (cam, alpha, beta) = self.camera();
        // Small platform rosters need not keep the hunter in slot zero; a
        // killer naming its own hunter (`hunterFile`) gets that one.
        let own = by
            .and_then(|i| self.herd.chars.get(i))
            .and_then(|c| self.herd.species.get(c.ctype))
            .and_then(|s| s.mee.as_ref()?.hunter_file.as_deref());
        let hunters = || {
            self.herd
                .species
                .iter()
                .enumerate()
                .filter(|(_, s)| s.info.ai == ai::HUNTER)
        };
        let hunter_type = own
            .and_then(|f| {
                hunters()
                    .find(|(_, s)| s.info.file.eq_ignore_ascii_case(f))
                    .map(|(i, _)| i)
            })
            .or_else(|| hunters().next().map(|(i, _)| i))
            .unwrap_or(0);
        let mut body = Character {
            ctype: hunter_type,
            alpha,
            ..Default::default()
        };
        if let Some(sp) = self.herd.species.get(hunter_type) {
            body.reset(sp, &mut self.rng, self.opts.opt_agres);
        }
        body.health = 0;
        body.pos = [self.player.x, self.player.y, self.player.z];
        let killer = by.and_then(|i| self.herd.chars.get(i).map(|c| (i, c.clone())));
        // The dialect's `killedwater`: the killer stood in water deeper than
        // it wades.
        let killer_view = killer.as_ref().and_then(|(k, c)| {
            let sp = self.herd.species.get(c.ctype)?;
            let level = sp.mee.as_ref().map_or(0, |m| m.water_level) as f32;
            let depth = area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2]);
            Some((*k, depth > level * c.scale))
        });
        if let Some((_, c)) = &killer {
            let mee = self.herd.species.get(c.ctype).and_then(|s| s.mee.as_ref());
            let pl = match (mee, c.ai) {
                // The dialect: the kill type's `hunterOffset`.
                (Some(m), _) => m.kill(c.kill_type).map_or(0.0, |k| k.offset),
                (None, ai::SPINO) => 200.0,
                (None, ai::CERAT) => 320.0,
                (None, ai::TREX) => 0.0,
                _ => 170.0,
            };
            let (x, z) = (
                c.pos[0] + c.lookx * pl * c.scale,
                c.pos[2] + c.lookz * pl * c.scale,
            );
            body.pos = [x, area.land_qh(x, z, self.player.y), z];
        }
        body.phase = kind.max(0) as usize;
        // A kill type borrowed for another hunter may name an animation past
        // its own: the first death instead.
        if self
            .herd
            .species
            .get(hunter_type)
            .is_some_and(|sp| body.phase >= sp.ani_time.len())
        {
            body.phase = 0;
        }
        body.prev_phase = body.phase;
        self.herd.chars.push(body);
        let index = self.herd.chars.len() - 1;
        self.herd.demo_index = Some(index);
        if let Some((k, c)) = killer {
            // The dialect does not carry the body (`elevate` is disabled).
            let mee = self
                .herd
                .species
                .get(c.ctype)
                .is_some_and(|s| s.mee.is_some());
            if c.ai == ai::TREX && !mee {
                // The T-Rex carries the body: same size and heading, and the
                // camera watches the T-Rex.
                let b = &mut self.herd.chars[index];
                b.scale = c.scale;
                b.alpha = c.alpha;
                self.herd.demo_index = Some(k);
            }
        }
        let carried = self.herd.demo_index != Some(index);
        let view = killer_view.map(|(k, in_water)| {
            let c = &self.herd.chars[k];
            (c, &self.herd.species[c.ctype], in_water)
        });
        let (base, lift) = death_cam_view(
            view.map(|(c, s, _)| (c, s)),
            view.is_some_and(|v| v.2),
            carried,
        );
        self.death = Some(DeathCam {
            body: index,
            killer: by,
            time: 1,
            pos: cam,
            alpha,
            beta,
            base,
            lift,
        });
        self.sync_killed_body(area);
        out.push(GameEvent::HunterDied { kind, scream });
    }

    /// MEE `AnimateHuntDead`: a synchronized kill uses the killer's origin,
    /// heading, size and animation clock. The hunter clip already contains
    /// the relative motion (for example the Diabloceratops toss).
    pub(super) fn sync_killed_body(&mut self, area: &Area) {
        let Some(d) = self.death else { return };
        let Some(killer) = d.killer.and_then(|i| self.herd.chars.get(i)) else {
            return;
        };
        let Some(m) = self
            .herd
            .species
            .get(killer.ctype)
            .and_then(|s| s.mee.as_ref())
        else {
            return;
        };
        let Some(k) = m.kill(killer.kill_type) else {
            return;
        };
        let water = area.land_up_h(killer.pos[0], killer.pos[2])
            - area.land_height(killer.pos[0], killer.pos[2])
            > m.water_level as f32 * killer.scale;
        if (water && !crate::dino::mee::is_fish(m.ai))
            || !(k.carry_corpse || (k.hunter_sync && killer.phase == k.anim))
        {
            return;
        }
        let pose = (
            killer.pos,
            killer.alpha,
            killer.beta,
            killer.gamma,
            killer.scale,
            killer.ftime,
        );
        let carry_anim = (killer.phase != k.anim && k.carry_corpse)
            .then_some(k.hunter_carry_anim)
            .flatten();
        if let Some(body) = self.herd.chars.get_mut(d.body) {
            if let Some(phase) = carry_anim.and_then(|a| usize::try_from(a).ok()) {
                body.phase = phase;
                body.prev_phase = phase;
            }
            (
                body.pos, body.alpha, body.beta, body.gamma, body.scale, body.ftime,
            ) = pose;
            (body.lookx, body.lookz) = (body.alpha.cos(), body.alpha.sin());
        }
    }

    /// `ProcessDemoMovement`: fly to the body, then circle it.
    pub(super) fn death_cam(&mut self, area: &Area, mut d: DeathCam, dt: i32) {
        let dtf = dt as f32;
        let Some(watched) = self.herd.demo_index.and_then(|i| self.herd.chars.get(i)) else {
            return;
        };
        let mut target = watched.pos;
        target[1] += d.lift;
        let base = d.base;
        let l = ((target[0] - d.pos[0]).powi(2) + (target[2] - d.pos[2]).powi(2)).sqrt();
        if d.time == 1 && l < base {
            d.time = 2;
        }
        let mut nv = [
            target[0] - d.pos[0],
            target[1] - d.pos[1],
            target[2] - d.pos[2],
        ];
        let nl = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2])
            .sqrt()
            .max(1e-6);
        nv = [nv[0] / nl, nv[1] / nl, nv[2] / nl];
        if d.time == 1 {
            delta_func(&mut d.pos[0], target[0], nv[0].abs() * dtf * 3.0);
            delta_func(&mut d.pos[2], target[2], nv[2].abs() * dtf * 3.0);
        } else {
            d.time += dt;
            d.alpha += dtf / 1224.0;
            let (sa, ca) = d.alpha.sin_cos();
            // The circle moves at base / 1224 units per millisecond.
            // A fixed 1-unit step cannot catch it for large modded kills,
            // pulling the camera inside its intended radius.
            let step = dtf * (1.0 + base / 1224.0);
            delta_func(&mut d.pos[0], target[0] - sa * base, step);
            delta_func(&mut d.pos[2], target[2] + ca * base, step);
        }
        let horiz = ((target[0] - d.pos[0]).powi(2) + (target[2] - d.pos[2]).powi(2)).sqrt();
        let mut b = find_vector_alpha(horiz, target[1] - d.pos[1] - 400.0);
        if b > PI {
            b -= 2.0 * PI;
        }
        delta_func(&mut d.beta, -b, dtf / 4000.0);
        let h = area.land_qh(d.pos[0], d.pos[2], d.pos[1]);
        delta_func(&mut d.pos[1], h + 128.0, dtf / 8.0);
        if d.pos[1] < h + 80.0 {
            d.pos[1] = h + 80.0;
        }
        // `ProcessDemoMovement` keeps the camera at least as high as
        // its demo point; without this a large killer fills the sky.
        d.pos[1] = d.pos[1].max(target[1]);
        self.death = Some(d);
    }
}
