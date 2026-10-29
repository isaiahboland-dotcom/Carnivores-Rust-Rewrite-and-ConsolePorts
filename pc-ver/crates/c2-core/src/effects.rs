//! Particles, blood trails and water rings (`Game.cpp: AddElements,
//! AnimateElements, AddBloodTrail, AddWCircle`).

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

use crate::area::Area;
use crate::rng::MsRand;

/// `part*` element kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Blood,
    Water,
    Ground,
    Bubble,
}

#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub pos: [f32; 3],
    speed: [f32; 3],
    /// Radius in the original's units; world radius is `r * 0.64`.
    pub r: f32,
    landed: bool,
}

/// `TElements`: one burst. Colours are `[a, r, g, b]` (centre, edge).
#[derive(Debug, Clone)]
pub struct Burst {
    pub kind: Part,
    pub rgba: [u8; 4],
    pub rgba2: [u8; 4],
    pub parts: Vec<Particle>,
    done: usize,
}

/// `TBloodP`: a drop on the ground; fades over its last 20 s.
#[derive(Debug, Clone, Copy)]
pub struct BloodDrop {
    pub pos: [f32; 3],
    pub life: i32,
}

/// `TWCircle`: an expanding water ring (`WCIRCLE2.CAR`, animation 0).
#[derive(Debug, Clone, Copy)]
pub struct Ring {
    pub pos: [f32; 3],
    pub ftime: i32,
    pub scale: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Effects {
    pub bursts: Vec<Burst>,
    pub blood: Vec<BloodDrop>,
    pub rings: Vec<Ring>,
    /// Falling snow (Ice Age's areas; the dialect's `areatable` `snow{}`).
    pub snow: Option<Snowfall>,
}

/// A snowflake (`TSnowElement`): falling, or lying (`ftime` > 0, ms).
#[derive(Debug, Clone, Copy, Default)]
pub struct Flake {
    pub pos: [f32; 3],
    hl: f32,
    pub ftime: f32,
}

/// The snow round the hunter (`Game.cpp: AnimateElements`, `RespawnSnow`
/// in the Modder's Edition).
#[derive(Debug, Clone)]
pub struct Snowfall {
    pub kind: c2_formats::mee::Snow,
    pub flakes: Vec<Flake>,
    /// At most this many (the 3DS: fewer than the area asks).
    cap: usize,
}

impl Snowfall {
    pub fn new(kind: c2_formats::mee::Snow, cap: usize) -> Self {
        Self {
            kind,
            flakes: Vec::new(),
            cap,
        }
    }

    /// A flake's alpha (0..1): fading while it lies.
    pub fn alpha(&self, f: &Flake) -> f32 {
        let a = f32::from(self.kind.rgba[3]) / 255.0;
        if f.ftime > 0.0 {
            a * ((2000.0 - f.ftime) / 2000.0).max(0.0)
        } else {
            a
        }
    }

    fn respawn(
        f: &mut Flake,
        area: &Area,
        rng: &mut MsRand,
        at: (f32, f32),
        nv: [f32; 3],
        high: bool,
    ) {
        f.pos[0] = at.0 + nv[0] + rng.si_rand(12 * 256) as f32;
        f.pos[2] = at.1 + nv[2] + rng.si_rand(12 * 256) as f32;
        f.hl = area.land_up_h(f.pos[0], f.pos[2]);
        f.ftime = 0.0;
        f.pos[1] = if high {
            f.hl + 256.0 + rng.r_rand(12 * 256) as f32
        } else {
            f.hl + (8 + rng.r_rand(5)) as f32 * 256.0
        };
    }

    /// One step: `at` the hunter, `wind` its heading and speed, `look` the
    /// camera's heading (the flakes sway across it).
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        area: &Area,
        rng: &mut MsRand,
        at: (f32, f32),
        wind: ([f32; 3], f32),
        look: f32,
        real_time: i32,
        dt: i32,
    ) {
        let k = self.kind;
        let dtf = dt as f32;
        let n = (4.0 + wind.1) * k.h_spd as f32 * dtf / 1000.0;
        let nv = [wind.0[0] * n, 0.0, wind.0[2] * n];
        let want = (k.dens.max(0) as usize).min(self.cap);
        while self.flakes.len() < want {
            let mut f = Flake::default();
            Self::respawn(&mut f, area, rng, at, nv, true);
            self.flakes.push(f);
        }
        self.flakes.truncate(want);
        let (sa, ca) = look.sin_cos();
        let slow = (k.v_spd / 192).max(1) as f32;
        for (s, f) in self.flakes.iter_mut().enumerate() {
            if (f.pos[0] - at.0).abs() > 14.0 * 256.0 || (f.pos[2] - at.1).abs() > 14.0 * 256.0 {
                f.pos[0] = at.0 + rng.si_rand(12 * 256) as f32;
                f.pos[2] = at.1 + rng.si_rand(12 * 256) as f32;
                f.pos[1] -= f.hl;
                f.hl = area.land_up_h(f.pos[0], f.pos[2]);
                f.pos[1] += f.hl;
            }
            if f.ftime == 0.0 {
                let v =
                    (((real_time + s as i32 * 23).rem_euclid(800)) - 400) as f32 * dtf / 16000.0;
                f.pos[0] += ca * v + nv[0];
                f.pos[2] += sa * v + nv[2];
                f.hl = area.land_up_h(f.pos[0], f.pos[2]);
                f.pos[1] -= dtf * k.v_spd as f32 / 1000.0;
                if f.pos[1] < f.hl + 8.0 {
                    f.pos[1] = f.hl + 8.0;
                    f.ftime = 1.0;
                }
            } else {
                f.ftime += dtf;
                f.pos[1] -= dtf * (k.v_spd / 64) as f32 / 1000.0;
                if f.ftime > 2000.0 / slow {
                    Self::respawn(f, area, rng, at, nv, false);
                }
            }
        }
    }
}

