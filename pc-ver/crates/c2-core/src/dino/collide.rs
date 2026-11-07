//! Place collision: where an animal may stand or move (`CheckPlaceCollision*`).

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use alloc::{boxed::Box, string::String, vec, vec::Vec};

use c2_formats::mee::Region;

use crate::area::{cell_flags, idx, Area, NO_OBJECT};
use crate::CELL;

fn cell_of(v: f32) -> i32 {
    (v as i32) / 256
}

fn flags_around(area: &Area, ccx: i32, ccz: i32) -> u16 {
    let f = |x: i32, z: i32| area.map.flags[idx(x as usize, z as usize)];
    f(ccx - 1, ccz)
        | f(ccx, ccz - 1)
        | f(ccx - 1, ccz - 1)
        | f(ccx, ccz)
        | f(ccx, ccz + 1)
        | f(ccx + 1, ccz)
        | f(ccx + 1, ccz + 1)
}

/// Any object with `Radius >= 10` within `Radius + 64` of (x, z)?
fn near_big_object(area: &Area, x: f32, z: f32, ccx: i32, ccz: i32) -> bool {
    for dz in -2..=2 {
        for dx in -2..=2 {
            let ob = area.map.objects[idx((ccx + dx) as usize, (ccz + dz) as usize)];
            if ob == NO_OBJECT {
                continue;
            }
            let Some(o) = area.rsc.objects.get(usize::from(ob)) else {
                continue;
            };
            // MObjects[].info.Radius, which LoadResources doubled.
            let r = o.info.radius as f32 * 2.0;
            if r < 10.0 {
                continue;
            }
            let (ox, oz) = (
                (ccx + dx) as f32 * CELL + 128.0,
                (ccz + dz) as f32 * CELL + 128.0,
            );
            if ((ox - x).powi(2) + (oz - z).powi(2)).sqrt() < r + 64.0 {
                return true;
            }
        }
    }
    false
}

/// `CheckPlaceCollisionP`: is this a bad spot to place/target a character?
pub fn check_place_collision_p(area: &Area, v: &mut [f32; 3]) -> bool {
    place_collision(area, v, false)
}

/// `CheckPlaceCollisionP(v, aquatic)` as the Modder's Edition Engine has it
/// (`Game/CharacterCollision.cpp`): an aquatic animal may stand in water.
pub fn check_place_collision_aquatic(area: &Area, v: &mut [f32; 3]) -> bool {
    place_collision(area, v, true)
}

fn place_collision(area: &Area, v: &mut [f32; 3], aquatic: bool) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !crate::collide::WALK_CELLS.contains(&ccx) || !crate::collide::WALK_CELLS.contains(&ccz) {
        return true;
    }
    let blocked = if aquatic {
        cell_flags::NO_WAY
    } else {
        cell_flags::WATER | cell_flags::NO_WAY
    };
    if flags_around(area, ccx, ccz) & blocked != 0 {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    v[1] = h;
    for (dx, dz) in [
        (-164.0, -164.0),
        (164.0, -164.0),
        (-164.0, 164.0),
        (164.0, 164.0),
    ] {
        if (area.land_height(v[0] + dx, v[2] + dz) - h).abs() > 160.0 {
            return true;
        }
    }
    near_big_object(area, v[0], v[2], ccx, ccz)
}

/// Is (x, z) strictly inside region `r` (cells)?
pub(crate) fn in_region(r: &Region, x: f32, z: f32) -> bool {
    x > r.xmin as f32 * 256.0
        && x < r.xmax as f32 * 256.0
        && z > r.ymin as f32 * 256.0
        && z < r.ymax as f32 * 256.0
}

/// Is cell (ccx, ccz) strictly inside one of the `avoid` regions?
fn in_avoided_cell(avoid: &[Region], ccx: i32, ccz: i32) -> bool {
    avoid
        .iter()
        .any(|r| ccx > r.xmin && ccx < r.xmax && ccz > r.ymin && ccz < r.ymax)
}

/// `CheckPlaceCollision(v, wc, mc)`.
pub fn check_place_collision(area: &Area, v: &mut [f32; 3], wc: bool, mc: bool) -> bool {
    check_place_collision_avoid(area, v, wc, mc, &[])
}

/// `CheckPlaceCollision(cptr, v, wc, mc)` as the dialect has it: the
/// character's spawn group's avoided regions block too.
pub(crate) fn check_place_collision_avoid(
    area: &Area,
    v: &mut [f32; 3],
    wc: bool,
    mc: bool,
    avoid: &[Region],
) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !crate::collide::WALK_CELLS.contains(&ccx) || !crate::collide::WALK_CELLS.contains(&ccz) {
        return true;
    }
    if wc && flags_around(area, ccx, ccz) & cell_flags::WATER != 0 {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    if area.map.flags[idx(ccx as usize, ccz as usize)] & cell_flags::WATER == 0
        && (h - v[1]).abs() > 64.0
    {
        return true;
    }
    if in_avoided_cell(avoid, ccx, ccz) {
        return true;
    }
    v[1] = h;
    if slope_too_steep(area, v[0], v[2], h) {
        return true;
    }
    mc && near_big_object(area, v[0], v[2], ccx, ccz)
}

fn slope_too_steep(area: &Area, x: f32, z: f32, h: f32) -> bool {
    [(-64.0, -64.0), (64.0, -64.0), (-64.0, 64.0), (64.0, 64.0)]
        .iter()
        .any(|(dx, dz)| (area.land_height(x + dx, z + dz) - h).abs() > 100.0)
}

