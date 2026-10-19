//! Drawing a hunt: sky and sun, terrain and water (built in chunks around
//! the hunter), RSC objects (animated ones re-posed each frame), the animals
//! and the ship, particles and water rings, and the view model. Mirrors what the PC
//! renderer does (`c2-pc/src/render.rs`, `terrain.wgsl`), on the PICA200.

use std::collections::HashMap;

use c2_core::area::{cell_flags, Area, Placement, NO_WATER};
use c2_core::dino::pose;
use c2_core::effects::Effects;
use c2_core::game::Hunt;
use c2_core::model::{car_anim_duration_ms, pose_car, pose_object, vertex_lights, PreparedModel};
use c2_core::{CELL, H_SCALE};
use c2_formats::car::Car;
use c2_formats::cooked::parse_texture;
use c2_formats::map::MAP_SIZE;
use c2_formats::rsc::object_flags;
use c2_vfs::Vfs;
use glam::{Mat4, Vec3};

use crate::gpu::{Blend, FlatVertex, Gpu, LinearBuf, Lit, Texture, Vertex};

const CHUNK: usize = 16;
const CHUNKS: usize = MAP_SIZE / CHUNK;
/// `Characters`: model light (`RenderCharacter`, 210).
const CHAR_LIGHT: f32 = 210.0 / 255.0;
/// The cloud plane: quads per side, half its size, height above the eye
/// (`RenderSkyPlane`: 4*512*16 up).
const SKY_GRID: usize = 8;
const SKY_R: f32 = 300_000.0;
const SKY_H: f32 = 32_768.0;
/// Particle discs: rim vertices, and world radius per unit of the
/// original's screen-space radius (as `c2-pc/src/effects_view.rs`).
const DISC_SIDES: usize = 6;
const DISC_K: f32 = 0.64;

/// Pack settings that affect drawing (`C2RS/INFO.TXT`).
#[derive(Debug, Clone)]
pub struct DrawInfo {
    pub tile: usize,
    pub cols: usize,
    pub view: u32,
    pub object_view: u32,
    /// Terrain textures in the atlas (the pack's RSC no longer has them).
    pub terrain_count: usize,
}

fn load_texture(vfs: &Vfs, path: &str) -> Option<Texture> {
    let t = parse_texture(&vfs.read(path).ok()?).ok()?;
    Texture::new(&t)
}

/// A model in GPU memory: opaque faces first, then transparent ones.
struct Model {
    buf: LinearBuf<Vertex>,
    opaque: usize,
    total: usize,
    tex: Option<Texture>,
}

/// Face corners of `p` posed at `pos` (or its rest pose), with `lights`
/// (4 per vertex) as signed bytes. Returns (vertices, opaque count).
fn corners(
    p: &PreparedModel,
    pos: Option<&[[f32; 3]]>,
    lights: &[[f32; 4]],
) -> (Vec<Vertex>, usize) {
    let pos = pos.unwrap_or(&p.positions);
    let mut out = Vec::with_capacity(p.faces.len() * 3);
    let mut opaque = 0;
    for pass in [false, true] {
        for f in p.faces.iter().filter(|f| f.is_transparent() == pass) {
            for k in 0..3 {
                let v = f.v[k] as usize;
                let lit = lights
                    .get(v)
                    .map_or([0; 4], |l| l.map(|x| x.clamp(-127.0, 127.0) as i8 as u8));
                out.push(Vertex {
                    pos: pos.get(v).copied().unwrap_or([0.0; 3]),
                    uv: [f.uv[k][0], f.uv[k][1]],
                    lit,
                });
            }
        }
        if !pass {
            opaque = out.len();
        }
    }
    (out, opaque)
}

impl Model {
    /// Rewrites the vertices for a new pose (same faces, same order).
    fn repose(&mut self, p: &PreparedModel, pos: &[[f32; 3]], lights: &[[f32; 4]]) {
        let (v, _) = corners(p, Some(pos), lights);
        if v.len() == self.total {
            self.buf.write(&v);
        }
    }

    fn new(p: &PreparedModel, lights: &[[f32; 4]], tex: Option<Texture>) -> Option<Self> {
        let (v, opaque) = corners(p, None, lights);
        Some(Self {
            total: v.len(),
            buf: LinearBuf::from_slice(&v)?,
            opaque,
            tex,
        })
    }

