//! Loading species (`_RES.TXT` + `.CAR`) and drawing characters.

use std::path::Path;

use c2_core::dino::{pose, Character, DinoInfo, Herd, Species};
use c2_core::model::PreparedModel;
use c2_formats::car::{self, Car};
use c2_formats::res_txt::ResTxt;

use crate::area_view::find_file;
use crate::gpu::Gpu;
use crate::render::{Globals, GpuModel, Instance, ModelRenderer};

/// `RenderCharacterPost` draws characters with a fixed light of 210.
const CHAR_LIGHT: f32 = 210.0 / 255.0;

/// A character model and its render-ready form.
pub type LoadedCar = (Car, PreparedModel);

pub struct DinoAssets {
    /// Indexed by `CType`; `None` if the `.CAR` failed to load.
    pub cars: Vec<Option<LoadedCar>>,
    renderer: ModelRenderer,
    /// One GPU model per character (each has its own pose).
    models: Vec<Option<GpuModel>>,
}

/// Reads `HUNTDAT/_RES.TXT` and every character `.CAR` it names.
pub fn load_species(huntdat: &Path) -> anyhow::Result<(Vec<Species>, Vec<Option<LoadedCar>>)> {
    let res_path = find_file(huntdat, "_RES.TXT")
        .ok_or_else(|| anyhow::anyhow!("no _RES.TXT in {}", huntdat.display()))?;
    let res = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(&res_path)?))?.to_data();
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
        };
        species.push(Species::new(info, car.as_ref().unwrap_or(&empty)));
        cars.push(car.map(|c| {
            let p = PreparedModel::from_mesh(&c.mesh);
            (c, p)
        }));
    }
    Ok((species, cars))
}

impl DinoAssets {
    pub fn new(gpu: &Gpu, globals: &Globals, cars: Vec<Option<LoadedCar>>) -> Self {
        let blank = vec![0u8; 256 * 256 * 4];
        let textures: Vec<&[u8]> = cars
            .iter()
            .map(|c| {
                c.as_ref()
                    .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice())
            })
            .collect();
        let fallback: [&[u8]; 1] = [&blank];
        let renderer = ModelRenderer::new(
            gpu,
            globals,
            if textures.is_empty() {
                &fallback
            } else {
                &textures
            },
        );
        Self {
            cars,
            renderer,
            models: Vec::new(),
        }
    }

    /// Poses and draws every character within `max_dist` of `eye`.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        herd: &Herd,
        eye: [f32; 3],
        max_dist: f32,
    ) {
        while self.models.len() < herd.chars.len() {
            let ct = herd.chars[self.models.len()].ctype;
            let m = self
                .cars
                .get(ct)
                .and_then(Option::as_ref)
                .map(|(_, p)| GpuModel::new(gpu, p, Vec::new()));
            self.models.push(m);
        }
        let mut insts: Vec<[Instance; 1]> = Vec::new();
        let mut which: Vec<usize> = Vec::new();
        for (i, c) in herd.chars.iter().enumerate() {
            let (dx, dz) = (c.pos[0] - eye[0], c.pos[2] - eye[2]);
            if dx * dx + dz * dz > max_dist * max_dist {
                continue;
            }
            let (Some(Some(m)), Some(Some((car, _)))) =
                (self.models.get(i), self.cars.get(c.ctype))
            else {
                continue;
            };
            if let Some(v) = pose::pose(c, car) {
                m.set_pose(gpu, &v);
            }
            insts.push([instance(c)]);
            which.push(i);
        }
        let batches: Vec<(&GpuModel, &[Instance])> = which
            .iter()
            .zip(&insts)
            .filter_map(|(&i, inst)| self.models[i].as_ref().map(|m| (m, inst.as_slice())))
            .collect();
        self.renderer.draw(gpu, pass, globals, &batches);
    }
}

fn instance(c: &Character) -> Instance {
    Instance {
        offset: c.pos,
        turns: pose::quarter_turns(c),
        light: CHAR_LIGHT,
        layer: c.ctype as u32,
    }
}
