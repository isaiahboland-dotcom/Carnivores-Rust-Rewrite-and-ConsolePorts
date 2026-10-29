//! A menu screen on a panel standing in the world (`shaders/panel.wgsl`):
//! one of the game's 800x600 menu pictures, its lit version and its hit map
//! (`c2-pc menu.rs: Board`), so the regions light as on PC. It stands in the
//! tracking space where it was put (`place`); a controller's ray picks a
//! region (`pick`).

#![allow(unsafe_code)] // Raw Vulkan.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use ash::vk;
use glam::{Mat4, Vec3};

use crate::gpu::{Gpu, Image};
use crate::scene::{self, Ray};

const SHADER: &str = include_str!("../shaders/panel.wgsl");
/// The menu canvas (`c2-pc menu.rs: CW, CH`) and its hit map's size.
const CW: usize = 800;
const CH: usize = 600;
const HW: usize = 400;
const HH: usize = 300;
/// The panel's size and distance, metres: 4:3, about 45 degrees across.
const WIDTH_M: f32 = 2.0;
const HEIGHT_M: f32 = 1.5;
const DIST_M: f32 = 2.5;
/// Its middle this far below the eyes.
const DROP_M: f32 = 0.15;
/// `Push` in the shader.
const PUSH: u32 = 112;

/// A screen's pictures as read from the game (`MENUM.TGA`, `MENUM_ON.TGA`,
/// `MAIN_MAP.RAW`).
pub struct Art {
    /// RGBA, 800x600 each.
    base: Vec<u8>,
    on: Vec<u8>,
    /// Region ids, 400x300.
    hit: Vec<u8>,
}

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

fn read_tga(path: &Path) -> Result<Vec<u8>> {
    let t = c2_formats::tga::parse(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("{}", path.display()))?;
    if (t.width as usize, t.height as usize) != (CW, CH) {
        bail!(
            "{}: {}x{}, not the menu's {CW}x{CH}",
            path.display(),
            t.width,
            t.height
        );
    }
    Ok(c2_core::color::image_to_rgba8(&t.pixels, false))
}

impl Art {
    /// The screen `base` (`on`, `map`) from the first of `menus` (the
    /// `HUNTDAT/MENU` folders, overlays first) that has it.
    pub fn load(menus: &[PathBuf], base: &str, on: &str, map: &str) -> Result<Self> {
        let dir = menus
            .iter()
            .find(|d| find_ci(d, base).is_some())
            .ok_or_else(|| anyhow!("no {base} in the game's HUNTDAT/MENU"))?;
        let file = |n: &str| find_ci(dir, n).ok_or_else(|| anyhow!("no {n} beside {base}"));
        let hit = std::fs::read(file(map)?)?;
        if hit.len() != HW * HH {
            bail!("{map}: {} bytes, not {HW}x{HH}", hit.len());
        }
        Ok(Self {
            base: read_tga(&file(base)?)?,
            on: read_tga(&file(on)?)?,
            hit,
        })
    }

    /// The region at canvas point `c` (0: none).
    pub fn region(&self, c: (f32, f32)) -> u8 {
        let (x, y) = (c.0 as usize / 2, c.1 as usize / 2);
        if c.0 < 0.0 || c.1 < 0.0 || x >= HW || y >= HH {
            return 0;
        }
        self.hit[y * HW + x]
    }

