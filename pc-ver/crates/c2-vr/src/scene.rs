//! What the frame loop (`xr`) hands a scene each frame, and the Vulkan
//! pieces the scenes share (render passes, pipelines).

#![allow(unsafe_code)] // Raw Vulkan.

use anyhow::{anyhow, Result};
use ash::vk;
use glam::{Quat, Vec2, Vec3};

use crate::gpu::Gpu;

/// A standing player's eye height, metres: the hunter's (`HEAD_STAND`) is
/// drawn at this.
pub const EYE_HEIGHT_M: f32 = 1.7;

/// One eye this frame, in the tracking space (metres, y up).
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    pub pos: Vec3,
    pub rot: Quat,
    /// Tangents of the view's edges: left, right, up, down (left and down
    /// negative).
    pub tan: [f32; 4],
}

/// Where a controller points, in the tracking space.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub pos: Vec3,
    /// Unit length.
    pub dir: Vec3,
}

/// The controllers (sticks: Index, Touch, WMR; Vive trackpads) and the
/// gamepad.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Left stick: walk (x right, y forward), -1..1.
    pub walk: Vec2,
    /// Right stick pushed past half way this frame: -1 left, 1 right.
    pub snap_turn: i32,
    /// Each hand's pointing ray (left, right), where tracked.
    pub aim: [Option<Ray>; 2],
    /// A controller's trigger went down this frame (left, right).
    pub select: [bool; 2],
    /// The left controller's menu button or the pad's - / Select went down:
    /// quits, unless the scene `handles_back`.
    pub back: bool,
    /// The pad's right stick click: the floor was just set from the head
    /// (the scene may put its menu in front again).
    pub recenter: bool,
    pub pad: c2_pad::Frame,
}

pub struct Frame {
    pub cmd: vk::CommandBuffer,
    /// Which of the frames in flight (per-frame buffers).
    pub slot: usize,
    pub eyes: [Eye; 2],
    pub hands: [Option<Vec3>; 2],
    /// The tracking space's floor height (0 in the stage space).
    pub floor: f32,
    pub extent: vk::Extent2D,
    pub framebuffers: [vk::Framebuffer; 2],
    pub dt: f32,
    pub input: Input,
}

impl Frame {
    /// Between the eyes.
    pub fn head(&self) -> Vec3 {
        (self.eyes[0].pos + self.eyes[1].pos) * 0.5
    }
}

pub trait Scene {
    /// Its render pass: colour first, then whatever `eye_targets` adds.
    fn render_pass(&self) -> vk::RenderPass;
    /// Attachments after the colour one, per eye (a depth buffer).
    fn eye_targets(&mut self, gpu: &Gpu, extent: vk::Extent2D) -> Result<[Vec<vk::ImageView>; 2]>;
    /// Records both eyes into `f.cmd`.
    fn record(&mut self, gpu: &Gpu, f: &Frame) -> Result<()>;
    /// Frees everything; the GPU is idle.
    fn destroy(&mut self, gpu: &Gpu);
    /// `Input::back` is the scene's (back to the menu) rather than quitting.
    fn handles_back(&self) -> bool {
        false
    }
    /// After each frame: carry on, quit, or swap to another scene.
    fn next(&mut self) -> Next {
        Next::Stay
    }
}

/// Makes the scene once the device and the swapchain's format are known.
pub type MakeScene = Box<dyn FnOnce(&Gpu, vk::Format) -> Result<Box<dyn Scene>>>;

pub enum Next {
    Stay,
    Quit,
    /// This scene goes and that one comes (another area, the menu).
    Load(MakeScene),
}

/// A colour (and depth) render pass, the colour left as the compositor
/// wants it.
pub fn render_pass(
    gpu: &Gpu,
    color: vk::Format,
    depth: Option<vk::Format>,
) -> Result<vk::RenderPass> {
    let mut attachments = vec![vk::AttachmentDescription {
        format: color,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::CLEAR,
        store_op: vk::AttachmentStoreOp::STORE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        ..Default::default()
    }];
    if let Some(d) = depth {
        attachments.push(vk::AttachmentDescription {
            format: d,
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::DONT_CARE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ..Default::default()
        });
    }
    let color_ref = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let mut subpass = vk::SubpassDescription::default()
        .color_attachments(&color_ref)
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);
    if depth.is_some() {
        subpass = subpass.depth_stencil_attachment(&depth_ref);
    }
    let stages = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS;
    let dependency = vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: stages,
        dst_stage_mask: stages,
        src_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ..Default::default()
    };
    // SAFETY: plain object creation on our device.
    Ok(unsafe {
        gpu.device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&[subpass])
                .dependencies(&[dependency]),
            None,
        )?
    })
}

