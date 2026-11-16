//! Water alpha regression (docs/DECISIONS.md, 2026-11-12: water transparency
//! is a depth coefficient). Genesis AREA2's sea has RSC `transp = 0.001`;
//! using it as the final alpha made the whole sea invisible. Also: water
//! narrower than a coarse cell of the high-end view's levels (AUDIT-V2), and
//! the animated waves option (2026-11-14), water reflections and the
//! surface seen from under the water (2026-11-15).

use super::*;
use crate::scene::Scene;
use c2_core::area::{cell_flags, Area};
use c2_formats::rsc::Water;

const SIZE: u32 = 64;
/// AREA2-like: seabed at 60, water at 62, an eye 220 units above the seabed.
const SEABED: u8 = 60;
const LEVEL: i32 = 62;
const EYE_Y: f32 = SEABED as f32 * 64.0 + 220.0;
const RED: u16 = 0x7c00;
const BLUE: u16 = 0x001f;

struct Shallows {
    terrain: TerrainRenderer,
    globals: Globals,
    water: bool,
    pitch: f32,
    waves: bool,
    /// What the water over the camera lets through (ones: no depth tint).
    absorb: [f32; 3],
}

impl Scene for Shallows {
    fn title(&self) -> String {
        "synthetic shallows".into()
    }
    fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
        let eye = Vec3::new(512.5 * 256.0, EYE_Y, 512.5 * 256.0);
        let forward = Vec3::new(0.0, self.pitch.sin(), -self.pitch.cos());
        let view = glam::camera::rh::view::look_at_mat4(eye, eye + forward, Vec3::Y);
        let frustum = write_globals(gpu, &self.globals, view, eye, self.absorb);
        self.terrain
            .set_window(gpu, (512, 512), 48, false, 2, 0, self.waves, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
        if self.water {
            self.terrain.draw_water(pass, &self.globals);
        }
    }
}

/// Unfogged full daylight from `eye` with `view`; the frame's frustum.
fn write_globals(gpu: &Gpu, globals: &Globals, view: Mat4, eye: Vec3, absorb: [f32; 3]) -> Frustum {
    let proj = crate::camera::projection(1.0, 64.0 * 256.0, 1.0);
    globals.write(
        gpu,
        &FrameParams {
            view,
            proj,
            eye,
            fog_rgb: [0.0; 3],
            fog: (1.0e9, 2.0e9),
            daylight: [0.0, 1.0, 0.0],
            stars: false,
            night_vision: 0.0,
            ambient: 1.0,
            flashlight: false,
            narrow_flashlight: false,
            time_s: 0.0,
            brightness: 1.0,
            night_grey: 0.0,
            absorb,
            sky: Default::default(),
            mirror: None,
        },
    );
    Frustum::new(proj * view)
}

/// A red seabed under shallow blue water with AREA2's coefficient.
fn shallows() -> Area {
    let mut area = Area::flat(SEABED);
    area.rsc.textures = vec![vec![RED; 128 * 128], vec![BLUE; 128 * 128]];
    area.rsc.waters = vec![Water {
        texture_index: 1,
        level: LEVEL,
        transp: 0.001,
        fog_rgb: 0,
    }];
    area.map.water.fill(0);
    area.map.flags.fill(cell_flags::WATER);
    area
}

