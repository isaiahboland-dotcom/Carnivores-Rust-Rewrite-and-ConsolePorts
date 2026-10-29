//! An area's objects (trees, plants, rocks) in the headset, drawn by the
//! PC's own `model.wgsl` (naga to SPIR-V; fragment entries from
//! `shaders/model_vr.wgsl`), as `c2-pc area_view.rs` does: every model's
//! faces in one vertex buffer (opaque first, then see-through), the
//! placements around the player as instances, opaque faces of every model
//! before the blended ones. Objects are whole models out to the view range:
//! the original's far sprites (`RenderBMPModel`, past `SPRITE_CELLS`; the
//! PC's "Far sprites") look flat in stereo and swim as the head turns
//! (owner, 2026-10-29), so they are off unless `far_sprites` is set.
//! Animated objects stand still for now.

#![allow(unsafe_code)] // Raw Vulkan.

use anyhow::{anyhow, Result};
use ash::vk;
use c2_core::area::{Area, Placement};
use c2_core::model::{
    sprite_mesh, vertex_lights, PreparedFace, PreparedModel, MODEL_TEXTURE_SIZE, SPRITE_CELLS,
};
use c2_core::CELL;
use c2_formats::rsc::object_flags;
use glam::Vec3;

use crate::gpu::{Buffer, Gpu, Image};
use crate::scene;
use crate::xr::DEPTH;

const COMMON: &str = include_str!("../../c2-pc/src/shaders/common.wgsl");
const MODEL: &str = include_str!("../../c2-pc/src/shaders/model.wgsl");
const MODEL_VR: &str = include_str!("../shaders/model_vr.wgsl");

/// Placement buckets, as `c2-pc area_view.rs`.
const CHUNK: usize = 32;
const CHUNKS: usize = c2_formats::map::MAP_SIZE / CHUNK;
/// `ModelVertex` (pos, uv, vlight) and `Instance` (offset, turns, light,
/// layer) in `c2-pc render.rs`.
const VERTEX_BYTES: u32 = 36;
const INSTANCE_BYTES: u32 = 24;

/// One model's faces in the shared vertex buffer.
#[derive(Clone, Copy, Default)]
struct Range {
    first: u32,
    opaque: u32,
    total: u32,
}

/// This frame's instances of one model.
#[derive(Clone, Copy, Default)]
struct Batch {
    first_instance: u32,
    count: u32,
}

pub struct Objects {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    opaque: vk::Pipeline,
    blend: vk::Pipeline,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    vertices: Buffer,
    ground: Buffer,
    textures: Image,
    textures_view: vk::ImageView,
    sampler: vk::Sampler,
    /// Per frame in flight.
    instances: Vec<Buffer>,
    capacity: usize,
    /// Models, then sprites (`None`: `ofNOBMP`), indexed as their layers.
    ranges: Vec<Range>,
    has_sprite: Vec<bool>,
    batches: Vec<Batch>,
    /// Placements bucketed by chunk, with their light (negative: lit by
    /// the ground per vertex) and their sprite's.
    chunks: Vec<Vec<(Placement, f32, f32)>>,
    /// Past `SPRITE_CELLS`, objects as their sprites (off in VR).
    pub far_sprites: bool,
}

fn floats(v: &[f32]) -> impl Iterator<Item = u8> + '_ {
    v.iter().flat_map(|f| f.to_ne_bytes())
}

/// `GpuModel::new`: faces expanded to unshared vertices, opaque first.
fn push_model(out: &mut Vec<u8>, m: &PreparedModel, vlights: &[[f32; 4]]) -> (u32, u32) {
    let (opaque, blended): (Vec<&PreparedFace>, Vec<&PreparedFace>) =
        m.faces.iter().partition(|f| !f.is_transparent());
    for f in opaque.iter().chain(&blended) {
        for k in 0..3 {
            let i = f.v[k] as usize;
            out.extend(floats(&m.positions.get(i).copied().unwrap_or_default()));
            out.extend(floats(&f.uv[k]));
            out.extend(floats(&vlights.get(i).copied().unwrap_or_default()));
        }
    }
    (
        opaque.len() as u32 * 3,
        (opaque.len() + blended.len()) as u32 * 3,
    )
}

