#![allow(unsafe_code)]
extern crate alloc;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/static_scene.rs"]
mod static_scene;
use c2_core::{
    area::{Area, TimeOfDay},
    model::PreparedModel,
};
use static_scene::{DrawRange, ResidentObject, ResidentScenery, SceneryCandidates};

#[test]
fn resident_scenery_preserves_original_visible_streams() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    // Captured from the previous CPU-cache builder with individual visibility
    // filtering: every visible position/UV/light float, at 100 original-map views.
    // Day lighting since the time-of-day fix (slot 0 is dawn); the vertex
    // counts are unchanged.
    let golden = [
        (895269, 0x439d7d8b1dd93462u64),
        (868662, 0x82413da8c13d680a),
        (418407, 0x291335f3f63be9f8),
        (978168, 0xb46c7f163bacb666),
        (906630, 0x1b095c5b66c4bb14),
    ];
    for map in 1..=5 {
        let read = |ext| std::fs::read(root.join(format!("AREAS/AREA{map}.{ext}"))).unwrap();
        let area = Area::new(
            c2_formats::map::parse(&read("MAP")).unwrap(),
            c2_formats::rsc::parse(&read("RSC")).unwrap(),
            TimeOfDay::Day,
        );
        let models: Vec<_> = area
            .rsc
            .objects
            .iter()
            .map(|o| PreparedModel::from_mesh(&o.model.mesh))
            .collect();
        let uvs = static_scene::scenery_uvs(&models);
        let radii: Vec<_> = models.iter().map(PreparedModel::radius).collect();
        let mut lights = vec![0.0; 1024 * 1024];
        for placement in area.placements() {
            lights[placement.cell.1 as usize * 1024 + placement.cell.0 as usize] =
                area.object_light(&placement);
        }
        let mut scratch =
            vec![[0.0; 4]; models.iter().map(|m| m.positions.len()).max().unwrap_or(0)];
        let landing = area.landings.first().copied().unwrap_or((256, 256));
        let mut candidates = SceneryCandidates::default();
        let mut hash = 14695981039346656037u64;
        let mut total = 0u64;
        for cell in [
            (0, 0),
            (1, 1022),
            (512, 512),
            (1022, 1022),
            (landing.0 as i32, landing.1 as i32),
        ] {
            candidates.update(&area.map.objects, models.len(), cell);
            let count: usize = candidates
                .cells
                .iter()
                .map(|&i| models[area.map.objects[i] as usize].faces.len() * 3)
                .sum();
            let mut p = vec![0f32; count * 4 + 1];
            let mut a = p.clone();
            p[count * 4] = f32::from_bits(0x12345678);
            a[count * 4] = f32::from_bits(0x12345678);
            let mut objects = Vec::new();
            let mut offset = 0;
            for &i in &candidates.cells {
                let id = area.map.objects[i] as usize;
                let center = [
                    (i % 1024) as f32 * 256.0 + 128.0,
                    area.map.object_height[i] as f32 * 64.0,
                    (i / 1024) as f32 * 256.0 + 128.0,
                ];
                let written = unsafe {
                    static_scene::emit_scenery_model(
                        &models[id],
                        &uvs[id],
                        lights[i],
                        center,
                        ((area.map.flags[i] >> 2) & 3) as u8,
                        &mut scratch,
                        p.as_mut_ptr().add(offset * 4),
                        a.as_mut_ptr().add(offset * 4),
                    )
                };
                objects.push(ResidentObject {
                    center,
                    radius: radii[id],
                    range: DrawRange {
                        first: offset as u32,
                        count: written,
                    },
                });
                offset += written as usize;
            }
            assert_eq!(offset, count);
            assert_eq!(p[count * 4].to_bits(), 0x12345678);
            assert_eq!(a[count * 4].to_bits(), 0x12345678);
            let eye = [
                cell.0 as f32 * 256.0 + 123.25,
                area.map.height[cell.1 as usize * 1024 + cell.0 as usize] as f32 * 64.0 + 200.5,
                cell.1 as f32 * 256.0 + 67.75,
            ];
            for (yaw, pitch) in [
                (0.0, 0.0),
                (0.7, 0.4),
                (-2.1, -0.6),
                (core::f32::consts::PI, 1.2),
            ] {
                let mut ranges = vec![DrawRange::default(); objects.len()];
                let (range_count, visible) =
                    static_scene::resident_ranges(&objects, eye, yaw, pitch, &mut ranges);
                ranges.truncate(range_count);
                total += visible as u64;
                let mut feed = |bytes: &[u8]| {
                    for byte in bytes {
                        hash ^= *byte as u64;
                        hash = hash.wrapping_mul(1099511628211);
                    }
                };
                feed(&visible.to_le_bytes());
                for stream in [&p, &a] {
                    for range in &ranges {
                        assert!(range.first + range.count <= count as u32 && range.count % 3 == 0);
                        for value in &stream
                            [range.first as usize * 4..(range.first + range.count) as usize * 4]
                        {
                            feed(&value.to_bits().to_le_bytes());
                        }
                    }
                }
            }
        }
        assert_eq!(
            (total, hash),
            golden[map - 1],
            "AREA{map} resident visible stream changed"
        );
    }
}

