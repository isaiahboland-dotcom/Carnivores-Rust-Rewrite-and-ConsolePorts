//! Water alpha regression (docs/DECISIONS.md, 2026-11-12: water transparency
//! is a depth coefficient). Genesis AREA2's sea has RSC `transp = 0.001`;
//! using it as the final alpha made the whole sea invisible. Also: water
//! narrower than a coarse cell of the high-end view's levels (AUDIT-V2), and
//! the animated waves option (2026-11-14).

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
            absorb,
            sky: Default::default(),
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
