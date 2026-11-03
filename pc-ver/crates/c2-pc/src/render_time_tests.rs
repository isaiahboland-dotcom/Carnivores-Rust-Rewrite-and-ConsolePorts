use super::*;

#[test]
#[ignore = "requires an available wgpu adapter"]
fn time_blending_pipelines_validate() -> anyhow::Result<()> {
    let (gpu, _) = Gpu::offscreen((8, 8))?;
    let errors = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let globals = Globals::new(&gpu);
    let area = c2_core::area::Area::flat(0);
    let _terrain = TerrainRenderer::new(&gpu, &globals, &area);
    let images = core::array::from_fn(|_| vec![128; 256 * 256 * 4]);
    let _sky = SkyRenderer::new(&gpu, &globals, &images, [None, None]);
    let _models = ModelRenderer::with_ground(&gpu, &globals, &[&images[0]], &[[64, 128, 192]]);
    globals.write(
        &gpu,
        &FrameParams {
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
            eye: Vec3::ZERO,
            fog_rgb: [0.0; 3],
            fog: (0.0, 100.0),
            daylight: [0.25, 0.25, 0.5],
            night_vision: 0.5,
            ambient: 1.0,
            flashlight: false,
            time_s: 0.0,
            brightness: 1.0,
        },
    );
    if let Some(error) = pollster::block_on(errors.pop()) {
        anyhow::bail!("{error}");
    }
    Ok(())
}

// Read actual pixels: catches wrong texture-layer order and a binary night
// switch even when all shaders and bindings validate successfully.
#[test]
#[ignore = "requires an available wgpu adapter"]
fn sky_pixels_blend_at_midpoint_and_return_to_day() -> anyhow::Result<()> {
    use crate::scene::Scene;
    struct SkyScene {
        sky: SkyRenderer,
        globals: Globals,
        weights: [f32; 3],
    }
    impl Scene for SkyScene {
        fn title(&self) -> String {
            "synthetic sky".into()
        }
        fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
        fn clear_color(&self) -> [f64; 3] {
            [0.0; 3]
        }
        fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
            let view = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::Y, Vec3::Z);
            self.globals.write(
                gpu,
                &FrameParams {
                    view,
                    proj: glam::camera::rh::proj::directx::perspective(1.0, 1.0, 1.0, 100000.0),
                    eye: Vec3::ZERO,
                    fog_rgb: [0.0; 3],
                    fog: (0.0, 100000.0),
                    daylight: self.weights,
                    night_vision: self.weights[2],
                    ambient: 1.0,
                    flashlight: false,
                    time_s: 0.0,
                    brightness: 1.0,
                },
            );
            self.sky
                .draw(gpu, pass, &self.globals, view, [0.0; 3], self.weights[2]);
        }
    }
    let (gpu, target) = Gpu::offscreen((8, 8))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let view = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (8, 8));
    let globals = Globals::new(&gpu);
    let colors = [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]];
    let images = colors.map(|c| c.repeat(256 * 256));
    let sky = SkyRenderer::new(&gpu, &globals, &images, [None, None]);
    let mut scene = SkyScene {
        sky,
        globals,
        weights: [0.0, 1.0, 0.0],
    };
    let mut sample = |weights| -> anyhow::Result<[u8; 3]> {
        scene.weights = weights;
        crate::scene::render(&gpu, &mut scene, &view, &frame);
        let pixels = gpu.read_rgba(&texture)?;
        let i = (4 * 8 + 4) * 4;
        Ok([pixels[i], pixels[i + 1], pixels[i + 2]])
    };
    let day = sample([0.0, 1.0, 0.0])?;
    let dawn = sample([1.0, 0.0, 0.0])?;
    let middle = sample([0.5, 0.5, 0.0])?;
    let night = sample([0.0, 0.0, 1.0])?;
    assert!(day[2] > 240 && day[0] < 5, "{day:?}");
    assert!(dawn[0] > 240 && dawn[2] < 5, "{dawn:?}");
    assert!(
        middle[0] > 100 && middle[0] < day[2] && middle[2] > 100,
        "{middle:?}"
    );
    assert!(
        night[1] > 240 && night[0] < 40 && night[2] < 40,
        "{night:?}"
    );
    assert_eq!(sample([0.0, 1.0, 0.0])?, day);
    Ok(())
}

#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn flashlight_and_manual_night_vision_change_actual_pixels() -> anyhow::Result<()> {
    let (gpu, target) = Gpu::offscreen((8, 8))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (8, 8));
    let globals = Globals::new(&gpu);
    let module = shader(
        &gpu,
        "night equipment test",
        r#"
        @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
            var p = array<vec2<f32>, 6>(vec2(-1.,-1.), vec2(1.,-1.), vec2(1.,1.), vec2(-1.,-1.), vec2(1.,1.), vec2(-1.,1.));
            return vec4(p[i], 0.5, 1.);
        }
        @fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
            let point = select(vec3(0., 0., -2048.), vec3(6000., 0., -2048.), p.x > 4.);
            return vec4(surface_light(vec3(0.8, 0.6, 0.4), 0.7, point), 1.);
        }
    "#,
    );
    let layout = pipeline_layout(&gpu, "night equipment test", &[&globals.layout]);
    let pipe = pipeline(
        &gpu,
        PipelineDesc {
            label: "night equipment test",
            module: &module,
            layout: &layout,
            vs: "vs",
            fs: "fs",
            buffers: &[],
            blend: Blend::Opaque,
        },
    );
    let sample = |nv, ambient, flashlight| -> anyhow::Result<([u8; 3], [u8; 3])> {
        globals.write(
            &gpu,
            &FrameParams {
                view: Mat4::IDENTITY,
                proj: Mat4::IDENTITY,
                eye: Vec3::ZERO,
                fog_rgb: [0.; 3],
                fog: (0., 10000.),
                daylight: [0., 0., 1.],
                night_vision: nv,
                ambient,
                flashlight,
                time_s: 0.,
                brightness: 1.,
            },
        );
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &frame.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, &globals.group, &[]);
            pass.draw(0..6, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        let rgba = gpu.read_rgba(&texture)?;
        let at = |x: usize| {
            let i = (4 * 8 + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        Ok((at(2), at(6)))
    };
    let (dark, _) = sample(0., 0.12, false)?;
    let (beam, outside) = sample(0., 0.12, true)?;
    assert!(
        beam[0] > dark[0] * 3,
        "flashlight did not illuminate its cone"
    );
    assert_eq!(
        outside, dark,
        "flashlight must not brighten the whole scene"
    );
    let (nv, _) = sample(1., 1., false)?;
    assert!(
        nv[1] > 60 && nv[0] < nv[1] / 4 && nv[2] < nv[1] / 4,
        "{nv:?}"
    );
    assert_eq!(
        sample(0., 0.12, false)?.0,
        dark,
        "turning equipment off restores darkness"
    );
    Ok(())
}
