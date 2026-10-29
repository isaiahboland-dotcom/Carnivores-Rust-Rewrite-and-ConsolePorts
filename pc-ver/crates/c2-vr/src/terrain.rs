//! An area's ground and water in the headset (GAMEPLAN milestone 2): the
//! PC's own `terrain.wgsl` (with `common.wgsl`), compiled to SPIR-V by naga,
//! so both draw the terrain the same way; the fragment entry is
//! `shaders/terrain_vr.wgsl` (gamma colour into sRGB images). Objects are
//! `objects`, drawn between the ground and the water as on PC; the sky and
//! sun (`sky`) go first.
//!
//! The player stands at the area's first landing spot. The tracking space
//! (metres) maps onto the world at `UNITS_PER_M`, so a standing player's eyes
//! are about the hunter's (`HEAD_STAND`); the world's floor follows the
//! ground under the head. Left stick walks, right stick snap-turns.

#![allow(unsafe_code)] // Raw Vulkan.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use ash::vk;
use c2_core::area::{Area, TimeOfDay};
use c2_core::{CELL, H_SCALE};
use glam::{Mat4, Quat, Vec3};

use crate::animals::{Animals, Posed};
use crate::gpu::{Buffer, Gpu, Image};
use crate::objects::Objects;
use crate::scene::{self, Eye, Frame, Scene};
use crate::sky::Sky;
use crate::xr::DEPTH;

/// World units per metre: the hunter's standing eyes (220) at
/// `EYE_HEIGHT_M`.
pub const UNITS_PER_M: f32 = c2_core::player::HEAD_STAND / scene::EYE_HEIGHT_M;
/// Cells drawn around the player (the PC's default view range).
const RADIUS: u32 = 74;
/// The PC's near plane (`camera.rs: projection`).
const NEAR: f32 = 16.0;
const WALK_M_S: f32 = 3.0;
const SNAP: f32 = std::f32::consts::PI / 6.0;
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
const MAP: usize = c2_formats::map::MAP_SIZE;

const COMMON: &str = include_str!("../../c2-pc/src/shaders/common.wgsl");
const TERRAIN: &str = include_str!("../../c2-pc/src/shaders/terrain.wgsl");
const TERRAIN_VR: &str = include_str!("../shaders/terrain_vr.wgsl");

/// `GlobalsData` in `c2-pc/src/render.rs` (`Globals` in `common.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Globals {
    view_proj: [f32; 16],
    proj: [f32; 16],
    cam_pos: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    misc: [f32; 4],
}

/// `Params` in `terrain.wgsl`.
#[derive(Clone, Copy, Default)]
struct Params {
    origin: [i32; 2],
    width: u32,
    layers: u32,
    time_ms: u32,
    clouds: u32,
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

impl Globals {
    fn bytes(&self) -> Vec<u8> {
        let mut v = floats(&self.view_proj);
        for a in [
            &self.proj[..],
            &self.cam_pos,
            &self.fog_color,
            &self.fog,
            &self.misc,
        ] {
            v.extend(floats(a));
        }
        v
    }
}

impl Params {
    fn bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(32);
        for i in self.origin {
            v.extend(i.to_ne_bytes());
        }
        for u in [self.width, self.layers, self.time_ms, self.clouds, 0, 0] {
            v.extend(u.to_ne_bytes());
        }
        v
    }
}

/// `path`'s sibling with extension `ext`, any case (`c2-pc area_view.rs`).
fn sibling(path: &Path, ext: &str) -> Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&stem))
                && p.extension()
                    .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(ext))
        })
        .ok_or_else(|| anyhow!("no {stem}.{ext} next to {}", path.display()))
}

/// The area at `map_path` (its `.RSC` beside it), in daylight.
pub fn load(map_path: &Path) -> Result<Area> {
    load_at(map_path, TimeOfDay::Day)
}

/// The area at `map_path` at `time`.
pub fn load_at(map_path: &Path, time: TimeOfDay) -> Result<Area> {
    let rsc_path = sibling(map_path, "rsc")?;
    let m = c2_formats::map::parse(
        &std::fs::read(map_path).with_context(|| format!("reading {}", map_path.display()))?,
    )?;
    let r = c2_formats::rsc::parse(
        &std::fs::read(&rsc_path).with_context(|| format!("reading {}", rsc_path.display()))?,
    )?;
    Ok(Area::new(m, r, time))
}

