//! Wave animal placement and initialization.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;

use super::navigation::Flow;
use super::options::WAVE_HEALTH;
use crate::area::Area;
use crate::dino::{find_vector_alpha, Character};
use crate::game::Hunt;
use crate::rng::MsRand;

/// How far above or below the hunter a wave animal may start (map units:
/// a few cells of steep slope), when a spot like that is found.
const SPAWN_RISE: f32 = 1536.0;

impl Hunt {
    /// One animal of wave `n` at a free spot on land in the box `[x, z, half
    /// x, half z]`, coming for the hunter. A box made for the mobile maps can take in
    /// the PC maps' mountain sides: spots with a way down to the hunter, near
    /// his height, first.
    pub(super) fn spawn_in_box(&mut self, area: &Area, ctype: usize, n: u32, b: [f32; 4]) {
        let flow = self.wave_flow(area);
        for strict in [true, false] {
            for _ in 0..32 {
                let r = |rng: &mut MsRand, h: f32| (rng.r_rand(20000) as f32 / 10000.0 - 1.0) * h;
                let (x, z) = (b[0] + r(&mut self.rng, b[2]), b[1] + r(&mut self.rng, b[3]));
                if self.spawn_spot(area, (x, z), &flow, strict) {
                    self.add_wave_animal(area, ctype, n, (x, z));
                    return;
                }
            }
        }
        // Random attempts can miss a narrow usable strip. Check cell centres
        // inside the box, but never fall back to the ring behind the hunter.
        let x0 = libm::floorf((b[0] - b[2]) / 256.0).max(16.0) as i32;
        let x1 = libm::floorf((b[0] + b[2]) / 256.0).min(1007.0) as i32;
        let z0 = libm::floorf((b[1] - b[3]) / 256.0).max(16.0) as i32;
        let z1 = libm::floorf((b[1] + b[3]) / 256.0).min(1007.0) as i32;
        for z in z0..=z1 {
            for x in x0..=x1 {
                let p = (x as f32 * 256.0 + 128.0, z as f32 * 256.0 + 128.0);
                if (p.0 - b[0]).abs() <= b[2]
                    && (p.1 - b[1]).abs() <= b[3]
                    && self.spawn_spot(area, p, &flow, false)
                {
                    self.add_wave_animal(area, ctype, n, p);
                    return;
                }
            }
        }
    }

    /// The flow field for where the hunter stands (kept for the wave).
    fn wave_flow(&mut self, area: &Area) -> Flow {
        let cell = (
            (self.player.x / 256.0) as i32,
            (self.player.z / 256.0) as i32,
        );
        match self.survival.as_ref().and_then(|s| s.flow.clone()) {
            Some(f) if f.from == cell => f,
            _ => {
                let f = Flow::new(
                    area,
                    cell,
                    self.survival.as_ref().and_then(|s| s.fence.as_ref()),
                );
                if let Some(s) = self.survival.as_mut() {
                    s.flow = Some(f.clone());
                }
                f
            }
        }
    }

    /// Whether a wave animal may start at (`x`, `z`): dry and free;
    /// a walkable way to the hunter is required even on fallback attempts.
    /// `near_height` additionally limits elevation to [`SPAWN_RISE`].
    fn spawn_spot(&self, area: &Area, (x, z): (f32, f32), flow: &Flow, near_height: bool) -> bool {
        let y = area.land_height(x, z);
        if area.land_up_h(x, z) - y > 32.0 {
            return false;
        }
        {
            let f = flow;
            let (cx, cz) = ((x / 256.0) as i32, (z / 256.0) as i32);
            let fence = self.survival.as_ref().and_then(|s| s.fence.as_ref());
            if fence.is_some_and(|fe| !fe.allows(cx, cz)) {
                return false;
            }
            if f.get(cx, cz) == u16::MAX {
                return false;
            }
            if near_height
                && (y - area.land_height(self.player.x, self.player.z)).abs() > SPAWN_RISE
            {
                return false;
            }
        }
        let mut p = [x, y, z];
        !crate::dino::check_place_collision_p(area, &mut p)
    }

    fn add_wave_animal(&mut self, area: &Area, ctype: usize, n: u32, (x, z): (f32, f32)) {
        let Some(sp) = self.herd.species.get(ctype) else {
            return;
        };
        let (hx, hz) = (self.player.x, self.player.z);
        let mut c = Character {
            ctype,
            alpha: find_vector_alpha(hx - x, hz - z),
            pos: [x, area.land_height(x, z), z],
            ..Default::default()
        };
        c.reset(sp, &mut self.rng, self.opts.opt_agres);
        c.health = c.health.clamp(1, WAVE_HEALTH);
        c.wave = n;
        c.state = 2;
        c.afraid_time = 60_000;
        c.aware_hunter = true;
        (c.tgx, c.tgz) = (hx, hz);
        self.herd.chars.push(c);
    }

    /// One animal of wave `n` on land in a ring around the hunter, out of
    /// his sight, coming for him.
    pub(super) fn spawn_wave_animal(&mut self, area: &Area, ctype: usize, n: u32) {
        let (hx, hz) = (self.player.x, self.player.z);
        let r0 = (self.opts.view_r as f32 * 0.7).clamp(20.0, 40.0) * 256.0;
        let size = 1024.0 * 256.0;
        let flow = self.wave_flow(area);
        for strict in [true, false] {
            for _ in 0..40 {
                let a = self.rng.r_rand(6283) as f32 / 1000.0;
                let r = r0 + self.rng.r_rand(2048) as f32;
                let (x, z) = (hx + a.cos() * r, hz + a.sin() * r);
                if !(4096.0..size - 4096.0).contains(&x) || !(4096.0..size - 4096.0).contains(&z) {
                    continue;
                }
                if self.spawn_spot(area, (x, z), &flow, strict) {
                    self.add_wave_animal(area, ctype, n, (x, z));
                    return;
                }
            }
        }
    }
}
