//! Local fog volumes (docs/DECISIONS.md, 2026-11-16): the map's fog zones
//! drawn on the ground as `Hunt2.cpp: CalcFogLevel` gives them, checked
//! against `c2_core::fog::Zones::level` on a synthetic map; and the sun's
//! glow through them (PC option `modern_fog_glow`, 2026-11-16); the
//! original's underwater fog (`original_underwater_fog`) and desaturated
//! night (`desaturated_night`).

use super::*;
use crate::scene::Scene;
use c2_core::area::Area;
use c2_core::fog::Zones;
use c2_formats::rsc::Fog;

const SIZE: u32 = 64;
const GROUND: u8 = 60;
const RED: u16 = 0x7c00;
/// The camera's height over the ground and its look down.
const EYE_UP: f32 = 220.0;
const PITCH: f32 = -0.3;

/// Zone `zone` over the whole map; the list has one zone, blue, its top
/// 20 height units over the ground.
fn foggy(zone: u8) -> Area {
    let mut area = Area::flat(GROUND);
    area.rsc.textures = vec![vec![RED; 128 * 128]];
    area.rsc.fogs = vec![Fog {
        rgb: 0x00ff_0000,
        y_begin: f32::from(GROUND) + 20.0,
        mortal: 0,
        transp: 256.0,
        f_limit: 200.0,
    }];
    area.map.fogs.fill(zone);
    area
}

fn eye() -> Vec3 {
    Vec3::new(
        512.5 * 256.0,
        f32::from(GROUND) * 64.0 + EYE_UP,
        512.5 * 256.0,
    )
}

/// One frame's settings.
#[derive(Clone, Copy)]
struct Run {
    zones_on: bool,
    /// The sun's glow through the fog, and where the sun is.
    glow: f32,
    sun: Vec3,
    /// The original's underwater fog: its colour and top.
    underwater: Option<([f32; 3], f32)>,
    /// Desaturated night's strength.
    night_grey: f32,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            zones_on: true,
            glow: 0.0,
            sun: Vec3::Y,
            underwater: None,
            night_grey: 0.0,
        }
    }
}

struct Hollow {
    terrain: TerrainRenderer,
    globals: Globals,
    area: Area,
    run: Run,
}

impl Scene for Hollow {
    fn title(&self) -> String {
        "synthetic fog hollow".into()
    }
    fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
        let eye = eye();
        let forward = Vec3::new(0.0, PITCH.sin(), -PITCH.cos());
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
                night_grey: self.run.night_grey,
                absorb: [1.0; 3],
                sky: crate::render::SkyLight {
                    sun: self.run.sun.to_array(),
                    ..Default::default()
                },
                mirror: None,
            },
        );
        let camera = Zones::new(&self.area.map, &self.area.rsc.fogs).camera_zone(eye.to_array());
        self.globals.write_fog_frame(
            gpu,
            &crate::render::FogFrame {
                camera,
                on: self.run.zones_on,
                glow: self.run.glow,
                underwater: self.run.underwater,
            },
        );
        let frustum = Frustum::new(proj * view);
        self.terrain
            .set_window(gpu, (512, 512), 32, false, 1, 0, false, &frustum);
        self.terrain.draw_ground(pass, &self.globals);
    }
}

/// The middle pixel's RGB with the zones on and off.
fn render(area: Area) -> anyhow::Result<([f32; 3], [f32; 3])> {
    let off = Run {
        zones_on: false,
        ..Run::default()
    };
    let [on, off] = render_runs(area, [Run::default(), off])?;
    Ok((on, off))
}

/// The middle pixel's RGB for each (zones on, glow, sun direction).
fn render_each<const N: usize>(
    area: Area,
    runs: [(bool, f32, Vec3); N],
) -> anyhow::Result<[[f32; 3]; N]> {
    render_runs(
        area,
        runs.map(|(zones_on, glow, sun)| Run {
            zones_on,
            glow,
            sun,
            ..Run::default()
        }),
    )
}

