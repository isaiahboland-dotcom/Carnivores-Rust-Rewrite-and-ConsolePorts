//! Loading species (`_RES.TXT` + `.CAR`) and drawing characters.

use std::path::Path;

use c2_core::dino::{pose, Character, DinoInfo, Herd, Species};
use c2_core::model::PreparedModel;
use c2_core::ship::Ship;
use c2_formats::car::{self, Car};
use c2_formats::res_txt::{ResData, ResTxt};

use crate::area_view::find_file;
use crate::gpu::Gpu;
use crate::render::{Frustum, Globals, GpuModel, Instance, ModelRenderer};
use glam::Vec3;

/// `RenderCharacterPost` draws characters with a fixed light of 210.
const CHAR_LIGHT: f32 = 210.0 / 255.0;

/// A character model and its render-ready form.
pub type LoadedCar = (Car, PreparedModel);

pub struct DinoAssets {
    /// Indexed by `CType`; `None` if the `.CAR` failed to load.
    pub cars: Vec<Option<LoadedCar>>,
    /// The pickup ship (`SHIP2A.CAR`), drawn with texture layer `cars.len()`.
    ship: Option<(LoadedCar, GpuModel)>,
    renderer: ModelRenderer,
    /// One GPU model per character (each has its own pose), with the
    /// species it was built for (indices shift when characters are removed).
    models: Vec<Option<(usize, GpuModel)>>,
    /// Shared by successive animal/ship uploads; no pose is retained after upload.
    pose: Vec<[f32; 3]>,
    /// How far each car's rest pose reaches from its origin, padded for
    /// the animations ([`model_reach`]); the ship's.
    reach: Vec<f32>,
    ship_reach: f32,
}

/// Reads `HUNTDAT/_RES.TXT`.
pub fn load_res(huntdat: &Path) -> anyhow::Result<ResData> {
    let res_path = find_file(huntdat, "_RES.TXT")
        .ok_or_else(|| anyhow::anyhow!("no _RES.TXT in {}", huntdat.display()))?;
    Ok(ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(&res_path)?))?.to_data())
}

/// Reads every character `.CAR` named in `_RES.TXT`.
pub fn load_species(
    huntdat: &Path,
    res: &ResData,
) -> anyhow::Result<(Vec<Species>, Vec<Option<LoadedCar>>)> {
    let mut species = Vec::new();
    let mut cars = Vec::new();
    for def in &res.characters {
        let info = DinoInfo::from_def(def);
        let car = find_file(huntdat, &info.file).and_then(|p| {
            match std::fs::read(&p)
                .map_err(anyhow::Error::from)
                .and_then(|d| Ok(car::parse(&d)?))
            {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!("{}: {e:#}", p.display());
                    None
                }
            }
        });
        if car.is_none() {
            tracing::warn!("character {:?}: {} not loaded", info.name, info.file);
        }
        let empty = Car {
            name_raw: [0; 32],
            mesh: Default::default(),
            animations: vec![],
            sounds: vec![],
            anim_sounds: None,
            trailing: Vec::new(),
        };
        species.push(Species::new(info, car.as_ref().unwrap_or(&empty)));
        cars.push(car.map(|c| {
            let p = PreparedModel::from_mesh(&c.mesh);
            (c, p)
        }));
    }
    Ok((species, cars))
}

/// Loads `HUNTDAT/SHIP2A.CAR`.
pub fn load_ship(huntdat: &Path) -> Option<LoadedCar> {
    let p = find_file(huntdat, "SHIP2A.CAR")?;
    match std::fs::read(&p)
        .map_err(anyhow::Error::from)
        .and_then(|d| Ok(car::parse(&d)?))
    {
        Ok(c) => {
            let prepared = PreparedModel::from_mesh(&c.mesh);
            Some((c, prepared))
        }
        Err(e) => {
            tracing::warn!("{}: {e:#}", p.display());
            None
        }
    }
}