/// Is the water deeper than `limit` at (x, z) or `range` around it?
fn deep_around(area: &Area, x: f32, z: f32, limit: f32, range: f32) -> bool {
    [-range, 0.0, range].iter().any(|&dx| {
        [-range, 0.0, range]
            .iter()
            .any(|&dz| area.land_up_h(x + dx, z + dz) - area.land_height(x + dx, z + dz) > limit)
    })
}

/// Bounds, avoided regions, then the ground: within 64 of `v[1]`, not too
/// steep, clear of big objects (`mc`). The tail of the dialect's
/// brachiosaur checks.
fn brahi_ground(area: &Area, v: &mut [f32; 3], mc: bool) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    let h = area.land_height(v[0], v[2]);
    if (h - v[1]).abs() > 64.0 {
        return true;
    }
    v[1] = h;
    slope_too_steep(area, v[0], v[2], h) || (mc && near_big_object(area, v[0], v[2], ccx, ccz))
}

/// `CheckPlaceCollisionBrahi`: wading, but not deeper than 550.
pub(crate) fn check_place_collision_brahi(
    area: &Area,
    v: &mut [f32; 3],
    wc: bool,
    mc: bool,
    avoid: &[Region],
) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !crate::collide::WALK_CELLS.contains(&ccx)
        || !crate::collide::WALK_CELLS.contains(&ccz)
        || in_avoided_cell(avoid, ccx, ccz)
    {
        return true;
    }
    if wc && deep_around(area, v[0], v[2], 550.0, 256.0) {
        return true;
    }
    brahi_ground(area, v, mc)
}

/// `CheckPlaceCollisionLandBrahi`: no water (`wc`), or none deeper than
/// the species' `waterLevel`.
pub(crate) fn check_place_collision_land_brahi(
    area: &Area,
    v: &mut [f32; 3],
    wc: bool,
    mc: bool,
    avoid: &[Region],
    water_level: f32,
) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !crate::collide::WALK_CELLS.contains(&ccx)
        || !crate::collide::WALK_CELLS.contains(&ccz)
        || in_avoided_cell(avoid, ccx, ccz)
    {
        return true;
    }
    let wet = if wc {
        flags_around(area, ccx, ccz) & cell_flags::WATER != 0
    } else {
        deep_around(area, v[0], v[2], water_level, 256.0)
    };
    wet || brahi_ground(area, v, mc)
}

/// `CheckPlaceCollisionBrahiP`: a spot in water no deeper than 550.
pub(crate) fn check_place_collision_brahi_p(area: &Area, v: &mut [f32; 3]) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !crate::collide::WALK_CELLS.contains(&ccx) || !crate::collide::WALK_CELLS.contains(&ccz) {
        return true;
    }
    if area.land_up_h(v[0], v[2]) <= area.land_height(v[0], v[2])
        || deep_around(area, v[0], v[2], 550.0, 256.0)
    {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    v[1] = h;
    for (dx, dz) in [
        (-164.0, -164.0),
        (164.0, -164.0),
        (-164.0, 164.0),
        (164.0, 164.0),
    ] {
        if (area.land_height(v[0] + dx, v[2] + dz) - h).abs() > 160.0 {
            return true;
        }
    }
    near_big_object(area, v[0], v[2], ccx, ccz)
}

/// Is the water at (x, z) and 256 around it between `lo` and `hi` deep?
fn depth_band_around(area: &Area, x: f32, z: f32, lo: f32, hi: f32) -> bool {
    !deep_around(area, x, z, hi, 256.0)
        && [-256.0, 0.0, 256.0].iter().all(|&dx| {
            [-256.0, 0.0, 256.0]
                .iter()
                .all(|&dz| area.land_up_h(x + dx, z + dz) - area.land_height(x + dx, z + dz) >= lo)
        })
}

/// `CheckPlaceCollisionFish`: outside the species' depth band (or an
/// avoided region).
pub(crate) fn check_place_collision_fish(
    area: &Area,
    v: &[f32; 3],
    lo: f32,
    hi: f32,
    avoid: &[Region],
) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    !crate::collide::WALK_CELLS.contains(&ccx)
        || !crate::collide::WALK_CELLS.contains(&ccz)
        || in_avoided_cell(avoid, ccx, ccz)
        || !depth_band_around(area, v[0], v[2], lo, hi)
}

/// `CheckPlaceCollisionFishP`.
pub(crate) fn check_place_collision_fish_p(area: &Area, v: &[f32; 3], lo: f32, hi: f32) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    !crate::collide::WALK_CELLS.contains(&ccx)
        || !crate::collide::WALK_CELLS.contains(&ccz)
        || !depth_band_around(area, v[0], v[2], lo, hi)
}

/// `CheckPlaceCollision2(v, wc)`; the dialect's also honours `avoid`.
pub fn check_place_collision2(area: &Area, v: &mut [f32; 3], wc: bool) -> bool {
    check_place_collision2_avoid(area, v, wc, &[])
}

pub(crate) fn check_place_collision2_avoid(
    area: &Area,
    v: &mut [f32; 3],
    wc: bool,
    avoid: &[Region],
) -> bool {
    let (ccx, ccz) = (cell_of(v[0]), cell_of(v[2]));
    if !crate::collide::WALK_CELLS.contains(&ccx) || !crate::collide::WALK_CELLS.contains(&ccz) {
        return true;
    }
    if wc && flags_around(area, ccx, ccz) & cell_flags::WATER != 0 {
        return true;
    }
    if in_avoided_cell(avoid, ccx, ccz) {
        return true;
    }
    let h = area.land_height(v[0], v[2]);
    v[1] = h;
    slope_too_steep(area, v[0], v[2], h)
}
