//! Height queries and object collision against an [`Area`], following
//! `Game.cpp` (`GetLandQH`, `GetLandCeilH`, `GetLandUpH`, `GetLandQHNoObj`)
//! and `mathematics.cpp` (`CheckCollision`, `CheckBoundCollision`,
//! `CalcBoundBox`).
//!
//! The original reads the global `PlayerY`; here it is a parameter.

#[cfg(not(feature = "std"))]
#[cfg_attr(test, allow(unused_imports))]
use crate::portable_math::FloatMath;
use alloc::vec::Vec;

use c2_formats::map::MAP_SIZE;
use c2_formats::rsc::{object_flags, Object};

use crate::area::{cell_flags, idx, Area, NO_OBJECT};
use crate::{CELL, H_SCALE};

/// `TBound`: one axis-aligned box per bone (`owner`), in model space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundBox {
    pub cx: f32,
    pub cy: f32,
    /// Half extents along x and z.
    pub a: f32,
    pub b: f32,
    pub y1: f32,
    pub y2: f32,
}

/// Collision data for one RSC object, scaled like `LoadResources` does
/// (`Radius`, `YLo`, `YHi` * 2).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectShape {
    pub radius: f32,
    pub y_lo: f32,
    pub y_hi: f32,
    pub flags: i32,
    /// Only for `ofBOUND` objects.
    pub bounds: Vec<BoundBox>,
}

/// The cells the hunter can reach (`CheckCollision` keeps him in 36..980).
/// Animals keep to them too (not in the original, where they walk to the
/// map's edge, out of the hunter's reach: the owner's ask).
pub const WALK_CELLS: core::ops::RangeInclusive<i32> = 36..=980;
/// [`WALK_CELLS`] in map units.
pub const WALK_MIN: f32 = 36.0 * CELL;
pub const WALK_MAX: f32 = 980.0 * CELL;

impl ObjectShape {
    pub fn new(o: &Object) -> Self {
        let bounds = if o.info.flags & object_flags::BOUND != 0 {
            calc_bound_box(o)
        } else {
            Vec::new()
        };
        Self {
            radius: o.info.radius as f32 * 2.0,
            y_lo: o.info.y_lo as f32 * 2.0,
            y_hi: o.info.y_hi as f32 * 2.0,
            flags: o.info.flags,
            bounds,
        }
    }

    fn has(&self, f: i32) -> bool {
        self.flags & f != 0
    }
}

/// `CalcBoundBox`: boxes per owner 0..8 from the loaded (scaled, z-flipped)
/// vertices, widened by 72 units in x and z.
fn calc_bound_box(o: &Object) -> Vec<BoundBox> {
    let mut out = Vec::new();
    for owner in 0..8i16 {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        let mut any = false;
        for v in &o.model.mesh.vertices {
            if v.hide != 0 || v.owner != owner {
                continue;
            }
            let p = [v.x * 2.0, v.y * 2.0, v.z * -2.0];
            if !any {
                for k in 0..3 {
                    lo[k] = p[k] - 1.0;
                    hi[k] = p[k] + 1.0;
                }
                any = true;
            }
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if !any {
            continue;
        }
        let (x1, x2, z1, z2) = (lo[0] - 72.0, hi[0] + 72.0, lo[2] - 72.0, hi[2] + 72.0);
        out.push(BoundBox {
            cx: (x1 + x2) / 2.0,
            cy: (z1 + z2) / 2.0,
            a: (x2 - x1) / 2.0,
            b: (z2 - z1) / 2.0,
            y1: lo[1],
            y2: hi[1],
        });
    }
    out
}

/// A box rotated by `angle` quarter turns: (centre x, centre z, half x, half z).
fn rotated(b: &BoundBox, angle: u8) -> (f32, f32, f32, f32) {
    let a = f32::from(angle) * core::f32::consts::FRAC_PI_2;
    let (ca, sa) = (a.cos(), a.sin());
    let ccx = b.cx * ca + b.cy * sa;
    let ccy = b.cy * ca - b.cx * sa;
    if angle & 1 != 0 {
        (ccx, ccy, b.b, b.a)
    } else {
        (ccx, ccy, b.a, b.b)
    }
}

/// An object near a point: its shape, cell centre and base height.
struct Near<'a> {
    shape: &'a ObjectShape,
    ox: f32,
    oz: f32,
    land_y: f32,
    angle: u8,
}

