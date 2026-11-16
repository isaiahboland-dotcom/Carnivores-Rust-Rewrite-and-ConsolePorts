//! `c2-pc area <AREA.MAP>`: fly-cam over an area's terrain, water and objects.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use c2_core::area::{Area, ObjectLight, Placement, TimeOfDay};
use c2_core::model::{pose_object, sprite_mesh, vertex_lights, PreparedModel, SPRITE_CELLS};
use c2_core::CELL;
use c2_formats::rsc::object_flags;
use c2_formats::{map, rsc, three_df};
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::camera::{projection, FlyCam};
use crate::gpu::Gpu;
use crate::input::Input;
use crate::render::{
    FrameParams, Frustum, Globals, GpuModel, Instance, ModelRenderer, SkyRenderer, TerrainRenderer,
};
use crate::scene::Scene;

/// A placement, its light (negative: ground-lit), its sprite's light, and
/// its light without the time of day (for dynamic time).
type PlacedObject = (Placement, f32, f32, Option<ObjectLight>);

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

/// Finds `name` (a file name or a relative path) in `dir`, ignoring case.
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    // `_RES.TXT` paths may name subfolders, with either slash (some mods
    // keep models in `MODELS\...`); each part is matched ignoring case, as
    // on Windows.
    let mut cur = dir.to_path_buf();
    let mut any = false;
    for part in name
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        cur = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(part))
            })?;
        any = true;
    }
    any.then_some(cur)
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
    pub(crate) area: Area,
    pub night_vision: Option<bool>,
    pub flashlight: bool,
    pub narrow_flashlight: bool,
    globals: Globals,
    terrain: TerrainRenderer,
    sky: SkyRenderer,
    light_weights: [f32; 3],
    models: Vec<GpuModel>,
    /// Each object's far sprite (`None`: `ofNOBMP`); its texture is layer
    /// `models.len() + i`.
    sprites: Vec<Option<GpuModel>>,
    renderer: ModelRenderer,
    /// Placements bucketed by chunk, with their precomputed light (negative:
    /// ground-lit, per vertex), their sprite's, and their light without the
    /// time of day (for dynamic time).
    chunks: Vec<Vec<PlacedObject>>,
    /// Each frame's instances per model and per sprite, kept between frames.
    per_model: Vec<Vec<Instance>>,
    per_sprite: Vec<Vec<Instance>>,
    /// Each chunk's objects' bounds (min, max), for frustum culling; min
    /// above max: no objects.
    chunk_bounds: Vec<(Vec3, Vec3)>,
    /// Objects past this many cells as sprites ([`SPRITE_CELLS`], the
    /// original's look); `None`: models all the way.
    pub sprite_cells: Option<f32>,
    /// High-end view: the far ground in coarser levels and objects fading
    /// between model and sprite over [`FADE_CELLS`].
    pub high_end: bool,
    /// Animated waves on open water (`settings::PcSettings::modern_waves`).
    pub modern_waves: bool,
    /// Sun and moon glow (`settings::PcSettings::modern_sky_glow`).
    pub modern_sky_glow: bool,
    /// Water reflections (`settings::PcSettings::modern_reflections`).
    pub modern_reflections: bool,
    /// Sun through fog (`settings::PcSettings::modern_fog_glow`).
    pub modern_fog_glow: bool,
    /// The original's underwater fog
    /// (`settings::PcSettings::original_underwater_fog`).
    pub original_underwater_fog: bool,
    /// Night looks with night vision off (`settings::PcSettings`).
    pub pitch_black_night: bool,
    pub desaturated_night: bool,
    /// The mirror drawn for this frame (`draw_reflection`): the water level
    /// mirrored and the frame's size. Taken by the frame's own draw.
    mirror: Option<(f32, (u32, u32))>,
    pub cam: FlyCam,
    /// Free-fly controls; off when another scene drives the camera.
    pub fly: bool,
    /// Field-of-view magnification (binoculars, scope).
    pub zoom: f32,
    pub radius: u32,
    /// Fog begins at this fraction of `radius` (0.6, as the original).
    pub fog_start: f32,
    /// The map has fog zones (`c2_core::fog`), drawn above water.
    fog_zones: bool,
    /// `(OptBrightness + 128) / 256`: textures, sky and fog colour.
    pub brightness: f32,
    /// Underwater fog colour (the water texture's mean, 0..1 RGB): the
    /// whole scene fades into it. Set by the hunt each frame.
    pub underwater_fog: Option<[f32; 3]>,
    /// How deep the camera is in that water, 0..1 (`water_tint`), with the
    /// depth tint on: the ground loses colour by it. 0: no tint.
    pub water_depth: f32,
    start: Instant,
    /// Fixed clock and sky isolation for native renderer diagnostics only.
    pub(crate) capture: Option<(u32, bool)>,
    pub fps: f32,
}

/// Underwater fog reaches full strength at this fraction of the view
/// distance (the original's value is in its fog table, not reproduced).
const UNDERWATER_FOG_END: f32 = 0.35;

