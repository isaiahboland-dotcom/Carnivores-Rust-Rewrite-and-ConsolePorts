//! Camera, map overview, binoculars and scope queries.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec, vec::Vec};

use super::{Hunt, HuntInput, WeaponState};
use crate::area::Area;
use crate::trace::trace_look;

/// What the binoculars show about the animal in the cross-hairs.
#[derive(Debug, Clone, PartialEq)]
pub struct LifeInfo {
    pub index: usize,
    pub name: String,
    /// Tonnes (`Mass * scale^2`).
    pub weight: f32,
    /// World units.
    pub distance: f32,
}

impl Hunt {
    /// Camera position and angles for rendering.
    pub fn camera(&self) -> ([f32; 3], f32, f32) {
        match &self.death {
            Some(d) => {
                // The orbit position eases toward its circle. Its orbit
                // angle is not the actual direction to the watched body.
                let alpha = self
                    .herd
                    .demo_index
                    .and_then(|i| self.herd.chars.get(i))
                    .map_or(d.alpha, |c| {
                        (c.pos[0] - d.pos[0]).atan2(d.pos[2] - c.pos[2])
                    });
                (d.pos, alpha, d.beta)
            }
            None => {
                let (a, b) = self.player.camera_angles();
                (self.player.eye(), a, b)
            }
        }
    }

    /// `GenerateMapImage`: the 256x256 overview (1 pixel per 4 cells) made
    /// from each cell's texture at the 32x32 mip level (16x16 for water).
    pub fn map_image(area: &Area) -> Vec<u16> {
        let mip = |src: &[u16], l: usize| -> Vec<u16> {
            // GenerateMipMap: average 2x2 per 5-bit channel, rounded.
            let mut d = vec![0u16; l * l];
            for y in 0..l {
                for x in 0..l {
                    let at = |dx: usize, dy: usize| {
                        u32::from(
                            src.get((y * 2 + dy) * l * 2 + x * 2 + dx)
                                .copied()
                                .unwrap_or(0),
                        )
                    };
                    let px = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                    let ch = |sh: u32| {
                        ((px.iter().map(|p| (p >> sh) & 31).sum::<u32>() + 2) >> 2) as u16
                    };
                    d[y * l + x] = (ch(10) << 10) | (ch(5) << 5) | ch(0);
                }
            }
            d
        };
        let mips: Vec<(Vec<u16>, Vec<u16>)> = area
            .rsc
            .textures
            .iter()
            .map(|t| {
                let b = mip(t, 64);
                let c = mip(&b, 32);
                let d = mip(&c, 16);
                (c, d)
            })
            .collect();
        let mut out = vec![0u16; 256 * 256];
        for y in 0..256 {
            for x in 0..256 {
                let i = (y << 2) * c2_formats::map::MAP_SIZE + (x << 2);
                let px = if area.map.flags[i] & crate::area::cell_flags::WATER != 0 {
                    let t = area
                        .rsc
                        .waters
                        .get(usize::from(area.map.water[i]))
                        .map_or(0, |w| w.texture_index.max(0) as usize);
                    mips.get(t).map_or(0, |(_, d)| d[(y & 15) * 16 + (x & 15)])
                } else {
                    let t = usize::from(area.map.texture1[i]);
                    mips.get(t).map_or(0, |(c, _)| c[(y & 31) * 32 + (x & 31)])
                };
                out[y * 256 + x] = px;
            }
        }
        out
    }

    /// `ToggleBinocular` and the +/- power keys.
    pub(super) fn binoculars(&mut self, input: &HuntInput, dt: i32) {
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
        #[cfg(feature = "hunt-diagnostics")]
        if let Some(log) = self.prof.diagnostic_state {
            log(
                input.toggle_binoculars as u32,
                (self.weapons.state == WeaponState::Hidden) as u32,
                self.player.underwater as u32,
                self.health,
                self.bin_mode as u32,
            );
        }
        // With binoculars already off and no toggle, there is nothing to update.
        if !input.toggle_binoculars && !self.bin_mode {
            return;
        }
        if input.toggle_binoculars
            && self.weapons.state == WeaponState::Hidden
            && !self.player.underwater
            && self.health > 0
        {
            self.bin_mode = !self.bin_mode;
        }
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
        if self.player.underwater {
            self.bin_mode = false;
        }
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
        if self.bin_mode {
            let k = self.bin_power * dt as f32 / 4000.0;
            if input.zoom_in {
                self.bin_power += k;
            }
            if input.zoom_out {
                self.bin_power -= k;
            }
            self.bin_power = self.bin_power.clamp(1.5, 3.0);
        }
        #[cfg(feature = "hunt-diagnostics")]
        self.prof.mark(0);
    }