impl DinoAssets {
    pub fn new(
        gpu: &Gpu,
        globals: &Globals,
        cars: Vec<Option<LoadedCar>>,
        ship: Option<LoadedCar>,
    ) -> Self {
        let blank = vec![0u8; 256 * 256 * 4];
        let mut textures: Vec<&[u8]> = cars
            .iter()
            .map(|c| {
                c.as_ref()
                    .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice())
            })
            .collect();
        textures.push(
            ship.as_ref()
                .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice()),
        );
        let renderer = ModelRenderer::new(gpu, globals, &textures);
        let ship = ship.map(|(c, p)| {
            let m = GpuModel::new(gpu, &p, Vec::new());
            ((c, p), m)
        });
        let reach = cars
            .iter()
            .map(|c| c.as_ref().map_or(0.0, |(_, p)| model_reach(p)))
            .collect();
        let ship_reach = ship.as_ref().map_or(0.0, |((_, p), _)| model_reach(p));
        Self {
            cars,
            ship,
            renderer,
            models: Vec::new(),
            pose: Vec::new(),
            reach,
            ship_reach,
        }
    }

    pub fn ship_car(&self) -> Option<&Car> {
        self.ship.as_ref().map(|((c, _), _)| c)
    }

    /// Length of the ship's landing animation (`Animation[0]`).
    pub fn ship_anim_ms(&self) -> i32 {
        self.ship
            .as_ref()
            .and_then(|((c, _), _)| c.animations.first())
            .map_or(1, |a| c2_core::model::car_anim_duration_ms(a).max(1) as i32)
    }

    /// Poses and draws every character `seen` takes in, and the ship.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        herd: &Herd,
        ship: &Ship,
        seen: Seen<'_>,
    ) {
        self.models.truncate(herd.chars.len());
        for (i, c) in herd.chars.iter().enumerate() {
            let stale = self
                .models
                .get(i)
                .is_none_or(|m| m.as_ref().is_none_or(|(ct, _)| *ct != c.ctype));
            if stale {
                let m = self
                    .cars
                    .get(c.ctype)
                    .and_then(Option::as_ref)
                    .map(|(_, p)| (c.ctype, GpuModel::new(gpu, p, Vec::new())));
                if i < self.models.len() {
                    self.models[i] = m;
                } else {
                    self.models.push(m);
                }
            }
        }
        let near = |p: [f32; 3], reach: f32| seen.sees(p, reach);
        let mut insts: Vec<[Instance; 1]> = Vec::new();
        let mut models: Vec<&GpuModel> = Vec::new();
        for (i, c) in herd.chars.iter().enumerate() {
            let (Some(Some((_, m))), Some(Some((car, _)))) =
                (self.models.get(i), self.cars.get(c.ctype))
            else {
                continue;
            };
            // Not posed either when out of sight: the pose only feeds the GPU.
            if !near(c.pos, self.reach.get(c.ctype).copied().unwrap_or(0.0)) {
                continue;
            }
            let anims = herd.species.get(c.ctype).map_or(&[][..], |s| s.anims());
            if let Some(v) = pose::pose_into(c, car, anims, &mut self.pose) {
                m.set_pose(gpu, v);
            }
            insts.push([instance(c)]);
            models.push(m);
        }
        // The ship: landing animation at FTime, light 240 (RenderShip).
        if let Some(((car, _), m)) = &self.ship {
            if ship.state != -1 && near(ship.pos, self.ship_reach) {
                if let Some(v) = car.animations.first().and_then(|a| {
                    c2_core::model::pose_car_into(
                        a,
                        car.mesh.vertices.len(),
                        ship.ftime.max(0) as u32,
                        &mut self.pose,
                    )
                }) {
                    m.set_pose(gpu, v);
                }
                insts.push([Instance {
                    dither: 0.0,
                    offset: ship.pos,
                    turns: ship.quarter_turns(),
                    light: 240.0 / 255.0,
                    layer: self.cars.len() as u32,
                }]);
                models.push(m);
            }
        }
        let batches: Vec<(&GpuModel, &[Instance])> = models
            .into_iter()
            .zip(&insts)
            .map(|(m, i)| (m, i.as_slice()))
            .collect();
        self.renderer.draw(gpu, pass, globals, &batches);
    }
}

/// What the camera takes in: characters within `max` of `eye` (on the
/// ground plane) that `frustum` may see.
#[derive(Clone, Copy)]
pub struct Seen<'a> {
    pub eye: [f32; 3],
    pub max: f32,
    pub frustum: &'a Frustum,
}

impl Seen<'_> {
    fn sees(&self, p: [f32; 3], reach: f32) -> bool {
        let (dx, dz) = (p[0] - self.eye[0], p[2] - self.eye[2]);
        let r = Vec3::splat(reach);
        dx * dx + dz * dz <= self.max * self.max
            && self.frustum.sees_box(Vec3::from(p) - r, Vec3::from(p) + r)
    }
}

/// A rest pose's farthest vertex from the origin, doubled plus a cell:
/// room for animations that reach further (a lunge, a raised neck).
fn model_reach(p: &PreparedModel) -> f32 {
    let rest = p
        .positions
        .iter()
        .map(|v| Vec3::from(*v).length())
        .fold(0.0, f32::max);
    rest * 2.0 + c2_core::CELL
}

fn instance(c: &Character) -> Instance {
    Instance {
        dither: 0.0,
        offset: c.pos,
        turns: pose::quarter_turns(c),
        light: CHAR_LIGHT,
        layer: c.ctype as u32,
    }
}