/// Mean RGB over the image's rows `rows`.
fn mean(pixels: &[u8], rows: std::ops::Range<u32>) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    let mut n = 0.0;
    for y in rows {
        for x in 0..SIZE {
            let i = ((y * SIZE + x) * 4) as usize;
            for c in 0..3 {
                sum[c] += f32::from(pixels[i + c]);
            }
            n += 1.0;
        }
    }
    sum.map(|s| s / n)
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn shallow_low_coefficient_water_is_visible() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let globals = Globals::new(&gpu);
    let terrain = TerrainRenderer::new(&gpu, &globals, &shallows());
    let mut scene = Shallows {
        terrain,
        globals,
        water: false,
        pitch: 0.0,
        waves: false,
        absorb: [1.0; 3],
    };
    let mut render = |water, pitch| -> anyhow::Result<Vec<u8>> {
        scene.water = water;
        scene.pitch = pitch;
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        gpu.read_rgba(&texture)
    };

    // Looking at the seabed about three cells ahead: a faint tint over a
    // clearly visible seabed (alpha a few percent; the old rule's 0.001
    // changes the blue channel by well under 1).
    let bed = mean(&render(false, -0.3)?, 24..40);
    let near = mean(&render(true, -0.3)?, 24..40);
    assert!(bed[0] > 60.0 && bed[2] < 1.0, "seabed {bed:?}");
    assert!(
        near[2] > bed[2] + 2.0,
        "near water invisible: {near:?} over {bed:?}"
    );
    assert!(
        near[0] > bed[0] * 0.75,
        "near shallows opaque: {near:?} over {bed:?}"
    );

    // The two rows just below a level horizon (far, grazing): opaque water,
    // fading towards the camera.
    let level = render(true, 0.0)?;
    let far = mean(&level, 32..34);
    let nearer = mean(&level, 40..44);
    assert!(
        far[2] > 120.0 && far[0] < 5.0,
        "far water not opaque: {far:?}"
    );
    assert!(nearer[0] > 60.0, "nearer water not translucent: {nearer:?}");
    Ok(())
}

/// Map column of the channel's middle: 288 cells east of the window's
/// centre, in the level whose cells are 4 map cells across.
const CHANNEL_X: usize = 800;

/// Looking straight down on [`CHANNEL_X`], the ground drawn in levels
/// around (512, 512) out to 384 cells.
struct Channel {
    terrain: TerrainRenderer,
    globals: Globals,
}

impl Scene for Channel {
    fn title(&self) -> String {
        "synthetic channel".into()
    }
    fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
        let eye = Vec3::new(
            CHANNEL_X as f32 * 256.0,
            BANK as f32 * 64.0 + 4000.0,
            512.5 * 256.0,
        );
        let view = glam::camera::rh::view::look_at_mat4(eye, eye - Vec3::Y, Vec3::NEG_Z);
        let frustum = write_globals(gpu, &self.globals, view, eye, [1.0; 3]);
        self.terrain
            .set_window(gpu, (512, 512), 384, true, 2, 0, false, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
        self.terrain.draw_water(pass, &self.globals);
    }
}

/// Dry ground above the water's level.
const BANK: u8 = 70;

/// Red land at [`BANK`] with a north-south channel: water cells six wide
/// (banks included, as `create_tmap` spreads them) around [`CHANNEL_X`],
/// its bed at [`SEABED`] for four. The coarse cells' corners at
/// `CHANNEL_X +- 4` are dry; the one at `CHANNEL_X` is the bed.
fn channel() -> Area {
    let mut area = shallows();
    area.map.water.fill(c2_core::area::NO_WATER);
    area.map.flags.fill(0);
    area.map.height.fill(BANK);
    let size = c2_formats::map::MAP_SIZE;
    for y in 0..size {
        for x in CHANNEL_X - 3..=CHANNEL_X + 2 {
            area.map.water[y * size + x] = 0;
            area.map.flags[y * size + x] = cell_flags::WATER;
            if (CHANNEL_X - 2..=CHANNEL_X + 1).contains(&x) {
                area.map.height[y * size + x] = SEABED;
            }
        }
    }
    area.rsc.waters[0].transp = 4.0;
    area
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn narrow_water_survives_coarse_levels() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let globals = Globals::new(&gpu);
    let terrain = TerrainRenderer::new(&gpu, &globals, &channel());
    let mut scene = Channel { terrain, globals };
    crate::scene::render(&gpu, &mut scene, &output, &frame);
    let pixels = gpu.read_rgba(&texture)?;

    // Column means across the middle rows: the channel runs down the
    // middle of the image, dry ground at its sides.
    let column = |x: u32| {
        let rows = SIZE / 4..SIZE * 3 / 4;
        let n = rows.len() as f32;
        rows.map(|y| {
            let i = ((y * SIZE + x) * 4) as usize;
            [f32::from(pixels[i]), f32::from(pixels[i + 2])]
        })
        .fold([0.0; 2], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n])
    };
    let middle = column(SIZE / 2);
    let side = column(2);
    assert!(side[0] > 60.0 && side[1] < 1.0, "dry ground: {side:?}");
    // The old rule dropped every coarse quad with a dry corner, leaving the
    // red bed bare (blue 0).
    assert!(middle[1] > 40.0, "channel without water: {middle:?}");
    Ok(())
}

