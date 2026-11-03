//! Reachable terrain, spawn-box fencing and routes toward the hunter.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use crate::area::Area;

/// Height steps (the map's, 64 units each) a fenced wave animal may stand
/// above the lowest way between its spawn box and the hunter.
const FENCE_MARGIN: u8 = 4;
/// Cells around the hunter and the spawn box the fence covers.
const FENCE_PAD: i32 = 40;

/// The ground between a survival's spawn box and the hunter: the cells
/// reachable from him without climbing more than [`FENCE_MARGIN`] above the
/// highest point of the lowest way to the box (a ravine's floor, not its
/// walls), so the waves come straight on (the owner's ask). Outside its
/// window, nothing.
#[derive(Debug, Clone)]
pub(super) struct Fence {
    x0: i32,
    z0: i32,
    w: i32,
    ok: Vec<bool>,
}

impl Fence {
    pub(super) fn new(area: &Area, hunter: (i32, i32), spawn: [f32; 4]) -> Self {
        let cell = |v: f32| (v / 256.0) as i32;
        let (sx0, sx1) = (cell(spawn[0] - spawn[2]), cell(spawn[0] + spawn[2]));
        let (sz0, sz1) = (cell(spawn[1] - spawn[3]), cell(spawn[1] + spawn[3]));
        let x0 = (hunter.0.min(sx0) - FENCE_PAD).max(4);
        let z0 = (hunter.1.min(sz0) - FENCE_PAD).max(4);
        let x1 = (hunter.0.max(sx1) + FENCE_PAD).min(1019);
        let z1 = (hunter.1.max(sz1) + FENCE_PAD).min(1019);
        let (w, h) = (x1 - x0 + 1, z1 - z0 + 1);
        let at = |x: i32, z: i32| ((z - z0) * w + (x - x0)) as usize;
        let height = |x: i32, z: i32| area.map.height[crate::area::idx(x as usize, z as usize)];
        // The lowest-ceiling way from the hunter to every cell (minimax).
        let mut top = vec![u8::MAX; (w * h) as usize];
        let mut heap = alloc::collections::BinaryHeap::new();
        let start = (hunter.0.clamp(x0, x1), hunter.1.clamp(z0, z1));
        top[at(start.0, start.1)] = height(start.0, start.1);
        heap.push(core::cmp::Reverse((
            top[at(start.0, start.1)],
            start.0,
            start.1,
        )));
        while let Some(core::cmp::Reverse((t, x, z))) = heap.pop() {
            if t > top[at(x, z)] {
                continue;
            }
            for (dx, dz) in AROUND {
                let (nx, nz) = (x + dx, z + dz);
                if nx < x0 || nz < z0 || nx > x1 || nz > z1 {
                    continue;
                }
                let nt = t.max(height(nx, nz));
                if nt < top[at(nx, nz)] {
                    top[at(nx, nz)] = nt;
                    heap.push(core::cmp::Reverse((nt, nx, nz)));
                }
            }
        }
        let centre = (cell(spawn[0]).clamp(x0, x1), cell(spawn[1]).clamp(z0, z1));
        let limit = top[at(centre.0, centre.1)].saturating_add(FENCE_MARGIN);
        Self {
            x0,
            z0,
            w,
            ok: top.iter().map(|&t| t <= limit).collect(),
        }
    }

    /// Whether cell (`x`, `z`) is in.
    pub(super) fn allows(&self, x: i32, z: i32) -> bool {
        let (i, j) = (x - self.x0, z - self.z0);
        (0..self.w).contains(&i)
            && j >= 0
            && self
                .ok
                .get((j * self.w + i) as usize)
                .copied()
                .unwrap_or(false)
    }

    pub(super) fn allows_pos(&self, x: f32, z: f32) -> bool {
        self.allows((x / 256.0) as i32, (z / 256.0) as i32)
    }
}

/// Cells of the window a [`Flow`] covers (a side).
const FLOW_W: i32 = 192;
/// Closer than this (cells), wave animals go straight for the hunter.
/// Farther, they do too when the straight line is a way (no ridge or
/// ravine wall across it); else along the way.
const LURE_NEAR: u16 = 3;
/// How far along the way a lure is (cells).
const LURE_AHEAD: usize = 8;
/// A lure is never nearer the hunter than this (cells): the AIs measure
/// their kill distance to it.
const LURE_GAP: u16 = 3;
/// How far (cells) an animal that stands where there is no way (a slope
/// too steep, up a mountain side) looks for ground that has one.
const LURE_RESCUE: i32 = 10;
/// The slopes a way takes: this share of the steepest an animal can climb
/// (`slope_too_steep`), so the way keeps off ground it barely holds on
/// (owner, 2026-11-01: waves climbed the ravine's sides and stuck).
const FLOW_SLOPE: f32 = 0.8;
const AROUND: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Steps (cells) from each cell of a window to the hunter's, over ground an
/// animal can walk (no water, no steeper than `CheckPlaceCollision`
/// allows): the AIs only steer around what is just ahead, so a ridge
/// between a wave and the hunter would hold it up for good.
#[derive(Debug, Clone, Default)]
pub(super) struct Flow {
    /// The window's first cell, and the hunter's cell it was made for.
    x0: i32,
    z0: i32,
    pub(super) from: (i32, i32),
    dist: Vec<u16>,
    /// Local terrain height; NaN marks ground rejected by the movement slope probes.
    heights: Vec<f32>,
}

