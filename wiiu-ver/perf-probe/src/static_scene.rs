//! Static terrain data and occupied scenery cells; independent of GX2.
use alloc::vec::Vec;
use c2_core::area::Area;

const SIDE: usize = 1024;

pub struct TerrainCache {
    // Center height and conservative radius, indexed by map cell.
    pub bounds: Vec<[f32; 2]>,
    // Texture -> quarter-turn -> corner (z * 2 + x) -> atlas UV.
    pub uvs: Vec<[[[f32; 2]; 4]; 4]>,
}
impl TerrainCache {
    pub fn new(area: &Area) -> Self {
        let radii: [f32; 256] = core::array::from_fn(|span| {
            let half_height = span as f32 * 32.0;
            libm::sqrtf(32768.0 + half_height * half_height)
        });
        let mut bounds = alloc::vec![[0.0; 2]; SIDE * SIDE];
        for z in 0..SIDE - 1 {
            for x in 0..SIDE - 1 {
                let i = z * SIDE + x;
                let h = [
                    area.map.height[i],
                    area.map.height[i + 1],
                    area.map.height[i + SIDE],
                    area.map.height[i + SIDE + 1],
                ];
                let low = *h.iter().min().unwrap();
                let high = *h.iter().max().unwrap();
                bounds[i] = [
                    (f32::from(low) + f32::from(high)) * 32.0,
                    radii[(high - low) as usize],
                ];
            }
        }
        let uvs = (0..area.rsc.textures.len().max(1))
            .map(|tex| {
                core::array::from_fn(|turn| {
                    core::array::from_fn(|corner| {
                        let (x, z) = (corner % 2, corner / 2);
                        let (u, v) = match turn {
                            1 => (z, 1 - x),
                            2 => (1 - x, 1 - z),
                            3 => (1 - z, x),
                            _ => (x, z),
                        };
                        [
                            ((tex % 16) as f32 * 128.0 + 0.9465 + u as f32 * 126.107) / 2048.0,
                            ((tex / 16) as f32 * 128.0 + 0.9465 + v as f32 * 126.107) / 2048.0,
                        ]
                    })
                })
            })
            .collect();
        Self { bounds, uvs }
    }

    // Retains the original 148x148 window, culling, topology and projection.
    // Each visible cell projects four corners once, then emits the same six
    // vertices. The caller provides space for 148*148*6 float4s in each stream.
    pub unsafe fn emit(
        &self,
        area: &Area,
        eye: [f32; 3],
        yaw: f32,
        pitch: f32,
        cell: (i32, i32),
        positions: *mut f32,
        attrs: *mut f32,
    ) -> u32 {
        let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
        let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
        let mut n = 0;
        for dz in -74i32..74 {
            for dx in -74i32..74 {
                let x = cell.0 + dx;
                let z = cell.1 + dz;
                if !(0..1023).contains(&x) || !(0..1023).contains(&z) {
                    continue;
                }
                let i = z as usize * SIDE + x as usize;
                let [y, radius] = self.bounds[i];
                if !visible_sphere(
                    x as f32 * 256.0 + 128.0 - eye[0],
                    y - eye[1],
                    z as f32 * 256.0 + 128.0 - eye[2],
                    radius,
                    sa,
                    ca,
                    sb,
                    cb,
                ) {
                    continue;
                }
                let flags = area.map.flags[i];
                let tex =
                    (area.map.texture1[i] as usize).min(area.rsc.textures.len().saturating_sub(1));
                let uvs = &self.uvs[tex][(flags & 3) as usize];
                let mut p = [[0.0; 4]; 4];
                let mut a = [[0.0; 4]; 4];
                for corner in 0..4 {
                    let (ox, oz) = ((corner % 2) as i32, (corner / 2) as i32);
                    let vi = (z + oz) as usize * SIDE + (x + ox) as usize;
                    let wx = (x + ox) as f32 * 256.0 - eye[0];
                    let wy = f32::from(area.map.height[vi]) * 64.0 - eye[1];
                    let wz = (z + oz) as f32 * 256.0 - eye[2];
                    let side = wx * ca + wz * sa;
                    let forward = wx * sa - wz * ca;
                    let up = wy * cb + forward * sb;
                    let depth = forward * cb - wy * sb;
                    p[corner] = [
                        side * 0.75,
                        up * 1.333,
                        (32768.0 / (32768.0 - 16.0)) * (depth - 16.0),
                        depth,
                    ];
                    a[corner] = [
                        uvs[corner][0],
                        uvs[corner][1],
                        f32::from(area.map.light[0][vi]) / 255.0,
                        1.0,
                    ];
                }
                let corners = if flags & 0x10 != 0 {
                    [0, 1, 2, 2, 1, 3]
                } else {
                    [0, 1, 3, 0, 3, 2]
                };
                for corner in corners {
                    core::ptr::copy_nonoverlapping(p[corner].as_ptr(), positions.add(n * 4), 4);
                    core::ptr::copy_nonoverlapping(a[corner].as_ptr(), attrs.add(n * 4), 4);
                    n += 1;
                }
            }
        }
        n as u32
    }
}