/// Looking straight down on open water about four cells across, its
/// texture striped so that moved vertices show.
struct Swell {
    terrain: TerrainRenderer,
    globals: Globals,
    waves: bool,
}

impl Scene for Swell {
    fn title(&self) -> String {
        "synthetic swell".into()
    }
    fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
        let eye = Vec3::new(512.5 * 256.0, LEVEL as f32 * 64.0 + 500.0, 512.5 * 256.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, eye - Vec3::Y, Vec3::NEG_Z);
        let frustum = write_globals(gpu, &self.globals, view, eye, [1.0; 3]);
        self.terrain
            .set_window(gpu, (512, 512), 48, false, 2, 1000, self.waves, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
        self.terrain.draw_water(pass, &self.globals);
    }
}

/// [`shallows`] with nearly opaque water, each cell's texture half blue,
/// half green; `open`: the cells are open water (`fmWater`), else all bank.
fn striped(open: bool) -> Area {
    let mut area = shallows();
    area.rsc.textures[1] = (0..128 * 128)
        .map(|i| if i % 128 < 64 { BLUE } else { 0x03e0 })
        .collect();
    area.rsc.waters[0].transp = 4.0;
    if !open {
        area.map.flags.fill(0);
    }
    area
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn waves_move_open_water_only_when_on() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let render = |open, waves| -> anyhow::Result<Vec<u8>> {
        let globals = Globals::new(&gpu);
        let terrain = TerrainRenderer::new(&gpu, &globals, &striped(open));
        let mut scene = Swell {
            terrain,
            globals,
            waves,
        };
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        gpu.read_rgba(&texture)
    };
    // Pixels whose blue or green differs clearly: a stripe's edge moved.
    let moved = |a: &[u8], b: &[u8]| {
        a.chunks(4)
            .zip(b.chunks(4))
            .filter(|(p, q)| (1..3).any(|c| p[c].abs_diff(q[c]) > 40))
            .count()
    };
    let flat = render(true, false)?;
    assert_eq!(flat, render(true, false)?, "the flat water is not stable");
    let swell = render(true, true)?;
    let n = moved(&flat, &swell);
    assert!(n > 40, "waves on moved {n} pixels");
    // Bank vertices never move, so the shore keeps its line.
    assert_eq!(render(false, false)?, render(false, true)?);
    Ok(())
}

// The depth tint (2026-11-14): the ground loses colour by what the water
// lets through; the water's surface does not.
#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn depth_tint_absorbs_the_ground_not_the_surface() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let globals = Globals::new(&gpu);
    let mut area = shallows();
    area.rsc.waters[0].transp = 400.0; // opaque from here
    let terrain = TerrainRenderer::new(&gpu, &globals, &area);
    let mut scene = Shallows {
        terrain,
        globals,
        water: false,
        pitch: -0.3,
        waves: false,
        absorb: [1.0; 3],
    };
    let mut render = |water, absorb| -> anyhow::Result<Vec<u8>> {
        scene.water = water;
        scene.absorb = absorb;
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        gpu.read_rgba(&texture)
    };
    let tint = [0.5, 1.0, 1.0];
    let bed = mean(&render(false, [1.0; 3])?, 24..40);
    let deep = mean(&render(false, tint)?, 24..40);
    assert!(bed[0] > 60.0, "seabed {bed:?}");
    let ratio = deep[0] / bed[0];
    assert!((ratio - 0.5).abs() < 0.05, "seabed kept {ratio} of its red");
    let surface = render(true, [1.0; 3])?;
    assert_eq!(surface, render(true, tint)?);
    // The water of the camera's own chunk is there: its ground is below
    // the view, and chunks were culled by their ground alone.
    let near = mean(&surface, 50..SIZE);
    assert!(near[0] < 1.0 && near[2] > 120.0, "bare bed: {near:?}");
    Ok(())
}