    fn draw(&self, gpu: &mut Gpu, lit: Lit) {
        let Some(t) = &self.tex else { return };
        t.bind();
        gpu.draw(&self.buf, lit, 0, self.opaque);
        if self.total > self.opaque {
            gpu.blend(Blend::Alpha);
            gpu.texenv(Some(128));
            gpu.depth(false, false);
            gpu.draw(&self.buf, lit, self.opaque, self.total - self.opaque);
            gpu.blend(Blend::Cutout);
            gpu.texenv(None);
            gpu.depth(true, false);
        }
    }
}

/// An animated model whose vertices are rewritten every frame.
struct Posed {
    prepared: PreparedModel,
    tex: Option<Texture>,
}

/// A `.CAR` drawn outside the herd (the ship, the water ring), with its own
/// buffers.
struct Extra {
    car: Car,
    posed: Posed,
    bufs: Vec<Option<LinearBuf<Vertex>>>,
}

impl Extra {
    fn load(vfs: &Vfs, car: &str, tex: &str) -> Option<Self> {
        let car = c2_formats::car::parse(&vfs.read(car).ok()?).ok()?;
        let mut prepared = PreparedModel::from_mesh(&car.mesh);
        prepared.texture_rgba = Vec::new();
        Some(Self {
            posed: Posed {
                prepared,
                tex: load_texture(vfs, tex),
            },
            car,
            bufs: Vec::new(),
        })
    }

    /// Animation 0 at `ms`, scaled, into buffer `slot`; then drawn.
    fn draw(&mut self, gpu: &mut Gpu, slot: usize, ms: i32, scale: f32) {
        let Some(anim) = self.car.animations.first() else {
            return;
        };
        let Some(pos) = pose_car(anim, self.car.mesh.vertices.len(), ms.max(0) as u32) else {
            return;
        };
        let pos: Vec<[f32; 3]> = pos.iter().map(|p| p.map(|c| c * scale)).collect();
        let (v, _) = corners(&self.posed.prepared, Some(&pos), &[]);
        if self.bufs.len() <= slot {
            self.bufs.resize_with(slot + 1, || None);
        }
        let b = &mut self.bufs[slot];
        if b.as_ref().is_none_or(|b| b.cap() < v.len()) {
            *b = LinearBuf::new(v.len());
        }
        let (Some(b), Some(t)) = (b.as_mut(), &self.posed.tex) else {
            return;
        };
        b.write(&v);
        t.bind();
        gpu.draw(b, Lit::Signed, 0, b.len());
    }
}

struct Chunk {
    ground: Option<LinearBuf<Vertex>>,
    /// Water quads by water type: (type, first, count).
    water: Option<LinearBuf<Vertex>>,
    water_ranges: Vec<(usize, usize, usize)>,
    used: u32,
}

pub struct World {
    pub info: DrawInfo,
    terrain: Option<Texture>,
    atlas_size: (usize, usize),
    terrain_count: usize,
    chunks: HashMap<(usize, usize), Chunk>,
    objects: Vec<Option<Model>>,
    /// Animated RSC objects: their prepared model and lights, by index.
    object_anims: Vec<Option<(PreparedModel, Vec<[f32; 4]>)>>,
    sky_tex: Option<Texture>,
    sky_buf: Option<LinearBuf<Vertex>>,
    /// Sun or moon (`C2RS/SUN.3DF`).
    sun: Option<Model>,
    ship: Option<Extra>,
    ring: Option<Extra>,
    flat_buf: Option<LinearBuf<FlatVertex>>,
    placements: Vec<Vec<(Placement, f32)>>,
    species: Vec<Option<Posed>>,
    weapons: Vec<Option<Posed>>,
    /// One buffer per character slot, refilled when it is drawn.
    char_bufs: Vec<Option<LinearBuf<Vertex>>>,
    weapon_buf: Option<LinearBuf<Vertex>>,
    frame: u32,
    pub sky: [u8; 3],
    water_alpha: Vec<u8>,
}

