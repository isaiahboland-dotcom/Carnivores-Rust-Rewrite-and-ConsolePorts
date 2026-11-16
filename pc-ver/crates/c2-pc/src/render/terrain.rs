//! Terrain and water, built in the vertex shader from per-cell data.
use super::frustum::Frustum;
use super::reflection::Reflection;
use super::*;

// ---------------------------------------------------------------- terrain

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainParams {
    origin: [i32; 2],
    width: u32,
    layers: u32,
    time_ms: u32,
    clouds: u32,
    levels: u32,
    /// Cells drawn from `eye` (each axis): the coarsest level stops here.
    reach: u32,
    eye: [i32; 2],
    waves: u32,
    pad: u32,
}

/// Half-width, in cells, of the full-detail square when the ground is
/// drawn in levels; each level out doubles it and its cells' size.
pub const LOD_FINE_CELLS: u32 = 128;

/// The window for `radius` cells around `center`: the finest level's
/// half-width, the centre it is laid around and the number of levels.
/// Without `lod`, or within [`LOD_FINE_CELLS`], one level as the original
/// draws it: every cell, centred on the camera's.
pub fn window(center: (i32, i32), radius: u32, lod: bool) -> (u32, (i32, i32), u32) {
    if !lod || radius <= LOD_FINE_CELLS {
        return (radius, center, 1);
    }
    let mut levels = 1;
    while LOD_FINE_CELLS << (levels - 1) < radius {
        levels += 1;
    }
    // Every level's cells sit on its own grid: centre on the coarsest's.
    let s = 1 << (levels - 1);
    let snap = |v: i32| v.div_euclid(s) * s;
    (LOD_FINE_CELLS, (snap(center.0), snap(center.1)), levels)
}

/// Whether the square of `span` cells at `o` overlaps the square of
/// `reach.1` cells around cell `reach.0` (the radius asked for: the
/// coarsest level is wider, `shaders/terrain.wgsl: lod_cell` keeps the same
/// cells).
fn within_reach(o: (i32, i32), span: i32, reach: ((i32, i32), i32)) -> bool {
    let ((ex, ey), r) = reach;
    let axis = |v: i32, e: i32| v - e < r && v + span - e > -r;
    axis(o.0, ex) && axis(o.1, ey)
}

/// The window's chunks on the map and within `reach`, each level's
/// (top-left map cell, map cells across, level), before frustum culling.
fn window_chunks(
    center: (i32, i32),
    half: i32,
    levels: u32,
    reach: ((i32, i32), i32),
) -> impl Iterator<Item = ((i32, i32), i32, i32)> {
    let map = c2_formats::map::MAP_SIZE as i32;
    let per_side = (half * 2 + CHUNK_CELLS - 1) / CHUNK_CELLS;
    (0..levels as i32).flat_map(move |level| {
        let span = CHUNK_CELLS << level;
        let first = (center.0 - (half << level), center.1 - (half << level));
        let inner = if level > 0 { half << (level - 1) } else { 0 };
        (0..per_side * per_side).filter_map(move |k| {
            let o = (first.0 + k % per_side * span, first.1 + k / per_side * span);
            let inside = |v: i32| v >= -inner && v + span <= inner;
            if level > 0 && inside(o.0 - center.0) && inside(o.1 - center.1) {
                return None; // the level inside draws all of it
            }
            let end = (o.0 + span, o.1 + span);
            let on_map = end.0 > 0 && end.1 > 0 && o.0 < map - 1 && o.1 < map - 1;
            (on_map && within_reach(o, span, reach)).then_some((o, span, level))
        })
    })
}

/// Cells per side of a drawn chunk of ground, in its level's cells.
pub const CHUNK_CELLS: i32 = 16;
const CHUNK_VERTICES: u32 = (CHUNK_CELLS * CHUNK_CELLS * 6) as u32;
/// Map cells per side of a [`Blocks`] entry.
const BLOCK: usize = 16;
const BLOCKS: usize = c2_formats::map::MAP_SIZE / BLOCK;

/// How far above its level the water's surface is drawn at most
/// (`shaders/terrain.wgsl: wave_offset`: 0.8 of the three amplitudes).
const WAVE_CREST: f32 = 24.0;

