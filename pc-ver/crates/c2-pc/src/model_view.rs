//! `c2-pc model <file-or-dir>`: orbit viewer for `.CAR` / `.3DF` files.

use std::path::PathBuf;

use anyhow::{Context, Result};
use c2_core::model::{car_anim_duration_ms, pose_car, PreparedModel};
use c2_formats::{car, three_df};
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::camera::{projection, OrbitCam};
use crate::gpu::Gpu;
use crate::input::Input;
use crate::render::{FrameParams, Globals, GpuModel, Instance, ModelRenderer};
use crate::scene::Scene;

struct Loaded {
    car: Option<car::Car>,
    prepared: PreparedModel,
    gpu_model: GpuModel,
    renderer: ModelRenderer,
}

pub struct ModelView {
    files: Vec<PathBuf>,
    index: usize,
    loaded: Option<Loaded>,
    error: Option<String>,
    globals: Globals,
    cam: OrbitCam,
    anim: usize,
    time_ms: f32,
    paused: bool,
}

pub fn list_models(path: &std::path::Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut v = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).with_context(|| format!("reading {}", d.display()))? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("car") || e.eq_ignore_ascii_case("3df"))
            {
                v.push(p);
            }
        }
    }
    v.sort();
    anyhow::ensure!(
        !v.is_empty(),
        "no .CAR or .3DF files under {}",
        path.display()
    );
    Ok(v)
}

impl ModelView {
    pub fn new(gpu: &Gpu, files: Vec<PathBuf>, anim: usize, time_ms: f32, yaw: f32) -> Self {
        let mut s = Self {
            files,
            index: 0,
            loaded: None,
            error: None,
            globals: Globals::new(gpu),
            cam: OrbitCam {
                target: Vec3::ZERO,
                dist: 1000.0,
                yaw,
                pitch: 0.3,
            },
            anim,
            time_ms,
            paused: false,
        };
        s.load(gpu);
        s
    }

    fn load(&mut self, gpu: &Gpu) {
        let path = &self.files[self.index];
        let result = (|| -> Result<(Option<car::Car>, PreparedModel)> {
            let data = std::fs::read(path)?;
            let is_car = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("car"));
            Ok(if is_car {
                let c = car::parse(&data)?;
                let p = PreparedModel::from_mesh(&c.mesh);
                (Some(c), p)
            } else {
                (
                    None,
                    PreparedModel::from_mesh(&three_df::parse(&data)?.mesh),
                )
            })
        })();
        match result {
            Ok((car, prepared)) => {
                // Characters get no CalcLights in the original (commented out in
                // LoadCharacterInfo), so they are unlit here too.
                let gpu_model = GpuModel::new(gpu, &prepared, Vec::new());
                let renderer = ModelRenderer::new(gpu, &self.globals, &[&prepared.texture_rgba]);
                if car.as_ref().is_none_or(|c| self.anim >= c.animations.len()) {
                    self.anim = 0;
                }
                // Frame the first frame of the current animation (weapons and
                // the hunter are posed relative to the camera, far from rest).
                let posed = car
                    .as_ref()
                    .and_then(|c| pose_car(c.animations.get(self.anim)?, c.mesh.vertices.len(), 0))
                    .unwrap_or_else(|| prepared.positions.clone());
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for p in &posed {
                    lo = lo.min(Vec3::from(*p));
                    hi = hi.max(Vec3::from(*p));
                }
                if lo.x <= hi.x {
                    self.cam.target = (lo + hi) * 0.5;
                    self.cam.dist = (hi - lo).length().max(50.0) * 0.9;
                }
                self.loaded = Some(Loaded {
                    car,
                    prepared,
                    gpu_model,
                    renderer,
                });
                self.error = None;
            }
            Err(e) => {
                tracing::warn!("{}: {e:#}", path.display());
                self.loaded = None;
                self.error = Some(format!("{e:#}"));
            }
        }
    }
}

impl Scene for ModelView {
    fn title(&self) -> String {
        let file = self.files[self.index]
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let anim = self
            .loaded
            .as_ref()
            .and_then(|l| l.car.as_ref())
            .and_then(|c| {
                c.animations.get(self.anim).map(|a| {
                    format!(
                        " — anim {}/{} {:?}",
                        self.anim + 1,
                        c.animations.len(),
                        a.name()
                    )
                })
            })
            .unwrap_or_default();
        let err = self
            .error
            .as_ref()
            .map(|e| format!(" — ERROR: {e}"))
            .unwrap_or_default();
        format!(
            "c2rs model [{}/{}] {file}{anim}{err}",
            self.index + 1,
            self.files.len()
        )
    }

    fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        if input.dragging || input.scroll != 0.0 {
            self.cam.update(input);
        }
        let n = self.files.len();
        if input.pressed(KeyCode::BracketRight) || input.pressed(KeyCode::PageDown) {
            self.index = (self.index + 1) % n;
            self.load(gpu);
        }
        if input.pressed(KeyCode::BracketLeft) || input.pressed(KeyCode::PageUp) {
            self.index = (self.index + n - 1) % n;
            self.load(gpu);
        }
        if input.pressed(KeyCode::Space) {
            self.paused = !self.paused;
        }
        let Some(l) = &self.loaded else { return };
        let anims = l.car.as_ref().map_or(0, |c| c.animations.len());
        if anims > 0 {
            if input.pressed(KeyCode::KeyN) {
                self.anim = (self.anim + 1) % anims;
                self.time_ms = 0.0;
            }
            if input.pressed(KeyCode::KeyP) {
                self.anim = (self.anim + anims - 1) % anims;
                self.time_ms = 0.0;
            }
        }
        if !self.paused {
            self.time_ms += dt * 1000.0;
        }
        let pose = l.car.as_ref().and_then(|c| {
            let a = c.animations.get(self.anim)?;
            let d = car_anim_duration_ms(a).max(1) as f32;
            pose_car(a, c.mesh.vertices.len(), (self.time_ms % d) as u32)
        });
        match pose {
            Some(p) => l.gpu_model.set_pose(gpu, &p),
            None => l.gpu_model.set_pose(gpu, &l.prepared.positions),
        }
    }

    fn clear_color(&self) -> [f64; 3] {
        [0.18, 0.2, 0.24]
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        let Some(l) = &mut self.loaded else { return };
        self.globals.write(
            gpu,
            &FrameParams {
                view: self.cam.view(),
                proj: projection(aspect, self.cam.dist * 10.0 + 1000.0, 1.0),
                eye: self.cam.eye(),
                fog_rgb: [0.0; 3],
                fog: (1e9, 2e9),
                night_vision: false,
                time_s: 0.0,
            },
        );
        let inst = [Instance {
            offset: [0.0; 3],
            turns: 0.0,
            light: 1.0,
            layer: 0,
        }];
        l.renderer
            .draw(gpu, pass, &self.globals, &[(&l.gpu_model, &inst)]);
    }
}