/// DrawTPlane's texture rotations.
fn rotate_uv(d: [f32; 2], dir: u16) -> [f32; 2] {
    match dir {
        1 => [d[1], 1.0 - d[0]],
        2 => [1.0 - d[0], 1.0 - d[1]],
        3 => [1.0 - d[1], d[0]],
        _ => d,
    }
}

const NORMAL: [[usize; 2]; 6] = [[0, 0], [1, 0], [1, 1], [0, 0], [1, 1], [0, 1]];
const REVERSE: [[usize; 2]; 6] = [[0, 0], [1, 0], [0, 1], [0, 1], [1, 0], [1, 1]];

impl World {
    pub fn new(
        vfs: &Vfs,
        area: &Area,
        info: DrawInfo,
        cars: &[Option<Car>],
        weapon_cars: &[Option<Car>],
    ) -> Self {
        let terrain = parse_texture(&vfs.read("C2RS/TERRAIN.TX").unwrap_or_default()).ok();
        let atlas_size = terrain
            .as_ref()
            .map_or((1, 1), |t| (usize::from(t.width), usize::from(t.height)));
        let terrain = terrain.and_then(|t| Texture::new(&t));
        let sun = area.time.sun_pos();
        let mut object_anims = Vec::new();
        let objects = area
            .rsc
            .objects
            .iter()
            .enumerate()
            .map(|(k, o)| {
                let mut p = PreparedModel::from_mesh(&o.model.mesh);
                p.texture_rgba = Vec::new();
                let unlit = object_flags::NO_LIGHT | object_flags::GROUND_LIGHT;
                let vl = if o.info.flags & unlit != 0 {
                    Vec::new()
                } else {
                    vertex_lights(&o.model.mesh, sun)
                };
                let m = Model::new(&p, &vl, load_texture(vfs, &format!("C2RS/OBJ{k}.TX")));
                object_anims.push(o.animation.is_some().then_some((p, vl)));
                m
            })
            .collect();
        let mut sky_tex = load_texture(vfs, "C2RS/SKY.TX");
        if let Some(t) = &mut sky_tex {
            t.repeat();
        }
        let sun_model = vfs
            .read("C2RS/SUN.3DF")
            .ok()
            .and_then(|d| c2_formats::three_df::parse(&d).ok())
            .and_then(|m| {
                Model::new(
                    &PreparedModel::from_mesh(&m.mesh),
                    &[],
                    load_texture(vfs, "C2RS/SUN.TX"),
                )
            });
        let mut placements = vec![Vec::new(); CHUNKS * CHUNKS];
        for p in area.placements() {
            let c = usize::from(p.cell.1) / CHUNK * CHUNKS + usize::from(p.cell.0) / CHUNK;
            let l = area.object_light(&p);
            placements[c].push((p, l));
        }
        let posed = |cars: &[Option<Car>], prefix: &str| -> Vec<Option<Posed>> {
            cars.iter()
                .enumerate()
                .map(|(i, c)| {
                    let c = c.as_ref()?;
                    let mut prepared = PreparedModel::from_mesh(&c.mesh);
                    // The texture is on the GPU (from the .TX).
                    prepared.texture_rgba = Vec::new();
                    Some(Posed {
                        prepared,
                        tex: load_texture(vfs, &format!("C2RS/{prefix}{i}.TX")),
                    })
                })
                .collect()
        };
        let sky = area.sky_rgb().map(|c| c.clamp(0, 255) as u8);
        let water_alpha = area
            .rsc
            .waters
            .iter()
            .map(|w| (w.transp.clamp(0.0, 1.0) * 255.0) as u8)
            .collect();
        let terrain_count = info.terrain_count.max(1);
        Self {
            info,
            terrain,
            atlas_size,
            terrain_count,
            chunks: HashMap::new(),
            objects,
            object_anims,
            sky_tex,
            sky_buf: LinearBuf::new(SKY_GRID * SKY_GRID * 6),
            sun: sun_model,
            ship: Extra::load(vfs, "HUNTDAT/SHIP2A.CAR", "C2RS/SHIP.TX"),
            ring: Extra::load(vfs, "HUNTDAT/WCIRCLE2.CAR", "C2RS/RING.TX"),
            flat_buf: None,
            placements,
            species: posed(cars, "CHAR"),
            weapons: posed(weapon_cars, "WPN"),
            char_bufs: Vec::new(),
            weapon_buf: None,
            frame: 0,
            sky,
            water_alpha,
        }
    }

