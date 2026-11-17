//! An area's relief for its survey's 3D view ("DHC OS 3D specimens"): the
//! map's heights on a coarse grid as one model, textured with the map
//! screen's overview picture (`c2_formats::overview`) shaded by the map's
//! day light map, water cells raised to their surface, on a dark plinth
//! so it turns as a solid block. Objects are left out.

use c2_core::model::{PreparedFace, PreparedModel, MODEL_TEXTURE_SIZE};
use c2_core::{CELL, H_SCALE};
use c2_formats::map::{Map, MAP_SIZE};
use c2_formats::overview;
use c2_formats::rsc::Rsc;

/// Grid squares per side: one per 8x8 cells.
pub(super) const GRID: usize = 128;
const STEP: usize = MAP_SIZE / GRID;
/// World units per model unit, and how much taller the relief is drawn.
const SHRINK: f32 = 64.0;
const RELIEF: f32 = 2.5;
/// `FMap`'s water bit and `WMap`'s "no water" (as `c2_core::area`).
const WATER: u16 = 0x0080;
const NO_WATER: u8 = 255;
/// How far the plinth reaches below the lowest ground, in model units.
const PLINTH: f32 = 60.0;
/// The plinth's colour, in the texture's last texel (the overview's far
/// corner, which is under the plinth's edge anyway).
const PLINTH_RGBA: [u8; 4] = [16, 30, 20, 255];

/// The surface height at cell (`x`, `z`), in world units: the ground, or
/// the water over it.
fn surface(map: &Map, rsc: &Rsc, x: usize, z: usize) -> f32 {
    let i = z.min(MAP_SIZE - 1) * MAP_SIZE + x.min(MAP_SIZE - 1);
    let ground = f32::from(map.height.get(i).copied().unwrap_or(0)) * H_SCALE;
    let wet = map.flags.get(i).is_some_and(|f| f & WATER != 0);
    let level = map
        .water
        .get(i)
        .filter(|&&w| wet && w != NO_WATER)
        .and_then(|&w| rsc.waters.get(usize::from(w)))
        .map(|w| w.level as f32 * H_SCALE);
    level.map_or(ground, |l| ground.max(l))
}

/// The overview picture shaded as the terrain is by day: each pixel by its
/// cell's light (water at the shader's fixed 168).
fn texture(map: &Map, rsc: &Rsc) -> Vec<u8> {
    let mut rgba = overview::image_rgba8(map, rsc);
    let per = overview::CELLS_PER_PIXEL;
    for (p, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (p % overview::SIZE, p / overview::SIZE);
        let i = y * per * MAP_SIZE + x * per;
        let wet = map.flags.get(i).is_some_and(|f| f & WATER != 0);
        let light = if wet {
            168.0
        } else {
            f32::from(map.light[0].get(i).copied().unwrap_or(128))
        };
        let k = (light / 255.0 * 1.5).min(1.0);
        for c in &mut px[..3] {
            *c = (f32::from(*c) * k) as u8;
        }
    }
    let n = rgba.len();
    rgba[n - 4..].copy_from_slice(&PLINTH_RGBA);
    rgba
}

/// Grid squares of open water kept round the land.
const SHORE: usize = 4;

/// Whether cell (`x`, `z`) is under water.
fn wet(map: &Map, x: usize, z: usize) -> bool {
    let i = z.min(MAP_SIZE - 1) * MAP_SIZE + x.min(MAP_SIZE - 1);
    map.flags.get(i).is_some_and(|f| f & WATER != 0)
}

/// The square of the grid (first vertex, squares per side) that holds the
/// land and a shore round it: an island's sea is cut away so the island
/// fills the view. A map without water keeps all of it. Each grid square
/// is judged by its middle cell: the original maps leave their last row
/// and column of cells dry.
fn crop(map: &Map) -> (usize, usize, usize) {
    let (mut lo, mut hi) = ([GRID, GRID], [0, 0]);
    for gz in 0..GRID {
        for gx in 0..GRID {
            if !wet(map, gx * STEP + STEP / 2, gz * STEP + STEP / 2) {
                lo = [lo[0].min(gx), lo[1].min(gz)];
                hi = [hi[0].max(gx + 1), hi[1].max(gz + 1)];
            }
        }
    }
    if lo[0] > hi[0] {
        return (0, 0, GRID);
    }
    let n = ((hi[0] - lo[0]).max(hi[1] - lo[1]) + 2 * SHORE).min(GRID);
    let start = |a: usize, b: usize| ((a + b) / 2).saturating_sub(n / 2).min(GRID - n);
    (start(lo[0], hi[0]), start(lo[1], hi[1]), n.max(1))
}

