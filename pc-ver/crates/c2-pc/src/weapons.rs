//! First-person weapons: loading `HUNTDAT/WEAPONS/*.CAR` and bullet icons,
//! and drawing the view model (`DrawPostObjects`).

use std::path::Path;

use c2_core::game::{Hunt, WeaponAnims, WeaponInfo};
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
    /// HUD image id of each weapon's bullet icon, and of its chamber
    /// picture (the dialect's `picc`).
    bullets: Vec<Option<usize>>,
    chambers: Vec<Option<usize>>,
    /// The dialect's round flash, `WEAPONS/FLASH1..4.TGA`.
    flashes: Vec<Option<usize>>,
    /// `BINOCUL.3DF`, drawn with texture layer `cars.len()`.
    binoculars: Option<(PreparedModel, GpuModel)>,
    /// The dialect's muzzle flash, `MUZZ4.CAR`, layer `cars.len() + 1`.
    muzzle: Option<(LoadedCar, GpuModel)>,
    /// The dialect's rounds in flight (`bModel`) by weapon, drawn in the
    /// world by their own renderer (layer: the weapon), a GPU model each.
    rounds: Vec<Option<LoadedCar>>,
    round_renderer: Option<ModelRenderer>,
    round_models: Vec<(usize, GpuModel)>,
    /// World position of the view model's last vertex: the scope's aim point
    /// (`DrawOpticCross(VCount-1)`).
    pub aim_point: Option<[f32; 3]>,
    /// Not drawn while scoped: the phone kit's scope covers the view
    /// instead (an APK-built game's sniper has no scope model).
    pub hide_scoped: bool,
}

/// The most bullet icons drawn in a row; more rounds show as one and a
/// count.
const MAX_ICONS: i32 = 40;

/// `(info, animations, assets)` for every `_RES.TXT` weapon.
pub fn load_weapons(
    gpu: &Gpu,
    globals: &Globals,
    hud: &mut Hud,
    huntdat: &Path,
    defs: &[WeaponDef],
) -> (Vec<WeaponInfo>, Vec<WeaponAnims>, WeaponAssets) {
    let dir = find_file(huntdat, "WEAPONS").unwrap_or_else(|| huntdat.join("WEAPONS"));
    let mut infos = Vec::new();
    let mut anims = Vec::new();
    let mut cars = Vec::new();
    let mut bullets = Vec::new();
    let mut chambers = Vec::new();
    let mut rounds = Vec::new();
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
        let a = c
            .as_ref()
            .map(|c| WeaponAnims::from_car(&info, c))
            .unwrap_or_default();
        bullets.push(find_file(&dir, &info.pic).and_then(|p| match load_tga(&p) {
            Ok((w, h, rgba)) => Some(hud.add_image(gpu, w, h, &rgba)),
            Err(e) => {
                tracing::warn!("{}: {e:#}", p.display());
                None
            }
        }));
        let bmodel = info.mee.as_ref().and_then(|m| m.bullet_model.clone());
        rounds.push(bmodel.and_then(|f| read_car(&dir, &f)));
        let picc = info.mee.as_ref().and_then(|m| m.chamber_pic.clone());
        chambers.push(picc.and_then(|f| image(gpu, hud, &dir, &f)));
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
    let muzzle = rounds_wanted(&infos, |m| m.muzz_flash)
        .then(|| read_car(huntdat, "MUZZ4.CAR"))
        .flatten();
    textures.push(
        muzzle
            .as_ref()
            .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice()),
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
    let round_renderer = rounds.iter().any(Option::is_some).then(|| {
        let t: Vec<&[u8]> = rounds
            .iter()
            .map(|r| {
                r.as_ref()
                    .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice())
            })
            .collect();
        ModelRenderer::new(gpu, globals, &t)
    });
    let muzzle = muzzle.map(|(c, p)| {
        let m = GpuModel::new(gpu, &p, Vec::new());
        ((c, p), m)
    });
    (
        infos,
        anims,
        WeaponAssets {
            cars,
            models,
            renderer,
            bullets,
            chambers,
            flashes: (1..=4)
                .map(|n| image(gpu, hud, &dir, &format!("FLASH{n}.TGA")))
                .collect(),
            binoculars: bino.map(|p| {
                let m = GpuModel::new(gpu, &p, Vec::new());
                (p, m)
            }),
            muzzle,
            rounds,
            round_renderer,
            round_models: Vec::new(),
            aim_point: None,
            hide_scoped: false,
        },
    )
}