impl AreaView {
    pub fn load(gpu: &Gpu, map_path: &Path, time: TimeOfDay) -> Result<Self> {
        let rsc_path = sibling(map_path, "rsc")?;
        let mut m = map::parse(
            &std::fs::read(map_path).with_context(|| format!("reading {}", map_path.display()))?,
        )?;
        let mut r = rsc::parse(
            &std::fs::read(&rsc_path).with_context(|| format!("reading {}", rsc_path.display()))?,
        )?;
        // Side files past 254 kinds of object (docs/EXPANSION.md).
        let side = |ext: &str| {
            sibling(map_path, ext)
                .ok()
                .and_then(|p| std::fs::read(p).ok())
        };
        c2_formats::obx::apply(
            &mut m,
            &mut r.objects,
            side("obx").as_deref(),
            side("rsx").as_deref(),
        )
        .with_context(|| format!("reading the .OBX/.RSX beside {}", map_path.display()))?;
        let mut area = Area::new(m, r, time);
        let trophy = map_path
            .file_stem()
            .is_some_and(|s| s.to_string_lossy().to_ascii_lowercase().contains("trophy"));
        // Ours beside the map (the phone game's areas), as `c2_core::load`
        // reads them: its snow, a trophy room's own stands.
        let text = |ext: &str| {
            let p = sibling(map_path, ext).ok()?;
            std::fs::read(p)
                .ok()
                .map(|d| String::from_utf8_lossy(&d).into_owned())
        };
        if let Some(t) = text("snow") {
            area.set_snow(&t);
        }
        if trophy {
            area.set_trophy_mode();
            if let Some(t) = text("spot") {
                area.set_trophy_spots(&t);
            }
        }
        if area.bad_object_cells > 0 {
            tracing::warn!(
                "{} cells reference models missing from the RSC (ignored)",
                area.bad_object_cells
            );
        }
        let globals = Globals::with_fog_zones(gpu, &area.map.fogs, &area.rsc.fogs);
        let fog_zones = c2_core::fog::Zones::new(&area.map, &area.rsc.fogs).any();
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
        let skies =
            core::array::from_fn(|i| c2_core::color::image_to_rgba8(&area.rsc.skies[i], false));
        let sun = load_sun(map_path, TimeOfDay::Day);
        let moon = load_sun(map_path, TimeOfDay::Night);
        let sky = SkyRenderer::new(gpu, &globals, &skies, [sun.as_ref(), moon.as_ref()]);
        // RenderBMPModel: a sprite per object, but for ofNOBMP ones.
        let sprite_prep: Vec<Option<PreparedModel>> = area
            .rsc
            .objects
            .iter()
            .map(|o| {
                (o.info.flags & object_flags::NO_BMP == 0)
                    .then(|| PreparedModel::from_mesh(&sprite_mesh(&o.model.mesh, &o.sprite)))
            })
            .collect();
        let sprites = sprite_prep
            .iter()
            .map(|p| p.as_ref().map(|p| GpuModel::new(gpu, p, Vec::new())))
            .collect();
        let blank = vec![0u8; 256 * 256 * 4];
        let textures: Vec<&[u8]> = prepared
            .iter()
            .map(|p| p.texture_rgba.as_slice())
            .chain(sprite_prep.iter().map(|p| {
                p.as_ref()
                    .map_or(blank.as_slice(), |p| p.texture_rgba.as_slice())
            }))
            .collect();
        let fallback = vec![0u8; 256 * 256 * 4];
        let textures = if textures.is_empty() {
            vec![fallback.as_slice()]
        } else {
            textures
        };
        let renderer = ModelRenderer::with_ground(
            gpu,
            &globals,
            &textures,
            &(0..area.map.height.len())
                .map(|i| core::array::from_fn(|t| area.map.light[t][i]))
                .collect::<Vec<_>>(),
        );

        // How far each object reaches from its placement, model or sprite.
        let reach: Vec<f32> = prepared
            .iter()
            .zip(&sprite_prep)
            .map(|(m, s)| {
                let far = |m: &PreparedModel| {
                    m.positions
                        .iter()
                        .map(|p| Vec3::from(*p).length())
                        .fold(0.0, f32::max)
                };
                far(m).max(s.as_ref().map_or(0.0, far))
            })
            .collect();
        let mut chunks = vec![Vec::new(); CHUNKS * CHUNKS];
        let mut chunk_bounds = vec![(Vec3::INFINITY, Vec3::NEG_INFINITY); CHUNKS * CHUNKS];
        for p in area.placements() {
            let c = usize::from(p.cell.1) / CHUNK * CHUNKS + usize::from(p.cell.0) / CHUNK;
            let r = Vec3::splat(reach.get(usize::from(p.model)).copied().unwrap_or(0.0));
            let b = &mut chunk_bounds[c];
            *b = (
                b.0.min(Vec3::from(p.pos) - r),
                b.1.max(Vec3::from(p.pos) + r),
            );
            let ground = area
                .rsc
                .objects
                .get(usize::from(p.model))
                .is_some_and(|o| o.info.flags & object_flags::GROUND_LIGHT != 0);
            let l = if ground { -1.0 } else { area.object_light(&p) };
            let s = area.sprite_light(&p);
            let terms = area.object_light_terms(&p);
            chunks[c].push((p, l, s, terms));
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
            light_weights: c2_core::day_cycle::weights(time),
            night_vision: None,
            flashlight: false,
            narrow_flashlight: false,
            models,
            renderer,
            chunks,
            per_model: Vec::new(),
            per_sprite: Vec::new(),
            chunk_bounds,
            sprites,
            sprite_cells: Some(SPRITE_CELLS),
            high_end: false,
            modern_waves: false,
            modern_sky_glow: false,
            modern_reflections: false,
            modern_fog_glow: false,
            original_underwater_fog: false,
            pitch_black_night: false,
            desaturated_night: false,
            mirror: None,
            cam,
            fly: true,
            zoom: 1.0,
            radius: 74,
            fog_start: 0.6,
            fog_zones,
            underwater_fog: None,
            water_depth: 0.0,
            brightness: 1.0,
            start: Instant::now(),
            capture: None,
            fps: 0.0,
        })
    }

    pub fn globals(&self) -> &Globals {
        &self.globals
    }

    /// Diagnostic ablation of hardware-style object vertex lighting.
    pub(crate) fn capture_without_vertex_light(&mut self, gpu: &Gpu) {
        for (o, m) in self.area.rsc.objects.iter().zip(&mut self.models) {
            let positions: Vec<_> = o
                .model
                .mesh
                .vertices
                .iter()
                .map(|v| [v.x * 2.0, v.y * 2.0, v.z * -2.0])
                .collect();
            m.set_lighting(gpu, Vec::new(), &positions);
        }
    }

    pub fn area(&self) -> &Area {
        &self.area
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether `frustum` may see any object of chunk (`cx`, `cy`).
    fn chunk_seen(&self, frustum: &Frustum, cx: usize, cy: usize) -> bool {
        let (min, max) = self.chunk_bounds[cy * CHUNKS + cx];
        min.cmple(max).all() && frustum.sees_box(min, max)
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
            self.radius = (self.radius * 2).min(1024);
        }
        if self.fly && (input.pressed(KeyCode::Minus) || input.pressed(KeyCode::NumpadSubtract)) {
            self.radius = (self.radius / 2).max(16);
        }
        // Relight static normals only after a perceptible clock change; animated
        // objects upload their current pose below using these same lights.
        let weights = self.area.time_weights();
        if weights
            .iter()
            .zip(self.light_weights)
            .any(|(a, b)| (a - b).abs() >= 0.001)
        {
            self.light_weights = weights;
            if let Some(cycle) = self.area.cycle {
                for (o, m) in self.area.rsc.objects.iter().zip(&mut self.models) {
                    if o.info.flags & (object_flags::NO_LIGHT | object_flags::GROUND_LIGHT) == 0 {
                        let positions: Vec<_> = o
                            .model
                            .mesh
                            .vertices
                            .iter()
                            .map(|v| [v.x * 2.0, v.y * 2.0, v.z * -2.0])
                            .collect();
                        m.set_lighting(
                            gpu,
                            vertex_lights(&o.model.mesh, cycle.sun_pos()),
                            &positions,
                        );
                    }
                }
            }
        }
        // CreateMorphedObject: all instances of an animated object share one pose.
        let t = self.render_time().as_millis() as u32;
        for (o, m) in self.area.rsc.objects.iter().zip(&self.models) {
            if let Some(p) = o.animation.as_ref().and_then(|a| pose_object(a, t)) {
                if p.len() == o.model.mesh.vertices.len() {
                    m.set_pose(gpu, &p);
                }
            }
        }
    }

    fn clear_color(&self) -> [f64; 3] {
        if let Some(c) = self.underwater_fog {
            return c.map(|v| f64::from((v * self.brightness).clamp(0.0, 1.0)));
        }
        if self.area.cycle.is_some() {
            let weights = self.area.time_weights();
            let rgb: [f32; 3] = core::array::from_fn(|k| {
                (0..3)
                    .map(|i| self.area.rsc.fade_rgb[i][k] as f32 * weights[i])
                    .sum()
            });
            let rgb = if self.night_vision == Some(true) {
                [0.0, rgb[1], 0.0]
            } else {
                rgb
            };
            return rgb
                .map(|c| (c * self.brightness * self.ambient()).clamp(0.0, 255.0) as f64 / 255.0);
        }
        let ambient = self.ambient();
        self.area.sky_rgb().map(|c| {
            (f64::from(c) * f64::from(self.brightness * ambient))
                .clamp(0.0, 255.0)
                .floor()
                / 255.0
        })
    }

    fn prepare(&mut self, gpu: &Gpu, screen: (u32, u32)) {
        self.draw_reflection(gpu, screen, &mut |_, _, _, _| {});
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        self.draw_with(gpu, pass, aspect, &mut |_, _, _, _| {});
    }
}

mod draw;