const GREEN: u16 = 0x03e0;
/// Map row of the cliff's foot, 6.5 cells ahead of the camera.
const CLIFF_Z: usize = 506;

/// [`shallows`] under opaque water with a green cliff ahead (north).
fn cove() -> Area {
    let mut area = shallows();
    area.rsc.textures.push(vec![GREEN; 128 * 128]);
    area.rsc.waters[0].transp = 400.0;
    let size = c2_formats::map::MAP_SIZE;
    for i in 0..CLIFF_Z * size {
        area.map.height[i] = 120;
        area.map.texture1[i] = 2;
        area.map.water[i] = c2_core::area::NO_WATER;
        area.map.flags[i] = 0;
    }
    area
}

/// Looking level at the cliff over the water, from [`EYE_Y`]; with
/// `mirror`, the cliff is drawn mirrored first, as `AreaView` does.
struct Cove {
    terrain: TerrainRenderer,
    globals: Globals,
    mirror: bool,
    drawn: Option<(f32, (u32, u32))>,
}

impl Cove {
    fn camera() -> (Mat4, Vec3) {
        let eye = Vec3::new(512.5 * 256.0, EYE_Y, 512.5 * 256.0);
        (
            glam::camera::rh::view::look_at_mat4(eye, eye + Vec3::NEG_Z, Vec3::Y),
            eye,
        )
    }

    fn write(&self, gpu: &Gpu, view: Mat4, proj: Mat4, eye: Vec3) -> Frustum {
        self.globals.write(
            gpu,
            &FrameParams {
                view,
                proj,
                eye,
                fog_rgb: [0.0; 3],
                fog: (1.0e9, 2.0e9),
                daylight: [0.0, 1.0, 0.0],
                stars: false,
                night_vision: 0.0,
                ambient: 1.0,
                flashlight: false,
                narrow_flashlight: false,
                time_s: 0.0,
                brightness: 1.0,
                night_grey: 0.0,
                absorb: [1.0; 3],
                sky: Default::default(),
                mirror: self.drawn,
            },
        );
        Frustum::new(proj * view)
    }
}

impl Scene for Cove {
    fn title(&self) -> String {
        "synthetic cove".into()
    }
    fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }
    fn prepare(&mut self, gpu: &Gpu, screen: (u32, u32)) {
        self.drawn = None;
        if !self.mirror {
            return;
        }
        let level = LEVEL as f32 * 64.0;
        let (view, eye) = Self::camera();
        let view = reflection::mirrored(view, level);
        let proj = reflection::oblique(
            crate::camera::projection(1.0, 64.0 * 256.0, 1.0),
            view,
            level - reflection::CLIP_BIAS,
        );
        let frustum = self.write(gpu, view, proj, reflection::mirrored_eye(eye, level));
        self.terrain
            .set_window(gpu, (512, 512), 48, false, 3, 0, false, &frustum);
        let (color, depth) = self.terrain.reflection.target(gpu, screen);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("test reflection"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.terrain.draw_ground(&mut pass, &self.globals);
        }
        gpu.queue.submit([enc.finish()]);
        self.drawn = Some((level, screen));
    }
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
        let (view, eye) = Self::camera();
        let proj = crate::camera::projection(1.0, 64.0 * 256.0, 1.0);
        let frustum = self.write(gpu, view, proj, eye);
        self.terrain
            .set_window(gpu, (512, 512), 48, false, 3, 0, false, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
        self.terrain.draw_water(pass, &self.globals);
    }
}

