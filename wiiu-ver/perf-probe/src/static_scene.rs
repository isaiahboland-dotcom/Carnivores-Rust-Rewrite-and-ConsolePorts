//! Static terrain data and occupied scenery cells; independent of GX2.
use alloc::vec::Vec;
use c2_core::area::Area;

const SIDE: usize = 1024;

pub struct TerrainCache {
    // Center height and conservative radius, indexed by map cell.
    pub bounds: Vec<[f32; 2]>,
    // Texture -> quarter-turn -> corner (z * 2 + x) -> atlas UV.
    pub uvs: Vec<[[[f32; 2]; 4]; 4]>,
}
impl TerrainCache {
    pub fn new(area: &Area) -> Self {
        let radii: [f32; 256] = core::array::from_fn(|span| {
            let half_height = span as f32 * 32.0;
            libm::sqrtf(32768.0 + half_height * half_height)
        });
        let mut bounds = alloc::vec![[0.0; 2]; SIDE * SIDE];
        for z in 0..SIDE - 1 {
            for x in 0..SIDE - 1 {
                let i = z * SIDE + x;
                let h = [
                    area.map.height[i],
                    area.map.height[i + 1],
                    area.map.height[i + SIDE],
                    area.map.height[i + SIDE + 1],
                ];
                let low = *h.iter().min().unwrap();
                let high = *h.iter().max().unwrap();
                bounds[i] = [
                    (f32::from(low) + f32::from(high)) * 32.0,
                    radii[(high - low) as usize],
                ];
            }
        }
        let uvs = (0..area.rsc.textures.len().max(1))
            .map(|tex| {
                core::array::from_fn(|turn| {
                    core::array::from_fn(|corner| {
                        let (x, z) = (corner % 2, corner / 2);
                        let (u, v) = match turn {
                            1 => (z, 1 - x),
                            2 => (1 - x, 1 - z),
                            3 => (1 - z, x),
                            _ => (x, z),
                        };
                        [
                            ((tex % 16) as f32 * 128.0 + 0.9465 + u as f32 * 126.107) / 2048.0,
                            ((tex / 16) as f32 * 128.0 + 0.9465 + v as f32 * 126.107) / 2048.0,
                        ]
                    })
                })
            })
            .collect();
        Self { bounds, uvs }
    }

    // Retains the original 148x148 window, culling, topology and projection.
    // Each visible cell projects four corners once, then emits the same six
    // vertices. The caller provides space for 148*148*6 float4s in each stream.
    pub unsafe fn emit(
        &self,
        area: &Area,
        eye: [f32; 3],
        yaw: f32,
        pitch: f32,
        cell: (i32, i32),
        positions: *mut f32,
        attrs: *mut f32,
    ) -> u32 {
        let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
        let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
        let mut n = 0;
        for dz in -74i32..74 {
            for dx in -74i32..74 {
                let x = cell.0 + dx;
                let z = cell.1 + dz;
                if !(0..1023).contains(&x) || !(0..1023).contains(&z) {
                    continue;
                }
                let i = z as usize * SIDE + x as usize;
                let [y, radius] = self.bounds[i];
                if !visible_sphere(
                    x as f32 * 256.0 + 128.0 - eye[0],
                    y - eye[1],
                    z as f32 * 256.0 + 128.0 - eye[2],
                    radius,
                    sa,
                    ca,
                    sb,
                    cb,
                ) {
                    continue;
                }
                let flags = area.map.flags[i];
                let tex =
                    (area.map.texture1[i] as usize).min(area.rsc.textures.len().saturating_sub(1));
                let uvs = &self.uvs[tex][(flags & 3) as usize];
                let mut p = [[0.0; 4]; 4];
                let mut a = [[0.0; 4]; 4];
                for corner in 0..4 {
                    let (ox, oz) = ((corner % 2) as i32, (corner / 2) as i32);
                    let vi = (z + oz) as usize * SIDE + (x + ox) as usize;
                    let wx = (x + ox) as f32 * 256.0 - eye[0];
                    let wy = f32::from(area.map.height[vi]) * 64.0 - eye[1];
                    let wz = (z + oz) as f32 * 256.0 - eye[2];
                    let side = wx * ca + wz * sa;
                    let forward = wx * sa - wz * ca;
                    let up = wy * cb + forward * sb;
                    let depth = forward * cb - wy * sb;
                    p[corner] = [
                        side * 0.75,
                        up * 1.333,
                        (32768.0 / (32768.0 - 16.0)) * (depth - 16.0),
                        depth,
                    ];
                    a[corner] = [
                        uvs[corner][0],
                        uvs[corner][1],
                        f32::from(area.map.light[0][vi]) / 255.0,
                        1.0,
                    ];
                }
                let corners = if flags & 0x10 != 0 {
                    [0, 1, 2, 2, 1, 3]
                } else {
                    [0, 1, 3, 0, 3, 2]
                };
                for corner in corners {
                    core::ptr::copy_nonoverlapping(p[corner].as_ptr(), positions.add(n * 4), 4);
                    core::ptr::copy_nonoverlapping(a[corner].as_ptr(), attrs.add(n * 4), 4);
                    n += 1;
                }
            }
        }
        n as u32
    }
}

