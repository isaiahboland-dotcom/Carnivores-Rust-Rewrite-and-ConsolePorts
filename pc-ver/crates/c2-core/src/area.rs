//! An area (`.MAP` + `.RSC`) prepared the way the hardware renderers see it
//! after `Resources.cpp: LoadResources` + `CreateTMap` (D3D/3dfx path).

use alloc::{vec, vec::Vec};

use c2_formats::map::{Map, MAP_SIZE};
use c2_formats::rsc::{object_flags, Rsc};

use crate::{CELL, H_SCALE};

/// `FMap` flags (`Hunt.h: fm*`).
pub mod cell_flags {
    /// Bits 0-1: terrain texture rotation (`TDirection`).
    pub const TEX_DIR_MASK: u16 = 0x0003;
    /// Bits 2-3: object rotation in quarter turns.
    pub const OBJ_DIR_SHIFT: u16 = 2;
    pub const REVERSE: u16 = 0x0010;
    pub const NO_WAY: u16 = 0x0020;
    pub const WATER: u16 = 0x0080;
    pub const WATER2: u16 = 0x8000;
    pub const WATER_ANY: u16 = WATER | WATER2;
}

/// `OMap` value meaning "no object".
pub const NO_OBJECT: u8 = 255;
/// `OMap` value marking a helicopter landing spot (removed at load).
const LANDING: u8 = 254;
/// `WMap` value meaning "no water".
pub const NO_WATER: u8 = 255;

/// `OptDayNight`, the index of the light map, sky and fade colour: 0 is the
/// menu's "Dawn" (low sun, the dimmer light map, a warm sky; the code calls
/// it dusk), 1 full day, 2 night.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDay {
    Dusk = 0,
    Day = 1,
    Night = 2,
}

impl TimeOfDay {
    /// `Sun3dPos`: sun (moon at night) offset from the camera (`Game.cpp`).
    pub fn sun_pos(self) -> [f32; 3] {
        match self {
            TimeOfDay::Dusk => [-4048.0, 2048.0, -4048.0],
            TimeOfDay::Day => [-2048.0, 4048.0, -2048.0],
            TimeOfDay::Night => [3048.0, 3048.0, 3048.0],
        }
    }

    /// Night hunts are seen through night vision: `BrightenTexture` turns
    /// every texel into `(G/8, G, G/8)`.
    pub fn night_vision(self) -> bool {
        self == TimeOfDay::Night
    }
}

/// One placed object instance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub cell: (u16, u16),
    pub model: u8,
    /// World position of the model origin.
    pub pos: [f32; 3],
    /// Quarter turns around Y (`(FMap >> 2) & 3`).
    pub quarter_turns: u8,
}

pub struct Area {
    pub map: Map,
    pub rsc: Rsc,
    pub time: TimeOfDay,
    /// Landing cells (`LandingList`), from `OMap == 254`; in the trophy
    /// room the original's fixed ring ([`Area::set_trophy_mode`]).
    pub landings: Vec<(u16, u16)>,
    /// The map's own landing cells (`OMap == 254`), kept in the trophy room:
    /// the Modder's Edition places trophies on them (`tropPos`).
    pub marked_landings: Vec<(u16, u16)>,
    /// `OMap` entries naming a model the RSC doesn't have (the original halts).
    pub bad_object_cells: usize,
    /// Where the hunter starts in a trophy room laid out by its `.SPOT`
    /// file (`None`: the original's door).
    pub trophy_start: Option<(u16, u16)>,
    /// Snow of its own (`.SNOW`, ours: the phone's Ice Age areas), as Ice
    /// Age's at the area's density.
    pub snow: Option<c2_formats::mee::Snow>,
    /// Collision data per RSC object, in world units.
    pub shapes: Vec<crate::collide::ObjectShape>,
    /// `RandomMap[32][32]` (0..1023): darkens objects a little per cell.
    /// The original fills it with `rand()` once per run; ours is a fixed
    /// sequence of the same generator.
    pub random_map: Vec<i32>,
}

