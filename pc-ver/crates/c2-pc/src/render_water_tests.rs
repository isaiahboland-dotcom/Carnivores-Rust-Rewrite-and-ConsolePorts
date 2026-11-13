//! Water alpha regression (docs/DECISIONS.md, 2026-11-12: water transparency
//! is a depth coefficient). Genesis AREA2's sea has RSC `transp = 0.001`;
//! using it as the final alpha made the whole sea invisible.

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
        let proj = crate::camera::projection(1.0, 64.0 * 256.0, 1.0);
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
            },
        );
        let frustum = Frustum::new(proj * view);
        self.terrain
            .set_window(gpu, (512, 512), 48, false, 2, 0, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
        if self.water {
            self.terrain.draw_water(pass, &self.globals);
        }
    }
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
