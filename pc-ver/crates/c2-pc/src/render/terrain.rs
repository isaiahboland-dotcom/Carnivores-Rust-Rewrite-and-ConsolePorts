//! Terrain and water, built in the vertex shader from per-cell data.
use super::frustum::Frustum;
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

/// Cells per side of a drawn chunk of ground, in its level's cells.
pub const CHUNK_CELLS: i32 = 16;
const CHUNK_VERTICES: u32 = (CHUNK_CELLS * CHUNK_CELLS * 6) as u32;
/// Map cells per side of a [`Blocks`] entry.
const BLOCK: usize = 16;
const BLOCKS: usize = c2_formats::map::MAP_SIZE / BLOCK;

/// Per 16x16 map cells: the highest ground and whether any cell has
/// water, to cull chunks without looking at every cell.
struct Blocks {
    top: Vec<f32>,
    water: Vec<bool>,
}

impl Blocks {
    fn new(m: &c2_formats::map::Map) -> Self {
        let mut top = vec![0.0f32; BLOCKS * BLOCKS];
        let mut water = vec![false; BLOCKS * BLOCKS];
        let size = c2_formats::map::MAP_SIZE;
        for i in 0..m.height.len().min(size * size) {
            let b = (i / size / BLOCK) * BLOCKS + (i % size) / BLOCK;
            top[b] = top[b].max(f32::from(m.height[i]) * c2_core::H_SCALE);
            water[b] |= m.water[i] != 255;
        }
        Self { top, water }
    }

    /// Highest ground and any water over map cells `min..=max`.
    fn over(&self, min: (i32, i32), max: (i32, i32)) -> (f32, bool) {
        let last = BLOCKS as i32 - 1;
        let b = |v: i32| (v.max(0) as usize / BLOCK).min(last as usize);
        let (mut top, mut water) = (0.0f32, false);
        for by in b(min.1)..=b(max.1) {
            for bx in b(min.0)..=b(max.0) {
                top = top.max(self.top[by * BLOCKS + bx]);
                water |= self.water[by * BLOCKS + bx];
            }
        }
        (top, water)
    }
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
}

impl TerrainRenderer {
    pub fn new(gpu: &Gpu, globals: &Globals, area: &c2_core::area::Area) -> Self {
        let m = &area.map;

        let cells: Vec<[u32; 4]> = (0..m.height.len())
            .map(|i| {
                let size = c2_formats::map::MAP_SIZE;
                let phase = area.water_phase(i % size, i / size);
                [
                    u32::from(m.height[i])
                        | u32::from(area.light_map()[i]) << 8
                        | u32::from(m.water[i]) << 16
                        | u32::from(phase) << 24,
                    u32::from(m.texture1[i]) | u32::from(m.flags[i]) << 16,
                    u32::from(m.light[0][i])
                        | u32::from(m.light[1][i]) << 8
                        | u32::from(m.light[2][i]) << 16,
                    0,
                ]
            })
            .collect();
        let mut waters: Vec<[f32; 4]> = area
            .rsc
            .waters
            .iter()
            .map(|w| {
                let level = w.level as f32 * c2_core::H_SCALE;
                [level, w.texture_index as f32, w.transp.clamp(0.0, 1.0), 0.0]
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
        let sampler = gpu.sampler(true, false);
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
        let desc = |label, vs, blend| PipelineDesc {
            label,
            module: &module,
            layout: &pl,
            vs,
            fs: "fs",
            buffers: &buffers,
            blend,
        };
        let ground = pipeline(gpu, desc("ground", "vs_ground", Blend::Opaque));
        let water = pipeline(gpu, desc("water", "vs_water", Blend::Alpha));
        let capacity = 1024;
        Self {
            ground,
            water,
            group,
            params,
            blocks: Blocks::new(m),
            chunks: Vec::new(),
            ground_chunks: 0,
            instances: Self::instance_buffer(gpu, capacity),
            capacity,
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
    /// Only the chunks of it that `frustum` may see are drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn set_window(
        &mut self,
        gpu: &Gpu,
        center: (i32, i32),
        radius: u32,
        lod: bool,
        layers: u32,
        time_ms: u32,
        frustum: &Frustum,
    ) {
        let (half, center, levels) = window(center, radius, lod);
        let p = TerrainParams {
            origin: [center.0 - half as i32, center.1 - half as i32],
            width: half * 2,
            layers: layers.max(1),
            time_ms,
            // Game.cpp sets Clouds = TRUE; there is no option to turn it off.
            clouds: 1,
            levels,
            pad: 0,
        };
        gpu.queue
            .write_buffer(&self.params, 0, bytemuck::bytes_of(&p));
        self.cull(center, half as i32, levels, frustum);
        if self.chunks.len() > self.capacity {
            self.capacity = self.chunks.len().next_power_of_two();
            self.instances = Self::instance_buffer(gpu, self.capacity);
        }
        if !self.chunks.is_empty() {
            gpu.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.chunks));
        }
    }

    /// Fills `chunks` with the visible ground chunks of each level, then the
    /// visible chunks with water.
    fn cull(&mut self, center: (i32, i32), half: i32, levels: u32, frustum: &Frustum) {
        let map = c2_formats::map::MAP_SIZE as i32;
        let per_side = (half * 2 + CHUNK_CELLS - 1) / CHUNK_CELLS;
        let mut water = Vec::new();
        self.chunks.clear();
        for level in 0..levels as i32 {
            let step = 1 << level;
            let span = CHUNK_CELLS * step;
            let first = (center.0 - (half << level), center.1 - (half << level));
            let inner = if level > 0 { half << (level - 1) } else { 0 };
            for ky in 0..per_side {
                for kx in 0..per_side {
                    let o = (first.0 + kx * span, first.1 + ky * span);
                    let rel = (o.0 - center.0, o.1 - center.1);
                    let inside = |v: i32| v >= -inner && v + span <= inner;
                    if level > 0 && inside(rel.0) && inside(rel.1) {
                        continue; // the level inside draws all of it
                    }
                    let end = (o.0 + span, o.1 + span);
                    if end.0 <= 0 || end.1 <= 0 || o.0 >= map - 1 || o.1 >= map - 1 {
                        continue;
                    }
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
        self.draw(pass, globals, &self.water, self.ground_chunks..all);
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
}