// Water reflections (2026-11-15): the water under a cliff shows the cliff,
// not the bed the mirror pass must clip; off, the picture is the old one.
#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn reflections_mirror_what_stands_over_the_water() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let globals = Globals::new(&gpu);
    let terrain = TerrainRenderer::new(&gpu, &globals, &cove());
    let mut scene = Cove {
        terrain,
        globals,
        mirror: false,
        drawn: None,
    };
    let mut render = |mirror| -> anyhow::Result<Vec<u8>> {
        scene.mirror = mirror;
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        gpu.read_rgba(&texture)
    };
    // The cliff fills the picture above the horizon; its foot is three
    // rows under it, the water from there down.
    let plain = render(false)?;
    let cliff = mean(&plain, 8..24);
    assert!(cliff[1] > 60.0 && cliff[2] < 1.0, "cliff {cliff:?}");
    let water = mean(&plain, 37..42);
    assert!(water[2] > 120.0 && water[1] < 1.0, "water {water:?}");
    let mirrored = render(true)?;
    assert_eq!(mean(&mirrored, 8..24), cliff, "the mirror changed the land");
    // At this slant most of the light is the cliff's.
    let seen = mean(&mirrored, 37..42);
    assert!(
        seen[1] > cliff[1] * 0.35 && seen[1] > seen[2],
        "no cliff in the water: {seen:?}"
    );
    // The red bed: let through a little by the water, never mirrored.
    assert!(
        seen[0] < seen[1] * 0.5,
        "the bed is in the mirror: {seen:?}"
    );
    // At the camera's feet the view is steep: the water's own colour.
    let near = mean(&mirrored, 60..SIZE);
    assert!(near[2] > near[1], "a mirror at the feet: {near:?}");
    assert_eq!(plain, render(false)?, "switching off left a trace");
    Ok(())
}

/// Looking up at the surface from just over the bed.
struct Dive {
    terrain: TerrainRenderer,
    globals: Globals,
}

impl Scene for Dive {
    fn title(&self) -> String {
        "synthetic dive".into()
    }
    fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
        let eye = Vec3::new(512.5 * 256.0, SEABED as f32 * 64.0 + 40.0, 512.5 * 256.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, eye + Vec3::Y, Vec3::NEG_Z);
        let frustum = write_globals(gpu, &self.globals, view, eye, [1.0; 3]);
        self.terrain
            .set_window(gpu, (512, 512), 48, false, 2, 0, false, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
        self.terrain.draw_water(pass, &self.globals);
    }
}

// Hunt2.cpp's UNDERWATER alpha (2026-11-15): from below, the surface is a
// veil of 160/255 nearby whatever the RSC coefficient; the coefficient used
// to be the alpha (invisible at 0.001, a lid at 1 and over).
#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn the_surface_from_below_is_a_veil_at_any_coefficient() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let render = |transp| -> anyhow::Result<Vec<u8>> {
        let globals = Globals::new(&gpu);
        let mut area = shallows();
        area.rsc.waters[0].transp = transp;
        let terrain = TerrainRenderer::new(&gpu, &globals, &area);
        let mut scene = Dive { terrain, globals };
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        gpu.read_rgba(&texture)
    };
    let thin = render(0.001)?;
    assert_eq!(thin, render(400.0)?, "the coefficient shows from below");
    // Blue water lit 144..192 of 255 (the shimmer) at alpha 160/255, over
    // the black beyond it.
    let up = mean(&thin, 24..40);
    assert!(up[2] > 80.0 && up[2] < 125.0, "surface {up:?}");
    Ok(())
}
