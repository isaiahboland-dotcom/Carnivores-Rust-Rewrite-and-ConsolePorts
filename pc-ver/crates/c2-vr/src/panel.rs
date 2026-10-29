//! The menu standing in the world (`shaders/panel.wgsl`): a quad showing
//! the menu's canvas (`canvas`, drawn on the CPU and uploaded when it
//! changes). It stands in the tracking space where it was put (`place`);
//! a controller's ray picks a point on it (`pick`).

#![allow(unsafe_code)] // Raw Vulkan.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use ash::vk;
use glam::{Mat4, Vec3};

use crate::gpu::{Gpu, Image};
use crate::scene::{self, Ray};

const SHADER: &str = include_str!("../shaders/panel.wgsl");
use crate::canvas::{Canvas, H as CH, PH, PW, W as CW};
/// The panel's size and distance, metres: 4:3, about 45 degrees across.
const WIDTH_M: f32 = 2.0;
const HEIGHT_M: f32 = 1.5;
const DIST_M: f32 = 2.5;
/// Its middle this far above the eyes (the owner raised it twice from 0.15 m
/// below).
const RISE_M: f32 = 0.25;
/// `Push` in the shader.
const PUSH: u32 = 80;

/// `name` in `dir`, any case.
pub fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

/// Where the panel stands, in the tracking space.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub center: Vec3,
    /// Unit vectors along the panel: right and up.
    pub right: Vec3,
    pub up: Vec3,
}

impl Placement {
    /// Upright, `DIST_M` in front of `head` the way `look` points.
    pub fn facing(head: Vec3, look: Vec3) -> Self {
        let fwd = Vec3::new(look.x, 0.0, look.z)
            .try_normalize()
            .unwrap_or(Vec3::NEG_Z);
        Self {
            center: head + fwd * DIST_M + Vec3::Y * RISE_M,
            right: Vec3::new(-fwd.z, 0.0, fwd.x),
            up: Vec3::Y,
        }
    }

    /// Where `ray` meets the panel, canvas pixels.
    pub fn pick(&self, ray: &Ray) -> Option<(f32, f32)> {
        let n = self.right.cross(self.up);
        let denom = ray.dir.dot(n);
        if denom.abs() < 1e-4 {
            return None;
        }
        let t = (self.center - ray.pos).dot(n) / denom;
        if t <= 0.0 {
            return None;
        }
        let d = ray.pos + ray.dir * t - self.center;
        let (u, v) = (
            d.dot(self.right) / WIDTH_M + 0.5,
            0.5 - d.dot(self.up) / HEIGHT_M,
        );
        ((0.0..1.0).contains(&u) && (0.0..1.0).contains(&v))
            .then_some((u * CW as f32, v * CH as f32))
    }

    /// The unit quad (x, y in -0.5..0.5) to this placement taken through
    /// `to_world` (tracking space to world).
    pub fn model(&self, to_world: impl Fn(Vec3) -> Vec3) -> Mat4 {
        let c = to_world(self.center);
        let r = to_world(self.center + self.right * WIDTH_M) - c;
        let u = to_world(self.center + self.up * HEIGHT_M) - c;
        Mat4::from_cols(
            r.extend(0.0),
            u.extend(0.0),
            r.cross(u).normalize_or_zero().extend(0.0),
            c.extend(1.0),
        )
    }
}

pub struct Panel {
    pic: (Image, vk::ImageView),
    /// Per frame in flight: the canvas on its way to `pic`.
    staging: Vec<crate::gpu::Buffer>,
    sampler: vk::Sampler,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}

fn compile(entry: &str, stage: naga::ShaderStage) -> Result<Vec<u32>> {
    scene::wgsl(SHADER, entry, stage)
}

