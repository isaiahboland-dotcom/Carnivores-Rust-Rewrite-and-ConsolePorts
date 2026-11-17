//! Local fog volumes: the map's fog zones (`FogsMap`, 512x512, two map cells
//! per entry) with the `.RSC`'s fog list (`TFogEntity`; zone 0 is "no
//! fog", zone `n` is the list's `n`th entry).
//!
//! [`fill_floor`] is the load-time rule of `Resources.cpp: LoadMap`;
//! [`Zones::level`] is `Hunt2.cpp: CalcFogLevel`. The PC renderer evaluates
//! the same formula on the GPU (`c2-pc/src/shaders/fog_zones.wgsl`); this
//! one is its reference and the camera's side of it.

#[cfg(not(feature = "std"))]
#[cfg_attr(test, allow(unused_imports))]
use crate::portable_math::FloatMath;

use c2_formats::map::{Map, HALF_SIZE, MAP_SIZE};
use c2_formats::rsc::Fog;

use crate::H_SCALE;

/// `Resources.cpp: LoadMap`: when the first fog zone starts above height 1,
/// every zone-less `FogsMap` entry with any of its 3x3 height samples below
/// that level joins zone 1 (low ground fills with it). Runs on the heights
/// as loaded, before `CreateTMap`.
pub fn fill_floor(map: &mut Map, fogs: &[Fog]) {
    let Some(first) = fogs.first() else {
        return;
    };
    if first.y_begin <= 1.0 || map.fogs.len() < HALF_SIZE * HALF_SIZE {
        return;
    }
    for y in 0..HALF_SIZE - 2 {
        for x in 0..HALF_SIZE - 2 {
            let i = y * HALF_SIZE + x;
            if map.fogs[i] != 0 {
                continue;
            }
            let low = (0..3).any(|dy| {
                (0..3).any(|dx| {
                    let h = map.height[(y * 2 + dy) * MAP_SIZE + x * 2 + dx];
                    f32::from(h) < first.y_begin
                })
            });
            if low {
                map.fogs[i] = 1;
            }
        }
    }
}

/// `Game.cpp: InitGame`: the underwater fog's `FogsList[127]` (its colour
/// and top come from the camera's water each frame, `Hunt2.cpp`).
pub const UNDERWATER_TRANSP: f32 = 460.0;
pub const UNDERWATER_LIMIT: f32 = 200.0;

/// Scuba gear (not in the original): the underwater view reaches this many
/// times as far. Backends scale their underwater fog's distance by it (the
/// PC's fog end, and the original fog's `Transp`).
pub const SCUBA_VIEW_K: f32 = 2.0;

/// `Hunt2.cpp: CalcFogLevel` under water (`cf = 127`, `CAMERAINFOG`): the
/// fog between `eye` and `p`, both under water whose top is `level` (world
/// units), 0..`FLimit` of 255. The deeper both ends and the farther apart,
/// the thicker. `reach` scales `Transp` (1 in the original;
/// [`SCUBA_VIEW_K`] with scuba gear).
pub fn underwater_level(eye: [f32; 3], p: [f32; 3], level: f32, reach: f32) -> f32 {
    let transp = UNDERWATER_TRANSP * reach;
    let (dx, dy, dz) = (p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]);
    let mut d = (dx * dx + dy * dy + dz * dz).sqrt();
    let mut fla = (level - p[1]) / H_SCALE;
    let flb = ((level - eye[1]) / H_SCALE).max(0.0);
    if fla < 0.0 {
        if flb <= 0.0 {
            return 0.0;
        }
        d *= flb / (flb - fla);
        fla = 0.0;
    }
    let fl = (fla + flb) * (d + transp / 2.0) / transp;
    fl.min(UNDERWATER_LIMIT)
}

/// The fog zones of an area, for [`Zones::level`].
#[derive(Clone, Copy)]
pub struct Zones<'a> {
    /// `FogsMap`.
    pub map: &'a [u8],
    /// The `.RSC` fog list (zone `n` is `fogs[n - 1]`).
    pub fogs: &'a [Fog],
}

