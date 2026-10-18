//! GPU renderers for models and terrain, shared by the viewers.

use bytemuck::{Pod, Zeroable};
use c2_core::model::{PreparedFace, PreparedModel, MODEL_TEXTURE_SIZE};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::gpu::{Gpu, DEPTH_FORMAT};

const COMMON: &str = include_str!("shaders/common.wgsl");

fn shader(gpu: &Gpu, label: &str, body: &str) -> wgpu::ShaderModule {
    gpu.device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(format!("{COMMON}\n{body}").into()),
        })
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlobalsData {
    pub view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub fog_color: [f32; 4],
    pub fog: [f32; 4],
}

pub struct Globals {
    buf: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
    pub group: wgpu::BindGroup,
}

impl Globals {
    pub fn new(gpu: &Gpu) -> Self {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<GlobalsData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        Self { buf, layout, group }
    }

    pub fn write(&self, gpu: &Gpu, view_proj: Mat4, eye: Vec3, fog_rgb: [f32; 3], fog: (f32, f32)) {
        let d = GlobalsData {
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: eye.extend(1.0).to_array(),
            fog_color: [fog_rgb[0], fog_rgb[1], fog_rgb[2], 1.0],
            fog: [fog.0, fog.1, 0.0, 0.0],
        };
        gpu.queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&d));
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entries(first: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
    [
        wgpu::BindGroupLayoutEntry {
            binding: first,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: first + 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

fn depth(write: bool) -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: Default::default(),
        bias: Default::default(),
    })
}

#[allow(clippy::too_many_arguments)]
fn pipeline(
    gpu: &Gpu,
    label: &str,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vs: &str,
    fs: &str,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    blend: bool,
) -> wgpu::RenderPipeline {
    gpu.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth(!blend),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.color_format,
                    blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
}

// ---------------------------------------------------------------- models

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelVertex {
    pos: [f32; 3],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Instance {
    pub offset: [f32; 3],
    pub turns: f32,
    pub light: f32,
    pub layer: u32,
}

/// One model on the GPU: faces expanded to unshared vertices (opaque first,
/// then blended), so a new animation pose is a single buffer write.
pub struct GpuModel {
    vbuf: wgpu::Buffer,
    /// Face corner -> vertex index, in draw order.
    order: Vec<u32>,
    uvs: Vec<[f32; 2]>,
    opaque: u32,
    total: u32,
}

impl GpuModel {
    pub fn new(gpu: &Gpu, m: &PreparedModel) -> Self {
        let mut order = Vec::new();
        let mut uvs = Vec::new();
        let (opaque, blended): (Vec<&PreparedFace>, Vec<&PreparedFace>) =
            m.faces.iter().partition(|f| !f.is_transparent());
        for f in opaque.iter().chain(&blended) {
            for k in 0..3 {
                order.push(f.v[k]);
                uvs.push(f.uv[k]);
            }
        }
        let verts = Self::vertices(&order, &uvs, &m.positions);
        let vbuf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("model vertices"),
                contents: if verts.is_empty() {
                    &[0u8; 20]
                } else {
                    bytemuck::cast_slice(&verts)
                },
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        Self {
            vbuf,
            order,
            uvs,
            opaque: opaque.len() as u32 * 3,
            total: (opaque.len() + blended.len()) as u32 * 3,
        }
    }

    fn vertices(order: &[u32], uvs: &[[f32; 2]], positions: &[[f32; 3]]) -> Vec<ModelVertex> {
        order
            .iter()
            .zip(uvs)
            .map(|(&i, &uv)| ModelVertex {
                pos: positions.get(i as usize).copied().unwrap_or_default(),
                uv,
            })
            .collect()
    }

    /// Uploads a new pose (same vertex count as the rest pose).
    pub fn set_pose(&self, gpu: &Gpu, positions: &[[f32; 3]]) {
        let v = Self::vertices(&self.order, &self.uvs, positions);
        if !v.is_empty() {
            gpu.queue
                .write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&v));
        }
    }
}

pub struct ModelRenderer {
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    textures: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
}

const VERTEX_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32, 4 => Float32, 5 => Uint32];

