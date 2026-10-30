#![allow(unsafe_code)]
extern crate alloc;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/static_scene.rs"]
mod static_scene;
use c2_core::area::{Area, TimeOfDay};
use static_scene::{
    visible_sphere, water_cell, DrawRange, ResidentTerrain, TerrainCache, BLOCK_COUNT,
};

const VERTICES: usize = 148 * 148 * 6;

struct Buffers {
    tp: Vec<f32>,
    ta: Vec<f32>,
    wp: Vec<f32>,
    wa: Vec<f32>,
}
impl Buffers {
    fn new() -> Self {
        let b = || vec![f32::NAN; VERTICES * 4];
        Self {
            tp: b(),
            ta: b(),
            wp: b(),
            wa: b(),
        }
    }
    fn update(
        &mut self,
        r: &mut ResidentTerrain,
        area: &Area,
        cache: &TerrainCache,
        cell: (i32, i32),
    ) -> (u32, Vec<DrawRange>) {
        let mut written = Vec::new();
        let cells = unsafe {
            r.update(
                area,
                cache,
                cell,
                (self.tp.as_mut_ptr(), self.ta.as_mut_ptr()),
                (self.wp.as_mut_ptr(), self.wa.as_mut_ptr()),
                &mut written,
            )
        };
        (cells, written)
    }
}

// The shader's world mode, done on the CPU: the same projection as
// TerrainCache::emit and c2_water_vertices.
fn project(p: &[f32], eye: [f32; 3], yaw: f32, pitch: f32) -> [u32; 4] {
    let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
    let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
    let (wx, wy, wz) = (p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]);
    let side = wx * ca + wz * sa;
    let forward = wx * sa - wz * ca;
    let up = wy * cb + forward * sb;
    let depth = forward * cb - wy * sb;
    [
        side * 0.75,
        up * 1.333,
        (32768.0 / (32768.0 - 16.0)) * (depth - 16.0),
        depth,
    ]
    .map(f32::to_bits)
}

type Vertex = ([u32; 4], [u32; 4]);

fn legacy_water(area: &Area, eye: [f32; 3], yaw: f32, pitch: f32, cell: (i32, i32)) -> Vec<Vertex> {
    let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
    let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
    let mut out = Vec::new();
    for dz in -74i32..74 {
        for dx in -74i32..74 {
            let (x, z) = (cell.0 + dx, cell.1 + dz);
            if x < 0 || z < 0 || x >= 1023 || z >= 1023 {
                continue;
            }
            let Some((level, tex)) = water_cell(area, x, z) else {
                continue;
            };
            if !visible_sphere(
                x as f32 * 256.0 + 128.0 - eye[0],
                level - eye[1],
                z as f32 * 256.0 + 128.0 - eye[2],
                182.0,
                sa,
                ca,
                sb,
                cb,
            ) {
                continue;
            }
            for (ox, oz) in [(0, 0), (1, 0), (1, 1), (0, 0), (1, 1), (0, 1)] {
                let p = [(x + ox) as f32 * 256.0, level, (z + oz) as f32 * 256.0];
                let u = ((tex % 16) as f32 * 128.0 + 0.9465 + ox as f32 * 126.107) / 2048.0;
                let v = ((tex / 16) as f32 * 128.0 + 0.9465 + oz as f32 * 126.107) / 2048.0;
                out.push((
                    project(&p, eye, yaw, pitch),
                    [u, v, 1.0, 0.0].map(f32::to_bits),
                ));
            }
        }
    }
    out
}

// Non-degenerate triangles of the drawn ranges, projected, as sorted vertex triples.
fn drawn(
    pos: &[f32],
    attr: &[f32],
    ranges: &[DrawRange],
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
) -> Vec<[Vertex; 3]> {
    let mut out = Vec::new();
    for r in ranges {
        for t in (r.first..r.first + r.count).step_by(3) {
            let v = |k: u32| {
                let i = (t + k) as usize * 4;
                (&pos[i..i + 4], &attr[i..i + 4])
            };
            if (0..3).all(|k| v(k).0.iter().all(|&f| f == 0.0)) {
                continue;
            }
            out.push(core::array::from_fn(|k| {
                let (p, a) = v(k as u32);
                (
                    project(p, eye, yaw, pitch),
                    [a[0], a[1], a[2], a[3]].map(f32::to_bits),
                )
            }));
        }
    }
    out.sort();
    out
}

fn triangles(vertices: &[Vertex]) -> Vec<[Vertex; 3]> {
    let mut out: Vec<[Vertex; 3]> = vertices.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    out.sort();
    out
}

fn is_subset(small: &[[Vertex; 3]], big: &[[Vertex; 3]]) -> bool {
    let mut j = 0;
    small.iter().all(|t| {
        while j < big.len() && big[j] < *t {
            j += 1;
        }
        j < big.len() && big[j] == *t && {
            j += 1;
            true
        }
    })
}