/// `common.wgsl` + `terrain.wgsl` + `terrain_vr.wgsl` as SPIR-V, one module
/// per entry point.
fn compile(entry: &str, stage: naga::ShaderStage) -> Result<Vec<u32>> {
    scene::wgsl(&format!("{COMMON}\n{TERRAIN}\n{TERRAIN_VR}"), entry, stage)
}

/// A projection from OpenXR's view tangents: right-handed, y up, depth
/// 0..1 (wgpu's convention, as the PC's matrices; naga flips y for Vulkan).
fn projection(tan: [f32; 4], near: f32, far: f32) -> Mat4 {
    let [l, r, u, d] = tan;
    Mat4::from_cols_array(&[
        2.0 / (r - l),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / (u - d),
        0.0,
        0.0,
        (r + l) / (r - l),
        (u + d) / (u - d),
        far / (near - far),
        -1.0,
        0.0,
        0.0,
        near * far / (near - far),
        0.0,
    ])
}

pub struct TerrainScene {
    area: Area,
    render_pass: vk::RenderPass,
    set_layouts: [vk::DescriptorSetLayout; 2],
    layout: vk::PipelineLayout,
    ground: vk::Pipeline,
    water: vk::Pipeline,
    pool: vk::DescriptorPool,
    /// Per frame in flight: a globals set per eye, and the terrain set.
    globals_sets: Vec<[vk::DescriptorSet; 2]>,
    terrain_sets: Vec<vk::DescriptorSet>,
    globals: Vec<[Buffer; 2]>,
    params: Vec<Buffer>,
    storage: Vec<Buffer>,
    textures: Image,
    textures_view: vk::ImageView,
    sampler: vk::Sampler,
    depth: Vec<(Image, vk::ImageView)>,
    layers: u32,
    /// The fog (and sky) colour, gamma space.
    fog_rgb: [f32; 3],
    /// Where the tracking space's floor origin is in the world, and its
    /// turn about y.
    origin: Vec3,
    yaw: f32,
    grounded: bool,
    start: Instant,
    objects: Objects,
    sky: Sky,
    /// The sticks walk and turn (not while a menu shows).
    pub walking: bool,
    /// The hunt's characters (`add_animals`), drawn after the objects.
    animals: Option<Animals>,
}

/// Drawn over the scene in each eye's pass (a menu): the eye (0, 1) and
/// its world-to-clip matrix.
pub type Overlay<'a> = &'a mut dyn FnMut(&Gpu, vk::CommandBuffer, usize, Mat4);

