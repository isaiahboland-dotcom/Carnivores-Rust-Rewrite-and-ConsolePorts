//! Ray tests: `mathematics.cpp: TraceShot`, `TraceLook`, `TraceCheckPlane`,
//! `TraceModel`, `TraceCharacter`.

use c2_formats::car::Car;
use c2_formats::map::MAP_SIZE;
use c2_formats::model::face_flags;

use crate::area::{cell_flags, idx, Area, NO_OBJECT};
use crate::dino::{pose, Character, Species};
use crate::{CELL, H_SCALE};

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        a
    }
}

/// What a shot hit (`tres*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Ground,
    Model,
    Water,
    /// A character; `mortal` = a vital-zone face (`sfMortal`).
    Character {
        index: usize,
        mortal: bool,
    },
}

/// A ray from `a` toward `b`; `b` shrinks to the nearest hit.
struct Ray {
    a: V3,
    b: V3,
    nv: V3,
}

impl Ray {
    fn new(a: V3, b: V3) -> Self {
        Self {
            a,
            b,
            nv: norm(sub(b, a)),
        }
    }

    /// `TraceCheckPlane`: does the segment cross triangle (a, b, c) closer
    /// than the current end point? Moves the end point there if so.
    fn check_plane(&mut self, a: V3, b: V3, c: V3) -> bool {
        let pnv = norm(cross(sub(b, a), sub(c, a)));
        let sa = dot(sub(self.a, a), pnv);
        let sb = dot(sub(self.b, a), pnv);
        if sa * sb > -1.0 {
            return false;
        }
        let scvn = dot(self.nv, pnv);
        if scvn == 0.0 {
            return false;
        }
        let k = (sa / scvn).abs();
        let hp = [
            self.a[0] + self.nv[0] * k,
            self.a[1] + self.nv[1] * k,
            self.a[2] + self.nv[2] * k,
        ];
        if dot(cross(sub(b, a), sub(hp, a)), pnv) < 0.0
            || dot(cross(sub(c, b), sub(hp, b)), pnv) < 0.0
            || dot(cross(sub(a, c), sub(hp, c)), pnv) < 0.0
        {
            return false;
        }
        if len(sub(hp, self.a)) < len(sub(self.b, self.a)) {
            self.b = hp;
            return true;
        }
        false
    }

    /// `PointToVectorD`: distance from `c` to the ray's line.
    fn dist_to(&self, c: V3) -> f32 {
        len(cross(self.nv, sub(c, self.a)))
    }

    /// Cells to test: the ray's bounding box plus a 2-cell margin.
    fn cells(&self) -> impl Iterator<Item = (usize, usize)> {
        let ci = |v: f32| (v / CELL) as i32;
        let (x1, x2) = (
            ci(self.a[0]).min(ci(self.b[0])) - 2,
            ci(self.a[0]).max(ci(self.b[0])) + 2,
        );
        let (z1, z2) = (
            ci(self.a[2]).min(ci(self.b[2])) - 2,
            ci(self.a[2]).max(ci(self.b[2])) + 2,
        );
        (z1..=z2)
            .flat_map(move |z| (x1..=x2).map(move |x| (x, z)))
            .filter(|&(x, z)| (2..=1010).contains(&x) && (2..=1010).contains(&z))
            .map(|(x, z)| (x as usize, z as usize))
    }
}

fn ground_v(area: &Area, x: usize, z: usize) -> V3 {
    [
        x as f32 * CELL,
        f32::from(area.map.height[idx(x.min(MAP_SIZE - 1), z.min(MAP_SIZE - 1))]) * H_SCALE,
        z as f32 * CELL,
    ]
}

/// The two ground triangles of a cell, split along `fmReverse`.
fn ground_tris(area: &Area, x: usize, z: usize) -> [[V3; 3]; 2] {
    let g = |dx, dz| ground_v(area, x + dx, z + dz);
    if area.map.flags[idx(x, z)] & cell_flags::REVERSE != 0 {
        [[g(0, 0), g(1, 0), g(0, 1)], [g(0, 1), g(1, 0), g(1, 1)]]
    } else {
        [[g(0, 0), g(1, 0), g(1, 1)], [g(0, 0), g(1, 1), g(0, 1)]]
    }
}

