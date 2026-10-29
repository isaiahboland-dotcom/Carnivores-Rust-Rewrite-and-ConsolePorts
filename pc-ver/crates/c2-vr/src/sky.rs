//! The sky in the headset: the PC's `sky.wgsl` (naga; fragment entries from
//! `shaders/sky_vr.wgsl`), drawn first with no depth test as
//! `c2-pc render.rs: SkyRenderer` does. The area's sky picture is a
//! scrolling cloud plane high above the eye (`RenderSkyPlane`); the sun
//! (`SUN2.3DF`, the moon at night) is placed in each eye's view space
//! (`RenderSun`), so it shows at the same place to both eyes, as something
//! that far away does.

#![allow(unsafe_code)] // Raw Vulkan.

use anyhow::{anyhow, Result};
use ash::vk;
use c2_core::area::{Area, TimeOfDay};
use c2_core::model::PreparedModel;
use glam::{Mat4, Vec3};

use crate::gpu::{Buffer, Gpu, Image};
use crate::scene;
use crate::xr::DEPTH;

const COMMON: &str = include_str!("../../c2-pc/src/shaders/common.wgsl");
const SKY: &str = include_str!("../../c2-pc/src/shaders/sky.wgsl");
const SKY_VR: &str = include_str!("../shaders/sky_vr.wgsl");
/// `SunVertex` (view-space position, uv).
const SUN_VERTEX_BYTES: u32 = 20;

pub struct Sky {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    sky: vk::Pipeline,
    sun: vk::Pipeline,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    textures: Image,
    textures_view: vk::ImageView,
    sampler: vk::Sampler,
    /// The sun model's triangles (position, uv), model space.
    sun_tris: Vec<([f32; 3], [f32; 2])>,
    /// Per frame in flight, per eye.
    sun_bufs: Vec<[Buffer; 2]>,
    /// This frame's sun vertices per eye (0: not in view).
    sun_count: [u32; 2],
    night: bool,
    sun_pos: Vec3,
}