#[test]
fn resident_terrain_draws_every_visible_cell() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let (mut legacy_total, mut drawn_total, mut water_total) = (0usize, 0usize, 0usize);
    for map in 1..=5 {
        let read = |ext| std::fs::read(root.join(format!("AREAS/AREA{map}.{ext}"))).unwrap();
        let area = Area::new(
            c2_formats::map::parse(&read("MAP")).unwrap(),
            c2_formats::rsc::parse(&read("RSC")).unwrap(),
            TimeOfDay::Day,
        );
        let cache = TerrainCache::new(&area);
        let landing = area.landings.first().copied().unwrap_or((256, 256));
        let start = (landing.0 as i32, landing.1 as i32);
        // Walk across cells (including edges and a jump) and compare the
        // incrementally maintained buffers with a fresh full write.
        let mut walked = ResidentTerrain::default();
        let mut buffers = Buffers::new();
        let mut path = vec![start];
        for step in 0..40 {
            let last = *path.last().unwrap();
            path.push(match step % 4 {
                0 => (last.0 + 1, last.1),
                1 => (last.0 + 1, last.1 + 1),
                2 => (last.0, last.1 - 1),
                _ => (last.0 - 3, last.1 + 2),
            });
        }
        path.extend([
            (0, 0),
            (1, 0),
            (1, 1),
            (1022, 1022),
            (1021, 1022),
            (512, 512),
            (512 + 147, 512),
            (512 + 300, 512),
        ]);
        for (step, &cell) in path.iter().enumerate() {
            let (cells, written) = buffers.update(&mut walked, &area, &cache, cell);
            assert!(cells > 0 && !written.is_empty());
            assert_eq!(buffers.update(&mut walked, &area, &cache, cell).0, 0);
            let mut fresh = ResidentTerrain::default();
            let mut full = Buffers::new();
            assert_eq!(full.update(&mut fresh, &area, &cache, cell).0, 148 * 148);
            assert!(
                buffers.tp == full.tp
                    && buffers.ta == full.ta
                    && buffers.wp == full.wp
                    && buffers.wa == full.wa,
                "map {map} step {step} cell {cell:?}: incremental buffers differ from a full write"
            );
            for view in 0..4 {
                let yaw = view as f32 * 1.7 + step as f32 * 0.31;
                let pitch = (view as f32 - 1.5) * 0.2;
                let eye = [
                    cell.0 as f32 * 256.0 + 100.0,
                    f32::from(
                        area.map.height[(cell.1.clamp(0, 1022) as usize) * 1024
                            + cell.0.clamp(0, 1022) as usize],
                    ) * 64.0
                        + 220.0,
                    cell.1 as f32 * 256.0 + 60.0,
                ];
                let mut legacy = vec![0.0f32; VERTICES * 4];
                let mut legacy_attr = vec![0.0f32; VERTICES * 4];
                let n = unsafe {
                    cache.emit(
                        &area,
                        eye,
                        yaw,
                        pitch,
                        cell,
                        legacy.as_mut_ptr(),
                        legacy_attr.as_mut_ptr(),
                    )
                } as usize;
                let legacy: Vec<Vertex> = (0..n)
                    .map(|i| {
                        let p: [f32; 4] = legacy[i * 4..i * 4 + 4].try_into().unwrap();
                        let a: [f32; 4] = legacy_attr[i * 4..i * 4 + 4].try_into().unwrap();
                        (p.map(f32::to_bits), a.map(f32::to_bits))
                    })
                    .collect();
                let mut tr = vec![DrawRange::default(); BLOCK_COUNT];
                let mut wr = vec![DrawRange::default(); BLOCK_COUNT];
                let (tn, _, wn, _) = walked.ranges(eye, yaw, pitch, &mut tr, &mut wr);
                let terrain = drawn(&buffers.tp, &buffers.ta, &tr[..tn], eye, yaw, pitch);
                legacy_total += legacy.len() / 3;
                drawn_total += terrain.len();
                assert!(
                    is_subset(&triangles(&legacy), &terrain),
                    "map {map} cell {cell:?} view {view}: a visible terrain cell is not drawn"
                );
                let water = drawn(&buffers.wp, &buffers.wa, &wr[..wn], eye, yaw, pitch);
                let legacy_water = legacy_water(&area, eye, yaw, pitch, cell);
                water_total += legacy_water.len() / 3;
                assert!(
                    is_subset(&triangles(&legacy_water), &water),
                    "map {map} cell {cell:?} view {view}: a visible water cell is not drawn"
                );
            }
        }
    }
    eprintln!("terrain triangles: {legacy_total} visible, {drawn_total} drawn; water triangles: {water_total}");
    assert!(legacy_total > 0 && water_total > 0);
}