    /// Atlas texture coordinate of `d` (0..1 in the cell) for texture `tex`.
    fn atlas_uv(&self, tex: usize, d: [f32; 2]) -> [f32; 2] {
        let (t, cols) = (self.info.tile as f32, self.info.cols.max(1));
        let (aw, ah) = (self.atlas_size.0 as f32, self.atlas_size.1 as f32);
        let (cx, cy) = ((tex % cols) as f32 * t, (tex / cols) as f32 * t);
        // Half a texel in from the edges, so neighbours don't bleed in.
        let inset = 0.5 / t;
        let k = 1.0 - 2.0 * inset;
        let u = (cx + (inset + d[0] * k) * t) / aw;
        let v = (cy + (inset + d[1] * k) * t) / ah;
        [u, v]
    }

    fn build_chunk(&self, area: &Area, cx: usize, cy: usize) -> Chunk {
        let m = &area.map;
        let light = area.light_map();
        let mut ground = Vec::with_capacity(CHUNK * CHUNK * 6);
        let mut water: Vec<(usize, [Vertex; 6])> = Vec::new();
        // (The RSC's texels are freed after loading; the count is kept.)
        let ntex = self.terrain_count.max(1);
        for y in cy * CHUNK..((cy + 1) * CHUNK).min(MAP_SIZE - 1) {
            for x in cx * CHUNK..((cx + 1) * CHUNK).min(MAP_SIZE - 1) {
                let i = y * MAP_SIZE + x;
                let flags = m.flags[i];
                let tex = usize::from(m.texture1[i]).min(ntex - 1);
                let order = if flags & cell_flags::REVERSE != 0 {
                    &REVERSE
                } else {
                    &NORMAL
                };
                for d in order {
                    let (vx, vy) = (x + d[0], y + d[1]);
                    let vi = vy * MAP_SIZE + vx;
                    let uv =
                        rotate_uv([d[0] as f32, d[1] as f32], flags & cell_flags::TEX_DIR_MASK);
                    ground.push(Vertex {
                        pos: [
                            vx as f32 * CELL,
                            f32::from(m.height[vi]) * H_SCALE,
                            vy as f32 * CELL,
                        ],
                        uv: self.atlas_uv(tex, uv),
                        lit: [light[vi], 0, 0, 0],
                    });
                }
                // ProcessMapW: all four corners must be water.
                let w = m.water[i];
                let wet = |dx: usize, dy: usize| m.water[(y + dy) * MAP_SIZE + x + dx] != NO_WATER;
                if w != NO_WATER && wet(1, 0) && wet(0, 1) && wet(1, 1) {
                    if let Some(wt) = area.rsc.waters.get(usize::from(w)) {
                        let level = wt.level as f32 * H_SCALE;
                        let tex = (wt.texture_index.max(0) as usize).min(ntex - 1);
                        let quad = NORMAL.map(|d| Vertex {
                            pos: [(x + d[0]) as f32 * CELL, level, (y + d[1]) as f32 * CELL],
                            uv: self.atlas_uv(tex, [d[0] as f32, d[1] as f32]),
                            lit: [255, 0, 0, 0],
                        });
                        water.push((usize::from(w), quad));
                    }
                }
            }
        }
        water.sort_by_key(|(w, _)| *w);
        let mut ranges: Vec<(usize, usize, usize)> = Vec::new();
        let mut wv = Vec::with_capacity(water.len() * 6);
        for (w, q) in &water {
            match ranges.last_mut() {
                Some(r) if r.0 == *w => r.2 += 6,
                _ => ranges.push((*w, wv.len(), 6)),
            }
            wv.extend_from_slice(q);
        }
        Chunk {
            ground: LinearBuf::from_slice(&ground),
            water: if wv.is_empty() {
                None
            } else {
                LinearBuf::from_slice(&wv)
            },
            water_ranges: ranges,
            used: self.frame,
        }
    }