impl TerrainScene {
    /// `sun`: the sun (or moon) model (`sky::load_sun`), if found.
    pub fn new(
        gpu: &Gpu,
        format: vk::Format,
        area: Area,
        sun: Option<c2_core::model::PreparedModel>,
    ) -> Result<Self> {
        let m = &area.map;
        let light = area.light_map();
        let cells: Vec<u8> = (0..m.height.len())
            .flat_map(|i| {
                let a =
                    u32::from(m.height[i]) | u32::from(light[i]) << 8 | u32::from(m.water[i]) << 16;
                let b = u32::from(m.texture1[i]) | u32::from(m.flags[i]) << 16;
                [a.to_ne_bytes(), b.to_ne_bytes()].concat()
            })
            .collect();
        let mut waters: Vec<f32> = area
            .rsc
            .waters
            .iter()
            .flat_map(|w| {
                [
                    w.level as f32 * H_SCALE,
                    w.texture_index as f32,
                    w.transp.clamp(0.0, 1.0),
                    0.0,
                ]
            })
            .collect();
        if waters.is_empty() {
            waters.extend([0.0; 4]); // storage buffers can't be empty
        }
        let mut sky_map = area.rsc.sky_map.clone();
        sky_map.resize(128 * 128, 0);
        let storage_usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let storage = vec![
            gpu.buffer_with(&cells, storage_usage)?,
            gpu.buffer_with(&floats(&waters), storage_usage)?,
            gpu.buffer_with(&sky_map, storage_usage)?,
        ];

        // The area's 128x128 textures as one array (c2-pc: Rgba8Unorm).
        let mut texels: Vec<u8> = area
            .rsc
            .textures
            .iter()
            .flat_map(|t| c2_core::color::image_to_rgba8(t, false))
            .collect();
        let layers = (texels.len() / (128 * 128 * 4)).max(1) as u32;
        texels.resize(layers as usize * 128 * 128 * 4, 255);
        let (textures, textures_view) = gpu.texture_array(128, &texels, layers)?;

        let render_pass = scene::render_pass(gpu, format, Some(DEPTH_FORMAT))?;
        let all = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
        let binding = |b: u32, kind: vk::DescriptorType| {
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
            let set0 = dev.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .bindings(&[binding(0, D::UNIFORM_BUFFER)]),
                None,
            )?;
            let set1 = dev.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                    binding(0, D::STORAGE_BUFFER),
                    binding(1, D::STORAGE_BUFFER),
                    binding(2, D::UNIFORM_BUFFER),
                    binding(3, D::SAMPLED_IMAGE),
                    binding(4, D::SAMPLER),
                    binding(5, D::STORAGE_BUFFER),
                ]),
                None,
            )?;
            let layout = dev.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&[set0, set1]),
                None,
            )?;
            let n = DEPTH as u32;
            let pool = dev.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(n * 3)
                    .pool_sizes(&[
                        vk::DescriptorPoolSize {
                            ty: D::UNIFORM_BUFFER,
                            descriptor_count: n * 3,
                        },
                        vk::DescriptorPoolSize {
                            ty: D::STORAGE_BUFFER,
                            descriptor_count: n * 3,
                        },
                        vk::DescriptorPoolSize {
                            ty: D::SAMPLED_IMAGE,
                            descriptor_count: n,
                        },
                        vk::DescriptorPoolSize {
                            ty: D::SAMPLER,
                            descriptor_count: n,
                        },
                    ]),
                None,
            )?;
            let sampler = gpu.linear_sampler(false)?;

            let globals_size = std::mem::size_of::<Globals>() as u64;
            let uniform = vk::BufferUsageFlags::UNIFORM_BUFFER;
            let mut globals = Vec::new();
            let mut params = Vec::new();
            let mut globals_sets = Vec::new();
            let mut terrain_sets = Vec::new();
            for _ in 0..DEPTH {
                let g = [
                    gpu.buffer(globals_size, uniform)?,
                    gpu.buffer(globals_size, uniform)?,
                ];
                let p = gpu.buffer(32, uniform)?;
                let s0 = dev.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(pool)
                        .set_layouts(&[set0, set0]),
                )?;
                let s1 = *dev
                    .allocate_descriptor_sets(
                        &vk::DescriptorSetAllocateInfo::default()
                            .descriptor_pool(pool)
                            .set_layouts(&[set1]),
                    )?
                    .first()
                    .ok_or_else(|| anyhow!("no descriptor set"))?;
                let buf = |b: &Buffer| {
                    [vk::DescriptorBufferInfo {
                        buffer: b.buffer,
                        offset: 0,
                        range: vk::WHOLE_SIZE,
                    }]
                };
                let infos = [
                    buf(&g[0]),
                    buf(&g[1]),
                    buf(&storage[0]),
                    buf(&storage[1]),
                    buf(&p),
                    buf(&storage[2]),
                ];
                let image = [vk::DescriptorImageInfo {
                    sampler: vk::Sampler::null(),
                    image_view: textures_view,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }];
                let samp = [vk::DescriptorImageInfo {
                    sampler,
                    ..Default::default()
                }];
                let w = |set, b, kind| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(b)
                        .descriptor_type(kind)
                };
                let [s0a, s0b] = [s0[0], s0[1]];
                dev.update_descriptor_sets(
                    &[
                        w(s0a, 0, D::UNIFORM_BUFFER).buffer_info(&infos[0]),
                        w(s0b, 0, D::UNIFORM_BUFFER).buffer_info(&infos[1]),
                        w(s1, 0, D::STORAGE_BUFFER).buffer_info(&infos[2]),
                        w(s1, 1, D::STORAGE_BUFFER).buffer_info(&infos[3]),
                        w(s1, 2, D::UNIFORM_BUFFER).buffer_info(&infos[4]),
                        w(s1, 3, D::SAMPLED_IMAGE).image_info(&image),
                        w(s1, 4, D::SAMPLER).image_info(&samp),
                        w(s1, 5, D::STORAGE_BUFFER).buffer_info(&infos[5]),
                    ],
                    &[],
                );
                globals.push(g);
                params.push(p);
                globals_sets.push([s0a, s0b]);
                terrain_sets.push(s1);
            }

            let vs_ground = gpu.shader(&compile("vs_ground", naga::ShaderStage::Vertex)?)?;
            let vs_water = gpu.shader(&compile("vs_water", naga::ShaderStage::Vertex)?)?;
            let fs = gpu.shader(&compile("fs_vr", naga::ShaderStage::Fragment)?)?;
            let desc = |vert, depth_write, blend| scene::PipelineDesc {
                vert,
                frag: (fs, c"fs_vr"),
                layout,
                render_pass,
                depth: Some(depth_write),
                blend,
                bindings: &[],
                attributes: &[],
            };
            let ground = scene::pipeline(
                gpu,
                &desc((vs_ground, c"vs_ground"), true, scene::Blend::Opaque),
            );
            let water = scene::pipeline(
                gpu,
                &desc((vs_water, c"vs_water"), false, scene::Blend::Alpha),
            );
            for m in [vs_ground, vs_water, fs] {
                dev.destroy_shader_module(m, None);
            }

            let (lx, ly) = area.landings.first().copied().unwrap_or((512, 512));
            let (x, z) = (f32::from(lx) * CELL + 128.0, f32::from(ly) * CELL + 128.0);
            let fog_rgb = area.sky_rgb().map(|c| (c as f32 / 255.0).clamp(0.0, 1.0));
            let objects = Objects::new(gpu, &area, render_pass, set0)?;
            let sky = Sky::new(gpu, &area, sun.as_ref(), render_pass, set0)?;
            Ok(Self {
                objects,
                sky,
                origin: Vec3::new(x, area.land_height(x, z), z),
                area,
                render_pass,
                set_layouts: [set0, set1],
                layout,
                ground: ground?,
                water: water?,
                pool,
                globals_sets,
                terrain_sets,
                globals,
                params,
                storage,
                textures,
                textures_view,
                sampler,
                depth: Vec::new(),
                layers,
                fog_rgb,
                yaw: 0.0,
                grounded: false,
                start: Instant::now(),
                walking: true,
                animals: None,
            })
        }
    }

    pub fn area(&self) -> &Area {
        &self.area
    }

    /// Cells drawn around the player, in world units.
    pub fn view_range(&self) -> f32 {
        RADIUS as f32 * CELL
    }

    /// Where the head is in the world, and its heading (0 looks to -z).
    pub fn head(&self, f: &Frame) -> (Vec3, f32) {
        let look = Quat::from_rotation_y(self.yaw) * f.eyes[0].rot * Vec3::NEG_Z;
        (self.world(f.head(), f.floor), f32::atan2(-look.x, -look.z))
    }

    /// Characters to draw (`models`: a texture layer each, as `Posed`
    /// names them).
    pub fn add_animals(
        &mut self,
        gpu: &Gpu,
        models: &[Option<&c2_core::model::PreparedModel>],
    ) -> Result<()> {
        if self.animals.is_none() {
            self.animals = Some(Animals::new(
                gpu,
                models,
                self.render_pass,
                self.set_layouts[0],
            )?);
        }
        Ok(())
    }

    /// A point in the tracking space, in the world.
    pub fn world(&self, p: Vec3, floor: f32) -> Vec3 {
        self.origin + Quat::from_rotation_y(self.yaw) * ((p - Vec3::Y * floor) * UNITS_PER_M)
    }

    /// Puts the head over the world point (`x`, `z`) on the ground, looking
    /// the way `heading` (world, y up; 0 looks to -z) says.
    pub fn place_head(&mut self, f: &Frame, x: f32, z: f32, heading: f32) {
        let head = f.head();
        let look = f.eyes[0].rot * Vec3::NEG_Z;
        // The tracking space's heading turned onto `heading`.
        self.yaw = heading - f32::atan2(-look.x, -look.z);
        let h = self.world(head, f.floor) - self.origin;
        self.origin = Vec3::new(x - h.x, self.area.land_height(x, z), z - h.z);
        self.grounded = false;
    }

    /// Snap turns about the head, the sticks' walking (when `walking`), the
    /// ground under the head.
    pub fn move_player(&mut self, f: &Frame) {
        let head = f.head();
        if self.walking && f.input.snap_turn != 0 {
            let before = self.world(head, f.floor);
            self.yaw -= SNAP * f.input.snap_turn as f32;
            let after = self.world(head, f.floor);
            self.origin.x += before.x - after.x;
            self.origin.z += before.z - after.z;
        }
        if self.walking && f.input.walk.length_squared() > 0.01 {
            let look = Quat::from_rotation_y(self.yaw) * f.eyes[0].rot * Vec3::NEG_Z;
            let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
            let right = Vec3::new(-fwd.z, 0.0, fwd.x);
            let step =
                (fwd * f.input.walk.y + right * f.input.walk.x) * (WALK_M_S * UNITS_PER_M * f.dt);
            self.origin += step;
        }
        let h = self.world(head, f.floor);
        let max = (MAP as f32 - 2.0) * CELL;
        self.origin.x += h.x.clamp(CELL, max) - h.x;
        self.origin.z += h.z.clamp(CELL, max) - h.z;
        let h = self.world(head, f.floor);
        let ground = self.area.land_height(h.x, h.z);
        // Eased so steps in the ground don't jolt the view.
        let k = if self.grounded {
            (f.dt * 8.0).min(1.0)
        } else {
            1.0
        };
        self.origin.y += (ground - self.origin.y) * k;
        self.grounded = true;
    }

    /// The eye's shader globals, and its view matrix.
    fn globals(&self, eye: &Eye, floor: f32, far: f32) -> (Globals, Mat4) {
        let pos = self.world(eye.pos, floor);
        let rot = Quat::from_rotation_y(self.yaw) * eye.rot;
        let view = Mat4::from_rotation_translation(rot, pos).inverse();
        let proj = projection(eye.tan, NEAR, far * 1.5);
        let f = self.fog_rgb;
        let g = Globals {
            view_proj: (proj * view).to_cols_array(),
            proj: proj.to_cols_array(),
            cam_pos: pos.extend(1.0).to_array(),
            fog_color: [f[0], f[1], f[2], 1.0],
            fog: [far * 0.6, far, 0.0, 0.0],
            misc: [
                f32::from(u8::from(self.area.time.night_vision())),
                self.start.elapsed().as_secs_f32(),
                1.0,
                0.0,
            ],
        };
        (g, view)
    }
}

