//! An area (`.MAP` + `.RSC`) prepared the way the hardware renderers see it
//! after `Resources.cpp: LoadResources` + `CreateTMap` (D3D/3dfx path).

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDay {
    Day = 0,
    Dusk = 1,
    Night = 2,
}

impl TimeOfDay {
    /// `Sun3dPos`: sun (moon at night) offset from the camera (`Game.cpp`).
    pub fn sun_pos(self) -> [f32; 3] {
        match self {
            TimeOfDay::Day => [-4048.0, 2048.0, -4048.0],
            TimeOfDay::Dusk => [-2048.0, 4048.0, -2048.0],
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
    /// Landing cells (`LandingList`), from `OMap == 254`.
    pub landings: Vec<(u16, u16)>,
    /// `OMap` entries naming a model the RSC doesn't have (the original halts).
    pub bad_object_cells: usize,
    /// Collision data per RSC object, in world units.
    pub shapes: Vec<crate::collide::ObjectShape>,
    /// `RandomMap[32][32]` (0..1023): darkens objects a little per cell.
    /// The original fills it with `rand()` once per run; ours is a fixed
    /// sequence of the same generator.
    pub random_map: Vec<i32>,
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
            landings,
            bad_object_cells: bad,
            random_map: random_map(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use c2_formats::map::{CELLS, HALF_CELLS};

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
