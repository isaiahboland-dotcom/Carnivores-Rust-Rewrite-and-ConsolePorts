//! The field manual's 3D models ("DHC OS 3D specimens", off by default):
//! an animal's model (its first animation playing), a weapon's (at rest)
//! or an area's relief (`relief.rs`) on a turntable in its file's picture
//! panel, drawn in the menu's own pass through a viewport, as the model
//! viewer draws it (`model_view.rs`: unlit). Drag to turn it, the wheel to
//! zoom; the pad's right stick turns it. It is made when a file shows and
//! dropped when the file closes.

use c2_core::model::{car_anim_duration_ms, pose_car, PreparedModel};
use c2_formats::car::Car;
use glam::Vec3;

use crate::camera::{projection, OrbitCam};
use crate::gpu::Gpu;
use crate::input::Input;
use crate::render::{FrameParams, Globals, GpuModel, Instance, ModelRenderer};

/// Radians a second the turntable turns, and how long it waits after a
/// drag before turning again.
const TURN: f32 = 0.45;
const REST_S: f32 = 2.0;
/// A long lens from further back (`camera::projection`'s zoom): little
/// perspective, so the near end of a long animal doesn't loom.
const LENS: f32 = 2.4;

/// What a file's model shows.
pub(in crate::menu) enum Subject {
    /// An animal: its first animation plays.
    Animal(Car),
    /// A weapon: its model at rest.
    Weapon(Car),
    /// An area's relief, seen from higher up.
    Terrain(PreparedModel),
}

pub(in crate::menu) struct Viewer {
    /// The animation played (animals only).
    anim: Option<Car>,
    prepared: PreparedModel,
    gpu_model: GpuModel,
    renderer: ModelRenderer,
    globals: Globals,
    cam: OrbitCam,
    /// The framed distance (zoom stays near it).
    home: f32,
    time_ms: f32,
    /// Seconds since the player last turned it.
    idle: f32,
}

impl Viewer {
    pub(in crate::menu) fn new(gpu: &Gpu, subject: Subject) -> Self {
        // An area is seen from higher up, and framed closer: its square's
        // corners are what the diagonal measures, not its outline.
        let (prepared, anim, pitch, frame) = match subject {
            Subject::Animal(car) => (PreparedModel::from_mesh(&car.mesh), Some(car), 0.25, 0.6),
            Subject::Weapon(car) => (PreparedModel::from_mesh(&car.mesh), None, 0.25, 0.6),
            Subject::Terrain(p) => (p, None, 0.6, 0.45),
        };
        let globals = Globals::new(gpu);
        // Characters are unlit, as in the hunt (`model_view.rs`).
        let gpu_model = GpuModel::new(gpu, &prepared, Vec::new());
        // Two identical layers: a one-layer array is a plain 2D texture
        // on GLES (`model_view.rs`).
        let renderer = ModelRenderer::new(
            gpu,
            &globals,
            &[&prepared.texture_rgba, &prepared.texture_rgba],
        );
        let posed = anim
            .as_ref()
            .and_then(|c| pose_car(c.animations.first()?, c.mesh.vertices.len(), 0))
            .unwrap_or_else(|| prepared.positions.clone());
        // Framed on the points a face uses (stray vertices would stretch it).
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for f in &prepared.faces {
            for p in f.v.iter().filter_map(|&v| posed.get(v as usize)) {
                lo = lo.min(Vec3::from(*p));
                hi = hi.max(Vec3::from(*p));
            }
        }
        let (target, dist) = if lo.x <= hi.x {
            ((lo + hi) * 0.5, (hi - lo).length().max(50.0) * frame * LENS)
        } else {
            (Vec3::ZERO, 1000.0)
        };
        Viewer {
            anim,
            prepared,
            gpu_model,
            renderer,
            globals,
            cam: OrbitCam {
                target,
                dist,
                yaw: 0.8,
                pitch,
            },
            home: dist,
            time_ms: 0.0,
            idle: REST_S,
        }
    }

    /// Turns, zooms and animates it; `over`: the pointer is on its panel.
    pub(in crate::menu) fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input, over: bool) {
        let stick = input.pad.look[0];
        if over && (input.dragging || input.scroll != 0.0) {
            self.cam.update(input);
            self.cam.dist = self.cam.dist.clamp(self.home * 0.4, self.home * 2.5);
            self.idle = 0.0;
        } else if stick.abs() > 0.2 {
            self.cam.yaw -= stick * 2.0 * dt;
            self.idle = 0.0;
        } else {
            self.idle += dt;
            if self.idle > REST_S {
                self.cam.yaw += TURN * dt;
            }
        }
        let Some(car) = &self.anim else { return };
        self.time_ms += dt * 1000.0;
        let pose = car.animations.first().and_then(|a| {
            let d = car_anim_duration_ms(a).max(1) as f32;
            pose_car(a, car.mesh.vertices.len(), (self.time_ms % d) as u32)
        });
        match pose {
            Some(p) => self.gpu_model.set_pose(gpu, &p),
            None => self.gpu_model.set_pose(gpu, &self.prepared.positions),
        }
    }

    /// Draws it into `rect` (window pixels: x, y, w, h), then gives the
    /// pass its whole `screen` back.
    pub(in crate::menu) fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        rect: (f32, f32, f32, f32),
        screen: (u32, u32),
    ) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let x0 = rect.0.clamp(0.0, sw);
        let y0 = rect.1.clamp(0.0, sh);
        let x1 = (rect.0 + rect.2).clamp(0.0, sw);
        let y1 = (rect.1 + rect.3).clamp(0.0, sh);
        let (w, h) = (x1 - x0, y1 - y0);
        if w < 2.0 || h < 2.0 {
            return;
        }
        self.globals.write(
            gpu,
            &FrameParams {
                view: self.cam.view(),
                proj: projection(w / h, self.cam.dist * 10.0 + 1000.0, LENS),
                eye: self.cam.eye(),
                fog_rgb: [0.0; 3],
                fog: (1e9, 2e9),
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
                mirror: None,
            },
        );
        pass.set_viewport(x0, y0, w, h, 0.0, 1.0);
        pass.set_scissor_rect(x0 as u32, y0 as u32, w as u32, h as u32);
        let inst = [Instance {
            dither: 0.0,
            offset: [0.0; 3],
            turns: 0.0,
            light: 1.0,
            layer: 0,
        }];
        self.renderer
            .draw(gpu, pass, &self.globals, &[(&self.gpu_model, &inst)]);
        pass.set_viewport(0.0, 0.0, sw, sh, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, screen.0, screen.1);
    }
}