impl Area {
    /// Objects in the `(2r+1)^2` cells around world (x, z).
    fn objects_near(&self, x: f32, z: f32, r: i32) -> impl Iterator<Item = Near<'_>> + '_ {
        let (ccx, ccz) = ((x / CELL) as i32, (z / CELL) as i32);
        (-r..=r)
            .flat_map(move |dz| (-r..=r).map(move |dx| (ccx + dx, ccz + dz)))
            .filter_map(move |(cx, cz)| {
                if cx < 0 || cz < 0 || cx >= MAP_SIZE as i32 || cz >= MAP_SIZE as i32 {
                    return None;
                }
                let i = idx(cx as usize, cz as usize);
                let ob = self.map.objects[i];
                if ob == NO_OBJECT {
                    return None;
                }
                let shape = self.shapes.get(usize::from(ob))?;
                Some(Near {
                    shape,
                    ox: cx as f32 * CELL + 128.0,
                    oz: cz as f32 * CELL + 128.0,
                    land_y: f32::from(self.map.object_height[i]) * H_SCALE,
                    angle: ((self.map.flags[i] >> cell_flags::OBJ_DIR_SHIFT) & 3) as u8,
                })
            })
    }

    /// `GetLandQHNoObj`: highest ground within ~128 units (keeps the camera
    /// above slopes).
    pub fn land_qh_no_obj(&self, x: f32, z: f32) -> f32 {
        const OFFS: [(f32, f32); 9] = [
            (0.0, 0.0),
            (-90.0, -90.0),
            (90.0, -90.0),
            (-90.0, 90.0),
            (90.0, 90.0),
            (128.0, 0.0),
            (-128.0, 0.0),
            (0.0, 128.0),
            (0.0, -128.0),
        ];
        OFFS.iter()
            .map(|(dx, dz)| self.land_height(x + dx, z + dz))
            .fold(f32::MIN, f32::max)
    }

    /// `GetLandQH`: like [`Self::land_qh_no_obj`] but also standing on objects
    /// whose top is within reach of `player_y`.
    pub fn land_qh(&self, x: f32, z: f32, player_y: f32) -> f32 {
        let mut h = self.land_qh_no_obj(x, z);
        for n in self.objects_near(x, z, 4) {
            let s = n.shape;
            if s.has(object_flags::BOUND) {
                // PointOnBound
                let (px, pz) = (x - n.ox, z - n.oz);
                let mut top: Option<f32> = None;
                for b in &s.bounds {
                    if b.y2 + n.land_y > player_y + 128.0 {
                        continue;
                    }
                    let (ccx, ccy, a, bb) = rotated(b, n.angle);
                    if (px - ccx).abs() < a && (pz - ccy).abs() < bb {
                        top = Some(top.map_or(b.y2, |t: f32| t.max(b.y2)));
                    }
                }
                if let Some(t) = top {
                    h = h.max(n.land_y + t);
                }
                continue;
            }
            if s.y_hi + n.land_y < h || s.y_hi + n.land_y > player_y + 128.0 {
                continue;
            }
            if dist(s, n.ox, n.oz, x, z) < s.radius - 1.0 {
                h = s.y_hi + n.land_y;
            }
        }
        h
    }

