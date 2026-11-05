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
            stars: false,
            night_vision: 0.5,
            ambient: 1.0,
            flashlight: false,
            narrow_flashlight: false,
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
                    stars: false,
                    night_vision: self.weights[2],
                    ambient: 1.0,
                    flashlight: false,
                    narrow_flashlight: false,
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
            var point = vec3(0., 0., -2048.);
            if p.x >= 2. { point = vec3(6000., 0., -2048.); }
            if p.x >= 4. { point = vec3(700., 0., -2048.); }
            if p.x >= 6. { point = vec3(0., 0., -6500.); }
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
    let sample = |nv, ambient, flashlight, narrow_flashlight| -> anyhow::Result<[[u8; 3]; 4]> {
        globals.write(
            &gpu,
            &FrameParams {
                view: Mat4::IDENTITY,
                proj: Mat4::IDENTITY,
                eye: Vec3::ZERO,
                fog_rgb: [0.; 3],
                fog: (0., 10000.),
                daylight: [0., 0., 1.],
                stars: false,
                night_vision: nv,
                ambient,
                flashlight,
                narrow_flashlight,
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
        Ok([at(0), at(2), at(4), at(6)])
    };
    let [dark, _, _, _] = sample(0., 0.12, false, false)?;
    let [beam, outside, broad_shoulder, broad_far] = sample(0., 0.12, true, false)?;
    let [narrow_near, narrow_outside, narrow_shoulder, narrow_far] = sample(0., 0.12, true, true)?;
    assert!(narrow_near[0] >= beam[0]);
    assert_eq!(narrow_outside, dark);
    assert!(
        broad_shoulder[0] > narrow_shoulder[0] + 20,
        "narrow beam must illuminate less width"
    );
    assert_eq!(narrow_shoulder, dark);
    assert_eq!(broad_far, dark);
    assert!(
        narrow_far[0] > broad_far[0] + 20,
        "narrow beam must reach farther"
    );
    assert_eq!(sample(0., 0.12, false, true)?[0], dark);
    assert!(
        beam[0] > dark[0] * 3,
        "flashlight did not illuminate its cone"
    );
    assert_eq!(
        outside, dark,
        "flashlight must not brighten the whole scene"
    );
    let [nv, _, _, _] = sample(1., 1., false, false)?;
    assert!(
        nv[1] > 60 && nv[0] < nv[1] / 4 && nv[2] < nv[1] / 4,
        "{nv:?}"
    );
    assert_eq!(
        sample(0., 0.12, false, false)?[0],
        dark,
        "turning equipment off restores darkness"
    );
    Ok(())
}

// Black synthetic sky isolates star emission from authored sky textures.
#[test]
#[ignore = "requires an available wgpu adapter; run with --test-threads=1"]
fn dynamic_stars_fade_and_stay_fixed_when_walking() -> anyhow::Result<()> {
    use crate::scene::Scene;
    struct Stars {
        sky: SkyRenderer,
        globals: Globals,
        enabled: bool,
        night: f32,
        eye: Vec3,
    }
    impl Scene for Stars {
        fn title(&self) -> String {
            "synthetic stars".into()
        }
        fn update(&mut self, _: &Gpu, _: f32, _: &crate::input::Input) {}
        fn clear_color(&self) -> [f64; 3] {
            [0.; 3]
        }
        fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, _: (u32, u32)) {
            let view = glam::camera::rh::view::look_at_mat4(self.eye, self.eye + Vec3::Y, Vec3::Z);
            self.globals.write(
                gpu,
                &FrameParams {
                    view,
                    proj: glam::camera::rh::proj::directx::perspective(1.0, 1.0, 1.0, 100000.0),
                    eye: self.eye,
                    fog_rgb: [0.; 3],
                    fog: (0., 100000.),
                    daylight: [0., 1. - self.night, self.night],
                    stars: self.enabled,
                    night_vision: 0.,
                    ambient: 0.12,
                    flashlight: false,
                    narrow_flashlight: false,
                    time_s: 0.,
                    brightness: 1.,
                },
            );
            self.sky
                .draw(gpu, pass, &self.globals, view, [0.; 3], self.night);
        }
    }
    let (gpu, target) = Gpu::offscreen((256, 256))?;
    let crate::gpu::Target::Offscreen { texture } = target else {
        unreachable!()
    };
    let output = texture.create_view(&Default::default());
    let frame = crate::gpu::Frame::new(&gpu, (256, 256));
    let globals = Globals::new(&gpu);
    let images = core::array::from_fn(|_| [0, 0, 0, 255].repeat(256 * 256));
    let sky = SkyRenderer::new(&gpu, &globals, &images, [None, None]);
    let mut scene = Stars {
        sky,
        globals,
        enabled: true,
        night: 1.,
        eye: Vec3::ZERO,
    };
    let mut sample = |enabled, night, eye| -> anyhow::Result<Vec<u8>> {
        scene.enabled = enabled;
        scene.night = night;
        scene.eye = eye;
        crate::scene::render(&gpu, &mut scene, &output, &frame);
        gpu.read_rgba(&texture)
    };
    let energy = |pixels: &[u8]| -> u64 {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
            .sum()
    };
    let night = sample(true, 1., Vec3::ZERO)?;
    assert!(
        energy(&night) > 1000,
        "stars must remain visible with dark ambient"
    );
    assert_eq!(
        energy(&sample(false, 1., Vec3::ZERO)?),
        0,
        "classic night has no added stars"
    );
    assert_eq!(
        energy(&sample(true, 0., Vec3::ZERO)?),
        0,
        "no stars in daylight"
    );
    let dusk = energy(&sample(true, 0.4, Vec3::ZERO)?);
    assert!(
        dusk > 0 && dusk < energy(&night),
        "stars fade during twilight"
    );
    let moved = sample(true, 1., Vec3::new(8192., 256., -8192.))?;
    let difference: u64 = night
        .iter()
        .zip(&moved)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    assert!(
        difference < energy(&night) / 100,
        "stars moved with the camera: {difference}"
    );
    Ok(())
}
