//! The wind indicator and the compass, bottom left and bottom right of the
//! hunt view (`Hunt2.cpp`, after the binoculars): `WIND.CAR` posed by the
//! wind's speed and turned by its direction, `COMPAS.3DF` turned by where
//! the hunter looks. Hidden with the binoculars or scope up, in the trophy
//! room, during the death camera, and while Caps Lock is toggled on.

use std::path::Path;

use c2_core::game::Hunt;
use c2_core::model::{pose_car, PreparedModel};
use c2_formats::{car, three_df};
use glam::{Mat4, Vec3};

use crate::area_view::find_file;
use crate::camera;
use crate::dinos::LoadedCar;
use crate::gpu::Gpu;
use crate::render::{Globals, GpuModel, Instance, ModelRenderer};

/// `RenderNearModel(.., light = 192, ..)`.
const LIGHT: f32 = 192.0 / 255.0;

pub struct Gauges {
    renderer: ModelRenderer,
    /// Texture layer 0.
    compass: Option<(PreparedModel, GpuModel)>,
    /// Texture layer 1.
    wind: Option<(LoadedCar, GpuModel)>,
}

impl Gauges {
    /// `None` when the game has neither model.
    pub fn load(gpu: &Gpu, globals: &Globals, huntdat: &Path) -> Option<Self> {
        let compass = find_file(huntdat, "COMPAS.3DF").and_then(|p| {
            let m = std::fs::read(&p)
                .map_err(anyhow::Error::from)
                .and_then(|b| Ok(three_df::parse(&b)?));
            match m {
                Ok(m) => Some(PreparedModel::from_mesh(&m.mesh)),
                Err(e) => {
                    tracing::warn!("{}: {e:#}", p.display());
                    None
                }
            }
        });
        let wind = find_file(huntdat, "WIND.CAR").and_then(|p| {
            let c = std::fs::read(&p)
                .map_err(anyhow::Error::from)
                .and_then(|b| Ok(car::parse(&b)?));
            match c {
                Ok(c) => {
                    let prepared = PreparedModel::from_mesh(&c.mesh);
                    Some((c, prepared))
                }
                Err(e) => {
                    tracing::warn!("{}: {e:#}", p.display());
                    None
                }
            }
        });
        if compass.is_none() && wind.is_none() {
            return None;
        }
        let blank = vec![0u8; 256 * 256 * 4];
        let textures: [&[u8]; 2] = [
            compass
                .as_ref()
                .map_or(blank.as_slice(), |p| p.texture_rgba.as_slice()),
            wind.as_ref()
                .map_or(blank.as_slice(), |(_, p)| p.texture_rgba.as_slice()),
        ];
        let renderer = ModelRenderer::near(gpu, globals, &textures);
        Some(Self {
            renderer,
            compass: compass.map(|p| {
                let m = GpuModel::new(gpu, &p, Vec::new());
                (p, m)
            }),
            wind: wind.map(|(c, p)| {
                let m = GpuModel::new(gpu, &p, Vec::new());
                ((c, p), m)
            }),
        })
    }

    /// Whether the original would draw them this frame (`hidden`: Caps Lock
    /// toggled on).
    pub fn shown(hunt: &Hunt, hidden: bool) -> bool {
        !hidden
            && !hunt.bin_mode
            && !hunt.weapons.optic_mode
            && !hunt.opts.trophy
            && hunt.death.is_none()
    }

