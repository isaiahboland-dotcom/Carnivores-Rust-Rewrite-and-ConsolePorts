//! The hunt's characters (animals, and the pickup ship) in the headset,
//! drawn by the PC's `model.wgsl` as `objects` are and as `c2-pc dinos.rs`
//! poses them: each this frame's pose (`c2_core::dino::pose`), its faces
//! expanded into one per-frame vertex buffer, one instance each at the
//! character's place and turn, a texture layer per `.CAR`.

#![allow(unsafe_code)] // Raw Vulkan.

use anyhow::Result;
use ash::vk;
use c2_core::model::{PreparedModel, MODEL_TEXTURE_SIZE};

use crate::gpu::{Buffer, Gpu, Image};
use crate::objects::{face_order, model_pipelines, model_set, INSTANCE_BYTES, VERTEX_BYTES};
use crate::xr::DEPTH;

/// One model's draw order.
struct Faces {
    order: Vec<u32>,
    uvs: Vec<[f32; 2]>,
    opaque: u32,
}

/// One posed character this frame.
pub struct Posed {
    /// Its model (as given to `new`).
    pub model: usize,
    /// Vertex positions, the model's own order.
    pub positions: Vec<[f32; 3]>,
    pub offset: [f32; 3],
    /// Quarter turns about y (`pose::quarter_turns`).
    pub turns: f32,
    /// 0..1 (`RenderCharacterPost`: 210 / 255).
    pub light: f32,
}

#[derive(Clone, Copy)]
struct Draw {
    first: u32,
    opaque: u32,
    total: u32,
}

pub struct Animals {
    faces: Vec<Option<Faces>>,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    opaque: vk::Pipeline,
    blend: vk::Pipeline,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    textures: Image,
    textures_view: vk::ImageView,
    sampler: vk::Sampler,
    /// Unused (characters aren't ground-lit), but bound.
    ground: Buffer,
    /// Per frame in flight: vertices and instances, grown as needed.
    vertices: Vec<Buffer>,
    instances: Vec<Buffer>,
    draws: Vec<Draw>,
}

fn floats(v: &[f32]) -> impl Iterator<Item = u8> + '_ {
    v.iter().flat_map(|f| f.to_ne_bytes())
}

impl Animals {
    /// `models`: per model (`None`: didn't load), its prepared form; model
    /// `i` uses texture layer `i`.
    pub fn new(
        gpu: &Gpu,
        models: &[Option<&PreparedModel>],
        render_pass: vk::RenderPass,
        globals_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let size = MODEL_TEXTURE_SIZE * MODEL_TEXTURE_SIZE * 4;
        let mut texels = Vec::with_capacity(models.len().max(1) * size);
        let mut faces = Vec::new();
        for m in models {
            match m {
                Some(p) if p.texture_rgba.len() == size => {
                    texels.extend_from_slice(&p.texture_rgba)
                }
                _ => texels.resize(texels.len() + size, 0),
            }
            faces.push(m.map(|p| {
                let (order, uvs, opaque) = face_order(p);
                Faces { order, uvs, opaque }
            }));
        }
        if texels.is_empty() {
            texels.resize(size, 0);
        }
        let layers = (texels.len() / size) as u32;
        let (textures, textures_view) =
            gpu.texture_array(MODEL_TEXTURE_SIZE as u32, &texels, layers)?;
        let sampler = gpu.linear_sampler(false)?;
        let ground = gpu.buffer_with(&[0; 4], vk::BufferUsageFlags::STORAGE_BUFFER)?;
        let (set_layout, layout, pool, set) =
            model_set(gpu, globals_layout, textures_view, sampler, &ground)?;
        let (opaque, blend) = model_pipelines(gpu, layout, render_pass)?;
        let mut vertices = Vec::new();
        let mut instances = Vec::new();
        for _ in 0..DEPTH {
            vertices.push(gpu.buffer(
                u64::from(VERTEX_BYTES) * 4096,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )?);
            instances.push(gpu.buffer(
                u64::from(INSTANCE_BYTES) * 64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )?);
        }
        Ok(Self {
            faces,
            set_layout,
            layout,
            opaque,
            blend,
            pool,
            set,
            textures,
            textures_view,
            sampler,
            ground,
            vertices,
            instances,
            draws: Vec::new(),
        })
    }