    /// Chunks within `radius` cells of `eye` and in front of the camera.
    fn visible_chunks(&self, eye: Vec3, fwd: Vec3, radius: f32) -> Vec<(usize, usize)> {
        let r = radius + CHUNK as f32;
        let (ex, ez) = (eye.x / CELL, eye.z / CELL);
        let lo = |e: f32| ((e - r).max(0.0) as usize) / CHUNK;
        let hi = |e: f32| (((e + r).max(0.0) as usize) / CHUNK).min(CHUNKS - 1);
        let f2 = glam::Vec2::new(fwd.x, fwd.z).normalize_or_zero();
        let mut out = Vec::new();
        for cy in lo(ez)..=hi(ez) {
            for cx in lo(ex)..=hi(ex) {
                let c = glam::Vec2::new(
                    (cx * CHUNK) as f32 + CHUNK as f32 / 2.0 - ex,
                    (cy * CHUNK) as f32 + CHUNK as f32 / 2.0 - ez,
                );
                if c.length() > r {
                    continue;
                }
                // Behind the camera by more than a chunk's half-diagonal.
                if c.dot(f2) < -(CHUNK as f32) * 0.75 {
                    continue;
                }
                out.push((cx, cy));
            }
        }
        out
    }

    /// Draws a frame of `hunt` seen from `view` with vertical FOV `fovy`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &mut Gpu,
        area: &Area,
        hunt: &Hunt,
        poser: &Poser<'_>,
        view: &Mat4,
        eye: Vec3,
        fwd: Vec3,
        fovy: f32,
    ) {
        self.frame = self.frame.wrapping_add(1);
        let radius = self.info.view as f32;
        let far = radius * CELL * 1.5;
        let near = 16.0;
        self.draw_sky(gpu, area, view, eye, fovy, hunt.real_time);
        gpu.set_projection(fovy, near, far);
        gpu.set_fog(self.sky, radius * CELL * 0.6, radius * CELL);
        gpu.texenv(None);

        // Terrain: build missing chunks (a few per frame once running).
        let vis = self.visible_chunks(eye, fwd, radius);
        let mut built = 0;
        for &k in &vis {
            if !self.chunks.contains_key(&k) && (built < 6 || self.chunks.is_empty()) {
                let c = self.build_chunk(area, k.0, k.1);
                self.chunks.insert(k, c);
                built += 1;
            }
        }
        let frame = self.frame;
        self.chunks.retain(|_, c| frame.wrapping_sub(c.used) < 120);
        gpu.set_model_view(view);
        gpu.set_light(0.0, Some(0));
        if let Some(t) = &self.terrain {
            t.bind();
            for k in &vis {
                if let Some(c) = self.chunks.get_mut(k) {
                    c.used = frame;
                    if let Some(g) = &c.ground {
                        gpu.draw(g, Lit::Unsigned, 0, g.len());
                    }
                }
            }
        }

        // Objects within their view distance, by model.
        let ov = self.info.object_view as f32 * CELL;
        let mut by_model: Vec<Vec<(Placement, f32)>> = vec![Vec::new(); self.objects.len()];
        for k in &vis {
            for (p, l) in &self.placements[k.1 * CHUNKS + k.0] {
                let d = Vec3::from(p.pos) - eye;
                if d.x * d.x + d.z * d.z <= ov * ov {
                    if let Some(v) = by_model.get_mut(usize::from(p.model)) {
                        v.push((*p, *l));
                    }
                }
            }
        }
        // CreateMorphedObject: all instances of an animated object share
        // one pose, so re-pose the visible ones once.
        for ((m, a), (list, o)) in self
            .objects
            .iter_mut()
            .zip(&self.object_anims)
            .zip(by_model.iter().zip(&area.rsc.objects))
        {
            let (Some(m), Some((p, vl)), Some(anim)) = (m, a, &o.animation) else {
                continue;
            };
            if list.is_empty() {
                continue;
            }
            if let Some(pos) = pose_object(anim, hunt.real_time.max(0) as u32) {
                m.repose(p, &pos, vl);
            }
        }
        for (m, list) in self.objects.iter().zip(&by_model) {
            let Some(m) = m else { continue };
            for (p, l) in list {
                let a = f32::from(p.quarter_turns) * std::f32::consts::FRAC_PI_2;
                let mv =
                    *view * Mat4::from_translation(Vec3::from(p.pos)) * Mat4::from_rotation_y(a);
                gpu.set_model_view(&mv);
                gpu.set_light(*l, Some(usize::from(p.quarter_turns)));
                m.draw(gpu, Lit::Signed);
            }
        }

        self.draw_animals(gpu, hunt, poser, view, eye);
        self.draw_ship(gpu, hunt, view, eye);

        // Water, blended, after everything opaque.
        gpu.set_model_view(view);
        gpu.set_light(0.0, Some(0));
        gpu.blend(Blend::Alpha);
        gpu.depth(false, false);
        if let Some(t) = &self.terrain {
            t.bind();
            for k in &vis {
                let Some(c) = self.chunks.get(k) else {
                    continue;
                };
                let Some(w) = &c.water else { continue };
                for &(ty, first, n) in &c.water_ranges {
                    gpu.texenv(Some(self.water_alpha.get(ty).copied().unwrap_or(128)));
                    gpu.draw(w, Lit::Unsigned, first, n);
                }
            }
        }
        gpu.blend(Blend::Cutout);
        gpu.texenv(None);
        gpu.depth(true, false);

        self.draw_effects(gpu, &hunt.effects, view);
        self.draw_weapon(gpu, area, hunt, poser);
    }

    /// `RenderSkyPlane` (a scrolling cloud plane above the eye, fading out
    /// with distance) and `RenderSun`. Drawn first, without depth.
    fn draw_sky(
        &mut self,
        gpu: &mut Gpu,
        area: &Area,
        view: &Mat4,
        eye: Vec3,
        fovy: f32,
        real_time: i32,
    ) {
        gpu.set_projection(fovy, 64.0, SKY_R * 2.0);
        gpu.depth_off();
        if let (Some(t), Some(buf)) = (&self.sky_tex, self.sky_buf.as_mut()) {
            // 0.002 texels per unit, scrolled by (RealTime & 0x1FFFF) / 512
            // texels; one texture is 256 texels.
            let dtt = (real_time & 0x1_FFFF) as f32 / 512.0;
            let step = 2.0 * SKY_R / SKY_GRID as f32;
            let y = eye.y + SKY_H;
            let mut v = Vec::with_capacity(SKY_GRID * SKY_GRID * 6);
            for gz in 0..SKY_GRID {
                for gx in 0..SKY_GRID {
                    for d in NORMAL {
                        let x = eye.x - SKY_R + (gx + d[0]) as f32 * step;
                        let z = eye.z - SKY_R + (gz + d[1]) as f32 * step;
                        v.push(Vertex {
                            pos: [x, y, z],
                            uv: [(x * 0.002 + dtt) / 256.0, (z * 0.002 - dtt) / 256.0],
                            lit: [0; 4],
                        });
                    }
                }
            }
            buf.write(&v);
            gpu.set_model_view(view);
            gpu.set_light(1.0, None);
            // Alpha 40240 / (40240 + max(0, d - 100200)) over the clear
            // colour, which is the fog colour: a linear fade is close.
            gpu.set_fog(self.sky, 100_200.0, 280_000.0);
            gpu.texenv(None);
            t.bind();
            gpu.draw(buf, Lit::Unsigned, 0, buf.len());
        }
        // RenderSun: in camera space, pulled closer near the screen centre;
        // only when in front (z < -2024). Alpha 200, added.
        if let Some(sun) = &self.sun {
            let night = area.time == c2_core::area::TimeOfDay::Night;
            let nv = view.transform_vector3(Vec3::from(area.time.sun_pos()));
            if nv.z < -2024.0 {
                let d = if night {
                    1.5
                } else {
                    (2048.0 + (nv.x * nv.x + nv.y * nv.y).sqrt().min(812.0)) / 3048.0
                };
                gpu.set_model_view(&Mat4::from_translation(nv * d));
                gpu.set_light(200.0 / 255.0, None);
                gpu.no_fog();
                gpu.blend(Blend::Add);
                sun.draw(gpu, Lit::Signed);
                gpu.blend(Blend::Cutout);
            }
        }
        gpu.depth(true, false);
    }

    /// The pickup ship: landing animation at `FTime`, light 240
    /// (`RenderShip`).
    fn draw_ship(&mut self, gpu: &mut Gpu, hunt: &Hunt, view: &Mat4, eye: Vec3) {
        let ship = &hunt.ship;
        let max = self.info.view as f32 * CELL;
        let d = Vec3::from(ship.pos) - eye;
        if ship.state == -1 || d.x * d.x + d.z * d.z > max * max {
            return;
        }
        let Some(x) = &mut self.ship else { return };
        let a = ship.quarter_turns() * std::f32::consts::FRAC_PI_2;
        gpu.set_model_view(
            &(*view * Mat4::from_translation(Vec3::from(ship.pos)) * Mat4::from_rotation_y(a)),
        );
        gpu.set_light(240.0 / 255.0, None);
        x.draw(gpu, 0, ship.ftime, 1.0);
    }

    /// Particles and blood as camera-facing discs (centre colour fading to
    /// the edge's), then water rings, added (`RenderElements`,
    /// `RenderWCircles`).
    fn draw_effects(&mut self, gpu: &mut Gpu, fx: &Effects, view: &Mat4) {
        // Colours are [a, r, g, b].
        let rgba = |c: [u8; 4]| [c[1], c[2], c[3], c[0]];
        let mut discs: Vec<([f32; 3], f32, [u8; 4], [u8; 4])> = Vec::new();
        for b in &fx.bursts {
            let (c0, c1) = (rgba(b.rgba), rgba(b.rgba2));
            discs.extend(b.parts.iter().map(|p| (p.pos, p.r * DISC_K, c0, c1)));
        }
        for d in &fx.blood {
            let a1 = (0xE0 * d.life / 20000).clamp(0, 0xE0) as u8;
            let a2 = (0x20 * d.life / 20000).clamp(0, 0x20) as u8;
            discs.push((d.pos, 12.0 * DISC_K, [0x70, 0, 0, a1], [0x30, 0, 0, a2]));
        }
        if !discs.is_empty() {
            let mut v = Vec::with_capacity(discs.len() * DISC_SIDES * 3);
            for (c, r, c0, c1) in discs {
                let cv = view.transform_point3(Vec3::from(c));
                if cv.z > -16.0 {
                    continue;
                }
                let rim = |k: usize| {
                    let a = k as f32 / DISC_SIDES as f32 * std::f32::consts::TAU;
                    FlatVertex {
                        pos: [cv.x + a.cos() * r, cv.y + a.sin() * r, cv.z],
                        rgba: c1,
                    }
                };
                for k in 0..DISC_SIDES {
                    v.push(FlatVertex {
                        pos: cv.to_array(),
                        rgba: c0,
                    });
                    v.push(rim(k));
                    v.push(rim(k + 1));
                }
            }
            if self.flat_buf.as_ref().is_none_or(|b| b.cap() < v.len()) {
                self.flat_buf = LinearBuf::new(v.len().next_power_of_two());
            }
            if let Some(b) = self.flat_buf.as_mut() {
                b.write(&v);
                gpu.set_model_view(&Mat4::IDENTITY);
                gpu.blend(Blend::Alpha);
                gpu.depth(false, false);
                gpu.draw_flat(b, 0, b.len());
                gpu.blend(Blend::Cutout);
                gpu.depth(true, false);
            }
        }
        // Rings: GlassL = 255 - (2000 - FTime) / 38, faint and fading.
        let Some(ring) = &mut self.ring else { return };
        gpu.blend(Blend::Add);
        gpu.depth(false, false);
        for (i, r) in fx.rings.iter().enumerate() {
            let alpha = ((2000 - r.ftime) as f32 / 38.0).clamp(0.0, 255.0) / 255.0;
            gpu.set_model_view(&(*view * Mat4::from_translation(Vec3::from(r.pos))));
            gpu.set_light(250.0 / 255.0 * alpha, None);
            ring.draw(gpu, i, r.ftime, r.scale);
        }
        gpu.blend(Blend::Cutout);
        gpu.depth(true, false);
    }
}

