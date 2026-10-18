//! `c2-pc area <AREA.MAP>`: fly-cam over an area's terrain, water and objects.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use c2_core::area::{Area, Placement, TimeOfDay};
use c2_core::model::{pose_object, vertex_lights, PreparedModel};
use c2_core::CELL;
use c2_formats::rsc::object_flags;
use c2_formats::{map, rsc, three_df};
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::camera::{projection, FlyCam};
use crate::gpu::Gpu;
use crate::input::Input;
use crate::render::{
    FrameParams, Globals, GpuModel, Instance, ModelRenderer, SkyRenderer, TerrainRenderer,
};
use crate::scene::Scene;

const CHUNK: usize = 32;
const CHUNKS: usize = map::MAP_SIZE / CHUNK;

/// Finds `<stem>.<ext>` next to `path`, ignoring case (data comes from Windows).
pub fn sibling(path: &Path, ext: &str) -> Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = e?.path();
        let same_stem = p
            .file_stem()
            .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&stem));
        let same_ext = p
            .extension()
            .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(ext));
        if same_stem && same_ext {
            return Ok(p);
        }
    }
    anyhow::bail!("no {stem}.{ext} next to {}", path.display())
}

/// Finds `name` in `dir`, ignoring case.
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

/// `Hunt2.cpp`: MOON.3DF at night, SUN2.3DF otherwise, from `HUNTDAT/`
/// (the parent of the `AREAS` folder).
fn load_sun(map_path: &Path, time: TimeOfDay) -> Option<PreparedModel> {
    let huntdat = map_path.parent()?.parent()?;
    let name = if time == TimeOfDay::Night {
        "MOON.3DF"
    } else {
        "SUN2.3DF"
    };
    let path = find_file(huntdat, name)?;
    match std::fs::read(&path)
        .map_err(anyhow::Error::from)
        .and_then(|d| Ok(three_df::parse(&d)?))
    {
        Ok(m) => Some(PreparedModel::from_mesh(&m.mesh)),
        Err(e) => {
            tracing::warn!("{}: {e:#}", path.display());
            None
        }
    }
}

pub struct AreaView {
    name: String,
    area: Area,
    globals: Globals,
    terrain: TerrainRenderer,
    sky: SkyRenderer,
    models: Vec<GpuModel>,
    renderer: ModelRenderer,
    /// Placements bucketed by chunk, with their precomputed light.
    chunks: Vec<Vec<(Placement, f32)>>,
    pub cam: FlyCam,
    /// Free-fly controls; off when another scene drives the camera.
    pub fly: bool,
    /// Field-of-view magnification (binoculars, scope).
    pub zoom: f32,
    pub radius: u32,
    start: Instant,
    pub fps: f32,
}

/// Object brightness, `renderd3d.cpp: _RenderObject` (`RandomMap` jitter omitted).
fn object_light(area: &Area, p: &Placement) -> f32 {
    let o = &area.rsc.objects[usize::from(p.model)];
    let i = usize::from(p.cell.1) * map::MAP_SIZE + usize::from(p.cell.0);
    // Ground-lit objects take the light of the ground under each vertex
    // (CalcModelGroundLight: 128 + LandLt - 128). Approximated per object.
    if o.info.flags & object_flags::GROUND_LIGHT != 0 {
        return f32::from(area.light_map()[i]) / 255.0;
    }
    let l = if o.info.flags & object_flags::DEF_LIGHT != 0 {
        o.info.default_light
    } else {
        i32::from(area.light_map()[i] >> 1) + 96
    };
    l.clamp(64, 192) as f32 / 255.0
}