/// A water vertex's light (`Hunt2.cpp`): `168 - (int)(wdelta * 24)` with
/// `wdelta = sin(-pi/2 + RandomMap / 128 + RealTime / 200)`, `phase` the
/// integer `RandomMap / 128` ([`Area::water_phase`]). 0..1, the vertex
/// colour the water texture is modulated by.
pub fn water_light(phase: u8, real_time_ms: f32) -> f32 {
    let wdelta =
        libm::sinf(-core::f32::consts::FRAC_PI_2 + f32::from(phase) + real_time_ms / 200.0);
    (168.0 - (wdelta * 24.0) as i32 as f32) / 255.0
}

/// `Resources.cpp`: `RandomMap[y][x] = rand() * 1024 / RAND_MAX`.
fn random_map() -> Vec<i32> {
    let mut r = crate::rng::MsRand::new(1);
    (0..32 * 32).map(|_| r.rand() * 1024 / 32767).collect()
}

pub(crate) fn idx(x: usize, y: usize) -> usize {
    y * MAP_SIZE + x
}

impl Area {
    /// The water shimmer's phase at map vertex (x, z): `RandomMap[z & 31][x & 31] / 128`
    /// (0..8), for [`water_light`].
    pub fn water_phase(&self, x: usize, z: usize) -> u8 {
        let r = self
            .random_map
            .get((z & 31) * 32 + (x & 31))
            .copied()
            .unwrap_or(0);
        (r / 128).clamp(0, 8) as u8
    }

    pub fn new(mut map: Map, rsc: Rsc, time: TimeOfDay) -> Self {
        let mut landings = Vec::new();
        let mut bad = 0;
        create_tmap(&mut map, &rsc, &mut landings, &mut bad);
        let shapes = rsc
            .objects
            .iter()
            .map(crate::collide::ObjectShape::new)
            .collect();
        Self {
            shapes,
            map,
            rsc,
            time,
            marked_landings: landings.clone(),
            landings,
            bad_object_cells: bad,
            trophy_start: None,
            snow: None,
            random_map: random_map(),
        }
    }

    /// An area's snow from its `.SNOW` text (`density D`: the phone's
    /// `snow_density`, 1 as Ice Age's on PC); nothing for none or 0.
    pub fn set_snow(&mut self, text: &str) {
        let d = text.lines().find_map(|l| {
            let mut w = l.split_whitespace();
            (w.next()? == "density").then_some(())?;
            w.next()?.parse::<f32>().ok()
        });
        let ice = c2_formats::mee::Snow::ICE_AGE;
        self.snow = d.filter(|&d| d > 0.0).map(|d| c2_formats::mee::Snow {
            dens: (ice.dens as f32 * d.min(4.0) + 0.5) as i32,
            ..ice
        });
    }

    /// A trophy room's own layout (ours: the phone game's room, written by
    /// the APK conversion), from its `.SPOT` text: `start X Z`, then a
    /// `stand X Z` per trophy (cells) in the original ring's order (six a
    /// side: north west to east, east north to south, south east to west,
    /// west south to north; each side faces inwards as the ring's do).
    /// Lines it can't read are skipped; no stands keeps the ring.
    pub fn set_trophy_spots(&mut self, text: &str) {
        let mut stands = Vec::new();
        for line in text.lines() {
            let mut w = line.split_whitespace();
            let (Some(k), Some(x), Some(z)) = (w.next(), w.next(), w.next()) else {
                continue;
            };
            let (Ok(x), Ok(z)) = (x.parse::<u16>(), z.parse::<u16>()) else {
                continue;
            };
            if usize::from(x) >= MAP_SIZE || usize::from(z) >= MAP_SIZE {
                continue;
            }
            match k {
                "start" => self.trophy_start = Some((x, z)),
                "stand" => stands.push((x, z)),
                _ => {}
            }
        }
        // Older APK imports generated evenly spaced stands away from the
        // floor markings. Migrate that specific generated layout on load,
        // so existing installs do not need to be imported again.
        if stands.len() == 24
            && text.starts_with("# c2rs: the phone game's trophy room (its walls), our stands")
            && !text.contains("# floor-centres-v2")
        {
            let (w, e, n, s) = (
                i32::from(stands[18].0) - 4,
                i32::from(stands[6].0) + 4,
                i32::from(stands[0].1) - 4,
                i32::from(stands[12].1) + 4,
            );
            if w >= 0
                && n >= 0
                && e < MAP_SIZE as i32
                && s < MAP_SIZE as i32
                && e - w >= 24
                && s - n >= 24
            {
                for i in 0..6 {
                    let x = w + 8 + (e - 9 - (w + 8)) * i as i32 / 5;
                    let z = n + 8 + (s - 9 - (n + 8)) * i as i32 / 5;
                    stands[i] = (x as u16, (n + 2) as u16);
                    stands[6 + i] = ((e - 3) as u16, z as u16);
                    stands[17 - i] = (x as u16, (s - 3) as u16);
                    stands[23 - i] = ((w + 2) as u16, z as u16);
                }
            }
        }
        if !stands.is_empty() {
            self.landings = stands;
        }
    }