/// Per 16x16 map cells: the highest ground or water surface and the level
/// of its water (its first water cell's), to cull chunks and find the water
/// about the camera without looking at every cell.
struct Blocks {
    top: Vec<f32>,
    water: Vec<Option<f32>>,
}

impl Blocks {
    fn new(area: &c2_core::area::Area) -> Self {
        let m = &area.map;
        let mut top = vec![0.0f32; BLOCKS * BLOCKS];
        let mut water = vec![None; BLOCKS * BLOCKS];
        let size = c2_formats::map::MAP_SIZE;
        for i in 0..m.height.len().min(size * size) {
            let b = (i / size / BLOCK) * BLOCKS + (i % size) / BLOCK;
            top[b] = top[b].max(f32::from(m.height[i]) * c2_core::H_SCALE);
            // The water lies above its bed: a chunk whose ground is all
            // below the view can still show its surface.
            if let Some(level) = area.water_level(i % size, i / size) {
                top[b] = top[b].max(level + WAVE_CREST);
                water[b].get_or_insert(level);
            }
        }
        Self { top, water }
    }

    /// Highest ground or water and any water over map cells `min..=max`.
    fn over(&self, min: (i32, i32), max: (i32, i32)) -> (f32, bool) {
        let last = BLOCKS as i32 - 1;
        let b = |v: i32| (v.max(0) as usize / BLOCK).min(last as usize);
        let (mut top, mut water) = (0.0f32, false);
        for by in b(min.1)..=b(max.1) {
            for bx in b(min.0)..=b(max.0) {
                top = top.max(self.top[by * BLOCKS + bx]);
                water |= self.water[by * BLOCKS + bx].is_some();
            }
        }
        (top, water)
    }

    /// The water level that covers most of the view around map `cell`,
    /// within `radius` cells: each block of water counts by how near it is,
    /// so the lake at the camera's feet wins over a far sea, and a sea over
    /// a far pond. `None`: no water there.
    fn main_water(&self, cell: (i32, i32), radius: i32) -> Option<f32> {
        let last = BLOCKS as i32 - 1;
        let b = |v: i32| (v.div_euclid(BLOCK as i32)).clamp(0, last);
        let mut levels: Vec<(f32, f32)> = Vec::new();
        for by in b(cell.1 - radius)..=b(cell.1 + radius) {
            for bx in b(cell.0 - radius)..=b(cell.0 + radius) {
                let Some(level) = self.water[by as usize * BLOCKS + bx as usize] else {
                    continue;
                };
                let mid = |v: i32| v * BLOCK as i32 + BLOCK as i32 / 2;
                let (dx, dy) = ((mid(bx) - cell.0) as f32, (mid(by) - cell.1) as f32);
                // Roughly the share of the screen a patch of water takes
                // from eye height: it falls with the cube of the distance.
                let weight = (dx * dx + dy * dy + (BLOCK * BLOCK) as f32).powf(-1.5);
                match levels.iter_mut().find(|(l, _)| *l == level) {
                    Some((_, w)) => *w += weight,
                    None => levels.push((level, weight)),
                }
            }
        }
        let best = levels.into_iter().max_by(|a, b| a.1.total_cmp(&b.1));
        best.map(|(level, _)| level)
    }
}

/// `RandomMap[y & 31][x & 31]` (0..1023), for the shader's waves.
fn random(area: &c2_core::area::Area, x: usize, y: usize) -> u32 {
    let r = area.random_map.get((y & 31) * 32 + (x & 31)).copied();
    r.unwrap_or(0).clamp(0, 1023) as u32
}

pub struct TerrainRenderer {
    ground: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    blocks: Blocks,
    /// Visible chunks (x, y, level, 0): the ground's, then the water's.
    chunks: Vec<[i32; 4]>,
    ground_chunks: u32,
    instances: wgpu::Buffer,
    capacity: usize,
    /// The mirror the water's shader reads (PC option `modern_reflections`).
    pub reflection: Reflection,
}