impl AreaView {
    pub fn load(gpu: &Gpu, map_path: &Path, time: TimeOfDay) -> Result<Self> {
        let rsc_path = sibling(map_path, "rsc")?;
        let m = map::parse(
            &std::fs::read(map_path).with_context(|| format!("reading {}", map_path.display()))?,
        )?;
        let r = rsc::parse(
            &std::fs::read(&rsc_path).with_context(|| format!("reading {}", rsc_path.display()))?,
        )?;
        let mut area = Area::new(m, r, time);
        let trophy = map_path
            .file_stem()
            .is_some_and(|s| s.to_string_lossy().to_ascii_lowercase().contains("trophy"));
        if trophy {
            area.set_trophy_mode();
        }
        if area.bad_object_cells > 0 {
            tracing::warn!(
                "{} cells reference models missing from the RSC (ignored)",
                area.bad_object_cells
            );
        }
        let globals = Globals::new(gpu);
        let terrain = TerrainRenderer::new(gpu, &globals, &area);
        let prepared: Vec<PreparedModel> = area
            .rsc
            .objects
            .iter()
            .map(|o| PreparedModel::from_mesh(&o.model.mesh))
            .collect();
        // CalcLights runs for every RSC object; NOLIGHT zeroes it and
        // ground-lit objects replace it with ground light.
        let sun = time.sun_pos();
        let models = prepared
            .iter()
            .zip(&area.rsc.objects)
            .map(|(p, o)| {
                let unlit = object_flags::NO_LIGHT | object_flags::GROUND_LIGHT;
                let vl = if o.info.flags & unlit != 0 {
                    Vec::new()
                } else {
                    vertex_lights(&o.model.mesh, sun)
                };
                GpuModel::new(gpu, p, vl)
            })
            .collect();
        let sky_rgba = c2_core::color::image_to_rgba8(&area.rsc.skies[time as usize], false);
        let sky = SkyRenderer::new(gpu, &globals, &sky_rgba, load_sun(map_path, time).as_ref());
        let textures: Vec<&[u8]> = prepared.iter().map(|p| p.texture_rgba.as_slice()).collect();
        let fallback = vec![0u8; 256 * 256 * 4];
        let textures = if textures.is_empty() {
            vec![fallback.as_slice()]
        } else {
            textures
        };
        let renderer = ModelRenderer::new(gpu, &globals, &textures);

        let mut chunks = vec![Vec::new(); CHUNKS * CHUNKS];
        for p in area.placements() {
            let c = usize::from(p.cell.1) / CHUNK * CHUNKS + usize::from(p.cell.0) / CHUNK;
            let l = object_light(&area, &p);
            chunks[c].push((p, l));
        }

        let (lx, ly) = area.landings.first().copied().unwrap_or((512, 512));
        let (x, z) = (f32::from(lx) * CELL + 128.0, f32::from(ly) * CELL + 128.0);
        // In the trophy room, face the middle of the landing ring.
        let yaw = if trophy {
            let c = 76.5 * CELL;
            f32::atan2(-(c - x), -(c - z))
        } else {
            0.0
        };
        let cam = FlyCam {
            pos: Vec3::new(x, area.land_height(x, z) + 400.0, z),
            yaw,
            pitch: -0.1,
            speed: 1500.0,
        };
        let name = map_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Self {
            name,
            area,
            globals,
            terrain,
            sky,
            models,
            renderer,
            chunks,
            cam,
            fly: true,
            zoom: 1.0,
            radius: 74,
            start: Instant::now(),
            fps: 0.0,
        })
    }

    pub fn globals(&self) -> &Globals {
        &self.globals
    }

    pub fn area(&self) -> &Area {
        &self.area
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn cam_cell(&self) -> (i32, i32) {
        (
            (self.cam.pos.x / CELL) as i32,
            (self.cam.pos.z / CELL) as i32,
        )
    }
}

impl Scene for AreaView {
    fn title(&self) -> String {
        let (cx, cy) = self.cam_cell();
        format!(
            "c2rs area {} ({:?}) — cell {cx},{cy} — view {} cells — {:.0} fps — WASD/QE move, drag to look, wheel speed, +/- view",
            self.name, self.area.time, self.radius, self.fps
        )
    }

    fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        if dt > 0.0 {
            self.fps = self.fps * 0.95 + (1.0 / dt) * 0.05;
        }
        if self.fly {
            self.cam.update(dt, input);
        }
        if self.fly && (input.pressed(KeyCode::Equal) || input.pressed(KeyCode::NumpadAdd)) {
            self.radius = (self.radius * 2).min(512);
        }
        if self.fly && (input.pressed(KeyCode::Minus) || input.pressed(KeyCode::NumpadSubtract)) {
            self.radius = (self.radius / 2).max(16);
        }
        // CreateMorphedObject: all instances of an animated object share one pose.
        let t = self.start.elapsed().as_millis() as u32;
        for (o, m) in self.area.rsc.objects.iter().zip(&self.models) {
            if let Some(p) = o.animation.as_ref().and_then(|a| pose_object(a, t)) {
                if p.len() == o.model.mesh.vertices.len() {
                    m.set_pose(gpu, &p);
                }
            }
        }
    }

    fn clear_color(&self) -> [f64; 3] {
        self.area
            .sky_rgb()
            .map(|c| f64::from(c.clamp(0, 255)) / 255.0)
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        self.draw_with(gpu, pass, aspect, &mut |_, _, _| {});
    }
}

/// Extra drawing hooked in after objects and before water.
pub type ExtraDraw<'a> = dyn FnMut(&Gpu, &mut wgpu::RenderPass<'_>, &Globals) + 'a;

impl AreaView {
    pub fn draw_with(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        aspect: f32,
        extra: &mut ExtraDraw<'_>,
    ) {
        let far = self.radius as f32 * CELL;
        let view = self.cam.view();
        let time = self.area.time;
        let t = self.start.elapsed();
        self.globals.write(
            gpu,
            &FrameParams {
                view,
                proj: projection(aspect, far * 1.5, self.zoom),
                eye: self.cam.pos,
                fog_rgb: self.clear_color().map(|c| c as f32),
                fog: (far * 0.6, far),
                night_vision: time.night_vision(),
                time_s: t.as_secs_f32(),
            },
        );
        self.sky.draw(
            gpu,
            pass,
            &self.globals,
            view,
            time.sun_pos(),
            time == TimeOfDay::Night,
        );

        let center = self.cam_cell();
        self.terrain.set_window(
            gpu,
            center,
            self.radius,
            self.area.rsc.textures.len() as u32,
            t.as_millis() as u32,
        );
        self.terrain.draw_ground(pass, &self.globals);

        // Collect object instances within the view radius, grouped by model.
        let mut per_model: Vec<Vec<Instance>> = vec![Vec::new(); self.models.len()];
        let r = self.radius as i32;
        let r2 = (r * r) as f32 * CELL * CELL;
        let (c0x, c0y) = (
            ((center.0 - r).max(0) as usize) / CHUNK,
            ((center.1 - r).max(0) as usize) / CHUNK,
        );
        let (c1x, c1y) = (
            (((center.0 + r).max(0) as usize) / CHUNK).min(CHUNKS - 1),
            (((center.1 + r).max(0) as usize) / CHUNK).min(CHUNKS - 1),
        );
        for cy in c0y..=c1y {
            for cx in c0x..=c1x {
                for (p, light) in &self.chunks[cy * CHUNKS + cx] {
                    let d = Vec3::from(p.pos) - self.cam.pos;
                    if d.x * d.x + d.z * d.z > r2 {
                        continue;
                    }
                    per_model[usize::from(p.model)].push(Instance {
                        offset: p.pos,
                        turns: f32::from(p.quarter_turns),
                        light: *light,
                        layer: u32::from(p.model),
                    });
                }
            }
        }
        let batches: Vec<(&GpuModel, &[Instance])> = self
            .models
            .iter()
            .zip(&per_model)
            .map(|(m, i)| (m, i.as_slice()))
            .collect();
        self.renderer.draw(gpu, pass, &self.globals, &batches);
        extra(gpu, pass, &self.globals);
        self.terrain.draw_water(pass, &self.globals);
    }
}
