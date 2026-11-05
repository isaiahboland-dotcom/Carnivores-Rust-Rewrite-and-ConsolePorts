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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::FrameParams;
    use crate::scene::Scene;
    use glam::{Mat4, Vec3};

    #[test]
    #[ignore = "requires an available wgpu adapter"]
    fn blood_spray_and_trails_produce_red_pixels() -> anyhow::Result<()> {
        struct BloodScene {
            renderer: EffectsView,
            globals: Globals,
            fx: Effects,
        }
        impl Scene for BloodScene {
            fn title(&self) -> String {
                "synthetic blood".into()
            }
            fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
            fn clear_color(&self) -> [f64; 3] {
                [1.0; 3]
            }
            fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
                let eye = Vec3::new(1000.0, 150.0, 1250.0);
                let view: Mat4 = glam::camera::rh::view::look_at_mat4(
                    eye,
                    Vec3::new(1000.0, 50.0, 1000.0),
                    Vec3::Y,
                );
                self.globals.write(
                    gpu,
                    &FrameParams {
                        view,
                        proj: glam::camera::rh::proj::directx::perspective(1.0, 1.0, 1.0, 10000.0),
                        eye,
                        fog_rgb: [1.0; 3],
                        fog: (5000.0, 10000.0),
                        daylight: [0.0, 1.0, 0.0],
                        stars: false,
                        night_vision: 0.0,
                        ambient: 1.0,
                        flashlight: false,
                        narrow_flashlight: false,
                        time_s: 0.0,
                        brightness: 1.0,
                    },
                );
                self.renderer.draw(gpu, pass, &self.globals, &self.fx);
            }
        }
        let (gpu, target) = Gpu::offscreen((128, 128))?;
        let crate::gpu::Target::Offscreen { texture } = target else {
            unreachable!()
        };
        let output = texture.create_view(&Default::default());
        let frame = crate::gpu::Frame::new(&gpu, (128, 128));
        let globals = Globals::new(&gpu);
        let renderer = EffectsView::new(&gpu, &globals, Path::new("/nonexistent-synthetic-assets"));
        let mut scene = BloodScene {
            renderer,
            globals,
            fx: Effects::default(),
        };
        let area = c2_core::area::Area::flat(0);
        let mut rng = c2_core::rng::MsRand::new(1234);
        for trail in [false, true] {
            scene.fx = Effects::default();
            if trail {
                scene.fx.blood_trail(&area, &mut rng, [1000.0, 0.0, 1000.0]);
            } else {
                scene.fx.add(
                    &area,
                    &mut rng,
                    [1000.0, 80.0, 1000.0],
                    c2_core::effects::Part::Blood,
                    16,
                    0.0,
                );
                scene.fx.step(&area, 80, 1);
            }
            crate::scene::render(&gpu, &mut scene, &output, &frame);
            let pixels = gpu.read_rgba(&texture)?;
            let red = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| {
                    i32::from(p[0]) > i32::from(p[1]) + 20 && i32::from(p[0]) > i32::from(p[2]) + 20
                })
                .count();
            assert!(red > 5, "trail={trail}: only {red} red pixels");
        }
        Ok(())
    }
}
