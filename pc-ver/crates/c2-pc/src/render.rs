//! GPU renderers for models, terrain and sky, shared by the viewers.

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
    pub proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub fog_color: [f32; 4],
    pub fog: [f32; 4],
    pub misc: [f32; 4],
}

/// Per-frame values shared by every shader.
pub struct FrameParams {
    pub view: Mat4,
    pub proj: Mat4,
    pub eye: Vec3,
    pub fog_rgb: [f32; 3],
    pub fog: (f32, f32),
    pub night_vision: bool,
    pub time_s: f32,
    /// `(OptBrightness + 128) / 256`, applied to every texture.
    pub brightness: f32,
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

    pub fn write(&self, gpu: &Gpu, f: &FrameParams) {
        let d = GlobalsData {
            view_proj: (f.proj * f.view).to_cols_array_2d(),
            proj: f.proj.to_cols_array_2d(),
            cam_pos: f.eye.extend(1.0).to_array(),
            fog_color: [f.fog_rgb[0], f.fog_rgb[1], f.fog_rgb[2], 1.0],
            fog: [f.fog.0, f.fog.1, 0.0, 0.0],
            misc: [
                f32::from(u8::from(f.night_vision)),
                f.time_s,
                f.brightness,
                0.0,
            ],
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

fn texture_group(
    gpu: &Gpu,
    label: &str,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &texture_entries(0),
        });
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (layout, group)
}

fn depth_state(write: bool, compare: wgpu::CompareFunction) -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: Default::default(),
        bias: Default::default(),
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Blend {
    Opaque,
    /// Alpha blending, depth-tested but not written.
    Alpha,
    /// Alpha blending without depth testing (sky).
    Background,
    /// `dst + src` without depth testing (sun, moon).
    Additive,
    /// `dst + src`, depth-tested but not written (water rings).
    AdditiveDepth,
}

struct PipelineDesc<'a> {
    label: &'a str,
    module: &'a wgpu::ShaderModule,
    layout: &'a wgpu::PipelineLayout,
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    blend: Blend,
}

fn pipeline(gpu: &Gpu, d: PipelineDesc<'_>) -> wgpu::RenderPipeline {
    use wgpu::CompareFunction::{Always, Less};
    let additive = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent::OVER,
    };
    let (blend, depth_stencil) = match d.blend {
        Blend::Opaque => (None, depth_state(true, Less)),
        Blend::Alpha => (
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth_state(false, Less),
        ),
        Blend::Background => (
            Some(wgpu::BlendState::ALPHA_BLENDING),
            depth_state(false, Always),
        ),
        Blend::Additive => (Some(additive), depth_state(false, Always)),
        Blend::AdditiveDepth => (Some(additive), depth_state(false, Less)),
    };
    gpu.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(d.label),
            layout: Some(d.layout),
            vertex: wgpu::VertexState {
                module: d.module,
                entry_point: Some(d.vs),
                compilation_options: Default::default(),
                buffers: d.buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: d.module,
                entry_point: Some(d.fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.color_format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
}

fn pipeline_layout(
    gpu: &Gpu,
    label: &str,
    groups: &[&wgpu::BindGroupLayout],
) -> wgpu::PipelineLayout {
    let groups: Vec<Option<&wgpu::BindGroupLayout>> = groups.iter().copied().map(Some).collect();
    gpu.device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &groups,
            immediate_size: 0,
        })
}

// ---------------------------------------------------------------- models

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    vlight: [f32; 4],
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
    /// Per-vertex `VLight` for the 4 rotations; empty = unlit (all zero).
    vlights: Vec<[f32; 4]>,
    opaque: u32,
    total: u32,
}

impl GpuModel {
    pub fn new(gpu: &Gpu, m: &PreparedModel, vlights: Vec<[f32; 4]>) -> Self {
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
        let verts = Self::vertices(&order, &uvs, &vlights, &m.positions);
        let empty = [0u8; std::mem::size_of::<ModelVertex>()];
        let vbuf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("model vertices"),
                contents: if verts.is_empty() {
                    &empty
                } else {
                    bytemuck::cast_slice(&verts)
                },
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        Self {
            vbuf,
            order,
            uvs,
            vlights,
            opaque: opaque.len() as u32 * 3,
            total: (opaque.len() + blended.len()) as u32 * 3,
        }
    }

    fn vertices(
        order: &[u32],
        uvs: &[[f32; 2]],
        vlights: &[[f32; 4]],
        positions: &[[f32; 3]],
    ) -> Vec<ModelVertex> {
        order
            .iter()
            .zip(uvs)
            .map(|(&i, &uv)| ModelVertex {
                pos: positions.get(i as usize).copied().unwrap_or_default(),
                uv,
                vlight: vlights.get(i as usize).copied().unwrap_or_default(),
            })
            .collect()
    }