#[test]
fn resident_ranges_merge_only_contiguous_visible_vertices() {
    let object = |first, count, center| ResidentObject {
        center,
        radius: 1.0,
        range: DrawRange { first, count },
    };
    let objects = [
        object(0, 3, [0.0, 0.0, -1000.0]),
        object(3, 3, [0.0, 0.0, -1000.0]),
        object(6, 3, [100000.0, 0.0, -1000.0]),
        object(9, 0, [0.0, 0.0, -1000.0]),
        object(9, 6, [0.0, 0.0, -1000.0]),
        object(15, 3, [0.0, 0.0, 1000.0]),
    ];
    let mut ranges = [DrawRange::default(); 6];
    let (n, vertices) = static_scene::resident_ranges(&objects, [0.0; 3], 0.0, 0.0, &mut ranges);
    assert_eq!(vertices, 12);
    assert_eq!(
        &ranges[..n],
        &[
            DrawRange { first: 0, count: 6 },
            DrawRange { first: 9, count: 6 }
        ]
    );
    assert_eq!(
        static_scene::resident_ranges(&[], [0.0; 3], 0.0, 0.0, &mut []),
        (0, 0)
    );
}

#[test]
fn resident_capacity_preflight_handles_exact_limit_and_overflow() {
    assert_eq!(static_scene::resident_vertex_count([1, 2], 9), Some(9));
    assert_eq!(static_scene::resident_vertex_count([1, 2], 8), None);
    assert_eq!(static_scene::resident_vertex_count([], 0), Some(0));
    assert_eq!(static_scene::resident_vertex_count([0, 0], 0), Some(0));
    assert_eq!(
        static_scene::resident_vertex_count([usize::MAX], usize::MAX),
        None
    );
    assert_eq!(
        static_scene::resident_vertex_count([usize::MAX / 3, 1], usize::MAX),
        None
    );
}