    /// Field-of-view magnification: binoculars, or the sniper scope while a
    /// scoped weapon is out.
    pub fn zoom(&self) -> f32 {
        if self.bin_mode {
            self.bin_power
        } else if self.scoped() {
            match self
                .weapons
                .info
                .get(self.weapons.current)
                .and_then(|i| i.mee.as_ref())
            {
                Some(m) => m.zoom.max(0.1),
                None => 3.0,
            }
        } else {
            1.0
        }
    }

    /// Looking through the current weapon's scope (zoomed): optic mode,
    /// the weapon out, and (an `unzoom` dialect weapon) ready.
    fn scoped(&self) -> bool {
        let w = &self.weapons;
        let unzoomed = w
            .info
            .get(w.current)
            .and_then(|i| i.mee.as_ref())
            .is_some_and(|m| m.unzoom && w.state != WeaponState::Ready);
        w.optic_mode && w.state != WeaponState::Hidden && self.death.is_none() && !unzoomed
    }

    /// Cross-hairs on the view model's aim point (`DrawOpticCross`): the
    /// original's when scoped; a dialect weapon's only with `cross` (scoped,
    /// or out without a scope), not while it `unzoom`s.
    pub fn scope_cross(&self) -> bool {
        let w = &self.weapons;
        if self.bin_mode || self.death.is_some() || w.state == WeaponState::Hidden {
            return false;
        }
        match w.info.get(w.current).map(|i| (i, i.mee.as_ref())) {
            Some((i, Some(m))) => {
                m.cross
                    && (!i.optic || w.optic_mode)
                    && (!m.unzoom || w.state == WeaponState::Ready)
            }
            Some((_, None)) => self.scoped(),
            None => false,
        }
    }

    /// Crouching steadies both the view model and the shot direction.
    pub fn weapon_sway(&self) -> (f32, f32) {
        if self.player.head_y < crate::player::HEAD_STAND {
            (0.0, 0.0)
        } else {
            self.weapons.sway(self.real_time)
        }
    }

    pub fn life_info(&self, area: &Area) -> Option<LifeInfo> {
        if !self.bin_mode {
            return None;
        }
        let (cam, alpha, beta) = self.camera();
        let (sa, ca) = alpha.sin_cos();
        let (sb, cb) = beta.sin_cos();
        let view_r = self.opts.view_r as f32 * 256.0;
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in self.herd.chars.iter().enumerate() {
            if c.health == 0 {
                continue;
            }
            // RotateVector: into camera space.
            let (x, y, z) = (c.pos[0] - cam[0], c.pos[1] - cam[1], c.pos[2] - cam[2]);
            let rx = x * ca + z * sa;
            let vz = z * ca - x * sa;
            let ry = y * cb - vz * sb;
            let rz = vz * cb + y * sb;
            if rz > -512.0 {
                continue;
            }
            let d = (rx * rx + ry * ry + rz * rz).sqrt();
            if d > view_r || (rx.abs() + ry.abs()) / d > 0.15 {
                continue;
            }
            if best.is_some_and(|(_, bd)| d >= bd) {
                continue;
            }
            if trace_look(area, [c.pos[0], c.pos[1] + 220.0, c.pos[2]], cam).0 {
                continue;
            }
            best = Some((i, d));
        }
        let (i, _) = best?;
        let c = &self.herd.chars[i];
        let info = &self.herd.species.get(c.ctype)?.info;
        let p = &self.player;
        let distance =
            ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2)).sqrt();
        Some(LifeInfo {
            index: i,
            name: info.name.clone(),
            weight: info.mass * c.scale * c.scale,
            distance,
        })
    }
}
