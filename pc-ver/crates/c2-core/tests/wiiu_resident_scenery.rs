#![allow(unsafe_code)]
extern crate alloc;
#[allow(dead_code)]
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
    let golden = [
        (895269, 0x67467ff06465670cu64),
        (868662, 0xdbcb1e9f2b68736f),
        (418407, 0xb9e3e3ce5d7ab4ee),
        (978168, 0x98a348d146fc78f9),
        (906630, 0x7c528a47ae723a48),
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
            for (yaw, pitch) in [(0.0, 0.0), (0.7, 0.4), (-2.1, -0.6), (3.14, 1.2)] {
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
        let models: Vec<_> = area.rsc.objects.iter().map(|o| PreparedModel::from_mesh(&o.model.mesh)).collect();
        let uvs = static_scene::scenery_uvs(&models);
        let radii: Vec<_> = models.iter().map(PreparedModel::radius).collect();
        let mut lights = vec![0.0; 1024 * 1024];
        for placement in area.placements() {
            lights[placement.cell.1 as usize * 1024 + placement.cell.0 as usize] = area.object_light(&placement);
        }
        let mut scratch = vec![[0.0; 4]; models.iter().map(|m| m.positions.len()).max().unwrap_or(0)];
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
        path.extend([(0, 0), (1, 1), (1023, 1023), (1022, 1023), (512, 512), (512 + 148, 512), (512 + 149, 512)]);
        // Room for the largest window on the path plus a few crossings.
        let mut candidates = SceneryCandidates::default();
        let largest = path.iter().map(|&cell| {
            candidates.update(&area.map.objects, models.len(), cell);
            candidates.cells.iter().map(|&i| faces(i) * 3).sum::<usize>()
        }).max().unwrap();
        let capacity = largest as u32 + 6000;
        let mut p = vec![0f32; capacity as usize * 4];
        let mut a = p.clone();
        let mut resident = ResidentScenery::default();
        for (step, &cell) in path.iter().enumerate() {
            let (pp, ap) = (p.as_mut_ptr(), a.as_mut_ptr());
            let mut emit = |i: usize, n: u32| {
                let id = area.map.objects[i] as usize;
                let center = [(i % 1024) as f32 * 256.0 + 128.0, area.map.object_height[i] as f32 * 64.0, (i / 1024) as f32 * 256.0 + 128.0];
                let count = unsafe {
                    static_scene::emit_scenery_model(&models[id], &uvs[id], lights[i], center,
                        ((area.map.flags[i] >> 2) & 3) as u8, &mut scratch, pp.add(n as usize * 4), ap.add(n as usize * 4))
                };
                ResidentObject { center, radius: radii[id], range: DrawRange { first: n, count } }
            };
            let (first, count) = resident.update(&mut candidates, &area.map.objects, models.len(), cell, capacity, faces, &mut emit).unwrap();
            if first == 0 { rebuilds += 1 } else { appends += 1 }
            assert!(first + count <= capacity);
            // Objects never overlap.
            let mut spans: Vec<_> = resident.objects.iter().map(|o| (o.range.first, o.range.count)).collect();
            spans.sort();
            assert!(spans.windows(2).all(|w| w[0].0 + w[0].1 <= w[1].0));
            let data = |o: &ResidentObject| {
                let r = o.range.first as usize * 4..(o.range.first + o.range.count) as usize * 4;
                (o.center.map(f32::to_bits), o.radius.to_bits(),
                 p[r.clone()].iter().chain(&a[r]).map(|f| f.to_bits()).collect::<Vec<_>>())
            };
            let mut walked: Vec<_> = resident.objects.iter().map(data).collect();
            walked.sort();
            let mut fresh = ResidentScenery::default();
            let mut fresh_candidates = SceneryCandidates::default();
            let mut fp = vec![0f32; capacity as usize * 4];
            let mut fa = fp.clone();
            let (fpp, fap) = (fp.as_mut_ptr(), fa.as_mut_ptr());
            fresh.update(&mut fresh_candidates, &area.map.objects, models.len(), cell, capacity, faces, |i, n| {
                let id = area.map.objects[i] as usize;
                let center = [(i % 1024) as f32 * 256.0 + 128.0, area.map.object_height[i] as f32 * 64.0, (i / 1024) as f32 * 256.0 + 128.0];
                let count = unsafe {
                    static_scene::emit_scenery_model(&models[id], &uvs[id], lights[i], center,
                        ((area.map.flags[i] >> 2) & 3) as u8, &mut scratch, fpp.add(n as usize * 4), fap.add(n as usize * 4))
                };
                ResidentObject { center, radius: radii[id], range: DrawRange { first: n, count } }
            }).unwrap();
            let mut expected: Vec<_> = fresh.objects.iter().map(|o| {
                let r = o.range.first as usize * 4..(o.range.first + o.range.count) as usize * 4;
                (o.center.map(f32::to_bits), o.radius.to_bits(),
                 fp[r.clone()].iter().chain(&fa[r]).map(|f| f.to_bits()).collect::<Vec<_>>())
            }).collect();
            expected.sort();
            assert!(walked == expected, "AREA{map} step {step} cell {cell:?}: resident objects differ from a rebuild");
        }
    }
    eprintln!("scenery crossings: {appends} appended, {rebuilds} rebuilt");
    assert!(appends > rebuilds && rebuilds > 5);
}
