//! Drawing particles, blood trails and water rings (`RenderElements`,
//! `RenderWCircles`).

use std::path::Path;

use c2_core::effects::Effects;
use c2_core::model::{pose_car, PreparedModel};
use c2_formats::car::{self, Car};

use crate::area_view::find_file;
use crate::gpu::Gpu;
use crate::render::{Disc, DiscRenderer, Globals, GpuModel, Instance, ModelRenderer};

/// World radius per unit of the original's screen-space circle radius
/// (`r * CameraW * 0.64 / z` pixels).
const RADIUS_K: f32 = 0.64;

pub struct EffectsView {
    discs: DiscRenderer,
    ring: Option<(Car, PreparedModel, ModelRenderer)>,
    ring_models: Vec<GpuModel>,
}

fn rgba(c: [u8; 4]) -> [f32; 4] {
    [
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
        f32::from(c[3]) / 255.0,
        f32::from(c[0]) / 255.0,
    ]
}

impl EffectsView {
    pub fn new(gpu: &Gpu, globals: &Globals, huntdat: &Path) -> Self {
        let ring = find_file(huntdat, "WCIRCLE2.CAR")
            .and_then(|p| car::parse(&std::fs::read(p).ok()?).ok())
            .map(|c| {
                let p = PreparedModel::from_mesh(&c.mesh);
                let r = ModelRenderer::additive(gpu, globals, &[&p.texture_rgba]);
                (c, p, r)
            });
        Self {
            discs: DiscRenderer::new(gpu, globals),
            ring,
            ring_models: Vec::new(),
        }
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        fx: &Effects,
    ) {
        let mut discs = Vec::new();
        for b in &fx.bursts {
            let (c0, c1) = (rgba(b.rgba), rgba(b.rgba2));
            discs.extend(b.parts.iter().map(|p| Disc {
                center: p.pos,
                radius: p.r * RADIUS_K,
                c0,
                c1,
            }));
        }
        for d in &fx.blood {
            let a1 = (0xE0 * d.life / 20000).min(0xE0) as f32 / 255.0;
            let a2 = (0x20 * d.life / 20000).min(0x20) as f32 / 255.0;
            let (r1, r2) = (f32::from(0x70u8) / 255.0, f32::from(0x30u8) / 255.0);
            discs.push(Disc {
                center: d.pos,
                radius: 12.0 * RADIUS_K,
                c0: [r1, 0.0, 0.0, a1],
                c1: [r2, 0.0, 0.0, a2],
            });
        }
        // Snow: soft dots, fading while they lie.
        if let Some(s) = &fx.snow {
            let [r, g, b, _] = s.kind.rgba.map(|c| f32::from(c) / 255.0);
            for f in &s.flakes {
                let a = s.alpha(f);
                discs.push(Disc {
                    center: f.pos,
                    radius: 8.0 * s.kind.rad * RADIUS_K,
                    c0: [r, g, b, a],
                    c1: [r / 2.0, g / 2.0, b / 2.0, a / 7.0],
                });
            }
        }
        self.discs.draw(gpu, pass, globals, &discs);

        let Some((car, prepared, renderer)) = &mut self.ring else {
            return;
        };
        let Some(anim) = car.animations.first() else {
            return;
        };
        while self.ring_models.len() < fx.rings.len() {
            self.ring_models
                .push(GpuModel::new(gpu, prepared, Vec::new()));
        }
        let mut insts = Vec::new();
        for (r, m) in fx.rings.iter().zip(&self.ring_models) {
            let Some(v) = pose_car(anim, car.mesh.vertices.len(), r.ftime.max(0) as u32) else {
                continue;
            };
            let scaled: Vec<[f32; 3]> = v
                .iter()
                .map(|p| [p[0] * r.scale, p[1] * r.scale, p[2] * r.scale])
                .collect();
            m.set_pose(gpu, &scaled);
            // GlassL = 255 - (2000 - FTime) / 38: faint, fading.
            let alpha = ((2000 - r.ftime) as f32 / 38.0).clamp(0.0, 255.0) / 255.0;
            insts.push([Instance {
                offset: r.pos,
                turns: 0.0,
                light: 250.0 / 255.0 * alpha,
                layer: 0,
            }]);
        }
        let batches: Vec<(&GpuModel, &[Instance])> = self
            .ring_models
            .iter()
            .zip(&insts)
            .map(|(m, i)| (m, i.as_slice()))
            .collect();
        renderer.draw(gpu, pass, globals, &batches);
    }
}
