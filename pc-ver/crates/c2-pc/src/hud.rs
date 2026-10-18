//! 2D overlay pictures (`DrawPicture`): bullets, prompts, map frame.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::gpu::{Gpu, DEPTH_FORMAT};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub struct Image {
    pub w: u32,
    pub h: u32,
    group: wgpu::BindGroup,
}

pub struct Hud {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    screen: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    images: Vec<Image>,
    vbuf: wgpu::Buffer,
    capacity: usize,
    /// The 8x8 font atlas and a 1x1 white image (ids).
    font: Option<usize>,
    white: Option<usize>,
}

/// An image placed on screen, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub image: usize,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// Source rectangle in UV space (u0, v0, u1, v1).
    pub uv: [f32; 4],
    /// Size on screen in pixels; `None` = source size times `scale`.
    pub size: Option<[f32; 2]>,
    /// Multiplied with the texel (RGBA).
    pub color: [f32; 4],
}

impl Sprite {
    /// A whole image at `scale`.
    pub fn new(image: usize, x: f32, y: f32, scale: f32) -> Self {
        Self {
            image,
            x,
            y,
            scale,
            uv: [0.0, 0.0, 1.0, 1.0],
            size: None,
            color: [1.0; 4],
        }
    }
}

/// ASCII 32..=127 in a 16 x 6 grid of 8 x 8 cells.
const FONT_COLS: u32 = 16;
const FONT_ROWS: u32 = 6;

fn font_atlas() -> Vec<u8> {
    use font8x8::{UnicodeFonts, BASIC_FONTS};
    let (w, h) = (FONT_COLS * 8, FONT_ROWS * 8);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for code in 32u32..128 {
        let Some(glyph) = char::from_u32(code).and_then(|c| BASIC_FONTS.get(c)) else {
            continue;
        };
        let (cx, cy) = ((code - 32) % FONT_COLS * 8, (code - 32) / FONT_COLS * 8);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits & (1 << col) != 0 {
                    let i = (((cy + row as u32) * w + cx + col) * 4) as usize;
                    px[i..i + 4].copy_from_slice(&[255; 4]);
                }
            }
        }
    }
    px
}

impl Hud {
    pub fn new(gpu: &Gpu) -> Self {
        let d = &gpu.device;
        let screen = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud screen"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let screen_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud screen"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen.as_entire_binding(),
            }],
        });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud"),
            bind_group_layouts: &[Some(&screen_layout), Some(&layout)],
            immediate_size: 0,
        });
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hud"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/hud.wgsl").into()),
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            primitive: Default::default(),
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
        let sampler = gpu.sampler(false, false);
        let capacity = 64;
        let vbuf = Self::buffer(gpu, capacity);
        let mut hud = Self {
            pipeline,
            layout,
            screen,
            screen_group,
            sampler,
            images: Vec::new(),
            vbuf,
            capacity,
            font: None,
            white: None,
        };
        hud.font = Some(hud.add_image(gpu, FONT_COLS * 8, FONT_ROWS * 8, &font_atlas()));
        hud.white = Some(hud.add_image(gpu, 1, 1, &[255; 4]));
        hud
    }

    fn buffer(gpu: &Gpu, sprites: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud quads"),
            size: (sprites * 6 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Adds an RGBA8 image; returns its id.
    pub fn add_image(&mut self, gpu: &Gpu, w: u32, h: u32, rgba: &[u8]) -> usize {
        let tex = gpu.device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("hud image"),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = tex.create_view(&Default::default());
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud image"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.images.push(Image { w, h, group });
        self.images.len() - 1
    }

    /// Text in the 8x8 font (`ddTextOut`); one sprite per glyph.
    pub fn text(&self, x: f32, y: f32, scale: f32, color: [f32; 4], text: &str) -> Vec<Sprite> {
        let (Some(font), cw, ch) = (self.font, 1.0 / FONT_COLS as f32, 1.0 / FONT_ROWS as f32)
        else {
            return Vec::new();
        };
        text.chars()
            .enumerate()
            .filter_map(|(i, c)| {
                let code = c as u32;
                if !(33..128).contains(&code) {
                    return None;
                }
                let (col, row) = ((code - 32) % FONT_COLS, (code - 32) / FONT_COLS);
                let (u, v) = (col as f32 * cw, row as f32 * ch);
                Some(Sprite {
                    uv: [u, v, u + cw, v + ch],
                    color,
                    ..Sprite::new(font, x + i as f32 * 8.0 * scale, y, scale)
                })
            })
            .collect()
    }

    /// A solid rectangle (crosshair lines).
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Option<Sprite> {
        let white = self.white?;
        Some(Sprite {
            size: Some([w, h]),
            color,
            ..Sprite::new(white, x, y, 1.0)
        })
    }

    pub fn image(&self, id: usize) -> Option<&Image> {
        self.images.get(id)
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        screen: (u32, u32),
        sprites: &[Sprite],
    ) {
        let sprites: Vec<&Sprite> = sprites
            .iter()
            .filter(|s| s.image < self.images.len())
            .collect();
        if sprites.is_empty() {
            return;
        }
        if sprites.len() > self.capacity {
            self.capacity = sprites.len().next_power_of_two();
            self.vbuf = Self::buffer(gpu, self.capacity);
        }
        gpu.queue.write_buffer(
            &self.screen,
            0,
            bytemuck::cast_slice(&[screen.0 as f32, screen.1 as f32, 0.0, 0.0]),
        );
        let mut verts = Vec::with_capacity(sprites.len() * 6);
        for s in &sprites {
            let im = &self.images[s.image];
            let [u0, v0, u1, v1] = s.uv;
            let [w, h] = s.size.unwrap_or([
                im.w as f32 * (u1 - u0) * s.scale,
                im.h as f32 * (v1 - v0) * s.scale,
            ]);
            let (x0, y0, x1, y1) = (s.x, s.y, s.x + w, s.y + h);
            let c = s.color;
            let v = |x, y, u, w| Vertex {
                pos: [x, y],
                uv: [u, w],
                color: c,
            };
            verts.extend_from_slice(&[
                v(x0, y0, u0, v0),
                v(x1, y0, u1, v0),
                v(x1, y1, u1, v1),
                v(x0, y0, u0, v0),
                v(x1, y1, u1, v1),
                v(x0, y1, u0, v1),
            ]);
        }
        gpu.queue
            .write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&verts));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_group, &[]);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        for (i, s) in sprites.iter().enumerate() {
            pass.set_bind_group(1, &self.images[s.image].group, &[]);
            let first = (i * 6) as u32;
            pass.draw(first..first + 6, 0..1);
        }
    }
}

/// Loads a TGA as an opaque RGBA image (the original blits them as-is).
pub fn load_tga(path: &std::path::Path) -> anyhow::Result<(u32, u32, Vec<u8>)> {
    let pic = c2_formats::tga::parse(&std::fs::read(path)?)?;
    Ok((
        pic.width as u32,
        pic.height as u32,
        c2_core::color::image_to_rgba8(&pic.pixels, false),
    ))
}
