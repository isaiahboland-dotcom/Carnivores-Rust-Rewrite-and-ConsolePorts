//! The survival arena: where the hunter stands and the box the waves come
//! from. Mount Ravan's ravine is the mobile game's; on any other map (a
//! standalone game's) the arena is found on the map itself.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;

use super::navigation::{Fence, Flow};
use crate::area::Area;

/// Cells of a spawn box the waves must be able to start from (and walk to
/// the hunter from) for the arena to be used.
const MIN_SPAWN_CELLS: usize = 8;
/// A found arena's spawn box: half its side (map units, 8 cells).
const FOUND_HALF: f32 = 2048.0;
/// How far a found spawn box is from the hunter (cells): about the
/// ravine's 80, within a [`Flow`]'s window.
const FOUND_DIST: [i32; 3] = [64, 48, 80];
/// Directions tried round a hunter's spot for a found spawn box.
const DIRECTIONS: usize = 16;
/// Hunter spots tried before giving up (rings out from the map's centre).
const MAX_SPOTS: usize = 64;
/// Ring spacing and step along a ring (cells) when looking for a spot.
const SPOT_STEP: i32 = 24;

/// The survival arena: the hunter's spot and the box the waves come from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arena {
    /// Where they come: centre `x`, `z`, half sizes (map units).
    pub spawn: [f32; 4],
    pub hunter: [f32; 2],
    /// Which way the hunter faces at his spot (rad).
    pub hunter_alpha: f32,
    /// Before the first wave, and between waves (ms).
    pub first_ms: i32,
    pub rest_ms: i32,
}

impl Arena {
    /// Mount Ravan's mobile survival ravine, in the original PC map units.
    /// Placement is independent of the wave roster and needs no APK at runtime.
    pub fn ravine() -> Self {
        Self::facing([89330.13, 65388.617], [84069.125, 45230.77, 3000.0, 2000.0])
    }

    /// An arena with the hunter at `hunter`, facing the box `spawn`.
    fn facing(hunter: [f32; 2], spawn: [f32; 4]) -> Self {
        Self {
            hunter,
            spawn,
            hunter_alpha: libm::atan2f(spawn[0] - hunter[0], hunter[1] - spawn[1]),
            first_ms: 500,
            rest_ms: 2000,
        }
    }

    /// This arena when it works on `area`, else one found there (a
    /// standalone game's map: the ravine's spot is somewhere else on it, or
    /// under water). No arena found: this one, as before.
    pub fn fitted(self, area: &Area) -> Self {
        if self.fits(area) {
            return self;
        }
        match Self::find(area) {
            Some(a) => {
                #[cfg(feature = "std")]
                tracing::info!(
                    "survival: the arena does not fit this map; hunter at cell ({}, {}), waves from ({}, {})",
                    (a.hunter[0] / 256.0) as i32,
                    (a.hunter[1] / 256.0) as i32,
                    (a.spawn[0] / 256.0) as i32,
                    (a.spawn[1] / 256.0) as i32,
                );
                a
            }
            None => {
                #[cfg(feature = "std")]
                tracing::warn!("survival: no arena found on this map; keeping the ravine's");
                self
            }
        }
    }