pub fn visible_sphere(
    wx: f32,
    wy: f32,
    wz: f32,
    radius: f32,
    sa: f32,
    ca: f32,
    sb: f32,
    cb: f32,
) -> bool {
    let side = wx * ca + wz * sa;
    let depth = (wx * sa - wz * ca) * cb - wy * sb;
    depth + radius >= 0.0 && (side * 0.75).abs() <= depth + radius * 1.25
}

#[derive(Default)]
pub struct SceneryCandidates {
    anchor: Option<(i32, i32)>,
    pub cells: Vec<usize>,
}
impl SceneryCandidates {
    // Preserve the previous near-first ring traversal exactly. Camera motion
    // only reruns culling over occupied cells; rebuild the list on cell changes.
    pub fn update(&mut self, objects: &[u8], model_count: usize, cell: (i32, i32)) {
        if self.anchor == Some(cell) {
            return;
        }
        self.cells.clear();
        for ring in 0i32..=74 {
            for dz in -ring..=ring {
                let step = if dz.abs() == ring {
                    1
                } else {
                    (ring * 2).max(1)
                };
                for dx in (-ring..=ring).step_by(step as usize) {
                    let (x, z) = (cell.0 + dx, cell.1 + dz);
                    if !(0..1024).contains(&x) || !(0..1024).contains(&z) {
                        continue;
                    }
                    let i = z as usize * SIDE + x as usize;
                    if (objects[i] as usize) < model_count {
                        self.cells.push(i);
                    }
                }
            }
        }
        self.anchor = Some(cell);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawRange {
    pub first: u32,
    pub count: u32,
}
#[derive(Clone, Copy)]
pub struct ResidentObject {
    pub center: [f32; 3],
    pub radius: f32,
    pub range: DrawRange,
}

// GPU ranges retain candidate order. Only merge adjacent visible spans: never
// submit invisible objects or reorder cutout faces to reduce draw calls.
pub fn resident_ranges(
    objects: &[ResidentObject],
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    out: &mut [DrawRange],
) -> (usize, u32) {
    assert!(out.len() >= objects.len());
    let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
    let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
    let mut n = 0;
    let mut vertices = 0;
    for object in objects {
        if object.range.count == 0
            || !visible_sphere(
                object.center[0] - eye[0],
                object.center[1] - eye[1],
                object.center[2] - eye[2],
                object.radius,
                sa,
                ca,
                sb,
                cb,
            )
        {
            continue;
        }
        vertices += object.range.count;
        if n > 0 && out[n - 1].first + out[n - 1].count == object.range.first {
            out[n - 1].count += object.range.count;
        } else {
            out[n] = object.range;
            n += 1;
        }
    }
    (n, vertices)
}

// Directly populate a resident stream without allocating/copying temporary
// CachedGeometry vectors per object. Same transform, face order and attributes
// as the legacy per-object CPU cache builder.
pub unsafe fn emit_scenery_model(
    model: &c2_core::model::PreparedModel,
    uvs: &[[[f32; 2]; 3]],
    light: f32,
    center: [f32; 3],
    turns: u8,
    scratch: &mut [[f32; 4]],
    positions: *mut f32,
    attrs: *mut f32,
) -> u32 {
    for (q, pos) in model.positions.iter().zip(scratch.iter_mut()) {
        let (x, z) = match turns {
            1 => (q[2], -q[0]),
            2 => (-q[0], -q[2]),
            3 => (-q[2], q[0]),
            _ => (q[0], q[2]),
        };
        *pos = [center[0] + x, center[1] + q[1], center[2] + z, 1.0];
    }
    let mut n = 0;
    for (face, uv) in model.faces.iter().zip(uvs) {
        for k in 0..3 {
            core::ptr::copy_nonoverlapping(
                scratch[face.v[k] as usize].as_ptr(),
                positions.add(n * 4),
                4,
            );
            let attr = [uv[k][0], uv[k][1], light, 1.0];
            core::ptr::copy_nonoverlapping(attr.as_ptr(), attrs.add(n * 4), 4);
            n += 1;
        }
    }
    n as u32
}

pub fn scenery_uvs(models: &[c2_core::model::PreparedModel]) -> Vec<Vec<[[f32; 2]; 3]>> {
    models
        .iter()
        .enumerate()
        .map(|(id, model)| {
            model
                .faces
                .iter()
                .map(|face| {
                    core::array::from_fn(|k| {
                        [
                            ((id % 8) as f32 * 256.0
                                + (face.uv[k][0] * 256.0).clamp(0.0, 255.0)
                                + 0.5)
                                / 2048.0,
                            ((id / 8) as f32 * 256.0
                                + (face.uv[k][1] * 256.0).clamp(0.0, 255.0)
                                + 0.5)
                                / 2048.0,
                        ]
                    })
                })
                .collect()
        })
        .collect()
}

// Reject oversized/overflowing windows before writing any resident GPU data.
pub fn resident_vertex_count(
    face_counts: impl IntoIterator<Item = usize>,
    capacity: usize,
) -> Option<usize> {
    let mut vertices = 0usize;
    for faces in face_counts {
        vertices = vertices.checked_add(faces.checked_mul(3)?)?;
        if vertices > capacity {
            return None;
        }
    }
    Some(vertices)
}