/// The relief model of `map` (with `rsc`'s textures and water levels).
pub(in crate::menu) fn model(map: &Map, rsc: &Rsc) -> PreparedModel {
    let (x0, z0, n) = crop(map);
    let side = n + 1;
    let half = (n * STEP) as f32 * CELL / SHRINK / 2.0;
    let mut positions = Vec::with_capacity(side * side + 8 * side);
    let mut lowest = f32::MAX;
    for gz in 0..side {
        for gx in 0..side {
            let (cx, cz) = ((x0 + gx) * STEP, (z0 + gz) * STEP);
            let y = surface(map, rsc, cx, cz) / SHRINK * RELIEF;
            lowest = lowest.min(y);
            let x = (gx * STEP) as f32 * CELL / SHRINK - half;
            let z = (gz * STEP) as f32 * CELL / SHRINK - half;
            positions.push([x, y, z]);
        }
    }
    let uv = |gx: usize, gz: usize| {
        [
            (x0 + gx) as f32 / GRID as f32,
            (z0 + gz) as f32 / GRID as f32,
        ]
    };
    let at = |gx: usize, gz: usize| (gz * side + gx) as u32;
    let mut faces = Vec::with_capacity(2 * n * n + 8 * n);
    let face = |v: [u32; 3], uv: [[f32; 2]; 3]| PreparedFace { v, uv, flags: 0 };
    for gz in 0..n {
        for gx in 0..n {
            let (a, b, c, d) = (
                at(gx, gz),
                at(gx + 1, gz),
                at(gx, gz + 1),
                at(gx + 1, gz + 1),
            );
            let (ua, ub, uc, ud) = (
                uv(gx, gz),
                uv(gx + 1, gz),
                uv(gx, gz + 1),
                uv(gx + 1, gz + 1),
            );
            faces.push(face([a, c, b], [ua, uc, ub]));
            faces.push(face([b, c, d], [ub, uc, ud]));
        }
    }
    // The plinth: each edge's surface points down to a floor.
    let floor = lowest - PLINTH;
    let dark = {
        let t = (MODEL_TEXTURE_SIZE as f32 - 0.5) / MODEL_TEXTURE_SIZE as f32;
        [t, t]
    };
    let edges: [Vec<(usize, usize)>; 4] = [
        (0..side).map(|g| (g, 0)).collect(),
        (0..side).map(|g| (n, g)).collect(),
        (0..side).rev().map(|g| (g, n)).collect(),
        (0..side).rev().map(|g| (0, g)).collect(),
    ];
    for edge in edges {
        for w in edge.windows(2) {
            let (top0, top1) = (at(w[0].0, w[0].1), at(w[1].0, w[1].1));
            let base = positions.len() as u32;
            for &(gx, gz) in w {
                let p = positions[at(gx, gz) as usize];
                positions.push([p[0], floor, p[2]]);
            }
            faces.push(face([top0, top1, base], [dark; 3]));
            faces.push(face([top1, base + 1, base], [dark; 3]));
        }
    }
    PreparedModel {
        positions,
        faces,
        texture_rgba: texture(map, rsc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat synthetic map with a raised square and a lake.
    fn map() -> (Map, Rsc) {
        let cells = MAP_SIZE * MAP_SIZE;
        let mut height = vec![10u8; cells];
        let mut flags = vec![0u16; cells];
        let mut water = vec![NO_WATER; cells];
        for z in 0..64 {
            for x in 0..64 {
                height[(512 + z) * MAP_SIZE + 512 + x] = 200;
                let lake = (100 + z) * MAP_SIZE + 100 + x;
                height[lake] = 2;
                flags[lake] = WATER;
                water[lake] = 0;
            }
        }
        let map = Map {
            height,
            texture1: vec![0; cells],
            texture2: Vec::new(),
            objects: Vec::new(),
            flags,
            light: [vec![255; cells], Vec::new(), Vec::new()],
            water,
            object_height: Vec::new(),
            fogs: Vec::new(),
            ambient: Vec::new(),
        };
        let rsc = Rsc {
            fade_rgb: [[0; 3]; 3],
            trans_rgb: [[0; 3]; 3],
            textures: Vec::new(),
            objects: Vec::new(),
            skies: Default::default(),
            sky_map: Vec::new(),
            fogs: Vec::new(),
            random_sounds: Vec::new(),
            ambients: Vec::new(),
            waters: vec![c2_formats::rsc::Water {
                texture_index: 0,
                level: 6,
                transp: 0.5,
                fog_rgb: 0,
            }],
        };
        (map, rsc)
    }

    #[test]
    fn the_relief_follows_the_ground_and_the_water() {
        let (map, rsc) = map();
        let m = model(&map, &rsc);
        let side = GRID + 1;
        let y = |gx: usize, gz: usize| m.positions[gz * side + gx][1];
        let k = H_SCALE / SHRINK * RELIEF;
        assert_eq!(y(0, 0), 10.0 * k, "plain ground");
        assert_eq!(y(520 / STEP, 520 / STEP), 200.0 * k, "the raised square");
        assert_eq!(y(104 / STEP, 104 / STEP), 6.0 * k, "the lake's surface");
        // Centred, and every face's vertices exist.
        let (a, b) = (m.positions[0], m.positions[side * side - 1]);
        assert_eq!(a[0], -b[0]);
        assert!(m
            .faces
            .iter()
            .all(|f| f.v.iter().all(|&v| (v as usize) < m.positions.len())));
        assert_eq!(
            m.faces.len(),
            2 * GRID * GRID + 8 * GRID,
            "top and four walls"
        );
        // The plinth reaches below the lowest ground, in its own colour.
        let floor = m.positions.last().map_or(0.0, |p| p[1]);
        assert!(floor < 2.0 * k);
        assert_eq!(
            m.texture_rgba.len(),
            MODEL_TEXTURE_SIZE * MODEL_TEXTURE_SIZE * 4
        );
        let n = m.texture_rgba.len();
        assert_eq!(m.texture_rgba[n - 4..], PLINTH_RGBA);
    }

    #[test]
    fn an_island_is_cut_from_its_sea_with_a_shore() {
        let (mut map, rsc) = map();
        // Sea everywhere but cells 256..512 on both axes.
        for z in 0..MAP_SIZE {
            for x in 0..MAP_SIZE {
                let land = (256..512).contains(&x) && (256..512).contains(&z);
                map.flags[z * MAP_SIZE + x] = if land { 0 } else { WATER };
            }
        }
        // The original maps' dry last row and column don't count.
        for k in 0..MAP_SIZE {
            map.flags[k * MAP_SIZE + MAP_SIZE - 1] = 0;
            map.flags[(MAP_SIZE - 1) * MAP_SIZE + k] = 0;
        }
        let (x0, z0, n) = crop(&map);
        // Land squares 32..64 (cells 256..512), plus the shore each side.
        assert_eq!((x0, z0, n), (32 - SHORE, 32 - SHORE, 32 + 2 * SHORE));
        let m = model(&map, &rsc);
        assert_eq!(m.faces.len(), 2 * n * n + 8 * n);
        let uv0 = m.faces[0].uv[0];
        assert_eq!(uv0, [x0 as f32 / GRID as f32, z0 as f32 / GRID as f32]);
        // All sea: the whole map; all land: the whole map.
        map.flags.iter_mut().for_each(|f| *f = WATER);
        assert_eq!(crop(&map), (0, 0, GRID));
        map.flags.iter_mut().for_each(|f| *f = 0);
        assert_eq!(crop(&map), (0, 0, GRID));
    }
}