/// The `.CAR`s the animals and view models are posed from.
pub struct Poser<'a> {
    pub cars: &'a [Option<Car>],
    pub weapon_cars: &'a [Option<Car>],
}

impl World {
    /// Every animal within the view distance, posed from its `.CAR`.
    fn draw_animals(
        &mut self,
        gpu: &mut Gpu,
        hunt: &Hunt,
        poser: &Poser<'_>,
        view: &Mat4,
        eye: Vec3,
    ) {
        let max = self.info.view as f32 * CELL;
        let herd = &hunt.herd;
        if self.char_bufs.len() < herd.chars.len() {
            self.char_bufs.resize_with(herd.chars.len(), || None);
        }
        gpu.set_light(CHAR_LIGHT, None);
        for (i, c) in herd.chars.iter().enumerate() {
            let d = Vec3::from(c.pos) - eye;
            if d.x * d.x + d.z * d.z > max * max {
                continue;
            }
            let (Some(Some(sp)), Some(Some(car))) =
                (self.species.get(c.ctype), poser.cars.get(c.ctype))
            else {
                continue;
            };
            let Some(tex) = &sp.tex else { continue };
            let Some(pos) = pose::pose(c, car) else {
                continue;
            };
            let (v, _) = corners(&sp.prepared, Some(&pos), &[]);
            let slot = &mut self.char_bufs[i];
            if slot.as_ref().is_none_or(|b| b.cap() < v.len()) {
                *slot = LinearBuf::new(v.len());
            }
            let Some(buf) = slot.as_mut() else { continue };
            buf.write(&v);
            let a = pose::quarter_turns(c) * std::f32::consts::FRAC_PI_2;
            let mv = *view * Mat4::from_translation(Vec3::from(c.pos)) * Mat4::from_rotation_y(a);
            gpu.set_model_view(&mv);
            tex.bind();
            gpu.draw(buf, Lit::Signed, 0, buf.len());
        }
    }