/// `TraceModel`: an RSC object's solid faces. Note the original rotates by
/// `(FMap >> 2) & 7` eighth-turns here although it renders with `& 3` quarter
/// turns; reproduced as-is.
fn trace_model(ray: &mut Ray, area: &Area, x: usize, z: usize, ob: usize) -> bool {
    let Some(o) = area.rsc.objects.get(ob) else {
        return false;
    };
    let i = idx(x, z);
    let base = [
        x as f32 * CELL + 128.0,
        f32::from(area.map.object_height[i]) * H_SCALE,
        z as f32 * CELL + 128.0,
    ];
    if ray.dist_to([base[0], base[1] + 700.0, base[2]]) > 1400.0 {
        return false;
    }
    let malp = f32::from((area.map.flags[i] >> 2) & 7) * 2.0 * std::f32::consts::PI / 8.0;
    let (sa, ca) = malp.sin_cos();
    let m = &o.model.mesh;
    let verts: Vec<V3> = m
        .vertices
        .iter()
        .map(|v| {
            let (vx, vy, vz) = (v.x * 2.0, v.y * 2.0, v.z * -2.0);
            [
                vx * ca + vz * sa + base[0],
                vy + base[1],
                vz * ca - vx * sa + base[2],
            ]
        })
        .collect();
    let mut hit = false;
    for f in &m.faces {
        if f.flags & (face_flags::OPACITY | face_flags::TRANSPARENT) != 0 {
            continue;
        }
        let g = |k: usize| verts.get(f.v[k] as usize).copied();
        if let (Some(a), Some(b), Some(c)) = (g(0), g(1), g(2)) {
            hit |= ray.check_plane(a, b, c);
        }
    }
    hit
}

/// `TraceShot`: nearest hit from `a` toward `b`. Returns the hit and the
/// point just in front of it (16 units back, 32 for characters).
pub fn trace_shot(
    area: &Area,
    chars: &[Character],
    species: &[Species],
    cars: &[Option<&Car>],
    a: V3,
    b: V3,
) -> Option<(Hit, V3)> {
    let mut ray = Ray::new(a, b);
    let mut res = None;
    let cells: Vec<_> = ray.cells().collect();
    for (x, z) in cells {
        for t in ground_tris(area, x, z) {
            if ray.check_plane(t[0], t[1], t[2]) {
                res = Some(Hit::Ground);
            }
        }
        if area.map.flags[idx(x, z)] & cell_flags::WATER_ANY != 0 {
            if let Some(l) = area.water_level(x, z) {
                let w = |dx: usize, dz: usize| [(x + dx) as f32 * CELL, l, (z + dz) as f32 * CELL];
                if ray.check_plane(w(0, 0), w(1, 0), w(1, 1))
                    | ray.check_plane(w(0, 0), w(1, 1), w(0, 1))
                {
                    res = Some(Hit::Water);
                }
            }
        }
        let ob = area.map.objects[idx(x, z)];
        if ob != NO_OBJECT && trace_model(&mut ray, area, x, z, usize::from(ob)) {
            res = Some(Hit::Model);
        }
    }

    // TraceCharacter
    for (i, c) in chars.iter().enumerate() {
        if ray.dist_to(c.pos) > 1024.0 {
            continue;
        }
        let Some(car) = cars.get(c.ctype).copied().flatten() else {
            continue;
        };
        let anims = species.get(c.ctype).map_or(&[][..], |s| s.anims());
        let Some(verts) = pose::pose(c, car, anims) else {
            continue;
        };
        let al = -c.alpha + std::f32::consts::FRAC_PI_2;
        let (sa, ca) = al.sin_cos();
        let world: Vec<V3> = verts
            .iter()
            .map(|v| {
                [
                    v[0] * ca + v[2] * sa + c.pos[0],
                    v[1] + c.pos[1],
                    v[2] * ca - v[0] * sa + c.pos[2],
                ]
            })
            .collect();
        for f in &car.mesh.faces {
            if f.flags & (face_flags::OPACITY | face_flags::TRANSPARENT) != 0 {
                continue;
            }
            let g = |k: usize| world.get(f.v[k] as usize).copied();
            if let (Some(a), Some(b), Some(cc)) = (g(0), g(1), g(2)) {
                if ray.check_plane(a, b, cc) {
                    res = Some(Hit::Character {
                        index: i,
                        mortal: f.flags & face_flags::MORTAL != 0,
                    });
                }
            }
        }
    }

    let hit = res?;
    let back = if matches!(hit, Hit::Character { .. }) {
        32.0
    } else {
        16.0
    };
    Some((
        hit,
        [
            ray.b[0] - ray.nv[0] * back,
            ray.b[1] - ray.nv[1] * back,
            ray.b[2] - ray.nv[2] * back,
        ],
    ))
}