    /// `buf` big enough for `bytes`, remade if not (the slot's last frame
    /// is done with it).
    fn fit(gpu: &Gpu, buf: &mut Buffer, bytes: &[u8]) -> Result<()> {
        if (bytes.len() as u64) > buf.size {
            let grown = gpu.buffer(
                (bytes.len() as u64).next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )?;
            gpu.destroy_buffer(buf);
            *buf = grown;
        }
        buf.write(0, bytes);
        Ok(())
    }

    /// This frame's characters, for frame `slot`.
    pub fn prepare(&mut self, gpu: &Gpu, slot: usize, posed: &[Posed]) -> Result<()> {
        let mut verts = Vec::new();
        let mut insts = Vec::new();
        self.draws.clear();
        for p in posed {
            let Some(Some(f)) = self.faces.get(p.model) else {
                continue;
            };
            let first = (verts.len() / VERTEX_BYTES as usize) as u32;
            for (&i, uv) in f.order.iter().zip(&f.uvs) {
                let pos = p.positions.get(i as usize).copied().unwrap_or_default();
                verts.extend(floats(&pos));
                verts.extend(floats(uv));
                verts.extend(floats(&[0.0; 4]));
            }
            insts.extend(floats(&p.offset));
            insts.extend(floats(&[p.turns, p.light]));
            insts.extend((p.model as u32).to_ne_bytes());
            self.draws.push(Draw {
                first,
                opaque: f.opaque,
                total: f.order.len() as u32,
            });
        }
        if self.draws.is_empty() {
            return Ok(());
        }
        Self::fit(gpu, &mut self.vertices[slot], &verts)?;
        Self::fit(gpu, &mut self.instances[slot], &insts)
    }

    /// Draws this frame's characters inside the render pass (`globals`:
    /// the eye's set 0).
    pub fn draw(&self, gpu: &Gpu, cmd: vk::CommandBuffer, slot: usize, globals: vk::DescriptorSet) {
        if self.draws.is_empty() {
            return;
        }
        let gfx = vk::PipelineBindPoint::GRAPHICS;
        // SAFETY: `cmd` is recording inside a render pass of this
        // pipeline's kind; the buffers are live and this slot's.
        unsafe {
            let dev = &gpu.device;
            dev.cmd_bind_descriptor_sets(cmd, gfx, self.layout, 0, &[globals, self.set], &[]);
            dev.cmd_bind_vertex_buffers(
                cmd,
                0,
                &[self.vertices[slot].buffer, self.instances[slot].buffer],
                &[0, 0],
            );
            for (pipeline, blended) in [(self.opaque, false), (self.blend, true)] {
                dev.cmd_bind_pipeline(cmd, gfx, pipeline);
                for (i, d) in self.draws.iter().enumerate() {
                    let (start, end) = if blended {
                        (d.opaque, d.total)
                    } else {
                        (0, d.opaque)
                    };
                    if end > start {
                        dev.cmd_draw(cmd, end - start, 1, d.first + start, i as u32);
                    }
                }
            }
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        // SAFETY: the GPU is idle; each handle is destroyed once.
        unsafe {
            let dev = &gpu.device;
            for b in self
                .vertices
                .iter()
                .chain(&self.instances)
                .chain([&self.ground])
            {
                gpu.destroy_buffer(b);
            }
            dev.destroy_pipeline(self.opaque, None);
            dev.destroy_pipeline(self.blend, None);
            dev.destroy_pipeline_layout(self.layout, None);
            dev.destroy_descriptor_pool(self.pool, None);
            dev.destroy_descriptor_set_layout(self.set_layout, None);
            dev.destroy_sampler(self.sampler, None);
            dev.destroy_image_view(self.textures_view, None);
            gpu.destroy_image(&self.textures);
        }
    }
}
