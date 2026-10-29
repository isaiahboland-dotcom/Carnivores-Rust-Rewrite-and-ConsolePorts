//! The test scene (`shaders/test_scene.frag`, no game data): sky, a 1 m
//! floor grid, eight boxes in a ring 4 m out (the gold one straight ahead,
//! -z), a ball per tracked controller.

#![allow(unsafe_code)] // Raw Vulkan.

use std::io::Cursor;

use anyhow::{Context, Result};
use ash::vk;

use crate::gpu::Gpu;
use crate::scene::{self, Frame, Scene};

/// `Push` in `test_scene.frag`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Push {
    pos: [f32; 4],
    rot: [f32; 4],
    tan: [f32; 4],
    size: [f32; 4],
    hand: [[f32; 4]; 2],
}

impl Push {
    fn bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(std::mem::size_of::<Self>());
        let all = [
            self.pos,
            self.rot,
            self.tan,
            self.size,
            self.hand[0],
            self.hand[1],
        ];
        for f in all.iter().flatten() {
            v.extend_from_slice(&f.to_ne_bytes());
        }
        v
    }
}

pub fn spv(bytes: &[u8]) -> Result<Vec<u32>> {
    ash::util::read_spv(&mut Cursor::new(bytes)).context("reading a shader")
}

pub struct TestScene {
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}

impl TestScene {
    pub fn new(gpu: &Gpu, format: vk::Format) -> Result<Self> {
        let render_pass = scene::render_pass(gpu, format, None)?;
        let vert = gpu.shader(&spv(include_bytes!("../shaders/fullscreen.vert.spv"))?)?;
        let frag = gpu.shader(&spv(include_bytes!("../shaders/test_scene.frag.spv"))?)?;
        // SAFETY: plain object creation; the modules go once the pipeline
        // is made.
        unsafe {
            let layout = gpu.device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&[
                    vk::PushConstantRange {
                        stage_flags: vk::ShaderStageFlags::FRAGMENT,
                        offset: 0,
                        size: std::mem::size_of::<Push>() as u32,
                    },
                ]),
                None,
            )?;
            let pipeline = scene::pipeline(
                gpu,
                &scene::PipelineDesc {
                    vert: (vert, c"main"),
                    frag: (frag, c"main"),
                    layout,
                    render_pass,
                    depth: None,
                    blend: scene::Blend::Opaque,
                    bindings: &[],
                    attributes: &[],
                },
            );
            gpu.device.destroy_shader_module(vert, None);
            gpu.device.destroy_shader_module(frag, None);
            Ok(Self {
                render_pass,
                layout,
                pipeline: pipeline?,
            })
        }
    }
}

impl Scene for TestScene {
    fn render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    fn eye_targets(&mut self, _: &Gpu, _: vk::Extent2D) -> Result<[Vec<vk::ImageView>; 2]> {
        Ok([Vec::new(), Vec::new()])
    }

    fn record(&mut self, gpu: &Gpu, f: &Frame) -> Result<()> {
        let mut push = Push {
            size: [f.extent.width as f32, f.extent.height as f32, f.floor, 0.0],
            ..Default::default()
        };
        for (i, h) in f.hands.iter().enumerate() {
            if let Some(p) = h {
                push.hand[i] = [p.x, p.y, p.z, 1.0];
            }
        }
        for (eye, fb) in f.eyes.iter().zip(f.framebuffers) {
            push.pos = eye.pos.extend(0.0).to_array();
            push.rot = eye.rot.to_array();
            push.tan = eye.tan;
            scene::begin(
                gpu,
                f.cmd,
                self.render_pass,
                fb,
                f.extent,
                &[vk::ClearValue::default()],
            );
            // SAFETY: `f.cmd` is recording inside our render pass.
            unsafe {
                gpu.device
                    .cmd_bind_pipeline(f.cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
                gpu.device.cmd_push_constants(
                    f.cmd,
                    self.layout,
                    vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &push.bytes(),
                );
                gpu.device.cmd_draw(f.cmd, 3, 1, 0, 0);
                gpu.device.cmd_end_render_pass(f.cmd);
            }
        }
        Ok(())
    }

    fn destroy(&mut self, gpu: &Gpu) {
        // SAFETY: the GPU is idle.
        unsafe {
            gpu.device.destroy_pipeline(self.pipeline, None);
            gpu.device.destroy_pipeline_layout(self.layout, None);
            gpu.device.destroy_render_pass(self.render_pass, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_constants_match_the_shader() {
        // 6 vec4s; Vulkan guarantees 128 bytes of push constants.
        assert_eq!(std::mem::size_of::<Push>(), 96);
        assert_eq!(Push::default().bytes().len(), 96);
    }

    #[test]
    fn shaders_are_spir_v() {
        for s in [
            &include_bytes!("../shaders/fullscreen.vert.spv")[..],
            &include_bytes!("../shaders/test_scene.frag.spv")[..],
        ] {
            assert!(spv(s).is_ok_and(|w| w.first() == Some(&0x0723_0203)));
        }
    }
}
