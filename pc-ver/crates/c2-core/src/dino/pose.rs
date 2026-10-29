//! `CreateChMorphedModel`: a character's vertices for this frame. Blends
//! the current and previous animation (for `PMORPHTIME` ms after a change),
//! then applies slope tilt (`beta`, `gamma`), body bend and scale.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use c2_formats::car::Car;

use super::{anim, Character, PMORPH_TIME};
use crate::model::car_anim_duration_ms;

fn frame(car: &Car, phase: usize, ftime: i32) -> Option<(&[i16], usize, f32)> {
    let a = car.animations.get(phase)?;
    let vc = car.mesh.vertices.len();
    if a.frames == 0 || a.data.len() < a.frames * vc * 3 {
        return None;
    }
    let t = car_anim_duration_ms(a).max(1) as i64;
    let cur = (a.frames as i64 - 1) * i64::from(ftime.max(0)) * 256 / t;
    let f0 = ((cur >> 8) as usize).min(a.frames - 1);
    let k2 = (cur & 0xff) as f32 / 256.0;
    Some((&a.data, f0, k2))
}

fn sample(data: &[i16], f0: usize, frames_vc: (usize, usize), k2: f32, v: usize) -> [f32; 3] {
    let (frames, vc) = frames_vc;
    let f1 = (f0 + 1).min(frames - 1);
    let (k1, k2) = ((1.0 - k2) / 8.0, k2 / 8.0);
    let a = (f0 * vc + v) * 3;
    let b = (f1 * vc + v) * 3;
    let m = |k: usize| f32::from(data[a + k]) * k1 + f32::from(data[b + k]) * k2;
    [m(0), m(1), -m(2)]
}

/// Model-space vertices (before the `pi/2 - alpha` turn and translation).
/// `anims` maps phases to the model's animations (`Species::anims`).
pub fn pose(c: &Character, car: &Car, anims: &[usize]) -> Option<Vec<[f32; 3]>> {
    let vc = car.mesh.vertices.len();
    let (phase, prev_phase) = (anim(anims, c.phase), anim(anims, c.prev_phase));
    let (data, f0, k2) = frame(car, phase, c.ftime)?;
    let frames = car.animations[phase].frames;
    let pmorph = c.phase != c.prev_phase && c.pp_morph_time < PMORPH_TIME;
    let prev = if pmorph {
        frame(car, prev_phase, c.prev_pf_time).map(|f| (f, car.animations[prev_phase].frames))
    } else {
        None
    };
    let pmk1 = c.pp_morph_time as f32 / PMORPH_TIME as f32;
    let pmk2 = 1.0 - pmk1;

    let scale = c.scale;
    let (sb, cb) = (c.beta.sin() * scale, c.beta.cos() * scale);
    let (sg, cg) = c.gamma.sin_cos();
    Some(
        (0..vc)
            .map(|v| {
                let [mut x, mut y, mut z] = sample(data, f0, (frames, vc), k2, v);
                if let Some(((pd, pf0, pk2), pframes)) = prev {
                    let [px, py, pz] = sample(pd, pf0, (pframes, vc), pk2, v);
                    x = x * pmk1 + px * pmk2;
                    y = y * pmk1 + py * pmk2;
                    z = z * pmk1 + pz * pmk2;
                }
                // Roll (gamma) around the body axis.
                let mut zz = z;
                let mut xx = cg * x - sg * y;
                let yy = cg * y + sg * x;
                // Bend: the head end (z > 0) and tail end bend in proportion.
                let fi = if z > 0.0 {
                    (z / 240.0).min(1.0)
                } else {
                    (z / 380.0).max(-1.0)
                } * c.bend;
                let (bs, bc) = fi.sin_cos();
                let bx = bc * xx - bs * zz;
                let bz = bc * zz + bs * xx;
                zz = bz;
                xx = bx;
                // Pitch (beta) and scale.
                [xx * scale, cb * yy - sb * zz, cb * zz + sb * yy]
            })
            .collect(),
    )
}

/// Rotation for the renderer, in quarter turns (`-alpha + pi/2`).
pub fn quarter_turns(c: &Character) -> f32 {
    1.0 - c.alpha * 2.0 / core::f32::consts::PI
}