    /// Uploads a new pose (same vertex count as the rest pose).
    pub fn set_pose(&self, gpu: &Gpu, positions: &[[f32; 3]]) {
        let v = Self::vertices(&self.order, &self.uvs, &self.vlights, positions);
        if !v.is_empty() {
            gpu.queue
                .write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&v));
        }
    }
}

pub struct ModelRenderer {
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    textures: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
}

fn model_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ModelVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRS,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRS,
        }),
    ]
}

const VERTEX_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![3 => Float32x3, 4 => Float32, 5 => Float32, 6 => Uint32];

impl ModelRenderer {
    /// `textures`: one 256x256 RGBA8 texture per model, indexed by `Instance::layer`.
    pub fn new(gpu: &Gpu, globals: &Globals, textures: &[&[u8]]) -> Self {
        Self::with_entry(gpu, globals, textures, "vs")
    }

    /// For the first-person weapon (always in front of the world).
    pub fn near(gpu: &Gpu, globals: &Globals, textures: &[&[u8]]) -> Self {
        Self::with_entry(gpu, globals, textures, "vs_near")
    }

    /// Additive, depth-tested (water rings).
    pub fn additive(gpu: &Gpu, globals: &Globals, textures: &[&[u8]]) -> Self {
        let mut r = Self::with_entry(gpu, globals, textures, "vs");
        r.opaque = r.blend_variant(gpu, globals, "fs_add", Blend::AdditiveDepth);
        r.blend = r.blend_variant(gpu, globals, "fs_add", Blend::AdditiveDepth);
        r
    }

    fn blend_variant(
        &self,
        gpu: &Gpu,
        globals: &Globals,
        fs: &str,
        blend: Blend,
    ) -> wgpu::RenderPipeline {
        let layout = pipeline_layout(gpu, "model", &[&globals.layout, &self.tex_layout]);
        let module = shader(gpu, "model", include_str!("shaders/model.wgsl"));
        let buffers = model_buffers();
        pipeline(
            gpu,
            PipelineDesc {
                label: "model additive",
                module: &module,
                layout: &layout,
                vs: "vs",
                fs,
                buffers: &buffers,
                blend,
            },
        )
    }