impl Flow {
    pub(super) fn new(area: &Area, (hx, hz): (i32, i32), fence: Option<&Fence>) -> Self {
        let (x0, z0) = (hx - FLOW_W / 2, hz - FLOW_W / 2);
        let n = (FLOW_W * FLOW_W) as usize;
        let inside = |x: i32, z: i32| (4..1020).contains(&x) && (4..1020).contains(&z);
        let at = |x: i32, z: i32| ((z - z0) * FLOW_W + (x - x0)) as usize;
        let mut h = vec![f32::NAN; n];
        for z in z0..z0 + FLOW_W {
            for x in x0..x0 + FLOW_W {
                if !inside(x, z) {
                    continue;
                }
                let wet = area.map.flags[crate::area::idx(x as usize, z as usize)]
                    & crate::area::cell_flags::WATER
                    != 0;
                if !wet && fence.is_none_or(|f| f.allows(x, z)) {
                    let (wx, wz) = (x as f32 * 256.0 + 128.0, z as f32 * 256.0 + 128.0);
                    let y = area.land_height(wx, wz);
                    // Match movement's local slope probes. An average across
                    // neighbouring cell centres can hide a steep triangle.
                    let safe = [(-64.0, -64.0), (64.0, -64.0), (-64.0, 64.0), (64.0, 64.0)]
                        .into_iter()
                        .all(|(dx, dz)| {
                            (area.land_height(wx + dx, wz + dz) - y).abs() <= 100.0 * FLOW_SLOPE
                        });
                    if safe {
                        h[at(x, z)] = y;
                    }
                }
            }
        }
        let mut dist = vec![u16::MAX; n];
        let mut queue = alloc::collections::VecDeque::new();
        if inside(hx, hz) {
            dist[at(hx, hz)] = 0;
            queue.push_back((hx, hz));
        }
        while let Some((x, z)) = queue.pop_front() {
            let (d, hh) = (dist[at(x, z)], h[at(x, z)]);
            for (dx, dz) in AROUND {
                let (nx, nz) = (x + dx, z + dz);
                if nx < x0 || nz < z0 || nx >= x0 + FLOW_W || nz >= z0 + FLOW_W {
                    continue;
                }
                let j = at(nx, nz);
                // 100 up per 64 across, as `slope_too_steep`; the start
                // (the hunter's cell) may be anything.
                let run = if dx != 0 && dz != 0 { 362.0 } else { 256.0 };
                let steep = d > 0 && (h[j] - hh).abs() > run * 100.0 / 64.0 * FLOW_SLOPE;
                let corner = dx != 0 && dz != 0 && (h[at(nx, z)].is_nan() || h[at(x, nz)].is_nan());
                if dist[j] == u16::MAX && !h[j].is_nan() && !steep && !corner {
                    dist[j] = d + 1;
                    queue.push_back((nx, nz));
                }
            }
        }
        Self {
            x0,
            z0,
            from: (hx, hz),
            dist,
            heights: h,
        }
    }

    /// Whether cell (`x`, `z`) is in the window (else [`Flow::get`] knows
    /// nothing of it).
    fn covers(&self, x: i32, z: i32) -> bool {
        (0..FLOW_W).contains(&(x - self.x0)) && (0..FLOW_W).contains(&(z - self.z0))
    }

    pub(super) fn get(&self, x: i32, z: i32) -> u16 {
        let (i, j) = (x - self.x0, z - self.z0);
        if (0..FLOW_W).contains(&i) && (0..FLOW_W).contains(&j) {
            self.dist[(j * FLOW_W + i) as usize]
        } else {
            u16::MAX
        }
    }

    /// Whether the straight line from cell (`x`, `z`) to the hunter is a
    /// way: every cell on it has one, none farther than where it starts
    /// (a cell across a ridge or a ravine wall is, or has none).
    fn straight(&self, x: i32, z: i32) -> bool {
        self.segment((x, z), self.from)
    }