    /// The trophy room's fixed landing ring (`CreateTMap`, `TrophyMode`).
    /// The original enables this when the area name contains "trophy".
    pub fn set_trophy_mode(&mut self) {
        let mut l = Vec::with_capacity(24);
        for i in 0..6u16 {
            l.push((69 + i * 3, 66));
        }
        for i in 0..6u16 {
            l.push((87, 69 + i * 3));
        }
        for i in 0..6u16 {
            l.push((84 - i * 3, 87));
        }
        for i in 0..6u16 {
            l.push((66, 84 - i * 3));
        }
        self.landings = l;
    }

    /// A flat, empty 1024x1024 area at height `h` (tests and tools).
    pub fn flat(h: u8) -> Self {
        use c2_formats::map::{CELLS, HALF_CELLS};
        let map = Map {
            height: vec![h; CELLS],
            texture1: vec![0; CELLS],
            texture2: vec![0; CELLS],
            objects: vec![NO_OBJECT; CELLS],
            flags: vec![0; CELLS],
            light: [vec![128; CELLS], vec![128; CELLS], vec![128; CELLS]],
            water: vec![NO_WATER; CELLS],
            object_height: vec![0; CELLS],
            fogs: vec![0; HALF_CELLS],
            ambient: vec![0; HALF_CELLS],
        };
        let rsc = Rsc {
            fade_rgb: [[0; 3]; 3],
            trans_rgb: [[0; 3]; 3],
            textures: vec![],
            objects: vec![],
            skies: [vec![], vec![], vec![]],
            sky_map: vec![],
            fogs: vec![],
            random_sounds: vec![],
            ambients: vec![],
            waters: vec![],
        };
        Self::new(map, rsc, TimeOfDay::Day)
    }

    pub fn light_map(&self) -> &[u8] {
        &self.map.light[self.time as usize]
    }

    /// `Game.cpp: GetLandH`: ground height in world units at world (x, z).
    pub fn land_height(&self, x: f32, z: f32) -> f32 {
        land_height(&self.map, x, z)
    }

    /// Water surface height in world units for a cell, if it has water.
    pub fn water_level(&self, cx: usize, cy: usize) -> Option<f32> {
        let w = *self.map.water.get(idx(cx, cy))?;
        if w == NO_WATER {
            return None;
        }
        self.rsc
            .waters
            .get(usize::from(w))
            .map(|wt| wt.level as f32 * H_SCALE)
    }