/// The middle pixel's RGB for each run.
fn render_runs<const N: usize>(area: Area, runs: [Run; N]) -> anyhow::Result<[[f32; 3]; N]> {
    let (gpu, target) = Gpu::offscreen((SIZE, SIZE))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (SIZE, SIZE));
    let globals = Globals::with_fog_zones(&gpu, &area.map.fogs, &area.rsc.fogs);
    let terrain = TerrainRenderer::new(&gpu, &globals, &area);
    let mut scene = Hollow {
        terrain,
        globals,
        area,
        run: Run::default(),
    };
    let mut out = [[0.0; 3]; N];
    for (o, run) in out.iter_mut().zip(runs) {
        scene.run = run;
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        let px = gpu.read_rgba(&texture)?;
        let i = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        *o = core::array::from_fn(|c| f32::from(px[i + c]));
    }
    Ok(out)
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn ground_in_a_fog_zone_takes_its_colour_by_the_original_amount() -> anyhow::Result<()> {
    let area = foggy(1);
    // The ground where the middle of the view meets it.
    let dir = Vec3::new(0.0, PITCH.sin(), -PITCH.cos());
    let p = eye() + dir * (EYE_UP / -PITCH.sin());
    let zones = Zones::new(&area.map, &area.rsc.fogs);
    let camera = zones.camera_zone(eye().to_array());
    assert_eq!(camera, Some(1));
    let (level, _) = zones.level(eye().to_array(), camera, p.to_array());
    let a = level / 255.0;
    assert!(a > 0.3 && a < 0.7, "test amount {a}");

    let (on, off) = render(area)?;
    assert!(off[0] > 60.0 && off[2] < 2.0, "unfogged ground {off:?}");
    let want = [
        off[0] * (1.0 - a),
        off[1] * (1.0 - a),
        off[2] * (1.0 - a) + 255.0 * a,
    ];
    for c in 0..3 {
        assert!(
            (on[c] - want[c]).abs() < 10.0,
            "fogged {on:?}, expected about {want:?} (amount {a})"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn zone_missing_from_the_list_draws_no_fog() -> anyhow::Result<()> {
    let (on, off) = render(foggy(2))?;
    assert_eq!(on, off);
    Ok(())
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn sun_glows_warm_through_the_fog_only_towards_it() -> anyhow::Result<()> {
    let ahead = Vec3::new(0.0, PITCH.sin(), -PITCH.cos());
    let [plain, toward, behind, clear] = render_each(
        foggy(1),
        [
            (true, 0.0, ahead),
            (true, 0.25, ahead),
            (true, 0.25, -ahead),
            (false, 0.25, ahead),
        ],
    )?;
    // Straight at the sun: (1, 0.85, 0.5) * 0.25 * the fog amount (about
    // half) added, so red gains most and blue least.
    let gain: [f32; 3] = core::array::from_fn(|c| toward[c] - plain[c]);
    assert!(gain[0] > 20.0, "no glow: {plain:?} -> {toward:?}");
    assert!(gain[0] > gain[1] && gain[1] > gain[2], "not warm: {gain:?}");
    assert_eq!(behind, plain, "glow with the sun behind");
    // No zone fog, no glow.
    let [off] = render_each(foggy(1), [(false, 0.0, ahead)])?;
    assert_eq!(clear, off);
    Ok(())
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn original_underwater_fog_takes_the_waters_colour_by_depth() -> anyhow::Result<()> {
    // Water 20 height units over the ground, the eye under it; no zones.
    let level = f32::from(GROUND) * 64.0 + 20.0 * 64.0;
    let green = [0.0, 1.0, 0.0];
    let dir = Vec3::new(0.0, PITCH.sin(), -PITCH.cos());
    let p = eye() + dir * (EYE_UP / -PITCH.sin());
    let a = c2_core::fog::underwater_level(eye().to_array(), p.to_array(), level) / 255.0;
    assert!(a > 0.2 && a < 0.8, "test amount {a}");
    let none = Run {
        zones_on: false,
        ..Run::default()
    };
    let under = Run {
        underwater: Some((green, level)),
        ..none
    };
    let [clear, fogged] = render_runs(foggy(0), [none, under])?;
    // Models cap at 250, the ground does not.
    let want = [
        clear[0] * (1.0 - a),
        clear[1] * (1.0 - a) + 255.0 * a,
        clear[2] * (1.0 - a),
    ];
    for c in 0..3 {
        assert!(
            (fogged[c] - want[c]).abs() < 10.0,
            "fogged {fogged:?}, expected about {want:?} (amount {a})"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn desaturated_night_greys_and_halves_the_world() -> anyhow::Result<()> {
    let plain = Run {
        zones_on: false,
        ..Run::default()
    };
    let night = Run {
        night_grey: 1.0,
        ..plain
    };
    let [day, grey] = render_runs(foggy(0), [plain, night])?;
    let luma = day[0] * 0.299 + day[1] * 0.587 + day[2] * 0.114;
    let want: [f32; 3] = core::array::from_fn(|c| (day[c] * 0.4 + luma * 0.6) * 0.5);
    for c in 0..3 {
        assert!(
            (grey[c] - want[c]).abs() < 4.0,
            "night {grey:?}, expected about {want:?} from {day:?}"
        );
    }
    Ok(())
}