    fn with_entry(gpu: &Gpu, globals: &Globals, textures: &[&[u8]], vs: &str) -> Self {
        let size = MODEL_TEXTURE_SIZE as u32;
        let view = gpu.texture_array("model textures", size, size, textures);
        let (tex_layout, group) = texture_group(gpu, "model tex", &view, &gpu.sampler(true, false));
        let layout = pipeline_layout(gpu, "model", &[&globals.layout, &tex_layout]);
        let module = shader(gpu, "model", include_str!("shaders/model.wgsl"));
        let buffers = model_buffers();
        let desc = |label, fs, blend| PipelineDesc {
            label,
            module: &module,
            layout: &layout,
            vs,
            fs,
            buffers: &buffers,
            blend,
        };
        let opaque = pipeline(gpu, desc("model opaque", "fs_opaque", Blend::Opaque));
        let blend = pipeline(gpu, desc("model blend", "fs_blend", Blend::Alpha));
        let capacity = 1024;
        let instances = Self::instance_buffer(gpu, capacity);
        Self {
            opaque,
            blend,
            tex_layout,
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
    time_ms: u32,
    clouds: u32,
    pad: [u32; 2],
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
        let view = gpu.texture_array("terrain textures", 128, 128, &layers);
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
        let module = shader(gpu, "terrain", include_str!("shaders/terrain.wgsl"));
        let desc = |label, vs, blend| PipelineDesc {
            label,
            module: &module,
            layout: &pl,
            vs,
            fs: "fs",
            buffers: &[],
            blend,
        };
        let ground = pipeline(gpu, desc("ground", "vs_ground", Blend::Opaque));
        let water = pipeline(gpu, desc("water", "vs_water", Blend::Alpha));
        Self {
            ground,
            water,
            group,
            params,
            width: 0,
        }
    }

    /// Sets the square window of cells to draw, centred on `center` cell.
    pub fn set_window(
        &mut self,
        gpu: &Gpu,
        center: (i32, i32),
        radius: u32,
        layers: u32,
        time_ms: u32,
    ) {
        self.width = radius * 2;
        let p = TerrainParams {
            origin: [center.0 - radius as i32, center.1 - radius as i32],
            width: self.width,
            layers: layers.max(1),
            time_ms,
            // Game.cpp sets Clouds = TRUE; there is no option to turn it off.
            clouds: 1,
            pad: [0; 2],
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

// ---------------------------------------------------------------- sky

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SunVertex {
    pos: [f32; 3],
    uv: [f32; 2],
}

const SUN_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

/// Cloud plane plus sun/moon billboard.
pub struct SkyRenderer {
    sky: wgpu::RenderPipeline,
    sun: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    /// Sun model triangles in model space.
    sun_tris: Vec<SunVertex>,
    sun_buf: wgpu::Buffer,
}

impl SkyRenderer {
    /// `sky_rgba`: 256x256 sky. `sun`: the SUN2/MOON model, if found.
    pub fn new(gpu: &Gpu, globals: &Globals, sky_rgba: &[u8], sun: Option<&PreparedModel>) -> Self {
        let blank = vec![0u8; 256 * 256 * 4];
        let sun_tex = sun.map_or(blank.as_slice(), |m| m.texture_rgba.as_slice());
        let view = gpu.texture_array("sky", 256, 256, &[sky_rgba, sun_tex]);
        let (tex_layout, group) = texture_group(gpu, "sky tex", &view, &gpu.sampler(true, true));
        let layout = pipeline_layout(gpu, "sky", &[&globals.layout, &tex_layout]);
        let module = shader(gpu, "sky", include_str!("shaders/sky.wgsl"));
        let sky = pipeline(
            gpu,
            PipelineDesc {
                label: "sky",
                module: &module,
                layout: &layout,
                vs: "vs_sky",
                fs: "fs_sky",
                buffers: &[],
                blend: Blend::Background,
            },
        );
        let sun_layout = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SunVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SUN_ATTRS,
        })];
        let sun_pipe = pipeline(
            gpu,
            PipelineDesc {
                label: "sun",
                module: &module,
                layout: &layout,
                vs: "vs_sun",
                fs: "fs_sun",
                buffers: &sun_layout,
                blend: Blend::Additive,
            },
        );
        let sun_tris: Vec<SunVertex> = sun
            .map(|m| {
                m.faces
                    .iter()
                    .flat_map(|f| {
                        (0..3).map(move |k| SunVertex {
                            pos: m.positions[f.v[k] as usize],
                            uv: f.uv[k],
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let sun_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun"),
            size: (sun_tris.len().max(1) * std::mem::size_of::<SunVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            sky,
            sun: sun_pipe,
            group,
            sun_tris,
            sun_buf,
        }
    }

    /// `sun_pos`: `Sun3dPos` (offset from the camera, world axes).
    pub fn draw(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        view: Mat4,
        sun_pos: [f32; 3],
        night: bool,
    ) {
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..6, 0..1);

        // RenderSun: sun position in camera space, pulled closer when near
        // the screen centre; drawn only when in front (nv.z < -2024).
        let nv = view.transform_vector3(Vec3::from(sun_pos));
        if self.sun_tris.is_empty() || nv.z >= -2024.0 {
            return;
        }
        let d = if night {
            1.5
        } else {
            (2048.0 + (nv.x * nv.x + nv.y * nv.y).sqrt().min(812.0)) / 3048.0
        };
        let c = nv * d;
        let verts: Vec<SunVertex> = self
            .sun_tris
            .iter()
            .map(|v| SunVertex {
                pos: (Vec3::from(v.pos) + c).to_array(),
                uv: v.uv,
            })
            .collect();
        gpu.queue
            .write_buffer(&self.sun_buf, 0, bytemuck::cast_slice(&verts));
        pass.set_pipeline(&self.sun);
        pass.set_vertex_buffer(0, self.sun_buf.slice(..));
        pass.draw(0..verts.len() as u32, 0..1);
    }
}

// ---------------------------------------------------------------- particles

/// One disc: centre, world radius, centre and edge colour (RGBA 0..1).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Disc {
    pub center: [f32; 3],
    pub radius: f32,
    pub c0: [f32; 4],
    pub c1: [f32; 4],
}

const DISC_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Float32x4];

pub struct DiscRenderer {
    pipeline: wgpu::RenderPipeline,
    buf: wgpu::Buffer,
    capacity: usize,
}

impl DiscRenderer {
    pub fn new(gpu: &Gpu, globals: &Globals) -> Self {
        let layout = pipeline_layout(gpu, "discs", &[&globals.layout]);
        let module = shader(gpu, "particles", include_str!("shaders/particles.wgsl"));
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Disc>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &DISC_ATTRS,
        })];
        let pipeline = pipeline(
            gpu,
            PipelineDesc {
                label: "discs",
                module: &module,
                layout: &layout,
                vs: "vs",
                fs: "fs",
                buffers: &buffers,
                blend: Blend::Alpha,
            },
        );
        let capacity = 256;
        let buf = Self::buffer(gpu, capacity);
        Self {
            pipeline,
            buf,
            capacity,
        }
    }

    fn buffer(gpu: &Gpu, n: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("discs"),
            size: (n * std::mem::size_of::<Disc>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        discs: &[Disc],
    ) {
        if discs.is_empty() {
            return;
        }
        if discs.len() > self.capacity {
            self.capacity = discs.len().next_power_of_two();
            self.buf = Self::buffer(gpu, self.capacity);
        }
        gpu.queue
            .write_buffer(&self.buf, 0, bytemuck::cast_slice(discs));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_vertex_buffer(0, self.buf.slice(..));
        pass.draw(0..6, 0..discs.len() as u32);
    }
}