    pub fn placements(&self) -> impl Iterator<Item = Placement> + '_ {
        let n = self.rsc.objects.len();
        self.map
            .objects
            .iter()
            .enumerate()
            .filter_map(move |(i, &o)| {
                if o == NO_OBJECT || usize::from(o) >= n {
                    return None;
                }
                let (x, y) = (i % MAP_SIZE, i / MAP_SIZE);
                Some(Placement {
                    cell: (x as u16, y as u16),
                    model: o,
                    pos: [
                        x as f32 * CELL + 128.0,
                        f32::from(self.map.object_height[i]) * H_SCALE,
                        y as f32 * CELL + 128.0,
                    ],
                    quarter_turns: ((self.map.flags[i] >> cell_flags::OBJ_DIR_SHIFT) & 3) as u8,
                })
            })
    }

    /// Object brightness 0..1, `renderd3d.cpp: _RenderObject`.
    pub fn object_light(&self, p: &Placement) -> f32 {
        let Some(o) = self.rsc.objects.get(usize::from(p.model)) else {
            return 0.5;
        };
        let l = self
            .light_map()
            .get(idx(usize::from(p.cell.0), usize::from(p.cell.1)))
            .copied()
            .unwrap_or(128);
        // Ground-lit objects take the light of the ground under each vertex
        // (CalcModelGroundLight: 128 + LandLt - 128); here, under the object
        // (the PC lights them per vertex, `ground_light`).
        if o.info.flags & object_flags::GROUND_LIGHT != 0 {
            return self.ground_light(p.pos[0], p.pos[2]) / 255.0;
        }
        let (x, y) = (usize::from(p.cell.0), usize::from(p.cell.1));
        let l = if o.info.flags & object_flags::DEF_LIGHT != 0 {
            o.info.default_light
        } else {
            let r = self
                .random_map
                .get((y & 31) * 32 + (x & 31))
                .copied()
                .unwrap_or(0);
            -(r >> 5) + i32::from(l >> 1) + 96
        };
        l.clamp(64, 192) as f32 / 255.0
    }

    /// The light of a far object's sprite: its light less 16
    /// (`RenderBMPModel(.., mlight-16)`; ground-lit ones from 128).
    pub fn sprite_light(&self, p: &Placement) -> f32 {
        let ground = self
            .rsc
            .objects
            .get(usize::from(p.model))
            .is_some_and(|o| o.info.flags & object_flags::GROUND_LIGHT != 0);
        let l = if ground {
            128.0 / 255.0
        } else {
            self.object_light(p)
        };
        (l - 16.0 / 255.0).max(0.0)
    }

    /// `Game.cpp: GetLandLt2`: the ground light at (x, z), 0..255, bilinear
    /// between every other cell's light.
    pub fn ground_light(&self, x: f32, z: f32) -> f32 {
        let lm = self.light_map();
        let max = MAP_SIZE as i32 - 1;
        let at = |cx: i32, cy: i32| {
            f32::from(
                lm.get(idx(cx.clamp(0, max) as usize, cy.clamp(0, max) as usize))
                    .copied()
                    .unwrap_or(128),
            )
        };
        let (xi, zi) = (x.max(0.0) as i32, z.max(0.0) as i32);
        let (cx, cy) = ((xi / 512) * 2, (zi / 512) * 2);
        let (dx, dy) = ((xi % 512) as f32, (zi % 512) as f32);
        let (h1, h2, h3, h4) = (
            at(cx, cy),
            at(cx + 2, cy),
            at(cx + 2, cy + 2),
            at(cx, cy + 2),
        );
        ((h1 * (512.0 - dx) + h2 * dx) * (512.0 - dy) + (h4 * (512.0 - dx) + h3 * dx) * dy)
            / 512.0
            / 512.0
    }

    /// Sky fade colour for the current time of day (`FadeRGB`), 0..=255.
    pub fn sky_rgb(&self) -> [i32; 3] {
        let c = self.rsc.fade_rgb[self.time as usize];
        if self.time == TimeOfDay::Night {
            // LoadResources zeroes red/blue at night (night-vision green).
            [0, c[1], 0]
        } else {
            c
        }
    }
}

pub fn land_height(map: &Map, x: f32, z: f32) -> f32 {
    let max = (MAP_SIZE - 2) as i32;
    let (xi, zi) = (x as i32, z as i32);
    let cx = (xi / 256).clamp(0, max) as usize;
    let cy = (zi / 256).clamp(0, max) as usize;
    let dx = xi.rem_euclid(256);
    let dy = zi.rem_euclid(256);
    let h = |x, y| i32::from(map.height[idx(x, y)]);
    let (mut h1, mut h2, mut h3, mut h4) =
        (h(cx, cy), h(cx + 1, cy), h(cx + 1, cy + 1), h(cx, cy + 1));
    if map.flags[idx(cx, cy)] & cell_flags::REVERSE != 0 {
        if 256 - dx > dy {
            h3 = h2 + h4 - h1;
        } else {
            h1 = h2 + h4 - h3;
        }
    } else if dx > dy {
        h4 = h1 + h3 - h2;
    } else {
        h2 = h1 + h3 - h4;
    }
    let hh = (h1 * (256 - dx) + h2 * dx) * (256 - dy) + (h4 * (256 - dx) + h3 * dx) * dy;
    hh as f32 / 256.0 / 256.0 * H_SCALE
}