impl ModelRenderer {
    /// `textures`: one 256x256 RGBA8 texture per model, indexed by `Instance::layer`.
    pub fn new(gpu: &Gpu, globals: &Globals, textures: &[&[u8]]) -> Self {
        let tex_layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("model tex"),
                entries: &texture_entries(0),
            });
        let size = MODEL_TEXTURE_SIZE as u32;
        let view = gpu.texture_array("model textures", size, size, textures);
        let sampler = gpu.sampler(true);
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model tex"),
            layout: &tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("model"),
                bind_group_layouts: &[Some(&globals.layout), Some(&tex_layout)],
                immediate_size: 0,
            });
        let module = shader(gpu, "model", include_str!("shaders/model.wgsl"));
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRS,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &INSTANCE_ATTRS,
            }),
        ];
        let opaque = pipeline(
            gpu,
            "model opaque",
            &module,
            &layout,
            "vs",
            "fs_opaque",
            &buffers,
            false,
        );
        let blend = pipeline(
            gpu,
            "model blend",
            &module,
            &layout,
            "vs",
            "fs_blend",
            &buffers,
            true,
        );
        let capacity = 1024;
        let instances = Self::instance_buffer(gpu, capacity);
        Self {
            opaque,
            blend,
            textures: group,
            instances,
            capacity,
        }
    }

    fn instance_buffer(gpu: &Gpu, n: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (n * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Draws `batches` of (model, instances). Opaque faces of every batch go
    /// first, then the blended faces, so transparency sorts over solids.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        batches: &[(&GpuModel, &[Instance])],
    ) {
        let total: usize = batches.iter().map(|(_, i)| i.len()).sum();
        if total == 0 {
            return;
        }
        if total > self.capacity {
            self.capacity = total.next_power_of_two();
            self.instances = Self::instance_buffer(gpu, self.capacity);
        }
        let all: Vec<Instance> = batches
            .iter()
            .flat_map(|(_, i)| i.iter().copied())
            .collect();
        gpu.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&all));
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_bind_group(1, &self.textures, &[]);
        pass.set_vertex_buffer(1, self.instances.slice(..));
        for (pipe, blended) in [(&self.opaque, false), (&self.blend, true)] {
            pass.set_pipeline(pipe);
            let mut first = 0u32;
            for (m, inst) in batches {
                let n = inst.len() as u32;
                let range = if blended {
                    m.opaque..m.total
                } else {
                    0..m.opaque
                };
                if !range.is_empty() && n > 0 {
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.draw(range, first..first + n);
                }
                first += n;
            }
        }
    }
}

// ---------------------------------------------------------------- terrain

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainParams {
    origin: [i32; 2],
    width: u32,
    layers: u32,
}

pub struct TerrainRenderer {
    ground: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    params: wgpu::Buffer,
    width: u32,
}

impl TerrainRenderer {
    pub fn new(gpu: &Gpu, globals: &Globals, area: &c2_core::area::Area) -> Self {
        let m = &area.map;
        let light = area.light_map();
        let cells: Vec<[u32; 2]> = (0..m.height.len())
            .map(|i| {
                [
                    u32::from(m.height[i]) | u32::from(light[i]) << 8 | u32::from(m.water[i]) << 16,
                    u32::from(m.texture1[i]) | u32::from(m.flags[i]) << 16,
                ]
            })
            .collect();
        let mut waters: Vec<[f32; 4]> = area
            .rsc
            .waters
            .iter()
            .map(|w| {
                [
                    w.level as f32 * c2_core::H_SCALE,
                    w.texture_index as f32,
                    w.transp.clamp(0.0, 1.0),
                    0.0,
                ]
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
        let layers: Vec<&[u8]> = rgba.iter().map(Vec::as_slice).collect();
        let fallback = [255u8; 128 * 128 * 4];
        let layers = if layers.is_empty() {
            vec![&fallback[..]]
        } else {
            layers
        };
        let view = gpu.texture_array("terrain textures", 128, 128, &layers);
        let sampler = gpu.sampler(true);

        let mk = |label, contents: &[u8], usage| {
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        let cell_buf = mk(
            "cells",
            bytemuck::cast_slice(&cells),
            wgpu::BufferUsages::STORAGE,
        );
        let water_buf = mk(
            "waters",
            bytemuck::cast_slice(&waters),
            wgpu::BufferUsages::STORAGE,
        );
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
            ],
        });
        let pl = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain"),
                bind_group_layouts: &[Some(&globals.layout), Some(&layout)],
                immediate_size: 0,
            });
        let module = shader(gpu, "terrain", include_str!("shaders/terrain.wgsl"));
        let ground = pipeline(gpu, "ground", &module, &pl, "vs_ground", "fs", &[], false);
        let water = pipeline(gpu, "water", &module, &pl, "vs_water", "fs", &[], true);
        Self {
            ground,
            water,
            group,
            params,
            width: 0,
        }
    }

    /// Sets the square window of cells to draw, centred on `center` cell.
    pub fn set_window(&mut self, gpu: &Gpu, center: (i32, i32), radius: u32, layers: u32) {
        self.width = radius * 2;
        let p = TerrainParams {
            origin: [center.0 - radius as i32, center.1 - radius as i32],
            width: self.width,
            layers: layers.max(1),
        };
        gpu.queue
            .write_buffer(&self.params, 0, bytemuck::bytes_of(&p));
    }

    fn vertex_count(&self) -> u32 {
        self.width * self.width * 6
    }

    pub fn draw_ground(&self, pass: &mut wgpu::RenderPass<'_>, globals: &Globals) {
        pass.set_pipeline(&self.ground);
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.draw(0..self.vertex_count(), 0..1);
    }

    pub fn draw_water(&self, pass: &mut wgpu::RenderPass<'_>, globals: &Globals) {
        pass.set_pipeline(&self.water);
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.draw(0..self.vertex_count(), 0..1);
    }
}