/// How a pipeline's colour lands (`c2-pc render.rs: Blend`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Opaque,
    /// Over what's there by alpha (water, see-through faces, the sky).
    Alpha,
    /// Added on top (the sun and moon).
    Additive,
}

pub struct PipelineDesc<'a> {
    /// The modules and their entry points (`main` from glslc, the WGSL
    /// function's name from naga).
    pub vert: (vk::ShaderModule, &'a std::ffi::CStr),
    pub frag: (vk::ShaderModule, &'a std::ffi::CStr),
    pub layout: vk::PipelineLayout,
    pub render_pass: vk::RenderPass,
    /// Depth test (less), and whether it writes depth; `None`: no test
    /// (drawn behind everything, the sky).
    pub depth: Option<bool>,
    pub blend: Blend,
    /// Vertex buffers (none: the shader makes its vertices).
    pub bindings: &'a [vk::VertexInputBindingDescription],
    pub attributes: &'a [vk::VertexInputAttributeDescription],
}

/// A triangle-list pipeline, viewport and scissor set per draw.
pub fn pipeline(gpu: &Gpu, d: &PipelineDesc<'_>) -> Result<vk::Pipeline> {
    let blend = match d.blend {
        Blend::Alpha => vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::TRUE,
            src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
            dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ONE,
            dst_alpha_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        },
        Blend::Additive => vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::TRUE,
            src_color_blend_factor: vk::BlendFactor::ONE,
            dst_color_blend_factor: vk::BlendFactor::ONE,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ONE,
            dst_alpha_blend_factor: vk::BlendFactor::ONE,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        },
        Blend::Opaque => vk::PipelineColorBlendAttachmentState {
            color_write_mask: vk::ColorComponentFlags::RGBA,
            ..Default::default()
        },
    };
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(d.depth.is_some())
        .depth_write_enable(d.depth == Some(true))
        .depth_compare_op(vk::CompareOp::LESS);
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(d.vert.0)
            .name(d.vert.1),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(d.frag.0)
            .name(d.frag.1),
    ];
    let attachments = [blend];
    let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(d.bindings)
        .vertex_attribute_descriptions(d.attributes);
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .scissor_count(1)
        .viewport_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .cull_mode(vk::CullModeFlags::NONE)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&color)
        .dynamic_state(&dynamic)
        .layout(d.layout)
        .render_pass(d.render_pass)
        .subpass(0);
    // SAFETY: plain object creation on our device.
    unsafe {
        gpu.device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
            .map_err(|(_, e)| anyhow!("creating a pipeline: {e}"))?
            .first()
            .copied()
            .ok_or_else(|| anyhow!("no pipeline"))
    }
}

/// Begins `render_pass` on `framebuffer` with the whole image as viewport.
pub fn begin(
    gpu: &Gpu,
    cmd: vk::CommandBuffer,
    render_pass: vk::RenderPass,
    framebuffer: vk::Framebuffer,
    extent: vk::Extent2D,
    clear: &[vk::ClearValue],
) {
    let area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent,
    };
    // SAFETY: `cmd` is recording; the pass and framebuffer are live.
    unsafe {
        gpu.device.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(render_pass)
                .framebuffer(framebuffer)
                .render_area(area)
                .clear_values(clear),
            vk::SubpassContents::INLINE,
        );
        gpu.device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        gpu.device.cmd_set_scissor(cmd, 0, &[area]);
    }
}

/// The sRGB transfer undone, for colours worked out in gamma space as the
/// PC game does (its window is not sRGB) written to an sRGB image.
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WGSL (the PC's shaders and ours) to SPIR-V for one entry point.
pub fn wgsl(src: &str, entry: &str, stage: naga::ShaderStage) -> Result<Vec<u32>> {
    let module = naga::front::wgsl::parse_str(src)
        .map_err(|e| anyhow!("shader: {}", e.emit_to_string(src)))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| anyhow!("shader {entry}: {e:?}"))?;
    naga::back::spv::write_vec(
        &module,
        &info,
        &naga::back::spv::Options::default(),
        Some(&naga::back::spv::PipelineOptions {
            shader_stage: stage,
            entry_point: entry.into(),
        }),
    )
    .map_err(|e| anyhow!("shader {entry}: {e}"))
}
