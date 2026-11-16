//! Drawing an area: the frame's pass and, with water reflections on, the
//! mirror pass before it.

use super::*;
use crate::render::reflection;
use glam::Mat4;

/// Android draws the ground past 128 cells (`render::terrain::LOD_FINE_CELLS`)
/// in coarser levels even without high-end view: full detail out there
/// costs a phone's GPU far more than it shows (docs/DECISIONS.md). Nearer,
/// including the original's whole view range, it is drawn as before.
const GROUND_LEVELS_ALWAYS: bool = cfg!(target_os = "android");

/// The sun's glow through the fog zones (PC option `modern_fog_glow`): the
/// modernized Modder's Edition's `uFogScatter` (GLTerrain.cpp).
const FOG_GLOW: f32 = 0.25;

/// Cells over which high-end view fades an object from model to sprite.
pub const FADE_CELLS: f32 = 4.0;

/// Extra drawing hooked in after objects and before water.
pub type ExtraDraw<'a> = dyn FnMut(&Gpu, &mut wgpu::RenderPass<'_>, &Globals, &Frustum) + 'a;

/// One pass over the world: the frame's, or the water's mirror.
struct Shot {
    view: Mat4,
    proj: Mat4,
    /// Where `view` looks from: distances (fog, sprites) are measured from it.
    eye: Vec3,
    /// The mirror the water reads (the frame's pass).
    mirror: Option<(f32, (u32, u32))>,
    /// Draw the water (not into its own mirror).
    water: bool,
}

impl AreaView {
    pub(super) fn render_time(&self) -> std::time::Duration {
        self.capture.map_or_else(
            || self.start.elapsed(),
            |(ms, _)| std::time::Duration::from_millis(u64::from(ms)),
        )
    }

    /// How much it is night with night vision off (0..1): the share the
    /// night looks below take. 0 with it on or not in the hunt's hands (the
    /// area viewer: the original's automatic night vision).
    pub(super) fn bare_night(&self) -> f32 {
        if self.night_vision == Some(false) {
            self.area.time_weights()[2]
        } else {
            0.0
        }
    }

    /// Pitch black night (PC option `pitch_black_night`): the world, the
    /// distance fog and the clear colour down to 12% at full night.
    pub(super) fn ambient(&self) -> f32 {
        if self.pitch_black_night {
            1.0 - 0.88 * self.bare_night()
        } else {
            1.0
        }
    }

    /// The frame: the world from the camera, `extra` after the objects,
    /// then the water (over the mirror, if [`AreaView::draw_reflection`]
    /// drew one for this frame).
    pub fn draw_with(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        aspect: f32,
        extra: &mut ExtraDraw<'_>,
    ) {
        let far = self.radius as f32 * CELL;
        let shot = Shot {
            view: self.cam.view(),
            proj: projection(aspect, far * 1.5, self.zoom),
            eye: self.cam.pos,
            mirror: self.mirror.take(),
            water: true,
        };
        self.draw_shot(gpu, pass, &shot, extra);
    }

    /// The water level the mirror takes: the water over the camera's cell,
    /// else the main water in view (`TerrainRenderer::main_water`).
    fn mirror_level(&self) -> Option<f32> {
        let (cx, cy) = self.cam_cell();
        let here = (cx >= 0 && cy >= 0)
            .then(|| self.area.water_level(cx as usize, cy as usize))
            .flatten();
        here.or_else(|| self.terrain.main_water((cx, cy), self.radius))
    }

