//! First-person weapons: loading `HUNTDAT/WEAPONS/*.CAR` and bullet icons,
//! and drawing the view model (`DrawPostObjects`).

use std::path::Path;

use c2_core::game::{Hunt, WeaponInfo};
use c2_core::model::{car_anim_duration_ms, pose_car, PreparedModel};
use c2_formats::car;
use c2_formats::res_txt::WeaponDef;
use glam::{Mat4, Vec3};

use crate::area_view::find_file;
use crate::dinos::LoadedCar;
use crate::gpu::Gpu;
use crate::hud::{load_tga, Hud, Sprite};
use crate::render::{Globals, GpuModel, Instance, ModelRenderer};

pub struct WeaponAssets {
    cars: Vec<Option<LoadedCar>>,
    models: Vec<Option<GpuModel>>,
    renderer: ModelRenderer,
    /// HUD image id of each weapon's bullet icon.
    bullets: Vec<Option<usize>>,
    /// `BINOCUL.3DF`, drawn with texture layer `cars.len()`.
    binoculars: Option<(PreparedModel, GpuModel)>,
    /// World position of the view model's last vertex: the scope's aim point
    /// (`DrawOpticCross(VCount-1)`).
    pub aim_point: Option<[f32; 3]>,
}

/// `(info, [draw, fire, hide, reload] ms, assets)` for every `_RES.TXT` weapon.
pub fn load_weapons(
    gpu: &Gpu,
    globals: &Globals,
    hud: &mut Hud,
    huntdat: &Path,
    defs: &[WeaponDef],
) -> (Vec<WeaponInfo>, Vec<[i32; 4]>, WeaponAssets) {
    let dir = find_file(huntdat, "WEAPONS").unwrap_or_else(|| huntdat.join("WEAPONS"));
    let mut infos = Vec::new();
    let mut anims = Vec::new();
    let mut cars = Vec::new();
    let mut bullets = Vec::new();
    for d in defs {
        let info = WeaponInfo::from_def(d);
        let c = find_file(&dir, &info.file).and_then(|p| {
            match std::fs::read(&p)
                .map_err(anyhow::Error::from)
                .and_then(|b| Ok(car::parse(&b)?))
            {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!("{}: {e:#}", p.display());
                    None
                }
            }
        });
        let mut a = [0; 4];
        if let Some(c) = &c {
            for (slot, t) in a.iter_mut().enumerate() {
                *t = c
                    .animations
                    .get(slot)
                    .map_or(0, |an| car_anim_duration_ms(an) as i32);
            }
        }
        bullets.push(find_file(&dir, &info.pic).and_then(|p| match load_tga(&p) {
            Ok((w, h, rgba)) => Some(hud.add_image(gpu, w, h, &rgba)),
            Err(e) => {
                tracing::warn!("{}: {e:#}", p.display());
                None
            }
        }));
        anims.push(a);
        infos.push(info);
        cars.push(c.map(|c| {
            let p = PreparedModel::from_mesh(&c.mesh);
            (c, p)
        }));
    }
    let bino = find_file(huntdat, "BINOCUL.3DF").and_then(|p| {
        let m = c2_formats::three_df::parse(&std::fs::read(&p).ok()?).ok()?;
        Some(PreparedModel::from_mesh(&m.mesh))
    });
    let blank = vec![0u8; 256 * 256 * 4];
    let mut textures: Vec<&[u8]> = cars
        .iter()
        .map(|c| {
            c.as_ref()
                .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice())
        })
        .collect();
    textures.push(
        bino.as_ref()
            .map_or(blank.as_slice(), |p| p.texture_rgba.as_slice()),
    );
    let fallback: [&[u8]; 1] = [&blank];
    let renderer = ModelRenderer::near(
        gpu,
        globals,
        if textures.is_empty() {
            &fallback
        } else {
            &textures
        },
    );
    let models = cars
        .iter()
        .map(|c| c.as_ref().map(|(_, p)| GpuModel::new(gpu, p, Vec::new())))
        .collect();
    (
        infos,
        anims,
        WeaponAssets {
            cars,
            models,
            renderer,
            bullets,
            binoculars: bino.map(|p| {
                let m = GpuModel::new(gpu, &p, Vec::new());
                (p, m)
            }),
            aim_point: None,
        },
    )
}