    /// Draws both, in camera space moved into the world (`view`).
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        hunt: &Hunt,
        view: Mat4,
        aspect: f32,
    ) {
        let inv = view.inverse();
        let (_, camera_alpha, _) = hunt.camera();
        let mut batches: Vec<(&GpuModel, [Instance; 1])> = Vec::new();
        let Self {
            renderer,
            compass,
            wind,
        } = self;
        // VideoCX = WinW / 5, VideoCY = WinH - WinH * 10 / 23;
        // CreateMorphedModel(.., Wind.speed * 50); RenderNearModel(-10, -37, -96, al = CameraAlpha - Wind.alpha).
        if let Some(((c, _), m)) = wind.as_ref() {
            let n = c.mesh.vertices.len();
            let t = (hunt.wind.speed * 50.0).max(0.0) as u32;
            if let Some(v) = c.animations.first().and_then(|a| pose_car(a, n, t)) {
                let a = camera_alpha - hunt.wind.alpha;
                m.set_pose(gpu, &place(&v, (-10.0, -37.0, -96.0), a, 0.2, &inv, aspect));
                batches.push((m, inst(1)));
            }
        }
        // VideoCX = WinW - WinW / 5; RenderNearModel(+8, -38, -96, al = CameraAlpha).
        if let Some((p, m)) = compass.as_ref() {
            let world = place(
                &p.positions,
                (8.0, -38.0, -96.0),
                camera_alpha,
                0.8,
                &inv,
                aspect,
            );
            m.set_pose(gpu, &world);
            batches.push((m, inst(0)));
        }
        let refs: Vec<(&GpuModel, &[Instance])> =
            batches.iter().map(|(m, i)| (*m, i.as_slice())).collect();
        renderer.draw(gpu, pass, globals, &refs);
    }
}

fn inst(layer: u32) -> [Instance; 1] {
    [Instance {
        dither: 0.0,
        offset: [0.0; 3],
        turns: 0.0,
        light: LIGHT,
        layer,
    }]
}

/// The original's projection centre for these models: `cx` of the width
/// across, 13/23 of the height down.
fn centre_ndc(cx: f32) -> (f32, f32) {
    (cx * 2.0 - 1.0, 1.0 - 2.0 * 13.0 / 23.0)
}

/// `RenderNearModel`: turned by `al` about the vertical, moved to `at`, then
/// shifted so it projects about the original's moved centre (`cx`), and
/// into the world.
fn place(
    verts: &[[f32; 3]],
    at: (f32, f32, f32),
    al: f32,
    cx: f32,
    inv: &Mat4,
    aspect: f32,
) -> Vec<[f32; 3]> {
    let proj = camera::projection(aspect, 1.0e4, 1.0);
    let (sx, sy) = (proj.x_axis.x, proj.y_axis.y);
    let (dx, dy) = centre_ndc(cx);
    let (sa, ca) = al.sin_cos();
    verts
        .iter()
        .map(|v| {
            let x = v[0] * ca + v[2] * sa + at.0;
            let y = v[1] + at.1;
            let z = v[2] * ca - v[0] * sa + at.2;
            // Same depth, screen position moved by (dx, dy) in NDC.
            let x = x - dx * z / sx;
            let y = y - dy * z / sy;
            inv.transform_point3(Vec3::new(x, y, z)).to_array()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_match_the_original() {
        // 640x480: wind at x 128, compass at x 512, both at y 271.
        let (wx, wy) = centre_ndc(0.2);
        let (cx, _) = centre_ndc(0.8);
        let px = |n: f32, size: f32| (n + 1.0) / 2.0 * size;
        assert!((px(wx, 640.0) - 128.0).abs() < 1e-3);
        assert!((px(cx, 640.0) - 512.0).abs() < 1e-3);
        let y = 480.0 - px(wy, 480.0);
        assert!((y - (480.0 - 480.0 * 10.0 / 23.0)).abs() < 1e-3);
    }

    #[test]
    fn a_point_on_the_axis_projects_to_the_moved_centre() {
        let aspect = 4.0 / 3.0;
        let w = place(
            &[[0.0; 3]],
            (0.0, 0.0, -96.0),
            0.0,
            0.2,
            &Mat4::IDENTITY,
            aspect,
        );
        let p = camera::projection(aspect, 1.0e4, 1.0).project_point3(Vec3::from(w[0]));
        let (dx, dy) = centre_ndc(0.2);
        assert!((p.x - dx).abs() < 1e-4 && (p.y - dy).abs() < 1e-4, "{p}");
    }
}