/// `TraceLook`: is the line from `a` to `b` blocked by terrain? Also counts
/// objects close to the line on the way (`ObjectsOnLook`, bigger ones count
/// twice) — cover that makes the hunter harder to see.
pub fn trace_look(area: &Area, a: V3, b: V3) -> (bool, i32) {
    let mut ray = Ray::new(a, b);
    let flat = norm([ray.nv[0], 0.0, ray.nv[2]]);
    let mut objects = 0;
    let cells: Vec<_> = ray.cells().collect();
    for (x, z) in cells {
        for t in ground_tris(area, x, z) {
            if ray.check_plane(t[0], t[1], t[2]) {
                return (true, objects);
            }
        }
        let ob = area.map.objects[idx(x, z)];
        if ob == NO_OBJECT {
            continue;
        }
        let mut v = [x as f32 * CELL + 128.0, ray.b[1], z as f32 * CELL + 128.0];
        let s1 = -dot(sub(v, ray.b), ray.nv);
        v[1] = ray.a[1];
        let s2 = dot(sub(v, ray.a), ray.nv);
        let flat_ray = Ray {
            a: ray.a,
            b: ray.b,
            nv: flat,
        };
        if s1 > 0.0 && s2 > 0.0 && flat_ray.dist_to(v) < 180.0 {
            objects += 1;
            if area
                .rsc
                .objects
                .get(usize::from(ob))
                .is_some_and(|o| o.info.radius * 2 > 32)
            {
                objects += 1;
            }
        }
    }
    (false, objects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shot_hits_flat_ground() {
        let area = Area::flat(10);
        let (hit, p) = trace_shot(
            &area,
            &[],
            &[],
            &[],
            [500.0 * 256.0, 2000.0, 500.0 * 256.0],
            [503.0 * 256.0, 0.0, 500.0 * 256.0],
        )
        .expect("hit");
        assert_eq!(hit, Hit::Ground);
        assert!((p[1] - 640.0).abs() < 20.0, "{p:?}");
    }

    #[test]
    fn shot_into_the_sky_misses() {
        let area = Area::flat(10);
        assert!(trace_shot(
            &area,
            &[],
            &[],
            &[],
            [500.0 * 256.0, 2000.0, 500.0 * 256.0],
            [510.0 * 256.0, 4000.0, 500.0 * 256.0]
        )
        .is_none());
    }

    #[test]
    fn look_is_blocked_by_a_hill() {
        let mut area = Area::flat(10);
        for z in 495..505 {
            area.map.height[idx(500, z)] = 200;
        }
        let a = [495.0 * 256.0, 900.0, 500.0 * 256.0];
        let b = [505.0 * 256.0, 900.0, 500.0 * 256.0];
        assert!(trace_look(&area, a, b).0);
        let c = [495.0 * 256.0, 900.0, 490.0 * 256.0];
        assert!(!trace_look(&area, a, c).0);
    }
}