impl TerrainRenderer {
    pub fn new(gpu: &Gpu, globals: &Globals, area: &c2_core::area::Area) -> Self {
        let m = &area.map;

        let cells: Vec<[u32; 4]> = (0..m.height.len())
            .map(|i| {
                let size = c2_formats::map::MAP_SIZE;
                let (x, y) = (i % size, i / size);
                let phase = area.water_phase(x, y);
                [
                    u32::from(m.height[i])
                        | u32::from(area.light_map()[i]) << 8
                        | u32::from(m.water[i]) << 16
                        | u32::from(phase) << 24,
                    u32::from(m.texture1[i]) | u32::from(m.flags[i]) << 16,
                    u32::from(m.light[0][i])
                        | u32::from(m.light[1][i]) << 8
                        | u32::from(m.light[2][i]) << 16,
                    random(area, x, y) | random(area, x + 7, y + 11) << 16,
                ]
            })
            .collect();
        let mut waters: Vec<[f32; 4]> = area
            .rsc
            .waters
            .iter()
            .map(|w| {
                let level = w.level as f32 * c2_core::H_SCALE;
                // `transp` is a depth coefficient (shaders/terrain.wgsl:
                // water_alpha), not a final alpha: keep it unclamped.
                [level, w.texture_index as f32, w.transp.max(0.0), 0.0]
            })
            .collect();
        if waters.is_empty() {
            waters.push([0.0; 4]); // storage buffers can't be empty
        }
        let rgba: Vec<Vec<u8>> = area
            .rsc
            .textures
            .iter()
            .map(|t| c2_core::color::image_to_rgba8(t, false))
            .collect();
        let fallback = [255u8; 128 * 128 * 4];
        let mut layers: Vec<&[u8]> = rgba.iter().map(Vec::as_slice).collect();
        if layers.is_empty() {
            layers.push(&fallback);
        }
        let view = gpu.world_texture_array("terrain textures", 128, 128, &layers);
        let sampler = gpu.sampler(crate::gpu::world_linear(), false);
        let mut sky_map = area.rsc.sky_map.clone();
        sky_map.resize(128 * 128, 0);

        let mk = |label, contents: &[u8], usage| {
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        let storage = wgpu::BufferUsages::STORAGE;
        let cell_buf = mk("cells", bytemuck::cast_slice(&cells), storage);
        let water_buf = mk("waters", bytemuck::cast_slice(&waters), storage);
        let sky_buf = mk("sky map", &sky_map, storage);
        let params = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain params"),
            size: std::mem::size_of::<TerrainParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let [t0, t1] = texture_entries(3);
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terrain"),
                entries: &[
                    storage_entry(0),
                    storage_entry(1),
                    uniform_entry(2, wgpu::ShaderStages::VERTEX),
                    t0,
                    t1,
                    storage_entry(5),
                ],
            });
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cell_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: water_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: sky_buf.as_entire_binding(),
                },
            ],
        });
        let pl = pipeline_layout(gpu, "terrain", &[&globals.layout, &layout]);
        let module = shader(gpu, "terrain", include_str!("../shaders/terrain.wgsl"));
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[i32; 4]>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Sint32x4],
        })];
        let desc = |label, vs, fs, blend| PipelineDesc {
            label,
            module: &module,
            layout: &pl,
            vs,
            fs,
            buffers: &buffers,
            blend,
        };
        let ground = pipeline(gpu, desc("ground", "vs_ground", "fs_ground", Blend::Opaque));
        let reflection = Reflection::new(gpu);
        let water_pl = pipeline_layout(
            gpu,
            "water",
            &[&globals.layout, &layout, reflection.layout()],
        );
        let water = pipeline(
            gpu,
            PipelineDesc {
                layout: &water_pl,
                ..desc("water", "vs_water", "fs_water", Blend::Alpha)
            },
        );
        let capacity = 1024;
        Self {
            ground,
            water,
            group,
            params,
            blocks: Blocks::new(area),
            chunks: Vec::new(),
            ground_chunks: 0,
            instances: Self::instance_buffer(gpu, capacity),
            capacity,
            reflection,
        }
    }

    fn instance_buffer(gpu: &Gpu, n: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ground chunks"),
            size: (n * std::mem::size_of::<[i32; 4]>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Sets the square window of cells to draw, centred on `center` cell;
    /// with `lod`, past [`LOD_FINE_CELLS`] in coarser levels (see [`window`]).
    /// Only the chunks of it that `frustum` may see are drawn. `waves`:
    /// open water sways (`shaders/terrain.wgsl: wave_offset`).
    #[allow(clippy::too_many_arguments)]
    pub fn set_window(
        &mut self,
        gpu: &Gpu,
        center: (i32, i32),
        radius: u32,
        lod: bool,
        layers: u32,
        time_ms: u32,
        waves: bool,
        frustum: &Frustum,
    ) {
        let eye = center;
        let (half, center, levels) = window(eye, radius, lod);
        let p = TerrainParams {
            origin: [center.0 - half as i32, center.1 - half as i32],
            width: half * 2,
            layers: layers.max(1),
            time_ms,
            // Game.cpp sets Clouds = TRUE; there is no option to turn it off.
            clouds: 1,
            levels,
            reach: radius,
            eye: [eye.0, eye.1],
            waves: u32::from(waves),
            pad: 0,
        };
        gpu.queue
            .write_buffer(&self.params, 0, bytemuck::bytes_of(&p));
        self.cull(center, half as i32, levels, (eye, radius as i32), frustum);
        if self.chunks.len() > self.capacity {
            self.capacity = self.chunks.len().next_power_of_two();
            self.instances = Self::instance_buffer(gpu, self.capacity);
        }
        if !self.chunks.is_empty() {
            gpu.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.chunks));
        }
    }

    /// Fills `chunks` with the visible ground chunks of each level within
    /// `reach` (the camera's cell and radius), then the visible chunks with water.
    fn cull(
        &mut self,
        center: (i32, i32),
        half: i32,
        levels: u32,
        reach: ((i32, i32), i32),
        frustum: &Frustum,
    ) {
        let mut water = Vec::new();
        self.chunks.clear();
        for (o, span, level) in window_chunks(center, half, levels, reach) {
            let end = (o.0 + span, o.1 + span);
            let (top, wet) = self.blocks.over(o, end);
            let cell = c2_core::CELL;
            let min = Vec3::new(o.0 as f32 * cell, 0.0, o.1 as f32 * cell);
            let max = Vec3::new(end.0 as f32 * cell, top, end.1 as f32 * cell);
            if !frustum.sees_box(min, max) {
                continue;
            }
            self.chunks.push([o.0, o.1, level, 0]);
            if wet {
                water.push([o.0, o.1, level, 0]);
            }
        }
        self.ground_chunks = self.chunks.len() as u32;
        self.chunks.extend(water);
    }

    pub fn draw_ground(&self, pass: &mut wgpu::RenderPass<'_>, globals: &Globals) {
        self.draw(pass, globals, &self.ground, 0..self.ground_chunks);
    }

    pub fn draw_water(&self, pass: &mut wgpu::RenderPass<'_>, globals: &Globals) {
        let all = self.chunks.len() as u32;
        pass.set_bind_group(2, self.reflection.group(), &[]);
        self.draw(pass, globals, &self.water, self.ground_chunks..all);
    }

    /// The water level to mirror for a camera over map `cell` seeing
    /// `radius` cells (`Blocks::main_water`).
    pub fn main_water(&self, cell: (i32, i32), radius: u32) -> Option<f32> {
        self.blocks.main_water(cell, radius as i32)
    }

    fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        pipeline: &wgpu::RenderPipeline,
        chunks: std::ops::Range<u32>,
    ) {
        if chunks.is_empty() {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..CHUNK_VERTICES, chunks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_full_level_unless_asked() {
        assert_eq!(window((300, 400), 74, false), (74, (300, 400), 1));
        assert_eq!(window((300, 400), 384, false), (384, (300, 400), 1));
        assert_eq!(window((300, 400), 128, true), (128, (300, 400), 1));
    }

    #[test]
    fn levels_reach_the_radius_on_the_coarsest_grid() {
        let (half, c, levels) = window((301, 403), 1024, true);
        assert_eq!((half, levels), (LOD_FINE_CELLS, 4));
        assert_eq!(c, (296, 400));
        assert!(LOD_FINE_CELLS << (levels - 1) >= 1024);
        assert_eq!(window((0, 0), 129, true).2, 2);
        assert_eq!(window((0, 0), 384, true).2, 3);
    }

    /// How many window chunks cover each cell of `eye`'s `radius` square,
    /// and the farthest any chunk reaches from `eye` along an axis.
    fn coverage(eye: (i32, i32), radius: u32, lod: bool) -> (Vec<u8>, i32) {
        let (half, center, levels) = window(eye, radius, lod);
        let r = radius as i32;
        let side = (2 * r) as usize;
        let mut hits = vec![0u8; side * side];
        let mut farthest = 0;
        for (o, span, _) in window_chunks(center, half as i32, levels, (eye, r)) {
            for y in o.1..o.1 + span {
                for x in o.0..o.0 + span {
                    let (dx, dy) = (x - eye.0, y - eye.1);
                    farthest = farthest.max(dx.abs()).max(dy.abs());
                    if (-r..r).contains(&dx) && (-r..r).contains(&dy) {
                        hits[(dy + r) as usize * side + (dx + r) as usize] += 1;
                    }
                }
            }
        }
        (hits, farthest)
    }

    // AUDIT-V3: the levels used to be drawn out to the coarsest level's
    // edge (512 cells for a 300-cell radius), past the fog and the radius.
    #[test]
    fn levels_stop_at_the_radius_and_still_cover_it() {
        for (eye, radius) in [((515, 509), 300), ((512, 512), 129), ((600, 450), 1024)] {
            let (hits, farthest) = coverage(eye, radius, true);
            let r = radius as i32;
            let side = (2 * r) as usize;
            for (i, &n) in hits.iter().enumerate() {
                let (x, y) = (eye.0 - r + (i % side) as i32, eye.1 - r + (i / side) as i32);
                // Off the map (and its last row/column) the shader drops
                // the cells of a chunk that is partly on it.
                let on_map = (0..1023).contains(&x) && (0..1023).contains(&y);
                assert!(
                    n == 1 || !on_map && n == 0,
                    "cell ({x}, {y}), radius {radius}: {n}"
                );
            }
            let (_, _, levels) = window(eye, radius, true);
            let coarsest = CHUNK_CELLS << (levels - 1);
            assert!(
                farthest < r + coarsest,
                "radius {radius}: chunks reach {farthest}"
            );
        }
    }

    #[test]
    fn the_mirrored_water_is_the_one_about_the_camera() {
        use c2_core::area::{Area, NO_WATER};
        use c2_formats::rsc::Water;
        let mut area = Area::flat(10);
        let water = |level| Water {
            texture_index: 0,
            level,
            transp: 1.0,
            fog_rgb: 0,
        };
        area.rsc.waters = vec![water(20), water(30)];
        area.map.water.fill(NO_WATER);
        let size = c2_formats::map::MAP_SIZE;
        // A pond (level 20) of one block at cells 96..112, and a sea
        // (level 30) over every cell from column 512 east.
        for y in 0..size {
            for x in 0..size {
                if (96..112).contains(&x) && (96..112).contains(&y) {
                    area.map.water[y * size + x] = 0;
                } else if x >= 512 {
                    area.map.water[y * size + x] = 1;
                }
            }
        }
        let blocks = Blocks::new(&area);
        let (pond, sea) = (20.0 * c2_core::H_SCALE, 30.0 * c2_core::H_SCALE);
        assert_eq!(blocks.main_water((100, 100), 74), Some(pond));
        assert_eq!(blocks.main_water((600, 600), 74), Some(sea));
        // By the pond with the whole map in view: still the pond.
        assert_eq!(blocks.main_water((120, 100), 1024), Some(pond));
        // From the far shore of the sea, the pond out of reach.
        assert_eq!(blocks.main_water((480, 100), 74), Some(sea));
        assert_eq!(blocks.main_water((300, 800), 40), None);
    }

    #[test]
    fn one_level_keeps_every_chunk_of_its_window() {
        let (hits, farthest) = coverage((300, 400), 74, false);
        assert!(hits.iter().all(|&n| n == 1));
        assert!(farthest < 74 + CHUNK_CELLS);
    }
}