    /// Only skip waypoints when the whole segment is traversable. Merely
    /// knowing both endpoints reach the hunter can cut through a cliff.
    fn segment(&self, (x, z): (i32, i32), (tx, tz): (i32, i32)) -> bool {
        let height = |x, z| {
            if self.covers(x, z) {
                self.heights[((z - self.z0) * FLOW_W + x - self.x0) as usize]
            } else {
                f32::NAN
            }
        };
        let n = (tx - x).abs().max((tz - z).abs());
        let mut prev = (x, z);
        for k in 1..=n {
            let f = k as f32 / n as f32;
            let next = (
                x + ((tx - x) as f32 * f).round() as i32,
                z + ((tz - z) as f32 * f).round() as i32,
            );
            if self.get(next.0, next.1) == u16::MAX {
                return false;
            }
            let diagonal = next.0 != prev.0 && next.1 != prev.1;
            let rise = (height(next.0, next.1) - height(prev.0, prev.1)).abs();
            let run = if diagonal { 362.0 } else { 256.0 };
            if !rise.is_finite()
                || rise > run * 100.0 / 64.0 * FLOW_SLOPE
                || (diagonal
                    && (height(prev.0, next.1).is_nan() || height(next.0, prev.1).is_nan()))
            {
                return false;
            }
            prev = next;
        }
        true
    }

    /// Where an animal at cell (`x`, `z`) should head: [`LURE_AHEAD`] cells
    /// along the way; off any way (on a slope too steep, outside the
    /// fence), the nearest ground with one; `None` (near, a straight way,
    /// or nothing known: straight on).
    pub(super) fn lure(&self, mut x: i32, mut z: i32) -> Option<(f32, f32)> {
        let centre = |x: i32, z: i32| (x as f32 * 256.0 + 128.0, z as f32 * 256.0 + 128.0);
        let d = self.get(x, z);
        if d == u16::MAX {
            if !self.covers(x, z) {
                return None;
            }
            // Down to the nearest cell with a way (the closest to the
            // hunter's of those as near).
            let near = (1..=LURE_RESCUE).find_map(|r| {
                (-r..=r)
                    .flat_map(|i| {
                        [
                            (x + i, z - r),
                            (x + i, z + r),
                            (x - r, z + i),
                            (x + r, z + i),
                        ]
                    })
                    .filter(|&(cx, cz)| self.get(cx, cz) != u16::MAX)
                    .min_by_key(|&(cx, cz)| self.get(cx, cz))
            })?;
            return Some(centre(near.0, near.1));
        }
        if d <= LURE_NEAR || self.straight(x, z) {
            return None;
        }
        let start = (x, z);
        for _ in 0..LURE_AHEAD {
            let here = self.get(x, z);
            let next = AROUND
                .into_iter()
                .map(|(dx, dz)| (x + dx, z + dz))
                .filter(|&next| self.segment((x, z), next))
                .min_by_key(|&(nx, nz)| self.get(nx, nz))?;
            let nd = self.get(next.0, next.1);
            if nd >= here || nd < LURE_GAP || !self.segment(start, next) {
                break;
            }
            (x, z) = next;
        }
        Some(centre(x, z))
    }
}

#[cfg(test)]
mod flow_tests {
    use super::{Flow, FLOW_W};
    use crate::area::Area;

    /// A flat area with a wall (cells x 500..=504, z below 560) between
    /// the hunter (480, 500) and the animals east of it.
    fn walled() -> Area {
        let mut a = Area::flat(10);
        for z in 0..560 {
            for x in 500..=504 {
                a.map.height[crate::area::idx(x, z)] = 250;
            }
        }
        a
    }

    #[test]
    fn open_ground_is_straight_on() {
        let f = Flow::new(&Area::flat(10), (480, 500), None);
        assert_eq!(f.lure(530, 500), None);
    }

    #[test]
    fn waypoints_near_a_ridge_do_not_cut_across_it() {
        let f = Flow::new(&walled(), (480, 500), None);
        for x in 505..514 {
            for z in 552..564 {
                if f.get(x, z) == u16::MAX {
                    continue;
                }
                if let Some((wx, wz)) = f.lure(x, z) {
                    assert!(f.segment((x, z), ((wx / 256.0) as i32, (wz / 256.0) as i32)));
                } else {
                    assert!(f.straight(x, z));
                }
            }
        }
    }

    #[test]
    fn a_steep_triangle_is_not_hidden_by_cell_average_height() {
        let mut area = Area::flat(10);
        area.map.height[crate::area::idx(510, 510)] = 30;
        let f = Flow::new(&area, (500, 500), None);
        assert!(f.heights[((510 - f.z0) * FLOW_W + 510 - f.x0) as usize].is_nan());
    }

    #[test]
    fn a_wall_is_walked_round() {
        let f = Flow::new(&walled(), (480, 500), None);
        let (lx, lz) = f.lure(530, 500).expect("a way round the wall");
        // Towards the wall's end (z 560), not into it.
        assert!(lz > 500.0 * 256.0, "{lx} {lz}");
        // The wall itself has no way: an animal stuck on it comes down.
        assert_eq!(f.get(502, 500), u16::MAX);
        let (dx, _) = f.lure(502, 500).expect("down off the wall");
        let cell = (dx / 256.0) as i32;
        assert!(!(500..=504).contains(&cell), "{cell}");
    }
}