impl WeaponAssets {
    pub fn cars(&self) -> Vec<Option<&car::Car>> {
        self.cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect()
    }

    /// The view model, posed in camera space and moved into the world.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        hunt: &Hunt,
        view: Mat4,
        light: f32,
        aspect: f32,
    ) {
        // The binocular and scope frames are drawn for 4:3; widen them to
        // cover wider screens.
        let widen = (aspect / (4.0 / 3.0)).max(1.0);
        self.aim_point = None;
        if hunt.death.is_some() {
            return;
        }
        let inv = view.inverse();
        // Binoculars (`RenderNearModel(Binocular, 0, 0, 2*(216-72*power), 192)`).
        if hunt.bin_mode {
            if let Some((p, m)) = &self.binoculars {
                let z0 = 2.0 * (216.0 - 72.0 * hunt.bin_power);
                let world: Vec<[f32; 3]> = p
                    .positions
                    .iter()
                    .map(|v| {
                        inv.transform_point3(Vec3::new(v[0] * widen, v[1], v[2] + z0))
                            .to_array()
                    })
                    .collect();
                m.set_pose(gpu, &world);
                let inst = [Instance {
                    offset: [0.0; 3],
                    turns: 0.0,
                    light: 192.0 / 255.0,
                    layer: self.cars.len() as u32,
                }];
                self.renderer.draw(gpu, pass, globals, &[(m, &inst)]);
            }
            return;
        }
        let w = &hunt.weapons;
        let Some((slot, ftime)) = w.pose() else {
            return;
        };
        let (Some(Some((car, _))), Some(Some(model))) =
            (self.cars.get(w.current), self.models.get(w.current))
        else {
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
        // RenderNearModel(.., al = -wpnDAlpha, bt = -wpnDBeta): small sway.
        let (da, db) = w.sway(hunt.real_time);
        let (sa, ca) = (-da).sin_cos();
        let (sb, cb) = (-db).sin_cos();
        let wx = if w.optic_mode { widen } else { 1.0 };
        let world: Vec<[f32; 3]> = verts
            .iter()
            .map(|v| {
                let vx = v[0] * wx;
                let x = vx * ca + v[2] * sa;
                let vz = v[2] * ca - vx * sa;
                let y = v[1] * cb - vz * sb;
                let z = vz * cb + v[1] * sb;
                inv.transform_point3(Vec3::new(x, y, z)).to_array()
            })
            .collect();
        model.set_pose(gpu, &world);
        self.aim_point = world.last().copied();
        let inst = [Instance {
            offset: [0.0; 3],
            turns: 0.0,
            light,
            layer: w.current as u32,
        }];
        self.renderer.draw(gpu, pass, globals, &[(model, &inst)]);
    }

    /// Bullet icons in the top-left corner (`DrawPostObjects`).
    pub fn hud_sprites(&self, hud: &Hud, hunt: &Hunt, scale: f32) -> Vec<Sprite> {
        let w = &hunt.weapons;
        let mut out = Vec::new();
        if w.pose().is_none() || hunt.death.is_some() {
            return out;
        }
        let Some(Some(img)) = self.bullets.get(w.current) else {
            return out;
        };
        let Some(im) = hud.image(*img) else {
            return out;
        };
        let mut y = 5.0 * scale;
        let row = |n: i32, y: f32, out: &mut Vec<Sprite>| {
            for b in 0..n.max(0) {
                out.push(Sprite::new(
                    *img,
                    (6.0 + (b as u32 * im.w) as f32) * scale,
                    y,
                    scale,
                ));
            }
        };
        if w.ammo_mag.get(w.current).copied().unwrap_or(0) > 0 {
            row(w.info[w.current].shots, y, &mut out);
            y += (im.h as f32 + 4.0) * scale;
        }
        row(w.shots_left[w.current], y, &mut out);
        out
    }
}