/// Per-byte saturating add (`ColorSum`) on `0x00RRGGBB`.
fn color_sum(a: u32, b: u32) -> u32 {
    (0..3).fold(0, |acc, i| {
        let s = 8 * i;
        acc | (((a >> s & 0xff) + (b >> s & 0xff)).min(255) << s)
    })
}

fn argb(c: u32) -> [u8; 4] {
    [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8]
}

/// Average colour of a terrain texture as `0x00RRGGBB` (`CalcMidColor`, D3D order).
pub fn texture_mid_color(tex: &[u16]) -> u32 {
    if tex.is_empty() {
        return 0;
    }
    let n = tex.len() as u32;
    let sum = |sh: u32| {
        tex.iter()
            .map(|&p| u32::from(p >> sh & 31) * 8)
            .sum::<u32>()
            / n
    };
    (sum(10) << 16) | (sum(5) << 8) | sum(0)
}

impl Effects {
    /// `AddElements`. `cam_alpha`: the camera yaw (sprays go away from it).
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        area: &Area,
        rng: &mut MsRand,
        pos: [f32; 3],
        kind: Part,
        count: i32,
        cam_alpha: f32,
    ) {
        if self.bursts.len() > 30 {
            self.bursts.remove(0);
        }
        let water_color = || {
            let (cx, cz) = ((pos[0] / 256.0) as usize, (pos[2] / 256.0) as usize);
            let w = area.map.water.get(cz * 1024 + cx).copied().unwrap_or(0);
            let t = area
                .rsc
                .waters
                .get(usize::from(w))
                .map_or(0, |w| w.texture_index.max(0) as usize);
            let c = area.rsc.textures.get(t).map_or(0, |t| texture_mid_color(t));
            color_sum((c & 0xfe_fefe) >> 1, 0x15_2020)
        };
        let (rgba, rgba2) = match kind {
            Part::Blood => (0xE060_0000, 0x2030_0000),
            Part::Ground => (0xF0F0_9E55, 0x10F0_9E55),
            Part::Bubble => {
                let c = water_color();
                (
                    0x7000_0000 + color_sum(c, color_sum(c, c)),
                    0x4000_0000 + color_sum(c, c),
                )
            }
            Part::Water => {
                let c = water_color();
                (0xB000_0000 + color_sum(c, color_sum(c, c)), 0x4000_0000 + c)
            }
        };
        let al = rng.si_rand(128) as f32 / 128.0 * core::f32::consts::PI / 4.0;
        let (ss, cc) = al.sin_cos();
        let (sa, ca) = cam_alpha.sin_cos();
        let n = count.clamp(0, 30);
        let mut parts = Vec::with_capacity(n as usize);
        for e in 0..n {
            let r = (6 + rng.r_rand(5)) as f32;
            let speed = match kind {
                Part::Blood => {
                    let v = (e * 6 + rng.r_rand(96) + 220) as f32;
                    let x = ss * ca * v + rng.si_rand(32) as f32;
                    let y = cc * v * 3.0;
                    [x, y, ss * sa * v + rng.si_rand(32) as f32]
                }
                Part::Ground => {
                    let x = rng.si_rand(52) as f32 - sa * 64.0;
                    let y = (rng.r_rand(100) + 600 + e * 20) as f32;
                    [x, y, rng.si_rand(52) as f32 + ca * 64.0]
                }
                Part::Water => {
                    let x = rng.si_rand(32) as f32;
                    let y = (rng.r_rand(80) + 400 + e * 40) as f32;
                    [x, y, rng.si_rand(32) as f32]
                }
                Part::Bubble => {
                    let x = rng.si_rand(40) as f32;
                    let y = (rng.r_rand(140) + 20) as f32;
                    [x, y, rng.si_rand(40) as f32]
                }
            };
            parts.push(Particle {
                pos,
                speed,
                r,
                landed: false,
            });
        }
        self.bursts.push(Burst {
            kind,
            rgba: argb(rgba),
            rgba2: argb(rgba2),
            parts,
            done: 0,
        });
    }

    /// `AddWCircle`.
    pub fn ring(&mut self, area: &Area, x: f32, z: f32, scale: f32) {
        self.rings.push(Ring {
            pos: [x, area.land_up_h(x, z), z],
            ftime: 0,
            scale,
        });
    }

    /// `AddBloodTrail`.
    pub fn blood_trail(&mut self, area: &Area, rng: &mut MsRand, pos: [f32; 3]) {
        if self.blood.len() > 508 {
            self.blood.remove(0);
        }
        let x = pos[0] + rng.si_rand(32) as f32;
        let z = pos[2] + rng.si_rand(32) as f32;
        self.blood.push(BloodDrop {
            pos: [x, area.land_height(x, z) + 4.0, z],
            life: 210_000,
        });
    }

    /// `AnimateElements` plus the ring and blood-trail timers.
    pub fn step(&mut self, area: &Area, dt: i32, takt: u32) {
        let dtf = dt as f32;
        let mut new_rings = Vec::new();
        self.bursts.retain_mut(|b| {
            let fade = |c: &mut [u8; 4], by: i32| c[0] = (i32::from(c[0]) - by).max(0) as u8;
            match b.kind {
                Part::Ground => {
                    fade(&mut b.rgba, dt / 4);
                    fade(&mut b.rgba2, dt / 4);
                    if b.rgba[0] == 0 && b.rgba2[0] == 0 {
                        return false;
                    }
                }
                Part::Water | Part::Bubble if b.done == b.parts.len() => return false,
                Part::Blood if takt & 3 == 0 && b.done == b.parts.len() => {
                    fade(&mut b.rgba, 1);
                    fade(&mut b.rgba2, 1);
                    if b.rgba[0] == 0 && b.rgba2[0] == 0 {
                        return false;
                    }
                }
                _ => {}
            }
            if b.parts.is_empty() {
                return false;
            }
            for p in b.parts.iter_mut().filter(|p| !p.landed) {
                for k in 0..3 {
                    p.pos[k] += p.speed[k] * dtf / 1000.0;
                }
                let h = area.land_up_h(p.pos[0], p.pos[2]);
                let on_water = area.land_height(p.pos[0], p.pos[2]) < h;
                let landed = if b.kind == Part::Bubble {
                    p.speed[1] = (p.speed[1] + 2.0 * 256.0 * dtf / 1000.0).min(824.0);
                    p.pos[1] > h
                } else {
                    p.speed[1] -= 9.8 * 256.0 * dtf / 1000.0;
                    p.pos[1] < h
                };
                if landed {
                    if on_water || b.kind == Part::Bubble {
                        new_rings.push((p.pos[0], p.pos[2]));
                    }
                    b.done += 1;
                    p.landed = true;
                    // Sunk out of sight in water; resting on the ground otherwise.
                    p.pos[1] = if on_water {
                        p.pos[1] - 10240.0
                    } else {
                        h + 4.0
                    };
                }
            }
            true
        });
        for (x, z) in new_rings {
            self.ring(area, x, z, 0.6);
        }
        self.blood.retain_mut(|d| {
            d.life -= dt;
            d.life > 0
        });
        self.rings.retain_mut(|r| {
            r.ftime += if r.scale > 1.0 {
                (dtf * 3.0 / r.scale) as i32
            } else {
                dt * 3
            };
            r.ftime < 2000
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_puff_rises_falls_and_fades() {
        let area = Area::flat(10);
        let mut fx = Effects::default();
        let mut rng = MsRand::new(1);
        fx.add(
            &area,
            &mut rng,
            [500.0 * 256.0, 700.0, 500.0 * 256.0],
            Part::Ground,
            10,
            0.0,
        );
        assert_eq!(fx.bursts[0].parts.len(), 10);
        fx.step(&area, 100, 1);
        assert!(
            fx.bursts[0].parts.iter().all(|p| p.pos[1] > 700.0),
            "thrown up"
        );
        for t in 0..200 {
            fx.step(&area, 10, t);
        }
        assert!(fx.bursts.is_empty(), "faded out");
    }

    #[test]
    fn rings_expire_and_blood_dries() {
        let area = Area::flat(10);
        let mut fx = Effects::default();
        let mut rng = MsRand::new(1);
        fx.ring(&area, 1000.0, 1000.0, 1.2);
        fx.blood_trail(&area, &mut rng, [1000.0, 0.0, 1000.0]);
        assert!((fx.blood[0].pos[1] - 644.0).abs() < 1e-3);
        fx.step(&area, 1000, 0);
        assert_eq!(fx.rings.len(), 0, "2000/3*1.2 ms");
        fx.step(&area, 210_000, 0);
        assert!(fx.blood.is_empty());
    }
}