    /// `GetLandCeilH`: lowest overhead object, or ground + 20480.
    pub fn land_ceil_h(&self, x: f32, z: f32, player_y: f32) -> f32 {
        let mut h = self.land_height(x, z) + 20480.0;
        for n in self.objects_near(x, z, 4) {
            let s = n.shape;
            if s.has(object_flags::BOUND) {
                // PointUnBound
                let (px, pz) = (x - n.ox, z - n.oz);
                let mut bottom: Option<f32> = None;
                for b in &s.bounds {
                    if b.y1 + n.land_y < player_y + 128.0 {
                        continue;
                    }
                    let (ccx, ccy, a, bb) = rotated(b, n.angle);
                    if (px - ccx).abs() < a && (pz - ccy).abs() < bb {
                        bottom = Some(bottom.map_or(b.y1, |t: f32| t.min(b.y1)));
                    }
                }
                if let Some(t) = bottom {
                    h = h.min(n.land_y + t);
                }
                continue;
            }
            if s.y_lo + n.land_y > h || s.y_lo + n.land_y < player_y + 100.0 {
                continue;
            }
            if dist(s, n.ox, n.oz, x, z) < s.radius - 1.0 {
                h = s.y_lo + n.land_y;
            }
        }
        h
    }

    /// `GetLandUpH`: water surface if the cell has water, else the ground.
    pub fn land_up_h(&self, x: f32, z: f32) -> f32 {
        let (cx, cz) = ((x / CELL) as usize, (z / CELL) as usize);
        if cx < MAP_SIZE
            && cz < MAP_SIZE
            && self.map.flags[idx(cx, cz)] & cell_flags::WATER_ANY != 0
        {
            if let Some(l) = self.water_level(cx, cz) {
                return l;
            }
        }
        self.land_height(x, z)
    }

    /// `CheckCollision`: clamps to the playable square and pushes (x, z) out
    /// of objects at the height of `player_y`.
    pub fn check_collision(&self, x: &mut f32, z: &mut f32, player_y: f32) {
        *x = x.clamp(WALK_MIN, WALK_MAX);
        *z = z.clamp(WALK_MIN, WALK_MAX);
        let near: Vec<Near<'_>> = self.objects_near(*x, *z, 4).collect();
        for n in near {
            let s = n.shape;
            if s.has(object_flags::BOUND) {
                check_bound_collision(x, z, &n, player_y);
                continue;
            }
            if s.y_hi + n.land_y < player_y + 128.0 || s.y_lo + n.land_y > player_y + 256.0 {
                continue;
            }
            let (dx, dz) = (n.ox - *x, n.oz - *z);
            if s.has(object_flags::CIRCLE) {
                let r = (dx * dx + dz * dz).sqrt();
                if r < s.radius && r > 0.0 {
                    *x -= dx * (s.radius - r) / r;
                    *z -= dz * (s.radius - r) / r;
                }
            } else {
                let r = dx.abs().max(dz.abs());
                if r < s.radius && r > 0.0 {
                    if dx.abs() > dz.abs() {
                        *x -= dx * (s.radius - r) / r;
                    } else {
                        *z -= dz * (s.radius - r) / r;
                    }
                }
            }
        }
    }
}

fn dist(s: &ObjectShape, ox: f32, oz: f32, x: f32, z: f32) -> f32 {
    if s.has(object_flags::CIRCLE) {
        ((ox - x).powi(2) + (oz - z).powi(2)).sqrt()
    } else {
        (ox - x).abs().max((oz - z).abs())
    }
}

/// `CheckBoundCollision`: push out of the nearest edge of each box at
/// player height.
fn check_bound_collision(px: &mut f32, pz: &mut f32, n: &Near<'_>, player_y: f32) {
    let (ppx, ppy) = (*px - n.ox, *pz - n.oz);
    for b in &n.shape.bounds {
        if b.y2 + n.land_y < player_y + 128.0 || b.y1 + n.land_y > player_y + 256.0 {
            continue;
        }
        let (ccx, ccy, w, h) = rotated(b, n.angle);
        let (w, h) = (w + 2.0, h + 2.0);
        let dw = (ppx - ccx).abs() - w;
        let dh = (ppy - ccy).abs() - h;
        if dw > 0.0 || dh > 0.0 {
            continue;
        }
        if dw > dh {
            *px = n.ox + ccx + w * (ppx - ccx).signum();
        } else {
            *pz = n.oz + ccy + h * (ppy - ccy).signum();
        }
    }
}