impl Sky {
    pub fn new(
        gpu: &Gpu,
        area: &Area,
        sun: Option<&PreparedModel>,
        render_pass: vk::RenderPass,
        globals_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let time = area.time;
        let mut texels = c2_core::color::image_to_rgba8(&area.rsc.skies[time as usize], false);
        texels.resize(256 * 256 * 4, 0);
        match sun {
            Some(m) => texels.extend_from_slice(&m.texture_rgba),
            None => texels.resize(2 * 256 * 256 * 4, 0),
        }
        texels.resize(2 * 256 * 256 * 4, 0);
        let (textures, textures_view) = gpu.texture_array(256, &texels, 2)?;
        let sampler = gpu.linear_sampler(true)?;
        let sun_tris: Vec<([f32; 3], [f32; 2])> = sun
            .map(|m| {
                m.faces
                    .iter()
                    .flat_map(|f| {
                        (0..3).map(move |k| {
                            (
                                m.positions
                                    .get(f.v[k] as usize)
                                    .copied()
                                    .unwrap_or_default(),
                                f.uv[k],
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let size = (sun_tris.len().max(1) as u64) * u64::from(SUN_VERTEX_BYTES);
        let mut sun_bufs = Vec::new();
        for _ in 0..DEPTH {
            sun_bufs.push([
                gpu.buffer(size, vk::BufferUsageFlags::VERTEX_BUFFER)?,
                gpu.buffer(size, vk::BufferUsageFlags::VERTEX_BUFFER)?,
            ]);
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
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .bindings(&[binding(0, D::SAMPLED_IMAGE), binding(1, D::SAMPLER)]),
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
            dev.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(0)
                        .descriptor_type(D::SAMPLED_IMAGE)
                        .image_info(&image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(1)
                        .descriptor_type(D::SAMPLER)
                        .image_info(&samp),
                ],
                &[],
            );

            let src = format!("{COMMON}\n{SKY}\n{SKY_VR}");
            let stage =
                |e, s| -> Result<vk::ShaderModule> { gpu.shader(&scene::wgsl(&src, e, s)?) };
            let vs_sky = stage("vs_sky", naga::ShaderStage::Vertex)?;
            let fs_sky = stage("fs_sky_vr", naga::ShaderStage::Fragment)?;
            let vs_sun = stage("vs_sun", naga::ShaderStage::Vertex)?;
            let fs_sun = stage("fs_sun_vr", naga::ShaderStage::Fragment)?;
            let sun_binding = [vk::VertexInputBindingDescription {
                binding: 0,
                stride: SUN_VERTEX_BYTES,
                input_rate: vk::VertexInputRate::VERTEX,
            }];
            let sun_attrs = [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: 0,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 12,
                },
            ];
            let sky = scene::pipeline(
                gpu,
                &scene::PipelineDesc {
                    vert: (vs_sky, c"vs_sky"),
                    frag: (fs_sky, c"fs_sky_vr"),
                    layout,
                    render_pass,
                    depth: None,
                    blend: scene::Blend::Alpha,
                    bindings: &[],
                    attributes: &[],
                },
            );
            let sun_pipe = scene::pipeline(
                gpu,
                &scene::PipelineDesc {
                    vert: (vs_sun, c"vs_sun"),
                    frag: (fs_sun, c"fs_sun_vr"),
                    layout,
                    render_pass,
                    depth: None,
                    blend: scene::Blend::Additive,
                    bindings: &sun_binding,
                    attributes: &sun_attrs,
                },
            );
            for m in [vs_sky, fs_sky, vs_sun, fs_sun] {
                dev.destroy_shader_module(m, None);
            }
            Ok(Self {
                set_layout,
                layout,
                sky: sky?,
                sun: sun_pipe?,
                pool,
                set,
                textures,
                textures_view,
                sampler,
                sun_tris,
                sun_bufs,
                sun_count: [0; 2],
                night: time == TimeOfDay::Night,
                sun_pos: Vec3::from(time.sun_pos()),
            })
        }
    }

    /// `SkyRenderer::draw`'s sun placement for one eye's `view`: in view
    /// space, pulled closer near the middle of the view, drawn only in
    /// front (`nv.z < -2024`).
    pub fn prepare(&mut self, slot: usize, eye: usize, view: Mat4) {
        self.sun_count[eye] = 0;
        let nv = view.transform_vector3(self.sun_pos);
        if self.sun_tris.is_empty() || nv.z >= -2024.0 {
            return;
        }
        let d = if self.night {
            1.5
        } else {
            (2048.0 + (nv.x * nv.x + nv.y * nv.y).sqrt().min(812.0)) / 3048.0
        };
        let c = nv * d;
        let mut bytes = Vec::with_capacity(self.sun_tris.len() * SUN_VERTEX_BYTES as usize);
        for (p, uv) in &self.sun_tris {
            let q = Vec3::from(*p) + c;
            for f in [q.x, q.y, q.z, uv[0], uv[1]] {
                bytes.extend_from_slice(&f.to_ne_bytes());
            }
        }
        self.sun_bufs[slot][eye].write(0, &bytes);
        self.sun_count[eye] = self.sun_tris.len() as u32;
    }

    /// Sky then sun, inside the render pass, before anything else.
    pub fn draw(
        &self,
        gpu: &Gpu,
        cmd: vk::CommandBuffer,
        slot: usize,
        eye: usize,
        globals: vk::DescriptorSet,
    ) {
        let gfx = vk::PipelineBindPoint::GRAPHICS;
        // SAFETY: `cmd` is recording inside a render pass of this
        // pipeline's kind; the buffers are live and this slot's.
        unsafe {
            let dev = &gpu.device;
            dev.cmd_bind_descriptor_sets(cmd, gfx, self.layout, 0, &[globals, self.set], &[]);
            dev.cmd_bind_pipeline(cmd, gfx, self.sky);
            dev.cmd_draw(cmd, 6, 1, 0, 0);
            if self.sun_count[eye] > 0 {
                dev.cmd_bind_pipeline(cmd, gfx, self.sun);
                dev.cmd_bind_vertex_buffers(cmd, 0, &[self.sun_bufs[slot][eye].buffer], &[0]);
                dev.cmd_draw(cmd, self.sun_count[eye], 1, 0, 0);
            }
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        // SAFETY: the GPU is idle; each handle is destroyed once.
        unsafe {
            let dev = &gpu.device;
            for b in self.sun_bufs.iter().flatten() {
                gpu.destroy_buffer(b);
            }
            dev.destroy_pipeline(self.sky, None);
            dev.destroy_pipeline(self.sun, None);
            dev.destroy_pipeline_layout(self.layout, None);
            dev.destroy_descriptor_pool(self.pool, None);
            dev.destroy_descriptor_set_layout(self.set_layout, None);
            dev.destroy_sampler(self.sampler, None);
            dev.destroy_image_view(self.textures_view, None);
            gpu.destroy_image(&self.textures);
        }
    }
}

/// `c2-pc area_view.rs: load_sun`: MOON.3DF at night, SUN2.3DF otherwise,
/// from `HUNTDAT/` (the `AREAS` folder's parent); `None` if not there.
pub fn load_sun(map_path: &std::path::Path, time: TimeOfDay) -> Option<PreparedModel> {
    let huntdat = map_path.parent()?.parent()?;
    let name = if time == TimeOfDay::Night {
        "MOON.3DF"
    } else {
        "SUN2.3DF"
    };
    let path = std::fs::read_dir(huntdat)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })?;
    let data = std::fs::read(&path).ok()?;
    match c2_formats::three_df::parse(&data) {
        Ok(m) => Some(PreparedModel::from_mesh(&m.mesh)),
        Err(e) => {
            println!("{}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_shaders_compile_for_vulkan() {
        let src = format!("{COMMON}\n{SKY}\n{SKY_VR}");
        for (e, s) in [
            ("vs_sky", naga::ShaderStage::Vertex),
            ("fs_sky_vr", naga::ShaderStage::Fragment),
            ("vs_sun", naga::ShaderStage::Vertex),
            ("fs_sun_vr", naga::ShaderStage::Fragment),
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