impl<'a> Zones<'a> {
    pub fn new(map: &'a Map, fogs: &'a [Fog]) -> Self {
        Self {
            map: &map.fogs,
            fogs,
        }
    }

    /// Whether any part of the map is in a zone the list defines.
    pub fn any(&self) -> bool {
        let n = self.fogs.len();
        self.map.iter().any(|&z| z != 0 && usize::from(z) <= n)
    }

    /// The zone at world (`x`, `z`); 0: none (or one the list lacks).
    pub fn zone_at(&self, x: f32, z: f32) -> u8 {
        if !(x >= 0.0 && z >= 0.0) {
            return 0;
        }
        let (cx, cz) = ((x as usize) >> 9, (z as usize) >> 9);
        if cx >= HALF_SIZE || cz >= HALF_SIZE {
            return 0;
        }
        let zone = self.map.get(cz * HALF_SIZE + cx).copied().unwrap_or(0);
        if usize::from(zone) <= self.fogs.len() {
            zone
        } else {
            0
        }
    }

    fn fog(&self, zone: u8) -> Option<&Fog> {
        self.fogs.get(usize::from(zone).checked_sub(1)?)
    }

    /// Whether `zone` is poisonous (`TFogEntity.Mortal`, AREA5's volcanic
    /// gas).
    pub fn is_mortal(&self, zone: u8) -> bool {
        self.fog(zone).is_some_and(|f| f.mortal != 0)
    }

    /// `Hunt2.cpp: CameraFogI` / `CAMERAINFOG`: the zone the camera is in,
    /// when it is below that zone's top.
    pub fn camera_zone(&self, eye: [f32; 3]) -> Option<u8> {
        let zone = self.zone_at(eye[0], eye[2]);
        let fog = self.fog(zone)?;
        (fog.y_begin * H_SCALE > eye[1]).then_some(zone)
    }

    /// `Hunt2.cpp: CalcFogLevel` (above water): how much fog lies between
    /// `eye` (in zone `camera`, see [`Zones::camera_zone`]) and the point
    /// `p`, 0..`FLimit` of 255, and the zone whose colour it takes.
    ///
    /// The fog thickness is the sum of the two ends' depths below the zone's
    /// top (height units), scaled by `(distance + Transp / 2) / Transp`;
    /// only the part of the sight line inside the fog counts. A point in no
    /// zone, seen from inside one, counts as above that zone's fog.
    pub fn level(&self, eye: [f32; 3], camera: Option<u8>, p: [f32; 3]) -> (f32, u8) {
        let mut zone = self.zone_at(p[0], p[2]);
        let mut point_in = true;
        if zone == 0 {
            match camera {
                Some(c) => {
                    zone = c;
                    point_in = false;
                }
                None => return (0.0, 0),
            }
        }
        let Some(fog) = self.fog(zone) else {
            return (0.0, 0);
        };
        let top = fog.y_begin * H_SCALE;
        let (dx, dy, dz) = (p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]);
        let mut d = (dx * dx + dy * dy + dz * dz).sqrt();
        let mut fla = (top - p[1]) / H_SCALE;
        if !point_in {
            fla = fla.min(0.0);
        }
        let mut flb = (top - eye[1]) / H_SCALE;
        if camera.is_none() {
            flb = flb.min(0.0);
        }
        if fla < 0.0 && flb < 0.0 {
            return (0.0, zone);
        }
        if fla < 0.0 {
            d *= flb / (flb - fla);
            fla = 0.0;
        }
        if flb < 0.0 {
            d *= fla / (fla - flb);
            flb = 0.0;
        }
        let transp = fog.transp.max(1.0);
        let fl = (fla + flb) * (d + transp / 2.0) / transp;
        (fl.min(fog.f_limit), zone)
    }
}