impl Scene for TerrainScene {
    fn render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    fn eye_targets(&mut self, gpu: &Gpu, extent: vk::Extent2D) -> Result<[Vec<vk::ImageView>; 2]> {
        if !self.depth.is_empty() {
            bail!("eye targets made twice");
        }
        for _ in 0..2 {
            let image = gpu.image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(DEPTH_FORMAT)
                    .extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT),
            )?;
            // SAFETY: a view of the image just made.
            let view = unsafe {
                gpu.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image.image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(DEPTH_FORMAT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )?
            };
            self.depth.push((image, view));
        }
        Ok([vec![self.depth[0].1], vec![self.depth[1].1]])
    }

    fn record(&mut self, gpu: &Gpu, f: &Frame) -> Result<()> {
        self.move_player(f);
        self.draw(gpu, f, &[], None)
    }

    fn destroy(&mut self, gpu: &Gpu) {
        self.destroy_all(gpu);
    }
}

impl TerrainScene {
    /// Both eyes with `posed` (characters, if `add_animals` was called),
    /// `overlay` drawn last in each.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        f: &Frame,
        posed: &[Posed],
        mut overlay: Option<Overlay<'_>>,
    ) -> Result<()> {
        let far = RADIUS as f32 * CELL;
        let head = self.world(f.head(), f.floor);
        let center = ((head.x / CELL) as i32, (head.z / CELL) as i32);
        let width = RADIUS * 2;
        let params = Params {
            origin: [center.0 - RADIUS as i32, center.1 - RADIUS as i32],
            width,
            layers: self.layers,
            time_ms: self.start.elapsed().as_millis() as u32,
            // Game.cpp sets Clouds = TRUE; there is no option to turn it off.
            clouds: 1,
        };
        self.params[f.slot].write(0, &params.bytes());
        // Sprites turn to the head's heading (`FlyCam` yaw: 0 looks to -z).
        let look = Quat::from_rotation_y(self.yaw) * f.eyes[0].rot * Vec3::NEG_Z;
        let yaw = f32::atan2(-look.x, -look.z);
        self.objects.prepare(f.slot, head, RADIUS, yaw);
        if let Some(a) = self.animals.as_mut() {
            a.prepare(gpu, f.slot, posed)?;
        }
        let clear = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [
                        scene::to_linear(self.fog_rgb[0]),
                        scene::to_linear(self.fog_rgb[1]),
                        scene::to_linear(self.fog_rgb[2]),
                        1.0,
                    ],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        for (i, (eye, fb)) in f.eyes.iter().zip(f.framebuffers).enumerate() {
            let (g, view) = self.globals(eye, f.floor, far);
            self.globals[f.slot][i].write(0, &g.bytes());
            self.sky.prepare(f.slot, i, view);
            scene::begin(gpu, f.cmd, self.render_pass, fb, f.extent, &clear);
            let sets = [self.globals_sets[f.slot][i], self.terrain_sets[f.slot]];
            // SAFETY: `f.cmd` is recording inside our render pass; the sets'
            // buffers are this frame slot's.
            unsafe {
                let dev = &gpu.device;
                let gfx = vk::PipelineBindPoint::GRAPHICS;
                self.sky.draw(gpu, f.cmd, f.slot, i, sets[0]);
                dev.cmd_bind_descriptor_sets(f.cmd, gfx, self.layout, 0, &sets, &[]);
                dev.cmd_bind_pipeline(f.cmd, gfx, self.ground);
                dev.cmd_draw(f.cmd, width * width * 6, 1, 0, 0);
                self.objects.draw(gpu, f.cmd, f.slot, sets[0]);
                if let Some(a) = &self.animals {
                    a.draw(gpu, f.cmd, f.slot, sets[0]);
                }
                dev.cmd_bind_descriptor_sets(f.cmd, gfx, self.layout, 0, &sets, &[]);
                dev.cmd_bind_pipeline(f.cmd, gfx, self.water);
                dev.cmd_draw(f.cmd, width * width * 6, 1, 0, 0);
                if let Some(o) = overlay.as_mut() {
                    o(gpu, f.cmd, i, Mat4::from_cols_array(&g.view_proj));
                }
                dev.cmd_end_render_pass(f.cmd);
            }
        }
        Ok(())
    }

    fn destroy_all(&mut self, gpu: &Gpu) {
        if let Some(a) = self.animals.as_mut() {
            a.destroy(gpu);
        }
        self.objects.destroy(gpu);
        self.sky.destroy(gpu);
        // SAFETY: the GPU is idle; each handle is destroyed once.
        unsafe {
            let dev = &gpu.device;
            for (image, view) in self.depth.drain(..) {
                dev.destroy_image_view(view, None);
                gpu.destroy_image(&image);
            }
            for b in self
                .globals
                .iter()
                .flatten()
                .chain(&self.params)
                .chain(&self.storage)
            {
                gpu.destroy_buffer(b);
            }
            dev.destroy_pipeline(self.ground, None);
            dev.destroy_pipeline(self.water, None);
            dev.destroy_pipeline_layout(self.layout, None);
            dev.destroy_descriptor_pool(self.pool, None);
            for l in self.set_layouts {
                dev.destroy_descriptor_set_layout(l, None);
            }
            dev.destroy_sampler(self.sampler, None);
            dev.destroy_image_view(self.textures_view, None);
            gpu.destroy_image(&self.textures);
            dev.destroy_render_pass(self.render_pass, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_shaders_compile_for_vulkan() {
        for (e, s) in [
            ("vs_ground", naga::ShaderStage::Vertex),
            ("vs_water", naga::ShaderStage::Vertex),
            ("fs_vr", naga::ShaderStage::Fragment),
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
    fn uniforms_match_the_shaders() {
        assert_eq!(Globals::default().bytes().len(), 192);
        assert_eq!(Params::default().bytes().len(), 32);
    }

    #[test]
    fn projection_maps_the_view_edges() {
        let p = projection([-1.0, 0.5, 0.8, -0.6], 16.0, 1000.0);
        let ndc = |v: Vec3| {
            let c = p * v.extend(1.0);
            c.truncate() / c.w
        };
        let a = ndc(Vec3::new(0.5 * 100.0, 0.8 * 100.0, -100.0));
        assert!((a.x - 1.0).abs() < 1e-5 && (a.y - 1.0).abs() < 1e-5);
        let b = ndc(Vec3::new(-100.0, -0.6 * 100.0, -100.0));
        assert!((b.x + 1.0).abs() < 1e-5 && (b.y + 1.0).abs() < 1e-5);
        assert!(ndc(Vec3::new(0.0, 0.0, -16.0)).z.abs() < 1e-5);
        assert!((ndc(Vec3::new(0.0, 0.0, -1000.0)).z - 1.0).abs() < 1e-5);
    }
}
