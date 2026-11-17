//! The terminal's CRT layers (scanlines, vignette, noise) as one shader
//! pass over the menu (`shaders/crt.wgsl`), drawn after its sprites. The
//! "DHC OS CRT effects" option turns them on; `dhc_crt_layers` in
//! `c2rs.cfg` picks which (1 scanlines, 2 vignette, 4 noise; all by
//! default).

use bytemuck::{Pod, Zeroable};

use crate::gpu::{Gpu, DEPTH_FORMAT};

/// Every layer.
pub(crate) const ALL_LAYERS: i32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    size: [f32; 2],
    scale: f32,
    time: f32,
    layers: u32,
    _pad: [u32; 3],
}

pub(crate) struct Crt {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    start: std::time::Instant,
}

impl Crt {
    pub(crate) fn new(gpu: &Gpu) -> Self {
        let d = &gpu.device;
        let uniform = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crt"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crt"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crt"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crt"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crt"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/crt.wgsl").into()),
        });
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crt"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            // The menu's pass has the scene's depth buffer: untouched.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Crt {
            pipeline,
            uniform,
            group,
            start: std::time::Instant::now(),
        }
    }

    /// Draws `layers` over the window (`screen` pixels, `scale` pixels
    /// per canvas unit).
    pub(crate) fn draw(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        screen: (u32, u32),
        scale: f32,
        layers: i32,
    ) {
        let u = Uniform {
            size: [screen.0 as f32, screen.1 as f32],
            scale,
            time: self.start.elapsed().as_secs_f32() % 3600.0,
            layers: (layers & ALL_LAYERS) as u32,
            _pad: [0; 3],
        };
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}