/// A `.CAR` (a round's, the muzzle flash) in `dir`, if it reads.
fn read_car(dir: &Path, name: &str) -> Option<LoadedCar> {
    let p = find_file(dir, name)?;
    match std::fs::read(&p)
        .map_err(anyhow::Error::from)
        .and_then(|b| Ok(car::parse(&b)?))
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

/// Whether any weapon has a dialect key `f` asks for.
fn rounds_wanted(infos: &[WeaponInfo], f: impl Fn(&c2_core::game::MeeWeapon) -> bool) -> bool {
    infos.iter().any(|i| i.mee.as_ref().is_some_and(&f))
}

/// A HUD picture from the weapons folder, if there is one.
fn image(gpu: &Gpu, hud: &mut Hud, dir: &Path, name: &str) -> Option<usize> {
    let p = find_file(dir, name)?;
    match load_tga(&p) {
        Ok((w, h, rgba)) => Some(hud.add_image(gpu, w, h, &rgba)),
        Err(e) => {
            tracing::warn!("{}: {e:#}", p.display());
            None
        }
    }
}

impl WeaponAssets {
    /// The muzzle flash's length (`MUZZ4.CAR`'s animation), if loaded.
    pub fn muzzle_ms(&self) -> Option<i32> {
        let ((c, _), _) = self.muzzle.as_ref()?;
        c.animations
            .first()
            .map(|a| car_anim_duration_ms(a).max(1) as i32)
    }

    /// The dialect's rounds in flight and lying where they stopped, their
    /// models along their way (`RenderBulletPost`, light 210; its spin
    /// animation looping).
    pub fn draw_rounds(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        hunt: &Hunt,
    ) {
        if self.round_renderer.is_none() || hunt.bullets.is_empty() {
            return;
        }
        let mut drawn: Vec<(usize, Vec<[f32; 3]>)> = Vec::new();
        for b in &hunt.bullets {
            let Some(Some((car, _))) = self.rounds.get(b.weapon) else {
                continue;
            };
            let Some(anim) = car.animations.first() else {
                continue;
            };
            let dur = car_anim_duration_ms(anim).max(1);
            let t = (b.ftime.max(0) as u32) % dur;
            let Some(verts) = pose_car(anim, car.mesh.vertices.len(), t) else {
                continue;
            };
            // The model's front (its file's +z, our -z) along the way.
            let v = Vec3::from(b.vel);
            let f = if v.length_squared() > 1e-6 {
                v.normalize()
            } else {
                Vec3::NEG_Z
            };
            let side = f.cross(Vec3::Y);
            let right = if side.length_squared() > 1e-6 {
                side.normalize()
            } else {
                Vec3::X
            };
            let up = right.cross(f);
            let m = glam::Mat3::from_cols(right, up, -f);
            let at = Vec3::from(b.pos);
            let world = verts
                .iter()
                .map(|p| (at + m * Vec3::from(*p)).to_array())
                .collect();
            drawn.push((b.weapon, world));
        }
        // A GPU model per round drawn, made again when its slot held
        // another weapon's round.
        self.round_models.truncate(drawn.len());
        for (i, (w, _)) in drawn.iter().enumerate() {
            if self.round_models.get(i).is_some_and(|(mw, _)| mw == w) {
                continue;
            }
            let Some(Some((_, p))) = self.rounds.get(*w) else {
                continue;
            };
            let m = (*w, GpuModel::new(gpu, p, Vec::new()));
            if i < self.round_models.len() {
                self.round_models[i] = m;
            } else {
                self.round_models.push(m);
            }
        }
        let insts: Vec<[Instance; 1]> = drawn
            .iter()
            .map(|(w, _)| {
                [Instance {
                    offset: [0.0; 3],
                    turns: 0.0,
                    light: 210.0 / 255.0,
                    layer: *w as u32,
                }]
            })
            .collect();
        let mut batches = Vec::new();
        for ((_, world), ((_, m), i)) in drawn.iter().zip(self.round_models.iter().zip(&insts)) {
            m.set_pose(gpu, world);
            batches.push((m, i.as_slice()));
        }
        if let Some(renderer) = self.round_renderer.as_mut() {
            renderer.draw(gpu, pass, globals, &batches);
        }
    }

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
        if self.hide_scoped && w.optic_mode && hunt.zoom() > 1.0 {
            return;
        }
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
        let sway = |v: [f32; 3]| {
            let vx = v[0] * wx;
            let x = vx * ca + v[2] * sa;
            let vz = v[2] * ca - vx * sa;
            let y = v[1] * cb - vz * sb;
            let z = vz * cb + v[1] * sb;
            inv.transform_point3(Vec3::new(x, y, z)).to_array()
        };
        // The muzzle flash: `MUZZ4.CAR` at the fire animation's time,
        // turned by its roll (`CreateMorphedModelBetaGamma`), where the
        // weapon is.
        let flash = match (&self.muzzle, w.muzzle) {
            (Some(((mc, _), mm)), Some(gamma)) if !hunt.player.underwater => mc
                .animations
                .first()
                .and_then(|a| pose_car(a, mc.mesh.vertices.len(), ftime.max(0) as u32))
                .map(|v| {
                    let (sg, cg) = gamma.sin_cos();
                    let world: Vec<[f32; 3]> = v
                        .iter()
                        .map(|p| sway([cg * p[0] - sg * p[1], cg * p[1] + sg * p[0], p[2]]))
                        .collect();
                    mm.set_pose(gpu, &world);
                    mm
                }),
            _ => None,
        };
        let flash_inst = [Instance {
            offset: [0.0; 3],
            turns: 0.0,
            light,
            layer: self.cars.len() as u32 + 1,
        }];
        match flash {
            Some(mm) => {
                self.renderer
                    .draw(gpu, pass, globals, &[(mm, &flash_inst), (model, &inst)])
            }
            None => self.renderer.draw(gpu, pass, globals, &[(model, &inst)]),
        }
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
        // An icon per round, as the original; past [`MAX_ICONS`] (a mod's
        // "unlimited" 9999999 asked for millions) one icon and the count.
        let row = |n: i32, y: f32, out: &mut Vec<Sprite>| {
            let n = n.max(0);
            let icons = if n > MAX_ICONS { 1 } else { n };
            for b in 0..icons {
                out.push(Sprite::new(
                    *img,
                    (6.0 + (b as u32 * im.w) as f32) * scale,
                    y,
                    scale,
                ));
            }
            if n > MAX_ICONS {
                let x = (10.0 + im.w as f32) * scale;
                let px = 8.0 * scale.max(1.0).round();
                out.extend(hud.label(x, y, px, [1.0; 4], &format!("x{n}")));
            }
        };
        let cur = w.current;
        if w.info[cur].chambers() {
            self.chamber_sprites(hud, hunt, *img, scale, &mut out);
            return out;
        }
        if w.ammo_mag.get(cur).copied().unwrap_or(0) > 0 {
            row(w.info[cur].shots, y, &mut out);
            y += (im.h as f32 + 4.0) * scale;
        }
        row(w.shots_left[cur], y, &mut out);
        out
    }

    /// The dialect's rounds (`DrawPostObjects`): the chamber picture, the
    /// rounds chambered, the reserve after room for a full load, the spare
    /// magazine below, and the round flash after a shot. (The rounds'
    /// slide during a reload is not drawn.)
    fn chamber_sprites(
        &self,
        hud: &Hud,
        hunt: &Hunt,
        img: usize,
        scale: f32,
        out: &mut Vec<Sprite>,
    ) {
        let w = &hunt.weapons;
        let cur = w.current;
        let Some(im) = hud.image(img) else { return };
        let (bw, bh) = (im.w as f32, im.h as f32);
        let y0 = 5.0;
        let ch = w.info[cur].reload.max(1) as f32;
        if let Some(Some(c)) = self.chambers.get(cur) {
            if let Some(ci) = hud.image(*c) {
                let y = (y0 - 1.0) + (bh - (ci.h as f32 - 2.0));
                out.push(Sprite::new(*c, 5.0 * scale, y * scale, scale));
            }
        }
        let icons = |n: i32, x0: f32, y: f32, out: &mut Vec<Sprite>| {
            let n = n.clamp(0, MAX_ICONS);
            for b in 0..n {
                out.push(Sprite::new(
                    img,
                    (x0 + b as f32 * bw) * scale,
                    y * scale,
                    scale,
                ));
            }
        };
        let chambered = w.chambered[cur];
        icons(chambered, 6.0, y0, out);
        let left = w.shots_left[cur];
        icons(left, 9.0 + ch * bw, y0, out);
        if left > MAX_ICONS {
            let x = (9.0 + ch * bw + bw + 4.0) * scale;
            let px = 8.0 * scale.max(1.0).round();
            out.extend(hud.label(x, y0 * scale, px, [1.0; 4], &format!("x{left}")));
        }
        if w.ammo_mag[cur] > 0 {
            icons(w.mag_shots[cur], 9.0 + ch * bw, bh + 9.0, out);
        }
        if w.flash > 0 {
            if let Some(Some(f)) = self.flashes.get(usize::from(w.flash) - 1) {
                out.push(Sprite::new(
                    *f,
                    (6.0 + chambered as f32 * bw) * scale,
                    y0 * scale,
                    scale,
                ));
            }
        }
    }
}