pub fn visible_sphere(
    wx: f32,
    wy: f32,
    wz: f32,
    radius: f32,
    sa: f32,
    ca: f32,
    sb: f32,
    cb: f32,
) -> bool {
    let side = wx * ca + wz * sa;
    let depth = (wx * sa - wz * ca) * cb - wy * sb;
    depth + radius >= 0.0 && (side * 0.75).abs() <= depth + radius * 1.25
}

#[derive(Default)]
pub struct SceneryCandidates {
    anchor: Option<(i32, i32)>,
    pub cells: Vec<usize>,
}
impl SceneryCandidates {
    // Preserve the previous near-first ring traversal exactly. Camera motion
    // only reruns culling over occupied cells; rebuild the list on cell changes.
    pub fn update(&mut self, objects: &[u8], model_count: usize, cell: (i32, i32)) {
        if self.anchor == Some(cell) {
            return;
        }
        self.cells.clear();
        for ring in 0i32..=74 {
            for dz in -ring..=ring {
                let step = if dz.abs() == ring {
                    1
                } else {
                    (ring * 2).max(1)
                };
                for dx in (-ring..=ring).step_by(step as usize) {
                    let (x, z) = (cell.0 + dx, cell.1 + dz);
                    if !(0..1024).contains(&x) || !(0..1024).contains(&z) {
                        continue;
                    }
                    let i = z as usize * SIDE + x as usize;
                    if (objects[i] as usize) < model_count {
                        self.cells.push(i);
                    }
                }
            }
        }
        self.anchor = Some(cell);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawRange {
    pub first: u32,
    pub count: u32,
}
#[derive(Clone, Copy)]
pub struct ResidentObject {
    pub center: [f32; 3],
    pub radius: f32,
    pub range: DrawRange,
}

// GPU ranges retain candidate order. Only merge adjacent visible spans: never
// submit invisible objects or reorder cutout faces to reduce draw calls.
pub fn resident_ranges(
    objects: &[ResidentObject],
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    out: &mut [DrawRange],
) -> (usize, u32) {
    assert!(out.len() >= objects.len());
    let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
    let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
    let mut n = 0;
    let mut vertices = 0;
    for object in objects {
        if object.range.count == 0
            || !visible_sphere(
                object.center[0] - eye[0],
                object.center[1] - eye[1],
                object.center[2] - eye[2],
                object.radius,
                sa,
                ca,
                sb,
                cb,
            )
        {
            continue;
        }
        vertices += object.range.count;
        if n > 0 && out[n - 1].first + out[n - 1].count == object.range.first {
            out[n - 1].count += object.range.count;
        } else {
            out[n] = object.range;
            n += 1;
        }
    }
    (n, vertices)
}

// Directly populate a resident stream without allocating/copying temporary
// CachedGeometry vectors per object. Same transform, face order and attributes
// as the legacy per-object CPU cache builder.
pub unsafe fn emit_scenery_model(
    model: &c2_core::model::PreparedModel,
    uvs: &[[[f32; 2]; 3]],
    light: f32,
    center: [f32; 3],
    turns: u8,
    scratch: &mut [[f32; 4]],
    positions: *mut f32,
    attrs: *mut f32,
) -> u32 {
    for (q, pos) in model.positions.iter().zip(scratch.iter_mut()) {
        let (x, z) = match turns {
            1 => (q[2], -q[0]),
            2 => (-q[0], -q[2]),
            3 => (-q[2], q[0]),
            _ => (q[0], q[2]),
        };
        *pos = [center[0] + x, center[1] + q[1], center[2] + z, 1.0];
    }
    let mut n = 0;
    for (face, uv) in model.faces.iter().zip(uvs) {
        for k in 0..3 {
            core::ptr::copy_nonoverlapping(
                scratch[face.v[k] as usize].as_ptr(),
                positions.add(n * 4),
                4,
            );
            let attr = [uv[k][0], uv[k][1], light, 1.0];
            core::ptr::copy_nonoverlapping(attr.as_ptr(), attrs.add(n * 4), 4);
            n += 1;
        }
    }
    n as u32
}

pub fn scenery_uvs(models: &[c2_core::model::PreparedModel]) -> Vec<Vec<[[f32; 2]; 3]>> {
    models
        .iter()
        .enumerate()
        .map(|(id, model)| {
            model
                .faces
                .iter()
                .map(|face| {
                    core::array::from_fn(|k| {
                        [
                            ((id % 8) as f32 * 256.0
                                + (face.uv[k][0] * 256.0).clamp(0.0, 255.0)
                                + 0.5)
                                / 2048.0,
                            ((id / 8) as f32 * 256.0
                                + (face.uv[k][1] * 256.0).clamp(0.0, 255.0)
                                + 0.5)
                                / 2048.0,
                        ]
                    })
                })
                .collect()
        })
        .collect()
}

// Reject oversized/overflowing windows before writing any resident GPU data.
pub fn resident_vertex_count(
    face_counts: impl IntoIterator<Item = usize>,
    capacity: usize,
) -> Option<usize> {
    let mut vertices = 0usize;
    for faces in face_counts {
        vertices = vertices.checked_add(faces.checked_mul(3)?)?;
        if vertices > capacity {
            return None;
        }
    }
    Some(vertices)
}

// World-space terrain and water for the original 148x148 window, kept on the
// GPU while the camera turns; the shader's world mode does the projection that
// TerrainCache::emit does on the CPU. Slots are toroidal (cell mod 148), so a
// cell crossing rewrites only the rows/columns entering the window. Slots are
// grouped into 4x4 blocks, contiguous in the buffers, for culling and draws.
pub const WINDOW: i32 = 148;
const BLOCK: usize = 4;
pub const BLOCKS_PER_SIDE: usize = WINDOW as usize / BLOCK;
pub const BLOCK_COUNT: usize = BLOCKS_PER_SIDE * BLOCKS_PER_SIDE;
pub const BLOCK_VERTICES: u32 = (BLOCK * BLOCK * 6) as u32;

#[derive(Clone, Copy, Default)]
struct TerrainBlock {
    center: [f32; 3],
    radius: f32,
    water: bool,
}

#[derive(Default)]
pub struct ResidentTerrain {
    anchor: Option<(i32, i32)>,
    blocks: Vec<TerrainBlock>,
    dirty: Vec<bool>,
}

// The world cell held by slot `s` of the window starting at `low`.
fn slot_cell(low: i32, s: i32) -> i32 {
    low + (s - low).rem_euclid(WINDOW)
}

fn slot_vertex(sx: usize, sz: usize) -> usize {
    let block = (sz / BLOCK) * BLOCKS_PER_SIDE + sx / BLOCK;
    (block * BLOCK * BLOCK + (sz % BLOCK) * BLOCK + sx % BLOCK) * 6
}

impl ResidentTerrain {
    pub fn invalidate(&mut self) {
        self.anchor = None;
    }

    // Write the slots whose cells changed since the last anchor and push the
    // vertex ranges written (merged, ascending) into `written`. Returns the
    // number of cells rewritten; zero when the anchor is unchanged.
    pub unsafe fn update(
        &mut self,
        area: &Area,
        cache: &TerrainCache,
        cell: (i32, i32),
        terrain: (*mut f32, *mut f32),
        water: (*mut f32, *mut f32),
        written: &mut Vec<DrawRange>,
    ) -> u32 {
        written.clear();
        if self.anchor == Some(cell) {
            return 0;
        }
        if self.blocks.len() != BLOCK_COUNT {
            self.blocks = alloc::vec![TerrainBlock::default(); BLOCK_COUNT];
            self.dirty = alloc::vec![false; BLOCK_COUNT];
        }
        let low = (cell.0 - WINDOW / 2, cell.1 - WINDOW / 2);
        let old = self.anchor.map(|c| (c.0 - WINDOW / 2, c.1 - WINDOW / 2));
        let changed = |axis: usize, s: i32| match old {
            Some(o) => {
                let (n, o) = if axis == 0 { (low.0, o.0) } else { (low.1, o.1) };
                slot_cell(n, s) != slot_cell(o, s)
            }
            None => true,
        };
        let columns: Vec<bool> = (0..WINDOW).map(|s| changed(0, s)).collect();
        let rows: Vec<bool> = (0..WINDOW).map(|s| changed(1, s)).collect();
        let mut cells = 0;
        for sz in 0..WINDOW as usize {
            for sx in 0..WINDOW as usize {
                if !columns[sx] && !rows[sz] {
                    continue;
                }
                let x = slot_cell(low.0, sx as i32);
                let z = slot_cell(low.1, sz as i32);
                let v = slot_vertex(sx, sz);
                self.write_cell(area, cache, x, z, terrain, water, v);
                self.dirty[v / BLOCK_VERTICES as usize] = true;
                cells += 1;
            }
        }
        for b in 0..BLOCK_COUNT {
            if !self.dirty[b] {
                continue;
            }
            self.dirty[b] = false;
            self.blocks[b] = block_bounds(area, cache, low, b);
            let first = b as u32 * BLOCK_VERTICES;
            match written.last_mut() {
                Some(r) if r.first + r.count == first => r.count += BLOCK_VERTICES,
                _ => written.push(DrawRange { first, count: BLOCK_VERTICES }),
            }
        }
        self.anchor = Some(cell);
        cells
    }

    // Same corners, triangulation, UVs and light as TerrainCache::emit, and
    // the same water quads as c2_water_vertices, before projection. Cells
    // outside the map are degenerate (all zero) triangles.
    unsafe fn write_cell(
        &self,
        area: &Area,
        cache: &TerrainCache,
        x: i32,
        z: i32,
        terrain: (*mut f32, *mut f32),
        water: (*mut f32, *mut f32),
        v: usize,
    ) {
        let (tp, ta) = (terrain.0.add(v * 4), terrain.1.add(v * 4));
        let (wp, wa) = (water.0.add(v * 4), water.1.add(v * 4));
        for p in [tp, ta, wp, wa] {
            core::ptr::write_bytes(p, 0, 24);
        }
        if !(0..1023).contains(&x) || !(0..1023).contains(&z) {
            return;
        }
        let i = z as usize * SIDE + x as usize;
        let flags = area.map.flags[i];
        let tex = (area.map.texture1[i] as usize).min(area.rsc.textures.len().saturating_sub(1));
        let uvs = &cache.uvs[tex][(flags & 3) as usize];
        let corners = if flags & 0x10 != 0 { [0, 1, 2, 2, 1, 3] } else { [0, 1, 3, 0, 3, 2] };
        for (n, corner) in corners.into_iter().enumerate() {
            let (ox, oz) = ((corner % 2) as i32, (corner / 2) as i32);
            let vi = (z + oz) as usize * SIDE + (x + ox) as usize;
            let p = [(x + ox) as f32 * 256.0, f32::from(area.map.height[vi]) * 64.0, (z + oz) as f32 * 256.0, 1.0];
            let a = [uvs[corner][0], uvs[corner][1], f32::from(area.map.light[0][vi]) / 255.0, 1.0];
            core::ptr::copy_nonoverlapping(p.as_ptr(), tp.add(n * 4), 4);
            core::ptr::copy_nonoverlapping(a.as_ptr(), ta.add(n * 4), 4);
        }
        let Some((level, tex)) = water_cell(area, x, z) else { return; };
        for (n, (ox, oz)) in [(0, 0), (1, 0), (1, 1), (0, 0), (1, 1), (0, 1)].into_iter().enumerate() {
            let p = [(x + ox) as f32 * 256.0, level, (z + oz) as f32 * 256.0, 1.0];
            let u = ((tex % 16) as f32 * 128.0 + 0.9465 + ox as f32 * 126.107) / 2048.0;
            let v = ((tex / 16) as f32 * 128.0 + 0.9465 + oz as f32 * 126.107) / 2048.0;
            let a = [u, v, 1.0, 0.0];
            core::ptr::copy_nonoverlapping(p.as_ptr(), wp.add(n * 4), 4);
            core::ptr::copy_nonoverlapping(a.as_ptr(), wa.add(n * 4), 4);
        }
    }

    // Visible blocks as merged draw ranges: (terrain ranges, terrain vertices,
    // water ranges, water vertices). Outputs need BLOCK_COUNT entries each.
    pub fn ranges(
        &self,
        eye: [f32; 3],
        yaw: f32,
        pitch: f32,
        terrain: &mut [DrawRange],
        water: &mut [DrawRange],
    ) -> (usize, u32, usize, u32) {
        assert!(terrain.len() >= self.blocks.len() && water.len() >= self.blocks.len());
        let (sa, ca) = (libm::sinf(yaw), libm::cosf(yaw));
        let (sb, cb) = (libm::sinf(pitch), libm::cosf(pitch));
        let (mut tn, mut tv, mut wn, mut wv) = (0, 0, 0, 0);
        for (b, block) in self.blocks.iter().enumerate() {
            if block.radius <= 0.0
                || !visible_sphere(
                    block.center[0] - eye[0],
                    block.center[1] - eye[1],
                    block.center[2] - eye[2],
                    block.radius,
                    sa,
                    ca,
                    sb,
                    cb,
                )
            {
                continue;
            }
            let range = DrawRange { first: b as u32 * BLOCK_VERTICES, count: BLOCK_VERTICES };
            push_range(terrain, &mut tn, range);
            tv += BLOCK_VERTICES;
            if block.water {
                push_range(water, &mut wn, range);
                wv += BLOCK_VERTICES;
            }
        }
        (tn, tv, wn, wv)
    }
}

fn push_range(out: &mut [DrawRange], n: &mut usize, range: DrawRange) {
    if *n > 0 && out[*n - 1].first + out[*n - 1].count == range.first {
        out[*n - 1].count += range.count;
    } else {
        out[*n] = range;
        *n += 1;
    }
}

// Water level and atlas texture for a cell whose four corners are all water.
pub fn water_cell(area: &Area, x: i32, z: i32) -> Option<(f32, usize)> {
    let i = z as usize * SIDE + x as usize;
    let water = area.rsc.waters.get(area.map.water[i] as usize)?;
    if [i + 1, i + SIDE, i + SIDE + 1]
        .iter()
        .any(|&j| area.map.water[j] == c2_core::area::NO_WATER)
    {
        return None;
    }
    let level = area.water_level(x as usize, z as usize)?;
    let tex = (water.texture_index.max(0) as usize).min(area.rsc.textures.len().saturating_sub(1));
    Some((level, tex))
}

// A sphere around everything a block holds: each cell's terrain bound and
// water quad. Blocks that straddle the window seam hold cells from both
// edges; their sphere is larger but still conservative.
fn block_bounds(area: &Area, cache: &TerrainCache, low: (i32, i32), block: usize) -> TerrainBlock {
    let (bx, bz) = (block % BLOCKS_PER_SIDE * BLOCK, block / BLOCKS_PER_SIDE * BLOCK);
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut water = false;
    let mut grow = |p: [f32; 3], r: f32| {
        for k in 0..3 {
            min[k] = min[k].min(p[k] - r);
            max[k] = max[k].max(p[k] + r);
        }
    };
    for sz in bz..bz + BLOCK {
        for sx in bx..bx + BLOCK {
            let x = slot_cell(low.0, sx as i32);
            let z = slot_cell(low.1, sz as i32);
            if !(0..1023).contains(&x) || !(0..1023).contains(&z) {
                continue;
            }
            let [y, radius] = cache.bounds[z as usize * SIDE + x as usize];
            let center = [x as f32 * 256.0 + 128.0, y, z as f32 * 256.0 + 128.0];
            grow(center, radius);
            if let Some((level, _)) = water_cell(area, x, z) {
                water = true;
                grow([center[0], level, center[2]], 182.0);
            }
        }
    }
    if min[0] > max[0] {
        return TerrainBlock::default();
    }
    let center = core::array::from_fn(|k| (min[k] + max[k]) * 0.5);
    let half: [f32; 3] = core::array::from_fn(|k| (max[k] - min[k]) * 0.5);
    let radius = libm::sqrtf(half[0] * half[0] + half[1] * half[1] + half[2] * half[2]);
    TerrainBlock { center, radius, water }
}

// Resident scenery across cell crossings. A crossing drops the objects that
// left the 149x149 window, keeping their vertex ranges as free gaps, and writes
// the objects that entered into gaps of their size (models repeat, so most fit
// exactly), else the first larger gap, else after the used part. Only when none
// of that fits is the window rebuilt from the start, in the original near-first
// order. Objects are kept sorted by buffer position so visible neighbours merge
// into one draw. Order only affects equal-depth ties; blending is off.
#[derive(Default)]
pub struct ResidentScenery {
    anchor: Option<(i32, i32)>,
    tail: u32,
    pub objects: Vec<ResidentObject>,
    free: Vec<DrawRange>,
    entering: Vec<(usize, u32)>,
}

fn in_window(cell: (i32, i32), x: i32, z: i32) -> bool {
    (x - cell.0).abs() <= 74 && (z - cell.1).abs() <= 74
}

// Sort and merge free ranges; a gap at the end of the used part shortens it.
fn coalesce(free: &mut Vec<DrawRange>, tail: &mut u32) {
    free.sort_unstable_by_key(|r| r.first);
    let mut merged: Vec<DrawRange> = Vec::with_capacity(free.len());
    for r in free.drain(..) {
        match merged.last_mut() {
            Some(m) if m.first + m.count == r.first => m.count += r.count,
            _ => merged.push(r),
        }
    }
    while merged.last().is_some_and(|m| m.first + m.count == *tail) {
        *tail = merged.pop().unwrap().first;
    }
    *free = merged;
}

// Take `count` vertices: an exact gap, else the first larger one, else the tail.
fn allocate(free: &mut Vec<DrawRange>, tail: &mut u32, count: u32, capacity: u32) -> Option<u32> {
    let exact = free.iter().position(|r| r.count == count);
    if let Some(i) = exact.or_else(|| free.iter().position(|r| r.count > count)) {
        let first = free[i].first;
        if free[i].count == count {
            free.remove(i);
        } else {
            free[i].first += count;
            free[i].count -= count;
        }
        return Some(first);
    }
    if *tail as u64 + count as u64 > capacity as u64 {
        return None;
    }
    *tail += count;
    Some(*tail - count)
}

impl ResidentScenery {
    pub fn invalidate(&mut self) {
        self.anchor = None;
        self.objects.clear();
        self.free.clear();
        self.tail = 0;
    }

    /// Bring the window to `cell`, writing vertex ranges into `written`
    /// (merged, ascending). Returns whether it was rebuilt from the start, or
    /// None if the window cannot fit even then (no objects are resident).
    /// `faces` gives a cell's object face count; `emit` writes a cell's
    /// object at a vertex offset and describes it.
    pub fn update(
        &mut self,
        candidates: &mut SceneryCandidates,
        objects: &[u8],
        model_count: usize,
        cell: (i32, i32),
        capacity: u32,
        faces: impl Fn(usize) -> usize,
        mut emit: impl FnMut(usize, u32) -> ResidentObject,
        written: &mut Vec<DrawRange>,
    ) -> Option<bool> {
        written.clear();
        if self.anchor == Some(cell) {
            return Some(false);
        }
        if let Some(old) = self.anchor.filter(|old| in_window(*old, cell.0, cell.1)) {
            let mut free = self.free.clone();
            let mut tail = self.tail;
            for o in &self.objects {
                if !in_window(cell, (o.center[0] / 256.0) as i32, (o.center[2] / 256.0) as i32) {
                    free.push(o.range);
                }
            }
            coalesce(&mut free, &mut tail);
            self.entering.clear();
            let mut fits = true;
            'cells: for z in (cell.1 - 74).max(0)..=(cell.1 + 74).min(SIDE as i32 - 1) {
                for x in (cell.0 - 74).max(0)..=(cell.0 + 74).min(SIDE as i32 - 1) {
                    let i = z as usize * SIDE + x as usize;
                    if in_window(old, x, z) || (objects[i] as usize) >= model_count {
                        continue;
                    }
                    match allocate(&mut free, &mut tail, faces(i) as u32 * 3, capacity) {
                        Some(first) => self.entering.push((i, first)),
                        None => {
                            fits = false;
                            break 'cells;
                        }
                    }
                }
            }
            if fits {
                self.objects.retain(|o| {
                    in_window(cell, (o.center[0] / 256.0) as i32, (o.center[2] / 256.0) as i32)
                });
                for &(i, first) in &self.entering {
                    let object = emit(i, first);
                    written.push(object.range);
                    self.objects.push(object);
                }
                self.objects.sort_unstable_by_key(|o| o.range.first);
                self.free = free;
                self.tail = tail;
                coalesce(written, &mut 0);
                self.anchor = Some(cell);
                return Some(false);
            }
        }
        candidates.update(objects, model_count, cell);
        self.invalidate();
        resident_vertex_count(candidates.cells.iter().map(|&i| faces(i)), capacity as usize)?;
        let mut n = 0;
        for &i in &candidates.cells {
            let object = emit(i, n);
            n += object.range.count;
            self.objects.push(object);
        }
        self.tail = n;
        self.anchor = Some(cell);
        written.push(DrawRange { first: 0, count: n });
        Some(true)
    }
}