/// `Resources.cpp: GetObjectH`.
fn object_height(map: &Map, x: usize, y: usize, r: i32) -> u8 {
    let (cx, cy) = ((x * 256 + 128) as f32, (y * 256 + 128) as f32);
    let r = r as f32;
    let hr = [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
        .iter()
        .map(|(dx, dy)| land_height(map, cx + dx, cy + dy))
        .fold(f32::INFINITY, f32::min);
    ((hr + 15.0) / H_SCALE).clamp(0.0, 255.0) as u8
}

/// `Resources.cpp: CreateTMap`, hardware-renderer branch.
fn create_tmap(map: &mut Map, rsc: &Rsc, landings: &mut Vec<(u16, u16)>, bad: &mut usize) {
    use cell_flags::*;
    for t in map.texture1.iter_mut().chain(map.texture2.iter_mut()) {
        if *t == 0xffff {
            *t = 1;
        }
    }

    // Spread water flags to the shoreline so water quads reach the bank.
    for y in 1..MAP_SIZE - 1 {
        for x in 1..MAP_SIZE - 1 {
            if map.flags[idx(x, y)] & WATER != 0 {
                continue;
            }
            for (nx, ny) in [
                (x + 1, y),
                (x, y + 1),
                (x - 1, y),
                (x, y - 1),
                (x - 1, y - 1),
                (x + 1, y - 1),
                (x - 1, y + 1),
                (x + 1, y + 1),
            ] {
                if map.flags[idx(nx, ny)] & WATER != 0 {
                    map.flags[idx(x, y)] |= WATER2;
                    map.water[idx(x, y)] = map.water[idx(nx, ny)];
                }
            }
            if map.flags[idx(x, y)] & WATER2 != 0 {
                if let Some(w) = rsc.waters.get(usize::from(map.water[idx(x, y)])) {
                    if i32::from(map.height[idx(x, y)]) == w.level {
                        map.height[idx(x, y)] = map.height[idx(x, y)].saturating_add(1);
                    }
                }
            }
        }
    }

    for y in 0..MAP_SIZE {
        for x in 0..MAP_SIZE {
            let i = idx(x, y);
            if map.flags[i] & WATER_ANY == 0 {
                map.water[i] = NO_WATER;
            }
            if map.objects[i] == LANDING {
                landings.push((x as u16, y as u16));
                map.objects[i] = NO_OBJECT;
            }
            let ob = map.objects[i];
            if ob == NO_OBJECT {
                map.object_height[i] = 0;
                continue;
            }
            let Some(obj) = rsc.objects.get(usize::from(ob)) else {
                *bad += 1;
                map.objects[i] = NO_OBJECT;
                continue;
            };
            if obj.info.flags & object_flags::PLACE_GROUND != 0 {
                map.object_height[i] = object_height(map, x, y, obj.info.grass_radius);
            }
        }
    }
    if landings.is_empty() {
        landings.push((256, 256));
    }
}

/// The original's densest scenery: no 48 x 48 cell window of its areas
/// holds objects of more faces than this (36 888, AREA5). Consoles thin
/// what is denser ([`thin_objects`]).
pub const ORIGINAL_FACES_48: u64 = 40_000;

/// Thins the trees and plants (objects with a far sprite) where a
/// `window`-cell square around them holds objects of more than `budget`
/// faces: each is kept with the chance `budget` / that, decided by its
/// cell alone, so the same map thins the same way for every player and a
/// removed tree is gone, collision too. For the consoles, where some
/// standalones' forests are 3-6 times the original's densest (owner,
/// 2026-11-01). Landing spots and objects without a sprite (buildings,
/// bridges) stay. How many were removed.
pub fn thin_objects(map: &mut Map, rsc: &Rsc, window: usize, budget: u64) -> usize {
    let n = MAP_SIZE;
    let faces = |o: u8| -> u64 {
        rsc.objects
            .get(usize::from(o))
            .map_or(0, |ob| ob.model.mesh.faces.len() as u64)
    };
    let thinnable = |o: u8| {
        rsc.objects
            .get(usize::from(o))
            .is_some_and(|ob| ob.info.flags & object_flags::NO_BMP == 0)
    };
    // Summed-area table of the faces.
    let w1 = n + 1;
    let mut sat = vec![0u64; w1 * w1];
    for y in 0..n {
        for x in 0..n {
            let f = faces(map.objects[idx(x, y)]);
            sat[(y + 1) * w1 + x + 1] =
                f + sat[y * w1 + x + 1] + sat[(y + 1) * w1 + x] - sat[y * w1 + x];
        }
    }
    let half = window / 2;
    let around = |x: usize, y: usize| {
        let (x0, y0) = (x.saturating_sub(half), y.saturating_sub(half));
        let (x1, y1) = ((x + half).min(n), (y + half).min(n));
        sat[y1 * w1 + x1] + sat[y0 * w1 + x0] - sat[y0 * w1 + x1] - sat[y1 * w1 + x0]
    };
    let mut removed = 0;
    for y in 0..n {
        for x in 0..n {
            let i = idx(x, y);
            let o = map.objects[i];
            if !thinnable(o) {
                continue;
            }
            let d = around(x, y);
            if d <= budget {
                continue;
            }
            // A fixed number per cell in 0..1 (a 32-bit integer hash).
            let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
            h ^= h >> 15;
            h = h.wrapping_mul(0x2C1B_3C6D);
            h ^= h >> 12;
            if f64::from(h) / f64::from(u32::MAX) >= budget as f64 / d as f64 {
                map.objects[i] = NO_OBJECT;
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, string::String};
    use c2_formats::map::{CELLS, HALF_CELLS};

    #[test]
    fn a_trophy_room_of_its_own_layout() {
        let mut a = Area::flat(10);
        a.set_trophy_mode();
        a.set_trophy_spots(
            "# ours\nstart 154 167\nstand 140 135\nstand 9999 1\nstand x 2\nstand 146 135\n",
        );
        assert_eq!(a.trophy_start, Some((154, 167)));
        assert_eq!(a.landings, vec![(140, 135), (146, 135)]);
        let mut c = Area::flat(10);
        c.set_snow("density 0.1\n");
        assert_eq!(c.snow.map(|s| s.dens), Some(200));
        c.set_snow("density 0");
        assert!(c.snow.is_none());
        // Nothing readable: the original's ring stays.
        let mut b = Area::flat(10);
        b.set_trophy_mode();
        b.set_trophy_spots("start 1 2");
        assert_eq!(b.landings.len(), 24);
    }

    #[test]
    fn old_mobile_stands_migrate_once_to_floor_centres() {
        let mut text = String::from(
            "# c2rs: the phone game's trophy room (its walls), our stands\nstart 143 168\n",
        );
        for side in 0..4 {
            for i in 0..6 {
                let forward = 140 + 29 * i / 5;
                let reverse = 169 - 29 * i / 5;
                let (x, z) = match side {
                    0 => (forward, 135),
                    1 => (174, forward),
                    2 => (reverse, 174),
                    _ => (135, reverse),
                };
                text += &format!("stand {x} {z}\n");
            }
        }
        let mut area = Area::flat(10);
        area.set_trophy_spots(&text);
        assert_eq!(
            (area.landings[0], area.landings[5]),
            ((139, 133), (169, 133))
        );
        assert_eq!(
            (area.landings[6], area.landings[23]),
            ((175, 139), (133, 139))
        );
        let expected = area.landings.clone();
        let mut modern = String::from(
            "# c2rs: the phone game's trophy room (its walls), our stands\n# floor-centres-v2\n",
        );
        for (x, z) in &expected {
            modern += &format!("stand {x} {z}\n");
        }
        area.set_trophy_spots(&modern);
        assert_eq!(area.landings, expected);
    }

    fn flat_map(h: u8) -> Map {
        Map {
            height: vec![h; CELLS],
            texture1: vec![0xffff; CELLS],
            texture2: vec![0; CELLS],
            objects: vec![NO_OBJECT; CELLS],
            flags: vec![0; CELLS],
            light: [vec![0; CELLS], vec![0; CELLS], vec![0; CELLS]],
            water: vec![0; CELLS],
            object_height: vec![0; CELLS],
            fogs: vec![0; HALF_CELLS],
            ambient: vec![0; HALF_CELLS],
        }
    }

    #[test]
    fn land_height_flat_and_slope() {
        let mut m = flat_map(10);
        assert_eq!(land_height(&m, 1000.0, 1000.0), 640.0);
        // Raise one corner; the centre of the far triangle is unaffected.
        m.height[idx(5, 5)] = 20;
        assert_eq!(land_height(&m, 4.0 * 256.0, 4.0 * 256.0), 640.0);
        assert!(land_height(&m, 5.0 * 256.0 - 1.0, 5.0 * 256.0 - 1.0) > 640.0);
        // Out-of-range positions clamp instead of panicking.
        let _ = land_height(&m, -1e9, 1e9);
    }

    #[test]
    fn tmap_fixups() {
        let mut m = flat_map(10);
        m.objects[idx(3, 3)] = LANDING;
        m.flags[idx(10, 10)] = cell_flags::WATER;
        m.water[idx(10, 10)] = 0;
        let mut landings = Vec::new();
        let mut bad = 0;
        let rsc_empty = empty_rsc();
        create_tmap(&mut m, &rsc_empty, &mut landings, &mut bad);
        assert_eq!(landings, vec![(3, 3)]);
        assert_eq!(m.objects[idx(3, 3)], NO_OBJECT);
        assert_eq!(m.texture1[0], 1);
        assert_ne!(
            m.flags[idx(11, 10)] & cell_flags::WATER2,
            0,
            "shore gets water2"
        );
        assert_eq!(m.water[idx(50, 50)], NO_WATER);
    }

    #[test]
    fn dense_forests_thin_the_same_way_every_time() {
        let face = c2_formats::model::Face {
            v: [0; 3],
            tu: [0; 3],
            tv: [0; 3],
            flags: 0,
            dmask: 0,
            distant: 0,
            next: 0,
            group: 0,
            reserved: [0; 12],
        };
        let object = |flags: i32| c2_formats::rsc::Object {
            info: c2_formats::rsc::ObjectInfo {
                radius: 0,
                y_lo: 0,
                y_hi: 0,
                line_length: 0,
                line_intensity: 0,
                circle_radius: 0,
                circle_intensity: 0,
                flags,
                grass_radius: 0,
                default_light: 0,
                last_ani_time: 0,
                bound_r: 0.0,
                reserved: [0; 16],
            },
            model: c2_formats::model::StaticModel {
                mesh: c2_formats::model::Mesh {
                    faces: vec![face; 100],
                    ..Default::default()
                },
                bones: vec![],
            },
            sprite: vec![],
            animation: None,
        };
        let mut rsc = empty_rsc();
        rsc.objects = vec![object(0), object(object_flags::NO_BMP)];
        // A tree on every cell of a 100 x 100 forest: 1 000 000 faces in a
        // 100-cell window, ten times a budget of 100 000; a hut in it.
        let mut m = flat_map(10);
        for y in 400..500 {
            for x in 400..500 {
                m.objects[idx(x, y)] = 0;
            }
        }
        m.objects[idx(450, 450)] = 1;
        m.objects[idx(451, 450)] = LANDING;
        let mut again = m.clone();
        let removed = thin_objects(&mut m, &rsc, 100, 100_000);
        assert_eq!(removed, thin_objects(&mut again, &rsc, 100, 100_000));
        assert_eq!(m.objects, again.objects);
        let left = (400..500)
            .flat_map(|y| (400..500).map(move |x| (x, y)))
            .filter(|&(x, y)| m.objects[idx(x, y)] == 0)
            .count();
        // About a tenth in the middle, more at the edges (less around them).
        assert!((1000..3500).contains(&left), "{left}");
        assert_eq!(m.objects[idx(450, 450)], 1);
        assert_eq!(m.objects[idx(451, 450)], LANDING);
        // A sparse wood is left as it is.
        let mut sparse = flat_map(10);
        sparse.objects[idx(10, 10)] = 0;
        assert_eq!(thin_objects(&mut sparse, &rsc, 100, 100_000), 0);
    }

    fn empty_rsc() -> Rsc {
        Rsc {
            fade_rgb: [[0; 3]; 3],
            trans_rgb: [[0; 3]; 3],
            textures: vec![],
            objects: vec![],
            skies: [vec![], vec![], vec![]],
            sky_map: vec![],
            fogs: vec![],
            random_sounds: vec![],
            ambients: vec![],
            waters: vec![],
        }
    }
}