impl Panel {
    /// `render_pass`: the scene's, which the panel is drawn in.
    pub fn new(gpu: &Gpu, render_pass: vk::RenderPass) -> Result<Self> {
        let pic = gpu.texture(
            (PW as u32, PH as u32),
            vk::Format::R8G8B8A8_SRGB,
            &Canvas::default().px,
            1,
        )?;
        let mut staging = Vec::new();
        for _ in 0..crate::xr::DEPTH {
            staging.push(gpu.buffer((PW * PH * 4) as u64, vk::BufferUsageFlags::TRANSFER_SRC)?);
        }
        let sampler = gpu.linear_sampler(false)?;
        let stages = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
        let binding = |b: u32, kind: vk::DescriptorType| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(b)
                .descriptor_type(kind)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
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
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[set_layout])
                    .push_constant_ranges(&[vk::PushConstantRange {
                        stage_flags: stages,
                        offset: 0,
                        size: PUSH,
                    }]),
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
                image_view: pic.1,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let samp = [vk::DescriptorImageInfo {
                sampler,
                ..Default::default()
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
                ],
                &[],
            );
            let vs = gpu.shader(&compile("vs_panel", naga::ShaderStage::Vertex)?)?;
            let fs = gpu.shader(&compile("fs_panel", naga::ShaderStage::Fragment)?)?;
            let pipeline = scene::pipeline(
                gpu,
                &scene::PipelineDesc {
                    vert: (vs, c"vs_panel"),
                    frag: (fs, c"fs_panel"),
                    layout,
                    render_pass,
                    // Over everything: nothing in the camp hides it.
                    depth: None,
                    blend: scene::Blend::Opaque,
                    bindings: &[],
                    attributes: &[],
                },
            );
            dev.destroy_shader_module(vs, None);
            dev.destroy_shader_module(fs, None);
            Ok(Self {
                pic,
                staging,
                sampler,
                set_layout,
                pool,
                set,
                layout,
                pipeline: pipeline?,
            })
        }
    }

    /// Records `canvas` into the panel's picture, before the render pass
    /// (`slot`: this frame's, whose last use is over).
    pub fn upload(&self, gpu: &Gpu, cmd: vk::CommandBuffer, slot: usize, canvas: &Canvas) {
        let Some(staging) = self.staging.get(slot) else {
            return;
        };
        staging.write(0, &canvas.px);
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let barrier = |from, to, src, dst| {
            vk::ImageMemoryBarrier::default()
                .old_layout(from)
                .new_layout(to)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.pic.0.image)
                .subresource_range(range)
        };
        // SAFETY: `cmd` is recording outside a render pass; the barriers
        // order the copy after earlier frames' reads and before this one's.
        unsafe {
            let dev = &gpu.device;
            dev.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier(
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::AccessFlags::SHADER_READ,
                    vk::AccessFlags::TRANSFER_WRITE,
                )],
            );
            dev.cmd_copy_buffer_to_image(
                cmd,
                staging.buffer,
                self.pic.0.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy {
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    image_extent: vk::Extent3D {
                        width: PW as u32,
                        height: PH as u32,
                        depth: 1,
                    },
                    ..Default::default()
                }],
            );
            dev.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier(
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                )],
            );
        }
    }

    /// Records the panel; `mvp` takes the unit quad to clip space;
    /// `pointer` in canvas units.
    pub fn draw(&self, gpu: &Gpu, cmd: vk::CommandBuffer, mvp: Mat4, pointer: Option<(f32, f32)>) {
        let pointer = pointer.map_or([0.0; 4], |(x, y)| [x, y, 1.0, 0.0]);
        let push: Vec<u8> = mvp
            .to_cols_array()
            .iter()
            .chain(&pointer)
            .flat_map(|f| f.to_ne_bytes())
            .collect();
        // SAFETY: `cmd` is recording inside the render pass the pipeline
        // was made for.
        unsafe {
            let dev = &gpu.device;
            let gfx = vk::PipelineBindPoint::GRAPHICS;
            dev.cmd_bind_pipeline(cmd, gfx, self.pipeline);
            dev.cmd_bind_descriptor_sets(cmd, gfx, self.layout, 0, &[self.set], &[]);
            dev.cmd_push_constants(
                cmd,
                self.layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
            dev.cmd_draw(cmd, 6, 1, 0, 0);
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        // SAFETY: the GPU is idle; each handle is destroyed once.
        unsafe {
            let dev = &gpu.device;
            dev.destroy_pipeline(self.pipeline, None);
            dev.destroy_pipeline_layout(self.layout, None);
            dev.destroy_descriptor_pool(self.pool, None);
            dev.destroy_descriptor_set_layout(self.set_layout, None);
            dev.destroy_sampler(self.sampler, None);
            for b in &self.staging {
                gpu.destroy_buffer(b);
            }
            dev.destroy_image_view(self.pic.1, None);
            gpu.destroy_image(&self.pic.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_shader_compiles_for_vulkan() {
        for (e, s) in [
            ("vs_panel", naga::ShaderStage::Vertex),
            ("fs_panel", naga::ShaderStage::Fragment),
        ] {
            let words = compile(e, s);
            assert!(
                words
                    .as_ref()
                    .is_ok_and(|w| w.first() == Some(&0x0723_0203)),
                "{e}: {:?}",
                words.err()
            );
        }
    }

    #[test]
    fn a_ray_down_the_middle_hits_the_middle() {
        let p = Placement::facing(Vec3::new(0.0, 1.7, 0.0), Vec3::NEG_Z);
        let eye = Vec3::new(0.0, 1.7, 0.0);
        let ray = |x: f32, y: f32| Ray {
            pos: eye,
            dir: (p.center + Vec3::new(x, y, 0.0) - eye).normalize(),
        };
        let (x, y) = p.pick(&ray(0.0, 0.0)).unwrap_or_default();
        assert!((x - 400.0).abs() < 0.5 && (y - 300.0).abs() < 0.5);
        // Up and right on the panel: up and right on the canvas.
        let (x, y) = p.pick(&ray(0.5, 0.375)).unwrap_or_default();
        assert!((x - 600.0).abs() < 0.5 && (y - 150.0).abs() < 0.5);
        // Pointing away misses.
        let back = Ray {
            pos: eye,
            dir: Vec3::Z,
        };
        assert!(p.pick(&back).is_none());
    }
}