// Walking across cells must leave the same objects, with the same vertices, as
// rebuilding the window at each cell; a tight buffer forces compactions too.
#[test]
fn resident_scenery_crossings_match_a_full_rebuild() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let (mut appends, mut rebuilds) = (0, 0);
    for map in 1..=5 {
        let read = |ext| std::fs::read(root.join(format!("AREAS/AREA{map}.{ext}"))).unwrap();
        let area = Area::new(
            c2_formats::map::parse(&read("MAP")).unwrap(),
            c2_formats::rsc::parse(&read("RSC")).unwrap(),
            TimeOfDay::Day,
        );
        let models: Vec<_> = area
            .rsc
            .objects
            .iter()
            .map(|o| PreparedModel::from_mesh(&o.model.mesh))
            .collect();
        let uvs = static_scene::scenery_uvs(&models);
        let radii: Vec<_> = models.iter().map(PreparedModel::radius).collect();
        let mut lights = vec![0.0; 1024 * 1024];
        for placement in area.placements() {
            lights[placement.cell.1 as usize * 1024 + placement.cell.0 as usize] =
                area.object_light(&placement);
        }
        let mut scratch =
            vec![[0.0; 4]; models.iter().map(|m| m.positions.len()).max().unwrap_or(0)];
        let faces = |i: usize| models[area.map.objects[i] as usize].faces.len();
        let landing = area.landings.first().copied().unwrap_or((256, 256));
        let mut path = vec![(landing.0 as i32, landing.1 as i32)];
        for step in 0..60 {
            let last = *path.last().unwrap();
            path.push(match step % 3 {
                0 => (last.0 + 1, last.1),
                1 => (last.0 + 1, last.1 - 1),
                _ => (last.0, last.1 + 2),
            });
        }
        path.extend([
            (0, 0),
            (1, 1),
            (1023, 1023),
            (1022, 1023),
            (512, 512),
            (512 + 148, 512),
            (512 + 149, 512),
        ]);
        // Room for the largest window on the path plus a few crossings.
        let mut candidates = SceneryCandidates::default();
        let largest = path
            .iter()
            .map(|&cell| {
                candidates.update(&area.map.objects, models.len(), cell);
                candidates
                    .cells
                    .iter()
                    .map(|&i| faces(i) * 3)
                    .sum::<usize>()
            })
            .max()
            .unwrap();
        let capacity = largest as u32 + 6000;
        let mut p = vec![0f32; capacity as usize * 4];
        let mut a = p.clone();
        let mut resident = ResidentScenery::default();
        for (step, &cell) in path.iter().enumerate() {
            let (pp, ap) = (p.as_mut_ptr(), a.as_mut_ptr());
            let mut emit = |i: usize, n: u32| {
                let id = area.map.objects[i] as usize;
                let center = [
                    (i % 1024) as f32 * 256.0 + 128.0,
                    area.map.object_height[i] as f32 * 64.0,
                    (i / 1024) as f32 * 256.0 + 128.0,
                ];
                let count = unsafe {
                    static_scene::emit_scenery_model(
                        &models[id],
                        &uvs[id],
                        lights[i],
                        center,
                        ((area.map.flags[i] >> 2) & 3) as u8,
                        &mut scratch,
                        pp.add(n as usize * 4),
                        ap.add(n as usize * 4),
                    )
                };
                ResidentObject {
                    center,
                    radius: radii[id],
                    range: DrawRange { first: n, count },
                }
            };
            let before: Vec<_> = resident
                .objects
                .iter()
                .map(|o| (o.center.map(f32::to_bits), o.range))
                .collect();
            let mut written = Vec::new();
            let rebuilt = resident
                .update(
                    &mut candidates,
                    &area.map.objects,
                    models.len(),
                    cell,
                    capacity,
                    faces,
                    &mut emit,
                    &mut written,
                )
                .unwrap();
            if rebuilt {
                rebuilds += 1
            } else {
                appends += 1
            }
            assert!(written
                .windows(2)
                .all(|w| w[0].first + w[0].count < w[1].first));
            assert!(written.iter().all(|r| r.first + r.count <= capacity));
            // Every object kept its place or was written this time.
            for o in &resident.objects {
                let kept = before.contains(&(o.center.map(f32::to_bits), o.range));
                let fresh = written.iter().any(|w| {
                    w.first <= o.range.first && o.range.first + o.range.count <= w.first + w.count
                });
                assert!(
                    kept || fresh,
                    "AREA{map} step {step}: an object moved without being written"
                );
            }
            // Objects never overlap.
            let mut spans: Vec<_> = resident
                .objects
                .iter()
                .map(|o| (o.range.first, o.range.count))
                .collect();
            spans.sort();
            assert!(spans.windows(2).all(|w| w[0].0 + w[0].1 <= w[1].0));
            let data = |o: &ResidentObject| {
                let r = o.range.first as usize * 4..(o.range.first + o.range.count) as usize * 4;
                (
                    o.center.map(f32::to_bits),
                    o.radius.to_bits(),
                    p[r.clone()]
                        .iter()
                        .chain(&a[r])
                        .map(|f| f.to_bits())
                        .collect::<Vec<_>>(),
                )
            };
            let mut walked: Vec<_> = resident.objects.iter().map(data).collect();
            walked.sort();
            let mut fresh = ResidentScenery::default();
            let mut fresh_candidates = SceneryCandidates::default();
            let mut fp = vec![0f32; capacity as usize * 4];
            let mut fa = fp.clone();
            let (fpp, fap) = (fp.as_mut_ptr(), fa.as_mut_ptr());
            fresh
                .update(
                    &mut fresh_candidates,
                    &area.map.objects,
                    models.len(),
                    cell,
                    capacity,
                    faces,
                    |i, n| {
                        let id = area.map.objects[i] as usize;
                        let center = [
                            (i % 1024) as f32 * 256.0 + 128.0,
                            area.map.object_height[i] as f32 * 64.0,
                            (i / 1024) as f32 * 256.0 + 128.0,
                        ];
                        let count = unsafe {
                            static_scene::emit_scenery_model(
                                &models[id],
                                &uvs[id],
                                lights[i],
                                center,
                                ((area.map.flags[i] >> 2) & 3) as u8,
                                &mut scratch,
                                fpp.add(n as usize * 4),
                                fap.add(n as usize * 4),
                            )
                        };
                        ResidentObject {
                            center,
                            radius: radii[id],
                            range: DrawRange { first: n, count },
                        }
                    },
                    &mut Vec::new(),
                )
                .unwrap();
            let mut expected: Vec<_> = fresh
                .objects
                .iter()
                .map(|o| {
                    let r =
                        o.range.first as usize * 4..(o.range.first + o.range.count) as usize * 4;
                    (
                        o.center.map(f32::to_bits),
                        o.radius.to_bits(),
                        fp[r.clone()]
                            .iter()
                            .chain(&fa[r])
                            .map(|f| f.to_bits())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            expected.sort();
            assert!(
                walked == expected,
                "AREA{map} step {step} cell {cell:?}: resident objects differ from a rebuild"
            );
        }
    }
    eprintln!("scenery crossings: {appends} appended, {rebuilds} rebuilt");
    // Only the first cell of each map, the jumps out of the window, and the
    // tight buffer's rare overflows rebuild.
    assert!(appends > rebuilds * 4);
}

// A long run through the densest area with the console's 1,048,576-vertex
// buffer: every window fits and freed gaps are reused, so only the first cell
// rebuilds.
#[test]
fn resident_scenery_long_run_rebuilds_once() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let read = |ext| std::fs::read(root.join(format!("AREAS/AREA4.{ext}"))).unwrap();
    let area = Area::new(
        c2_formats::map::parse(&read("MAP")).unwrap(),
        c2_formats::rsc::parse(&read("RSC")).unwrap(),
        TimeOfDay::Day,
    );
    let models: Vec<_> = area
        .rsc
        .objects
        .iter()
        .map(|o| PreparedModel::from_mesh(&o.model.mesh))
        .collect();
    let faces = |i: usize| models[area.map.objects[i] as usize].faces.len();
    let (mut resident, mut candidates, mut written) = (
        ResidentScenery::default(),
        SceneryCandidates::default(),
        Vec::new(),
    );
    let mut cell = (300, 300);
    let (mut rebuilds, mut fallbacks, mut was_fallback) = (0, 0, true);
    // Laps of a 400x400-cell square, one cell per crossing.
    for step in 0..6400 {
        cell = match step / 400 % 4 {
            0 => (cell.0 + 1, cell.1),
            1 => (cell.0, cell.1 + 1),
            2 => (cell.0 - 1, cell.1),
            _ => (cell.0, cell.1 - 1),
        };
        let emit = |i: usize, n: u32| ResidentObject {
            center: [
                (i % 1024) as f32 * 256.0 + 128.0,
                0.0,
                (i / 1024) as f32 * 256.0 + 128.0,
            ],
            radius: 1.0,
            range: DrawRange {
                first: n,
                count: faces(i) as u32 * 3,
            },
        };
        match resident.update(
            &mut candidates,
            &area.map.objects,
            models.len(),
            cell,
            1_048_576,
            faces,
            emit,
            &mut written,
        ) {
            None => {
                fallbacks += 1;
                was_fallback = true;
            }
            Some(rebuilt) => {
                if rebuilt {
                    assert!(was_fallback, "step {step} cell {cell:?}: rebuilt");
                    rebuilds += 1;
                }
                was_fallback = false;
            }
        }
    }
    eprintln!("long run: {rebuilds} rebuilds, {fallbacks} fallback crossings of 6400");
    assert_eq!((rebuilds, fallbacks), (1, 0));
}

#[test]
#[ignore]
fn largest_scenery_windows() {
    let root = std::env::var("C2_ORIGINAL_HUNTDAT").unwrap();
    for map in 1..=5 {
        let read = |ext| {
            std::fs::read(std::path::Path::new(&root).join(format!("AREAS/AREA{map}.{ext}")))
                .unwrap()
        };
        let area = Area::new(
            c2_formats::map::parse(&read("MAP")).unwrap(),
            c2_formats::rsc::parse(&read("RSC")).unwrap(),
            TimeOfDay::Day,
        );
        let models: Vec<_> = area
            .rsc
            .objects
            .iter()
            .map(|o| PreparedModel::from_mesh(&o.model.mesh))
            .collect();
        let mut sum = vec![0u64; 1025 * 1025];
        for z in 0..1024 {
            for x in 0..1024 {
                let o = area.map.objects[z * 1024 + x] as usize;
                let v = if o < models.len() {
                    models[o].faces.len() as u64 * 3
                } else {
                    0
                };
                sum[(z + 1) * 1025 + x + 1] =
                    v + sum[z * 1025 + x + 1] + sum[(z + 1) * 1025 + x] - sum[z * 1025 + x];
            }
        }
        let win = |x: i64, z: i64| {
            let (x0, z0, x1, z1) = (
                (x - 74).max(0) as usize,
                (z - 74).max(0) as usize,
                (x + 75).min(1024) as usize,
                (z + 75).min(1024) as usize,
            );
            sum[z1 * 1025 + x1] + sum[z0 * 1025 + x0] - sum[z0 * 1025 + x1] - sum[z1 * 1025 + x0]
        };
        let mut all: Vec<u64> = (0..1024 * 1024).map(|i| win(i % 1024, i / 1024)).collect();
        all.sort_unstable();
        let over = all.iter().filter(|&&v| v > 1_048_576).count();
        let landing = area.landings[0];
        eprintln!(
            "AREA{map}: max {} p99 {} p50 {} landing {}; cells over the buffer: {:.1}%",
            all[all.len() - 1],
            all[all.len() * 99 / 100],
            all[all.len() / 2],
            win(landing.0 as i64, landing.1 as i64),
            over as f64 * 100.0 / all.len() as f64
        );
    }
}
