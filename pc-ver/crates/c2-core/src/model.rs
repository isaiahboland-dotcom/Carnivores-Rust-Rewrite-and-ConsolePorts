//! Model preparation and vertex-animation posing.
//!
//! Applies the load-time transforms from `Resources.cpp: LoadModel` /
//! `LoadCharacterInfo` (`x*2, y*2, z*-2`, UV `/256`) and the morphing from
//! `Characters.cpp: CreateMorphedModel` / `CreateMorphedObject`.

use c2_formats::car::Animation;
use c2_formats::model::{face_flags, Mesh, TEXTURE_WIDTH};
use c2_formats::rsc::ObjectAnimation;

use crate::color::image_to_rgba8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreparedFace {
    pub v: [u32; 3],
    /// Normalised UVs into the 256x256 texture.
    pub uv: [[f32; 2]; 3],
    pub flags: u16,
}

impl PreparedFace {
    /// Blended (semi-transparent) rather than opaque or alpha-tested.
    pub fn is_transparent(&self) -> bool {
        self.flags & face_flags::TRANSPARENT != 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedModel {
    /// Rest pose in world units.
    pub positions: Vec<[f32; 3]>,
    /// Faces that reference out-of-range vertices are dropped.
    pub faces: Vec<PreparedFace>,
    /// Always 256x256 RGBA8; short textures are padded with transparent rows,
    /// as the hardware renderers allocate 256 rows (`TextureHeight = 256`).
    pub texture_rgba: Vec<u8>,
}

pub const MODEL_TEXTURE_SIZE: usize = 256;

/// Past this distance (cells) an object is drawn as its sprite
/// (`ctViewRM`, `_RenderObject`), unless `ofNOBMP`.
pub const SPRITE_CELLS: f32 = 24.0;

/// An RSC object's distance billboard (`Resources.cpp: LoadBMPModel`) as a
/// mesh on the model's scale: a quad over the model's x / y extent, its
/// 128x128 `sprite` in the top-left of the texture, texel 0 see-through.
/// It is drawn upright, turned to face the camera (`RenderBMPModel`).
pub fn sprite_mesh(model: &Mesh, sprite: &[u16]) -> Mesh {
    use c2_formats::model::{Face, Vertex};
    let v0 = model.vertices.first().map_or((0.0, 0.0), |v| (v.x, v.y));
    // As the original: the top starts from the first vertex's x.
    let (mut mnx, mut mxx, mut mny, mut mxy) = (v0.0 - 0.5, v0.0 + 0.5, v0.1 - 0.5, v0.0 + 0.5);
    for v in &model.vertices {
        mnx = mnx.min(v.x);
        mxx = mxx.max(v.x);
        mny = mny.min(v.y);
        mxy = mxy.max(v.y);
    }
    let corner = |x, y| Vertex {
        x,
        y,
        z: 0.0,
        owner: 0,
        hide: 0,
    };
    let s = c2_formats::rsc::SPRITE_SIZE;
    let mut texture = vec![0u16; TEXTURE_WIDTH * MODEL_TEXTURE_SIZE];
    for (y, row) in sprite.chunks(s).take(s).enumerate() {
        texture[y * TEXTURE_WIDTH..y * TEXTURE_WIDTH + row.len()].copy_from_slice(row);
    }
    let t = s as i32 - 1;
    let face = |v: [i32; 3], tu: [i32; 3], tv: [i32; 3]| Face {
        v,
        tu,
        tv,
        flags: face_flags::OPACITY | face_flags::DOUBLE_SIDE,
        dmask: 0,
        distant: 0,
        next: 0,
        group: 0,
        reserved: [0; 12],
    };
    Mesh {
        vertices: vec![
            corner(mnx, mxy),
            corner(mxx, mxy),
            corner(mxx, mny),
            corner(mnx, mny),
        ],
        faces: vec![
            face([0, 1, 2], [0, t, t], [0, 0, t]),
            face([0, 2, 3], [0, t, 0], [0, t, t]),
        ],
        texture,
    }
}

impl PreparedModel {
    pub fn from_mesh(mesh: &Mesh) -> Self {
        let positions = mesh
            .vertices
            .iter()
            .map(|v| [v.x * 2.0, v.y * 2.0, v.z * -2.0])
            .collect();
        let n = mesh.vertices.len() as i64;
        let faces = mesh
            .faces
            .iter()
            .filter(|f| f.v.iter().all(|&i| (0..n).contains(&i64::from(i))))
            .map(|f| PreparedFace {
                v: f.v.map(|i| i as u32),
                uv: [0, 1, 2].map(|k| [f.tu[k] as f32 / 256.0, f.tv[k] as f32 / 256.0]),
                flags: f.flags,
            })
            .collect();

        // GenerateAlphaFlags: if any face has sfOpacity, texel 0 is see-through.
        let opacity = mesh
            .faces
            .iter()
            .any(|f| f.flags & face_flags::OPACITY != 0);
        let mut texels = mesh.texture.clone();
        texels.resize(TEXTURE_WIDTH * MODEL_TEXTURE_SIZE, 0);
        let mut texture_rgba =
            image_to_rgba8(&texels[..TEXTURE_WIDTH * MODEL_TEXTURE_SIZE], opacity);
        // Padding rows are never meant to be visible.
        for px in texture_rgba[mesh.texture.len().min(TEXTURE_WIDTH * MODEL_TEXTURE_SIZE) * 4..]
            .as_chunks_mut::<4>()
            .0
        {
            px[3] = 0;
        }
        Self {
            positions,
            faces,
            texture_rgba,
        }
    }

    /// Radius of the bounding sphere around the origin.
    pub fn radius(&self) -> f32 {
        self.positions
            .iter()
            .map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
            .fold(0.0, f32::max)
    }
}

/// `mathematics.cpp: CalcLights`: per-vertex sun shading for each of the
/// four object rotations, in the original's light units (added to the
/// object's base light, 0..255). Computed from file-space vertices, as the
/// original calls it before the load-time scaling.
pub fn vertex_lights(mesh: &Mesh, sun_pos: [f32; 3]) -> Vec<[f32; 4]> {
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 {
            [v[0] / l, v[1] / l, v[2] / l]
        } else {
            v
        }
    };
    let slight = norm([-sun_pos[0], -sun_pos[1] / 2.0, -sun_pos[2]]);
    let n = mesh.vertices.len();
    let pos = |i: i32| {
        mesh.vertices
            .get(usize::try_from(i).ok()?)
            .map(|v| [v.x, v.y, v.z])
    };
    let mut sums = vec![[0.0f32; 3]; n];
    let mut used = vec![false; n];
    for f in &mesh.faces {
        let (Some(a), Some(b), Some(c)) = (pos(f.v[0]), pos(f.v[1]), pos(f.v[2])) else {
            continue;
        };
        if f.flags & face_flags::DOUBLE_SIDE != 0 {
            continue;
        }
        let (u, w) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        let nv = norm([
            u[1] * w[2] - u[2] * w[1],
            u[2] * w[0] - u[0] * w[2],
            u[0] * w[1] - u[1] * w[0],
        ]);
        for &i in &f.v {
            let i = i as usize;
            for k in 0..3 {
                sums[i][k] += nv[k];
            }
            used[i] = true;
        }
    }
    (0..n)
        .map(|v| {
            let mut out = [0.0; 4];
            if used[v] {
                let nv = norm(sums[v]);
                for (vt, o) in out.iter_mut().enumerate() {
                    let a = vt as f32 * std::f32::consts::FRAC_PI_2;
                    let (ca, sa) = (a.cos(), a.sin());
                    let rv = [nv[0] * sa + nv[2] * ca, nv[1], nv[2] * sa - nv[0] * ca];
                    *o = (rv[0] * slight[0] + rv[1] * slight[1] + rv[2] * slight[2]) * 64.0;
                }
            }
            out
        })
        .collect()
}

/// Duration in ms of a `.CAR` animation (`FramesCount * 1000 / aniKPS`).
pub fn car_anim_duration_ms(a: &Animation) -> u32 {
    if a.kps <= 0 {
        return 0;
    }
    (a.frames as u64 * 1000 / a.kps as u64) as u32
}

/// Interpolated vertex positions, `CreateMorphedModel` with `scale = 1`.
/// Returns `None` if the clip has no frames or doesn't match the vertex count.
pub fn pose_car(a: &Animation, vcount: usize, time_ms: u32) -> Option<Vec<[f32; 3]>> {
    pose(&a.data, a.frames, vcount, car_anim_duration_ms(a), time_ms)
}

/// `CreateMorphedObject`: RSC object animations store `frames + 1` keyframes.
pub fn pose_object(a: &ObjectAnimation, time_ms: u32) -> Option<Vec<[f32; 3]>> {
    let frames = a.frames_stored + 1;
    let duration = if a.kps <= 0 {
        0
    } else {
        (frames as u64 * 1000 / a.kps as u64) as u32
    };
    pose(&a.data, frames, a.vcount, duration, time_ms)
}

fn pose(
    data: &[i16],
    frames: usize,
    vcount: usize,
    duration: u32,
    time_ms: u32,
) -> Option<Vec<[f32; 3]>> {
    if frames == 0 || vcount == 0 || data.len() < frames * vcount * 3 {
        return None;
    }
    let t = if duration == 0 {
        0
    } else {
        u64::from(time_ms % duration)
    };
    // CurFrame = ((FramesCount-1) * FTime * 256) / AniTime, 8.8 fixed point.
    let cur = if duration == 0 {
        0
    } else {
        (frames as u64 - 1) * t * 256 / u64::from(duration)
    };
    let f0 = (cur >> 8) as usize;
    let f1 = (f0 + 1).min(frames - 1);
    let k2 = (cur & 0xff) as f32 / 256.0;
    let (k1, k2) = ((1.0 - k2) / 8.0, k2 / 8.0);
    let a = &data[f0 * vcount * 3..];
    let b = &data[f1 * vcount * 3..];
    Some(
        (0..vcount)
            .map(|v| {
                let i = v * 3;
                let m = |k: usize| f32::from(a[i + k]) * k1 + f32::from(b[i + k]) * k2;
                [m(0), m(1), -m(2)]
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_formats::model::{Face, Vertex};

    #[test]
    fn transforms_and_pads() {
        let mesh = Mesh {
            vertices: vec![
                Vertex {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    owner: 0,
                    hide: 0
                };
                3
            ],
            faces: vec![
                Face {
                    v: [0, 1, 2],
                    tu: [0, 128, 256],
                    tv: [0, 0, 64],
                    flags: face_flags::OPACITY,
                    dmask: 0,
                    distant: 0,
                    next: 0,
                    group: 0,
                    reserved: [0; 12],
                },
                Face {
                    v: [0, 1, 9],
                    tu: [0; 3],
                    tv: [0; 3],
                    flags: 0,
                    dmask: 0,
                    distant: 0,
                    next: 0,
                    group: 0,
                    reserved: [0; 12],
                },
            ],
            texture: vec![0x7fff; 256],
        };
        let m = PreparedModel::from_mesh(&mesh);
        assert_eq!(m.positions[0], [2.0, 4.0, -6.0]);
        assert_eq!(m.faces.len(), 1, "face with bad index dropped");
        assert_eq!(m.faces[0].uv[1], [0.5, 0.0]);
        assert_eq!(m.texture_rgba.len(), 256 * 256 * 4);
        assert_eq!(&m.texture_rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(m.texture_rgba[256 * 4 + 3], 0, "padding is transparent");
    }

    #[test]
    fn pose_interpolates_between_frames() {
        // 2 frames, 1 vertex: (0,0,0) then (8,16,24).
        let a = Animation {
            name_raw: [0; 32],
            kps: 2,
            frames: 2,
            data: vec![0, 0, 0, 8, 16, 24],
        };
        assert_eq!(car_anim_duration_ms(&a), 1000);
        assert_eq!(pose_car(&a, 1, 0), Some(vec![[0.0, 0.0, -0.0]]));
        // Halfway through the clip = halfway between frame 0 and 1.
        assert_eq!(pose_car(&a, 1, 500), Some(vec![[0.5, 1.0, -1.5]]));
        assert_eq!(pose_car(&a, 2, 0), None, "vertex count mismatch");
        let zero = Animation { kps: 0, ..a };
        assert!(
            pose_car(&zero, 1, 123).is_some(),
            "kps 0 must not divide by zero"
        );
    }
}
