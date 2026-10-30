#![allow(unsafe_code)] // Exercise the Wii U renderer's existing raw FFI stream API.
extern crate alloc;
#[allow(dead_code)]
#[path = "../../../../wiiu-ver/perf-probe/src/static_scene.rs"]
mod static_scene;
use c2_core::area::{Area, TimeOfDay};
use static_scene::{SceneryCandidates, TerrainCache};

#[test]
fn original_maps_preserve_terrain_streams_and_scenery_order() {
    let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    // Captured from the previous uncached, six-projections-per-cell renderer.
    // Hashes cover vertex counts and every emitted position/UV/light float in
    // 20 views per map: four rotations/pitches at four edges/center and landing.
    let golden = [
        (397710, 0x4043ff0130b8d36eu64),
        (363612, 0x177f5e47c20e33c8),
        (372678, 0xf5429adaaff487ad),
        (372300, 0x733eead97cc90654),
        (356904, 0xa31fbbfd80609d89),
    ];
    for map in 1..=5 {
        let read = |ext| std::fs::read(root.join(format!("AREAS/AREA{map}.{ext}"))).unwrap();
        let area = Area::new(
            c2_formats::map::parse(&read("MAP")).unwrap(),
            c2_formats::rsc::parse(&read("RSC")).unwrap(),
            TimeOfDay::Day,
        );
        let cache = TerrainCache::new(&area);
        assert_eq!(
            cache.bounds.len() * core::mem::size_of::<[f32; 2]>(),
            8 * 1024 * 1024
        );
        let mut positions = vec![0f32; 148 * 148 * 6 * 4];
        let mut attrs = positions.clone();
        let mut hash = 14695981039346656037u64;
        let mut total = 0u64;
        let landing = area.landings.first().copied().unwrap_or((256, 256));
        let mut candidates = SceneryCandidates::default();
        for cell in [
            (0, 0),
            (1, 1022),
            (512, 512),
            (1022, 1022),
            (landing.0 as i32, landing.1 as i32),
        ] {
            let eye = [
                cell.0 as f32 * 256.0 + 123.25,
                area.map.height[cell.1 as usize * 1024 + cell.0 as usize] as f32 * 64.0 + 200.5,
                cell.1 as f32 * 256.0 + 67.75,
            ];
            for (yaw, pitch) in [(0.0, 0.0), (0.7, 0.4), (-2.1, -0.6), (3.14, 1.2)] {
                // Extra sentinels detect writes beyond the documented capacity.
                let capacity = positions.len();
                positions.push(f32::from_bits(0x12345678));
                attrs.push(f32::from_bits(0x12345678));
                let count = unsafe {
                    cache.emit(
                        &area,
                        eye,
                        yaw,
                        pitch,
                        cell,
                        positions.as_mut_ptr(),
                        attrs.as_mut_ptr(),
                    )
                } as usize;
                assert!(count <= 148 * 148 * 6 && count % 6 == 0);
                assert_eq!(positions[capacity].to_bits(), 0x12345678);
                assert_eq!(attrs[capacity].to_bits(), 0x12345678);
                positions.pop();
                attrs.pop();
                total += count as u64;
                for byte in (count as u32).to_le_bytes().into_iter().chain(
                    positions[..count * 4]
                        .iter()
                        .chain(&attrs[..count * 4])
                        .flat_map(|v| v.to_bits().to_le_bytes()),
                ) {
                    hash ^= byte as u64;
                    hash = hash.wrapping_mul(1099511628211);
                }
            }
            candidates.update(&area.map.objects, area.rsc.objects.len(), cell);
            // Independent rectangular enumeration sorted by ring, row and column
            // must match the old near-first traversal (including map clipping).
            let mut expected = Vec::new();
            for z in (cell.1 - 74).max(0)..=(cell.1 + 74).min(1023) {
                for x in (cell.0 - 74).max(0)..=(cell.0 + 74).min(1023) {
                    let i = z as usize * 1024 + x as usize;
                    if (area.map.objects[i] as usize) < area.rsc.objects.len() {
                        expected.push(i);
                    }
                }
            }
            expected.sort_unstable_by_key(|&i| {
                let (dx, dz) = ((i % 1024) as i32 - cell.0, (i / 1024) as i32 - cell.1);
                (dx.abs().max(dz.abs()), dz, dx)
            });
            assert_eq!(candidates.cells, expected, "AREA{map} at {cell:?}");
            let storage = candidates.cells.as_ptr();
            candidates.update(&area.map.objects, area.rsc.objects.len(), cell);
            assert_eq!(candidates.cells.as_ptr(), storage);
            assert_eq!(candidates.cells, expected);
        }
        assert_eq!(
            (total, hash),
            golden[map - 1],
            "AREA{map} terrain stream changed"
        );
    }
}