    /// The view model, in camera space, drawn in front of everything.
    fn draw_weapon(&mut self, gpu: &mut Gpu, area: &Area, hunt: &Hunt, poser: &Poser<'_>) {
        let w = &hunt.weapons;
        if hunt.death.is_some() || hunt.bin_mode {
            return;
        }
        let Some((slot, ftime)) = w.pose() else {
            return;
        };
        let (Some(Some(car)), Some(Some(model))) = (
            poser.weapon_cars.get(w.current),
            self.weapons.get(w.current),
        ) else {
            return;
        };
        let Some(anim) = car.animations.get(slot) else {
            return;
        };
        let dur = car_anim_duration_ms(anim).max(1);
        let t = (ftime.max(0) as u32).min(dur - 1);
        let Some(verts) = pose_car(anim, car.mesh.vertices.len(), t) else {
            return;
        };
        let (da, db) = w.sway(hunt.real_time);
        let (sa, ca) = (-da).sin_cos();
        let (sb, cb) = (-db).sin_cos();
        let cam: Vec<[f32; 3]> = verts
            .iter()
            .map(|v| {
                let x = v[0] * ca + v[2] * sa;
                let vz = v[2] * ca - v[0] * sa;
                [x, v[1] * cb - vz * sb, vz * cb + v[1] * sb]
            })
            .collect();
        let (v, _) = corners(&model.prepared, Some(&cam), &[]);
        if self.weapon_buf.as_ref().is_none_or(|b| b.cap() < v.len()) {
            self.weapon_buf = LinearBuf::new(v.len());
        }
        let (Some(buf), Some(tex)) = (self.weapon_buf.as_mut(), &model.tex) else {
            return;
        };
        buf.write(&v);
        // Weapon light: 96 + ground light / 4 (`wpnlight`).
        let p = &hunt.player;
        let cell = MAP_SIZE * (p.z / CELL) as usize + (p.x / CELL) as usize;
        let ground = area.light_map().get(cell).copied().unwrap_or(128);
        gpu.set_light((96.0 + f32::from(ground) / 4.0) / 255.0, None);
        gpu.set_model_view(&Mat4::IDENTITY);
        gpu.no_fog();
        gpu.depth(true, true);
        tex.bind();
        gpu.draw(buf, Lit::Signed, 0, buf.len());
        gpu.depth(true, false);
    }
}