impl Objects {
    pub fn new(
        gpu: &Gpu,
        area: &Area,
        render_pass: vk::RenderPass,
        globals_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let objects = &area.rsc.objects;
        let sun = area.time.sun_pos();
        let mut verts = Vec::new();
        let mut ranges = Vec::new();
        let mut texels = Vec::new();
        let blank = vec![0u8; MODEL_TEXTURE_SIZE * MODEL_TEXTURE_SIZE * 4];
        for o in objects {
            let p = PreparedModel::from_mesh(&o.model.mesh);
            // CalcLights runs for every RSC object; NOLIGHT zeroes it and
            // ground-lit objects replace it with ground light.
            let unlit = object_flags::NO_LIGHT | object_flags::GROUND_LIGHT;
            let vl = if o.info.flags & unlit != 0 {
                Vec::new()
            } else {
                vertex_lights(&o.model.mesh, sun)
            };
            let first = (verts.len() as u32) / VERTEX_BYTES;
            let (opaque, total) = push_model(&mut verts, &p, &vl);
            ranges.push(Range {
                first,
                opaque,
                total,
            });
            texels.extend_from_slice(&p.texture_rgba);
        }
        let mut has_sprite = Vec::new();
        for o in objects {
            let sprite = o.info.flags & object_flags::NO_BMP == 0;
            has_sprite.push(sprite);
            let first = (verts.len() as u32) / VERTEX_BYTES;
            if sprite {
                let p = PreparedModel::from_mesh(&sprite_mesh(&o.model.mesh, &o.sprite));
                let (opaque, total) = push_model(&mut verts, &p, &[]);
                ranges.push(Range {
                    first,
                    opaque,
                    total,
                });
                texels.extend_from_slice(&p.texture_rgba);
            } else {
                ranges.push(Range::default());
                texels.extend_from_slice(&blank);
            }
        }
        if texels.is_empty() {
            texels = blank;
        }
        let layers = (texels.len() / (MODEL_TEXTURE_SIZE * MODEL_TEXTURE_SIZE * 4)) as u32;
        let (textures, textures_view) =
            gpu.texture_array(MODEL_TEXTURE_SIZE as u32, &texels, layers)?;
        let vertices = gpu.buffer_with(&verts, vk::BufferUsageFlags::VERTEX_BUFFER)?;
        let mut ground = area.light_map().to_vec();
        ground.resize(ground.len().div_ceil(4).max(1) * 4, 0);
        let ground = gpu.buffer_with(&ground, vk::BufferUsageFlags::STORAGE_BUFFER)?;
        let sampler = gpu.linear_sampler(false)?;

        let mut chunks = vec![Vec::new(); CHUNKS * CHUNKS];
        let mut count = 0;
        for p in area.placements() {
            let c = usize::from(p.cell.1) / CHUNK * CHUNKS + usize::from(p.cell.0) / CHUNK;
            let ground_lit = objects
                .get(usize::from(p.model))
                .is_some_and(|o| o.info.flags & object_flags::GROUND_LIGHT != 0);
            let l = if ground_lit {
                -1.0
            } else {
                area.object_light(&p)
            };
            if let Some(chunk) = chunks.get_mut(c) {
                chunk.push((p, l, area.sprite_light(&p)));
                count += 1;
            }
        }
        let capacity = count.max(1);
        let mut instances = Vec::new();
        for _ in 0..DEPTH {
            instances.push(gpu.buffer(
                (capacity as u64) * u64::from(INSTANCE_BYTES),
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )?);
        }

        let all = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
        let binding = |b: u32, kind| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(b)
                .descriptor_type(kind)
                .descriptor_count(1)
                .stage_flags(all)
        };
        use vk::DescriptorType as D;
        // SAFETY: plain object creation and descriptor writes on our device;
        // every handle written is live.
        unsafe {
            let dev = &gpu.device;
            let set_layout = dev.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                    binding(0, D::SAMPLED_IMAGE),
                    binding(1, D::SAMPLER),
                    binding(2, D::STORAGE_BUFFER),
                ]),
                None,
            )?;
            let layout = dev.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&[globals_layout, set_layout]),
                None,
            )?;
            let pool = dev.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&[
                        vk::DescriptorPoolSize {
                            ty: D::SAMPLED_IMAGE,
                            descriptor_count: 1,
                        },
                        vk::DescriptorPoolSize {
                            ty: D::SAMPLER,
                            descriptor_count: 1,
                        },
                        vk::DescriptorPoolSize {
                            ty: D::STORAGE_BUFFER,
                            descriptor_count: 1,
                        },
                    ]),
                None,
            )?;
            let set = *dev
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&[set_layout]),
                )?
                .first()
                .ok_or_else(|| anyhow!("no descriptor set"))?;
            let image = [vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: textures_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let samp = [vk::DescriptorImageInfo {
                sampler,
                ..Default::default()
            }];
            let buf = [vk::DescriptorBufferInfo {
                buffer: ground.buffer,
                offset: 0,
                range: vk::WHOLE_SIZE,
            }];
            let w = |b, kind| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(b)
                    .descriptor_type(kind)
            };
            dev.update_descriptor_sets(
                &[
                    w(0, D::SAMPLED_IMAGE).image_info(&image),
                    w(1, D::SAMPLER).image_info(&samp),
                    w(2, D::STORAGE_BUFFER).buffer_info(&buf),
                ],
                &[],
            );

            let src = format!("{COMMON}\n{MODEL}\n{MODEL_VR}");
            let vs = gpu.shader(&scene::wgsl(&src, "vs", naga::ShaderStage::Vertex)?)?;
            let fs_opaque = gpu.shader(&scene::wgsl(
                &src,
                "fs_opaque_vr",
                naga::ShaderStage::Fragment,
            )?)?;
            let fs_blend = gpu.shader(&scene::wgsl(
                &src,
                "fs_blend_vr",
                naga::ShaderStage::Fragment,
            )?)?;
            let bindings = [
                vk::VertexInputBindingDescription {
                    binding: 0,
                    stride: VERTEX_BYTES,
                    input_rate: vk::VertexInputRate::VERTEX,
                },
                vk::VertexInputBindingDescription {
                    binding: 1,
                    stride: INSTANCE_BYTES,
                    input_rate: vk::VertexInputRate::INSTANCE,
                },
            ];
            let attr = |location, binding, format, offset| vk::VertexInputAttributeDescription {
                location,
                binding,
                format,
                offset,
            };
            use vk::Format as F;
            let attributes = [
                attr(0, 0, F::R32G32B32_SFLOAT, 0),
                attr(1, 0, F::R32G32_SFLOAT, 12),
                attr(2, 0, F::R32G32B32A32_SFLOAT, 20),
                attr(3, 1, F::R32G32B32_SFLOAT, 0),
                attr(4, 1, F::R32_SFLOAT, 12),
                attr(5, 1, F::R32_SFLOAT, 16),
                attr(6, 1, F::R32_UINT, 20),
            ];
            let desc = |frag, depth_write, blend| scene::PipelineDesc {
                vert: (vs, c"vs"),
                frag,
                layout,
                render_pass,
                depth: Some(depth_write),
                blend,
                bindings: &bindings,
                attributes: &attributes,
            };
            let opaque = scene::pipeline(
                gpu,
                &desc((fs_opaque, c"fs_opaque_vr"), true, scene::Blend::Opaque),
            );
            let blend = scene::pipeline(
                gpu,
                &desc((fs_blend, c"fs_blend_vr"), false, scene::Blend::Alpha),
            );
            for m in [vs, fs_opaque, fs_blend] {
                dev.destroy_shader_module(m, None);
            }
            let n = ranges.len();
            Ok(Self {
                set_layout,
                layout,
                opaque: opaque?,
                blend: blend?,
                pool,
                set,
                vertices,
                ground,
                textures,
                textures_view,
                sampler,
                instances,
                capacity,
                ranges,
                has_sprite,
                batches: vec![Batch::default(); n],
                chunks,
                far_sprites: false,
            })
        }
    }

    /// This frame's instances around `head` (world), within `radius`
    /// cells; sprites face `yaw` (radians, 0 looking toward -z).
    pub fn prepare(&mut self, slot: usize, head: Vec3, radius: u32, yaw: f32) {
        let n_models = self.has_sprite.len();
        let mut per: Vec<Vec<[u8; 24]>> = vec![Vec::new(); self.ranges.len()];
        let facing = yaw / std::f32::consts::FRAC_PI_2;
        let sprite_d2 = (SPRITE_CELLS * CELL).powi(2);
        let r = radius as i32;
        let r2 = (r * r) as f32 * CELL * CELL;
        let center = ((head.x / CELL) as i32, (head.z / CELL) as i32);
        let lo = |c: i32| ((c - r).max(0) as usize) / CHUNK;
        let hi = |c: i32| (((c + r).max(0) as usize) / CHUNK).min(CHUNKS - 1);
        let instance = |pos: [f32; 3], turns: f32, light: f32, layer: u32| {
            let mut b = [0u8; 24];
            for (i, f) in [pos[0], pos[1], pos[2], turns, light].iter().enumerate() {
                b[i * 4..i * 4 + 4].copy_from_slice(&f.to_ne_bytes());
            }
            b[20..24].copy_from_slice(&layer.to_ne_bytes());
            b
        };
        for cy in lo(center.1)..=hi(center.1) {
            for cx in lo(center.0)..=hi(center.0) {
                let Some(chunk) = self.chunks.get(cy * CHUNKS + cx) else {
                    continue;
                };
                for (p, light, sprite_light) in chunk {
                    let d = Vec3::from(p.pos) - head;
                    if d.x * d.x + d.z * d.z > r2 {
                        continue;
                    }
                    let m = usize::from(p.model);
                    if m >= n_models {
                        continue;
                    }
                    if self.far_sprites && d.length_squared() > sprite_d2 && self.has_sprite[m] {
                        let layer = (n_models + m) as u32;
                        per[n_models + m].push(instance(p.pos, facing, *sprite_light, layer));
                    } else {
                        let turns = f32::from(p.quarter_turns);
                        per[m].push(instance(p.pos, turns, *light, m as u32));
                    }
                }
            }
        }
        let mut bytes = Vec::new();
        let mut first = 0u32;
        for (batch, inst) in self.batches.iter_mut().zip(&per) {
            let room = self.capacity.saturating_sub(first as usize);
            let count = inst.len().min(room) as u32;
            *batch = Batch {
                first_instance: first,
                count,
            };
            for b in &inst[..count as usize] {
                bytes.extend_from_slice(b);
            }
            first += count;
        }
        self.instances[slot].write(0, &bytes);
    }

    /// Draws this frame's objects inside the render pass (`globals`: the
    /// eye's set 0).
    pub fn draw(&self, gpu: &Gpu, cmd: vk::CommandBuffer, slot: usize, globals: vk::DescriptorSet) {
        let gfx = vk::PipelineBindPoint::GRAPHICS;
        // SAFETY: `cmd` is recording inside a render pass of this
        // pipeline's kind; the buffers are live and this slot's.
        unsafe {
            let dev = &gpu.device;
            dev.cmd_bind_descriptor_sets(cmd, gfx, self.layout, 0, &[globals, self.set], &[]);
            dev.cmd_bind_vertex_buffers(
                cmd,
                0,
                &[self.vertices.buffer, self.instances[slot].buffer],
                &[0, 0],
            );
            for (pipeline, blended) in [(self.opaque, false), (self.blend, true)] {
                dev.cmd_bind_pipeline(cmd, gfx, pipeline);
                for (r, b) in self.ranges.iter().zip(&self.batches) {
                    let (start, end) = if blended {
                        (r.opaque, r.total)
                    } else {
                        (0, r.opaque)
                    };
                    if end > start && b.count > 0 {
                        dev.cmd_draw(cmd, end - start, b.count, r.first + start, b.first_instance);
                    }
                }
            }
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        // SAFETY: the GPU is idle; each handle is destroyed once.
        unsafe {
            let dev = &gpu.device;
            for b in self.instances.iter().chain([&self.vertices, &self.ground]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_shaders_compile_for_vulkan() {
        let src = format!("{COMMON}\n{MODEL}\n{MODEL_VR}");
        for (e, s) in [
            ("vs", naga::ShaderStage::Vertex),
            ("fs_opaque_vr", naga::ShaderStage::Fragment),
            ("fs_blend_vr", naga::ShaderStage::Fragment),
        ] {
            let words = scene::wgsl(&src, e, s);
            assert!(
                words
                    .as_ref()
                    .is_ok_and(|w| w.first() == Some(&0x0723_0203)),
                "{e}: {:?}",
                words.err()
            );
        }
    }
}