    /// Water reflections (PC option): before the frame's pass, draws the
    /// world mirrored in the water into the terrain's mirror target, in a
    /// pass and submission of its own (each pass writes the shared buffers
    /// anew). `extra` draws what else should be mirrored. Nothing is drawn
    /// with the option off, under water, or with no water in view; the
    /// frame's water then reflects nothing.
    pub fn draw_reflection(&mut self, gpu: &Gpu, screen: (u32, u32), extra: &mut ExtraDraw<'_>) {
        self.mirror = None;
        let sky_only = self.capture.is_some_and(|(_, sky_only)| sky_only);
        if !self.modern_reflections || self.underwater_fog.is_some() || sky_only {
            return;
        }
        let Some(level) = self.mirror_level() else {
            return;
        };
        if self.cam.pos.y < level + reflection::MIN_EYE_HEIGHT {
            return;
        }
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        let far = self.radius as f32 * CELL;
        let view = reflection::mirrored(self.cam.view(), level);
        let proj = reflection::oblique(
            projection(aspect, far * 1.5, self.zoom),
            view,
            level - reflection::CLIP_BIAS,
        );
        let shot = Shot {
            view,
            proj,
            eye: reflection::mirrored_eye(self.cam.pos, level),
            mirror: None,
            water: false,
        };
        let (color, depth) = self.terrain.reflection.target(gpu, screen);
        let [r, g, b] = self.clear_color();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reflection"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
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
            self.draw_shot(gpu, &mut pass, &shot, extra);
        }
        gpu.queue.submit([enc.finish()]);
        self.mirror = Some((level, screen));
    }

    /// One pass over the world as `shot` sees it.
    fn draw_shot(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        shot: &Shot,
        extra: &mut ExtraDraw<'_>,
    ) {
        let far = self.radius as f32 * CELL;
        let (view, proj) = (shot.view, shot.proj);
        let time = self.area.time;
        let t = self.render_time();
        let frustum = Frustum::new(proj * view);
        let sun = self.area.cycle.map_or(time.sun_pos(), |c| {
            let w = c.weights();
            let dawn = TimeOfDay::Dusk.sun_pos();
            let day = TimeOfDay::Day.sun_pos();
            core::array::from_fn(|k| dawn[k] * (1.0 - w[1]) + day[k] * w[1])
        });
        self.globals.write(
            gpu,
            &FrameParams {
                view,
                proj,
                eye: shot.eye,
                fog_rgb: self.clear_color().map(|c| c as f32),
                fog: if self.underwater_fog.is_some() && !self.original_underwater_fog {
                    (0.0, far * UNDERWATER_FOG_END)
                } else {
                    (far * self.fog_start, far)
                },
                daylight: self.area.time_weights(),
                stars: self.area.cycle.is_some(),
                night_vision: self
                    .night_vision
                    .map_or(self.area.time_weights()[2], f32::from),
                ambient: self.ambient(),
                flashlight: self.flashlight,
                narrow_flashlight: self.narrow_flashlight,
                time_s: t.as_secs_f32(),
                brightness: self.brightness,
                night_grey: if self.desaturated_night {
                    self.bare_night()
                } else {
                    0.0
                },
                absorb: self.underwater_fog.map_or([1.0; 3], |c| {
                    crate::water_tint::transmitted(c, self.water_depth)
                }),
                sky: crate::render::SkyLight {
                    sun,
                    moon: TimeOfDay::Night.sun_pos(),
                    glow: self.modern_sky_glow,
                },
                mirror: shot.mirror,
            },
        );
        // Hunt2.cpp: under water the original fogs with the water's colour
        // through the same function (zone 127): with the original's
        // underwater fog on, the water's fog takes the zones' place;
        // otherwise our underwater fog does (the distance fog above).
        let underwater = self
            .underwater_fog
            .filter(|_| self.original_underwater_fog)
            .and_then(|c| {
                let (cx, cy) = self.cam_cell();
                let level = (cx >= 0 && cy >= 0)
                    .then(|| self.area.water_level(cx as usize, cy as usize))
                    .flatten()?;
                Some((c.map(|v| (v * self.brightness).clamp(0.0, 1.0)), level))
            });
        let zones_on = self.fog_zones && self.underwater_fog.is_none();
        let camera_zone = zones_on
            .then(|| {
                c2_core::fog::Zones::new(&self.area.map, &self.area.rsc.fogs)
                    .camera_zone(shot.eye.to_array())
            })
            .flatten();
        // Sun through fog: the modernized Modder's Edition's strength (0.25),
        // by daylight where theirs takes the sun's visibility on screen.
        let glow = if self.modern_fog_glow {
            FOG_GLOW * (1.0 - self.area.time_weights()[2])
        } else {
            0.0
        };
        self.globals.write_fog_frame(
            gpu,
            &crate::render::FogFrame {
                camera: camera_zone,
                on: zones_on,
                glow,
                underwater,
            },
        );
        self.sky.draw(
            gpu,
            pass,
            &self.globals,
            view,
            sun,
            self.area.time_weights()[2],
        );

        if self.capture.is_some_and(|(_, sky_only)| sky_only) {
            return;
        }
        let center = self.cam_cell();
        self.terrain.set_window(
            gpu,
            center,
            self.radius,
            self.high_end || GROUND_LEVELS_ALWAYS,
            self.area.rsc.textures.len() as u32,
            t.as_millis() as u32,
            self.modern_waves,
            &frustum,
        );
        self.terrain.draw_ground(pass, &self.globals);

        // Collect object instances within the view radius, grouped by model.
        let mut per_model = std::mem::take(&mut self.per_model);
        let mut per_sprite = std::mem::take(&mut self.per_sprite);
        per_model.resize_with(self.models.len(), Vec::new);
        per_sprite.resize_with(self.sprites.len(), Vec::new);
        per_model
            .iter_mut()
            .chain(&mut per_sprite)
            .for_each(Vec::clear);
        // Dynamic time: every object's light follows the time weights.
        let cycling = self.area.cycle.is_some();
        let weights = self.area.time_weights();
        // Sprites stand upright, turned to the camera.
        let facing = self.cam.yaw / std::f32::consts::FRAC_PI_2;
        let sprite_from = self.sprite_cells.map_or(f32::INFINITY, |c| c * CELL);
        let sprite_d2 = sprite_from * sprite_from;
        // The original switches outright (its crossfade in `_RenderObject`
        // is commented out); high-end view fades.
        let fade_end2 = if self.high_end {
            (sprite_from + FADE_CELLS * CELL).powi(2)
        } else {
            0.0
        };
        let n_models = self.models.len() as u32;
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
        // Nearest chunks first: what they cover fails the depth test
        // before the far ones are shaded (sprites discard, so the GPU can't
        // reject them early otherwise).
        let mut seen: Vec<(f32, usize)> = Vec::new();
        for cy in c0y..=c1y {
            for cx in c0x..=c1x {
                if self.chunk_seen(&frustum, cx, cy) {
                    let mid = (Vec3::new(cx as f32, 0.0, cy as f32) + 0.5) * (CHUNK as f32 * CELL);
                    let d = (mid - shot.eye) * Vec3::new(1.0, 0.0, 1.0);
                    seen.push((d.length_squared(), cy * CHUNKS + cx));
                }
            }
        }
        seen.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        for &(_, chunk) in &seen {
            for (p, light, sprite_light, terms) in &self.chunks[chunk] {
                let d = Vec3::from(p.pos) - shot.eye;
                if d.x * d.x + d.z * d.z > r2 {
                    continue;
                }
                let m = usize::from(p.model);
                let d2 = d.length_squared();
                let has_sprite = self.sprites.get(m).is_some_and(Option::is_some);
                // In the fade band: model and sprite share the pixels.
                let fade = if has_sprite && d2 > sprite_d2 && d2 < fade_end2 {
                    ((d2.sqrt() - sprite_from) / (FADE_CELLS * CELL)).clamp(1e-3, 1.0)
                } else {
                    0.0
                };
                if has_sprite && d2 > sprite_d2 {
                    per_sprite[m].push(Instance {
                        offset: p.pos,
                        turns: facing,
                        light: if cycling {
                            c2_core::area::sprite_light(*terms, weights)
                        } else {
                            *sprite_light
                        },
                        layer: n_models + u32::from(p.model),
                        dither: -fade,
                    });
                    if fade == 0.0 {
                        continue;
                    }
                }
                per_model[m].push(Instance {
                    offset: p.pos,
                    turns: f32::from(p.quarter_turns),
                    light: match terms {
                        Some(t) if cycling => t.at(weights),
                        _ => *light,
                    },
                    layer: u32::from(p.model),
                    dither: fade,
                });
            }
        }
        let batches: Vec<(&GpuModel, &[Instance])> = self
            .models
            .iter()
            .zip(&per_model)
            .map(|(m, i)| (m, i.as_slice()))
            .chain(
                self.sprites
                    .iter()
                    .zip(&per_sprite)
                    .filter_map(|(m, i)| Some((m.as_ref()?, i.as_slice()))),
            )
            .collect();
        self.renderer.draw(gpu, pass, &self.globals, &batches);
        drop(batches);
        (self.per_model, self.per_sprite) = (per_model, per_sprite);
        extra(gpu, pass, &self.globals, &frustum);
        if shot.water {
            self.terrain.draw_water(pass, &self.globals);
        }
    }
}