    /// Each region's bounding box, canvas pixels: x0, y0, x1, y1.
    fn boxes(&self) -> Vec<Option<[f32; 4]>> {
        let mut bb = vec![None::<[usize; 4]>; 256];
        for (i, &r) in self.hit.iter().enumerate() {
            if r == 0 {
                continue;
            }
            let (x, y) = (i % HW, i / HW);
            let b = bb[r as usize].get_or_insert([x, y, x, y]);
            *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
        bb.into_iter()
            .map(|b| b.map(|[x0, y0, x1, y1]| [x0, y0, x1 + 1, y1 + 1].map(|v| (v * 2) as f32)))
            .collect()
    }
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
            center: head + fwd * DIST_M - Vec3::Y * DROP_M,
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

/// What the panel shows this frame.
pub struct Look {
    /// The selected region (0: none).
    pub selected: u8,
    /// A bit per region offered (the rest are dimmed).
    pub offered: u32,
    /// The pointer, canvas pixels.
    pub pointer: Option<(f32, f32)>,
}

pub struct Panel {
    pub art: Art,
    boxes: Vec<Option<[f32; 4]>>,
    pics: (Image, vk::ImageView),
    hit: (Image, vk::ImageView),
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
    pub fn new(gpu: &Gpu, art: Art, render_pass: vk::RenderPass) -> Result<Self> {
        let pics = gpu.texture(
            (CW as u32, CH as u32),
            vk::Format::R8G8B8A8_SRGB,
            &[&art.base[..], &art.on[..]].concat(),
            2,
        )?;
        let hit = gpu.texture((HW as u32, HH as u32), vk::Format::R8_UINT, &art.hit, 1)?;
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
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                    binding(0, D::SAMPLED_IMAGE),
                    binding(1, D::SAMPLED_IMAGE),
                    binding(2, D::SAMPLER),
                ]),
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
                            descriptor_count: 2,
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
            let image = |view| {
                [vk::DescriptorImageInfo {
                    sampler: vk::Sampler::null(),
                    image_view: view,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }]
            };
            let (a, b) = (image(pics.1), image(hit.1));
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
                    w(0, D::SAMPLED_IMAGE).image_info(&a),
                    w(1, D::SAMPLED_IMAGE).image_info(&b),
                    w(2, D::SAMPLER).image_info(&samp),
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
                boxes: art.boxes(),
                art,
                pics,
                hit,
                sampler,
                set_layout,
                pool,
                set,
                layout,
                pipeline: pipeline?,
            })
        }
    }

    /// Records the panel; `mvp` takes the unit quad to clip space.
    pub fn draw(&self, gpu: &Gpu, cmd: vk::CommandBuffer, mvp: Mat4, look: &Look) {
        let sel_box = self
            .boxes
            .get(look.selected as usize)
            .copied()
            .flatten()
            .filter(|_| look.selected != 0)
            .unwrap_or([0.0; 4]);
        let pointer = look.pointer.map_or([0.0; 4], |(x, y)| [x, y, 1.0, 0.0]);
        let mut push: Vec<u8> = mvp
            .to_cols_array()
            .iter()
            .chain(&sel_box)
            .chain(&pointer)
            .flat_map(|f| f.to_ne_bytes())
            .collect();
        for u in [u32::from(look.selected), look.offered, 0, 0] {
            push.extend(u.to_ne_bytes());
        }
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
            for (image, view) in [&self.pics, &self.hit] {
                dev.destroy_image_view(*view, None);
                gpu.destroy_image(image);
            }
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
        let ray = |x: f32, y: f32| Ray {
            pos: Vec3::new(0.0, 1.7, 0.0),
            dir: (p.center + Vec3::new(x, y, 0.0) - Vec3::new(0.0, 1.7, 0.0)).normalize(),
        };
        let (x, y) = p.pick(&ray(0.0, 0.0)).unwrap_or_default();
        assert!((x - 400.0).abs() < 0.5 && (y - 300.0).abs() < 0.5);
        // Up and right on the panel: up and right on the canvas.
        let (x, y) = p.pick(&ray(0.5, 0.375)).unwrap_or_default();
        assert!((x - 600.0).abs() < 0.5 && (y - 150.0).abs() < 0.5);
        // Pointing away misses.
        let back = Ray {
            pos: Vec3::new(0.0, 1.7, 0.0),
            dir: Vec3::Z,
        };
        assert!(p.pick(&back).is_none());
    }

    #[test]
    fn boxes_bound_each_region() {
        let mut hit = vec![0u8; HW * HH];
        for y in 10..20 {
            for x in 30..50 {
                hit[y * HW + x] = 3;
            }
        }
        let art = Art {
            base: Vec::new(),
            on: Vec::new(),
            hit,
        };
        let b = art.boxes();
        assert_eq!(b[3], Some([60.0, 20.0, 100.0, 40.0]));
        assert_eq!(b[1], None);
        assert_eq!(art.region((61.0, 21.0)), 3);
        assert_eq!(art.region((10.0, 10.0)), 0);
    }
}