/// A `0x00BBGGRR` fog colour (the `.RSC`'s, red in the low byte, as the
/// Direct3D build reads it: `Resources.cpp` swaps it into a `D3DCOLOR`) as
/// 0..1 RGB.
pub fn rgb(c: i32) -> [f32; 3] {
    [c, c >> 8, c >> 16].map(|v| (v & 0xff) as f32 / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn fog(y_begin: f32) -> Fog {
        Fog {
            rgb: 0x00_30_60_90,
            y_begin,
            mortal: 0,
            transp: 512.0,
            f_limit: 200.0,
        }
    }

    fn map_with(fill: u8, height: u8) -> Map {
        let cells = MAP_SIZE * MAP_SIZE;
        Map {
            height: vec![height; cells],
            texture1: vec![0; cells],
            texture2: vec![0; cells],
            objects: vec![0; cells],
            flags: vec![0; cells],
            light: [vec![0; cells], vec![0; cells], vec![0; cells]],
            water: vec![0; cells],
            object_height: vec![0; cells],
            fogs: vec![fill; HALF_SIZE * HALF_SIZE],
            ambient: vec![0; HALF_SIZE * HALF_SIZE],
        }
    }

    #[test]
    fn floor_fill_takes_low_ground_into_zone_one() {
        let mut m = map_with(0, 100);
        m.height[40 * MAP_SIZE + 40] = 10;
        fill_floor(&mut m, &[fog(50.0)]);
        // Entries whose 3x3 samples (2x .. 2x + 2) reach cell 40: x, y in 19..=20.
        for (x, y) in [(19, 19), (20, 20), (19, 20), (20, 19)] {
            assert_eq!(m.fogs[y * HALF_SIZE + x], 1, "({x}, {y})");
        }
        assert_eq!(m.fogs[18 * HALF_SIZE + 18], 0);
        assert_eq!(m.fogs[21 * HALF_SIZE + 21], 0);
    }

    #[test]
    fn floor_fill_needs_a_first_zone_above_one() {
        let mut m = map_with(0, 0);
        fill_floor(&mut m, &[fog(1.0)]);
        assert!(m.fogs.iter().all(|&z| z == 0));
        fill_floor(&mut m, &[]);
        assert!(m.fogs.iter().all(|&z| z == 0));
    }

    #[test]
    fn floor_fill_keeps_existing_zones_and_the_last_two_rows() {
        let mut m = map_with(0, 0);
        m.fogs[5] = 2;
        fill_floor(&mut m, &[fog(50.0), fog(80.0)]);
        assert_eq!(m.fogs[5], 2);
        assert_eq!(m.fogs[0], 1);
        // `for (x = 0; x < 510; ...)`: the last two columns and rows stay.
        assert_eq!(m.fogs[HALF_SIZE - 1], 0);
        assert_eq!(m.fogs[(HALF_SIZE - 1) * HALF_SIZE], 0);
    }

    /// Zone 1 over the whole map, its top at height 100 (6400 units).
    fn foggy() -> Map {
        map_with(1, 0)
    }

    #[test]
    fn camera_zone_is_below_the_top_only() {
        let m = foggy();
        let fogs = [fog(100.0)];
        let z = Zones::new(&m, &fogs);
        assert_eq!(z.camera_zone([5000.0, 6000.0, 5000.0]), Some(1));
        assert_eq!(z.camera_zone([5000.0, 6500.0, 5000.0]), None);
        assert_eq!(z.camera_zone([-5.0, 0.0, 5000.0]), None);
    }

    #[test]
    fn level_sums_both_depths_scaled_by_distance() {
        let m = foggy();
        let fogs = [fog(100.0)];
        let z = Zones::new(&m, &fogs);
        let eye = [5000.0, 6400.0 - 640.0, 5000.0];
        // Both 10 units deep, 512 apart: 20 * (512 + 256) / 512 = 30.
        let (l, zone) = z.level(eye, Some(1), [5512.0, 6400.0 - 640.0, 5000.0]);
        assert_eq!(zone, 1);
        assert!((l - 30.0).abs() < 1e-3, "{l}");
        // Capped at FLimit.
        let (l, _) = z.level(eye, Some(1), [50000.0, 0.0, 5000.0]);
        assert_eq!(l, 200.0);
    }

    #[test]
    fn level_from_above_counts_only_the_part_in_the_fog() {
        let m = foggy();
        let fogs = [fog(100.0)];
        let z = Zones::new(&m, &fogs);
        // Eye 10 above the top, point 10 below it: half the line is in fog.
        let eye = [5000.0, 6400.0 + 640.0, 5000.0];
        let p = [6024.0, 6400.0 - 640.0, 5000.0];
        let d = (1024.0f32 * 1024.0 + 1280.0 * 1280.0).sqrt() / 2.0;
        let (l, _) = z.level(eye, z.camera_zone(eye), p);
        assert!((l - 10.0 * (d + 256.0) / 512.0).abs() < 1e-2, "{l}");
        // Both above: none.
        let (l, _) = z.level(eye, None, [6024.0, 6400.0 + 64.0, 5000.0]);
        assert_eq!(l, 0.0);
    }

    #[test]
    fn point_outside_any_zone_is_above_the_cameras_fog() {
        let mut m = foggy();
        m.fogs[(5000 >> 9) * HALF_SIZE + (9000 >> 9)] = 0;
        let fogs = [fog(100.0)];
        let z = Zones::new(&m, &fogs);
        let eye = [5000.0, 6400.0 - 640.0, 5000.0];
        // Deep below the top, but outside the zone: only the eye's depth.
        let p = [9000.0, 0.0, 5000.0];
        let (l, zone) = z.level(eye, Some(1), p);
        assert_eq!(zone, 1);
        let d = (4000.0f32 * 4000.0 + 5760.0 * 5760.0).sqrt();
        assert!((l - (10.0 * (d + 256.0) / 512.0).min(200.0)).abs() < 1e-2);
        // From outside every zone, nothing.
        assert_eq!(z.level(eye, None, p), (0.0, 0));
    }

    #[test]
    fn zones_past_the_list_are_ignored() {
        let mut m = foggy();
        m.fogs.fill(3);
        let fogs = [fog(100.0)];
        let z = Zones::new(&m, &fogs);
        assert!(!z.any());
        assert_eq!(z.zone_at(5000.0, 5000.0), 0);
    }

    #[test]
    fn underwater_fog_thickens_with_depth_and_distance() {
        let level = 6400.0;
        let eye = [0.0, level - 640.0, 0.0];
        // 10 deep each, 460 apart: 20 * (460 + 230) / 460 = 30.
        let l = underwater_level(eye, [460.0, level - 640.0, 0.0], level, 1.0);
        assert!((l - 30.0).abs() < 1e-3, "{l}");
        let deeper = underwater_level(eye, [460.0, level - 1280.0, 0.0], level, 1.0);
        assert!(deeper > l);
        assert_eq!(
            underwater_level(eye, [99999.0, 0.0, 0.0], level, 1.0),
            200.0
        );
        // Above the surface: only the part under it.
        let up = underwater_level(eye, [0.0, level + 640.0, 0.0], level, 1.0);
        assert!((up - 10.0 * (640.0 + 230.0) / 460.0).abs() < 1e-3, "{up}");
    }

    #[test]
    fn scuba_gear_thins_the_underwater_fog() {
        let level = 6400.0;
        let eye = [0.0, level - 640.0, 0.0];
        let far = [1840.0, level - 640.0, 0.0];
        let plain = underwater_level(eye, far, level, 1.0);
        let scuba = underwater_level(eye, far, level, SCUBA_VIEW_K);
        assert!(scuba < plain, "{scuba} vs {plain}");
        // The same fog lies twice as far away.
        let twice = underwater_level(eye, [3680.0, level - 640.0, 0.0], level, SCUBA_VIEW_K);
        assert!((twice - plain).abs() < 1e-3, "{twice} vs {plain}");
    }

    #[test]
    fn colour_is_red_in_the_low_byte() {
        assert_eq!(rgb(0x0000_00ff), [1.0, 0.0, 0.0]);
        assert_eq!(rgb(0x00ff_0000), [0.0, 0.0, 1.0]);
    }
}