    /// Whether the hunter can stand at his spot and enough of the spawn box
    /// is dry, free ground with a way to him inside the survival's fence.
    pub fn fits(&self, area: &Area) -> bool {
        let at = cell_of(self.hunter);
        if !standable(area, at) {
            return false;
        }
        let fence = Fence::new(area, at, self.spawn);
        let flow = Flow::new(area, at, Some(&fence));
        let b = self.spawn;
        let (x0, x1) = (
            ((b[0] - b[2]) / 256.0) as i32,
            ((b[0] + b[2]) / 256.0) as i32,
        );
        let (z0, z1) = (
            ((b[1] - b[3]) / 256.0) as i32,
            ((b[1] + b[3]) / 256.0) as i32,
        );
        let mut n = 0;
        for z in z0..=z1 {
            for x in x0..=x1 {
                if flow.get(x, z) != u16::MAX && standable(area, (x, z)) {
                    n += 1;
                    if n >= MIN_SPAWN_CELLS {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// An arena on `area`: the hunter on standable ground near the map's
    /// centre, the waves from a box of reachable ground some way off.
    pub fn find(area: &Area) -> Option<Self> {
        spots()
            .filter(|&c| standable(area, c))
            .take(MAX_SPOTS)
            .find_map(|at| {
                let flow = Flow::new(area, at, None);
                let mut boxes: alloc::vec::Vec<(usize, (i32, i32))> = alloc::vec::Vec::new();
                for dist in FOUND_DIST {
                    for d in 0..DIRECTIONS {
                        let a = d as f32 * core::f32::consts::TAU / DIRECTIONS as f32;
                        let c = (
                            at.0 + (libm::cosf(a) * dist as f32) as i32,
                            at.1 + (libm::sinf(a) * dist as f32) as i32,
                        );
                        let n = reachable_around(area, &flow, c);
                        if n >= MIN_SPAWN_CELLS * 4 {
                            boxes.push((n, c));
                        }
                    }
                }
                // Most ground first; the first found among equals.
                boxes.sort_by_key(|b| core::cmp::Reverse(b.0));
                let hunter = [at.0 as f32 * 256.0 + 128.0, at.1 as f32 * 256.0 + 128.0];
                boxes.into_iter().take(4).find_map(|(_, c)| {
                    let spawn = [
                        c.0 as f32 * 256.0 + 128.0,
                        c.1 as f32 * 256.0 + 128.0,
                        FOUND_HALF,
                        FOUND_HALF,
                    ];
                    let a = Self::facing(hunter, spawn);
                    a.fits(area).then_some(a)
                })
            })
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::ravine()
    }
}

/// The cell under a map position.
fn cell_of(p: [f32; 2]) -> (i32, i32) {
    let c = |v: f32| (v / 256.0).clamp(1.0, 1022.0) as i32;
    (c(p[0]), c(p[1]))
}

/// Whether cell `c` is dry ground an animal (or the hunter) can stand on:
/// inside the map's border, not water, no steeper than [`Flow`] walks, and
/// clear of objects.
fn standable(area: &Area, (x, z): (i32, i32)) -> bool {
    if !(4..1020).contains(&x) || !(4..1020).contains(&z) {
        return false;
    }
    let i = crate::area::idx(x as usize, z as usize);
    if area.map.flags[i] & crate::area::cell_flags::WATER != 0 {
        return false;
    }
    let (wx, wz) = (x as f32 * 256.0 + 128.0, z as f32 * 256.0 + 128.0);
    let y = area.land_height(wx, wz);
    if area.land_up_h(wx, wz) - y > 32.0 {
        return false;
    }
    let flat = [(-64.0, -64.0), (64.0, -64.0), (-64.0, 64.0), (64.0, 64.0)]
        .into_iter()
        .all(|(dx, dz)| (area.land_height(wx + dx, wz + dz) - y).abs() <= 80.0);
    let mut p = [wx, y, wz];
    flat && !crate::dino::check_place_collision_p(area, &mut p)
}

/// Standable cells with a way to the hunter in the found box round `c`.
fn reachable_around(area: &Area, flow: &Flow, c: (i32, i32)) -> usize {
    let r = (FOUND_HALF / 256.0) as i32;
    let mut n = 0;
    for z in c.1 - r..=c.1 + r {
        for x in c.0 - r..=c.0 + r {
            if flow.get(x, z) != u16::MAX && standable(area, (x, z)) {
                n += 1;
            }
        }
    }
    n
}

/// Candidate hunter cells: the map's centre, then rings round it.
fn spots() -> impl Iterator<Item = (i32, i32)> {
    const C: i32 = 512;
    core::iter::once((C, C)).chain((1..=(448 / SPOT_STEP)).flat_map(|k| {
        let r = k * SPOT_STEP;
        let side = (-r..r).step_by(SPOT_STEP as usize);
        let top = side.clone().map(move |i| (C + i, C - r));
        let right = side.clone().map(move |i| (C + r, C + i));
        let bottom = side.clone().map(move |i| (C - i, C + r));
        let left = side.map(move |i| (C - r, C - i));
        top.chain(right).chain(bottom).chain(left)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sea with an island of dry land: cells `x0..x1`, `z0..z1`.
    fn island(x0: usize, x1: usize, z0: usize, z1: usize) -> Area {
        let mut a = Area::flat(10);
        for z in 0..1024 {
            for x in 0..1024 {
                if !(x0..x1).contains(&x) || !(z0..z1).contains(&z) {
                    let i = crate::area::idx(x, z);
                    a.map.flags[i] |= crate::area::cell_flags::WATER;
                    a.map.water[i] = 0;
                    a.map.height[i] = 0;
                }
            }
        }
        a
    }

    #[test]
    fn the_ravine_is_kept_where_it_fits() {
        let area = Area::flat(10);
        assert!(Arena::ravine().fits(&area));
        assert_eq!(Arena::ravine().fitted(&area), Arena::ravine());
    }

    #[test]
    fn another_map_gets_an_arena_on_its_own_dry_land() {
        // Land round the map's centre only: the ravine (cell 349, 255) is sea.
        let area = island(400, 640, 400, 640);
        assert!(!Arena::ravine().fits(&area));
        let a = Arena::ravine().fitted(&area);
        assert_ne!(a, Arena::ravine());
        assert!(a.fits(&area));
        let (hx, hz) = cell_of(a.hunter);
        assert!((400..640).contains(&hx) && (400..640).contains(&hz));
        let (sx, sz) = cell_of([a.spawn[0], a.spawn[1]]);
        let d = (sx - hx).abs().max((sz - hz).abs());
        assert!(d >= 32, "the waves come from some way off ({d} cells)");
    }

    #[test]
    fn a_map_with_no_dry_land_keeps_the_ravine() {
        let area = island(0, 0, 0, 0);
        assert_eq!(Arena::find(&area), None);
        assert_eq!(Arena::ravine().fitted(&area), Arena::ravine());
    }
}
