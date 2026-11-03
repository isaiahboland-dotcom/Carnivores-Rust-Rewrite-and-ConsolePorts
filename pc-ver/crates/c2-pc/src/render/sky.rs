//! Sky and celestial billboards, with authored time-of-day blending.
use super::*;

// ---------------------------------------------------------------- sky

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SunVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    layer: f32,
    alpha: f32,
}

const SUN_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32, 3 => Float32];

/// Cloud plane plus sun/moon billboard.
pub struct SkyRenderer {
    sky: wgpu::RenderPipeline,
    sun: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    /// Sun model triangles in model space.
    sun_tris: Vec<SunVertex>,
    sun_buf: wgpu::Buffer,
}

impl SkyRenderer {
    /// Three 256x256 skies in dawn/day/night order, plus optional SUN2/MOON models.
    pub fn new(
        gpu: &Gpu,
        globals: &Globals,
        skies: &[Vec<u8>; 3],
        bodies: [Option<&PreparedModel>; 2],
    ) -> Self {
        let blank = vec![0u8; 256 * 256 * 4];
        let body_tex = bodies.map(|b| b.map_or(blank.as_slice(), |m| m.texture_rgba.as_slice()));
        let view = gpu.texture_array(
            "sky",
            256,
            256,
            &[&skies[0], &skies[1], &skies[2], body_tex[0], body_tex[1]],
        );
        let (tex_layout, group) = texture_group(gpu, "sky tex", &view, &gpu.sampler(true, true));
        let layout = pipeline_layout(gpu, "sky", &[&globals.layout, &tex_layout]);
        let module = shader(gpu, "sky", include_str!("../shaders/sky.wgsl"));
        let sky = pipeline(
            gpu,
            PipelineDesc {
                label: "sky",
                module: &module,
                layout: &layout,
                vs: "vs_sky",
                fs: "fs_sky",
                buffers: &[],
                blend: Blend::Background,
            },
        );
        let sun_layout = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SunVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SUN_ATTRS,
        })];
        let sun_pipe = pipeline(
            gpu,
            PipelineDesc {
                label: "sun",
                module: &module,
                layout: &layout,
                vs: "vs_sun",
                fs: "fs_sun",
                buffers: &sun_layout,
                blend: Blend::Additive,
            },
        );
        let sun_tris: Vec<SunVertex> = bodies
            .iter()
            .enumerate()
            .flat_map(|(i, model)| {
                model.iter().flat_map(move |m| {
                    m.faces.iter().flat_map(move |f| {
                        (0..3).map(move |k| SunVertex {
                            pos: m.positions[f.v[k] as usize],
                            uv: f.uv[k],
                            layer: (3 + i) as f32,
                            alpha: 1.0,
                        })
                    })
                })
            })
            .collect();
        let sun_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun"),
            size: (sun_tris.len().max(1) * std::mem::size_of::<SunVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            sky,
            sun: sun_pipe,
            group,
            sun_tris,
            sun_buf,
        }
    }

    /// `sun_pos`: `Sun3dPos` (offset from the camera, world axes).
    pub fn draw(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        globals: &Globals,
        view: Mat4,
        sun_pos: [f32; 3],
        night: f32,
    ) {
        pass.set_bind_group(0, &globals.group, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..6, 0..1);

        let verts: Vec<SunVertex> = self
            .sun_tris
            .iter()
            .map(|v| {
                let moon = v.layer == 4.0;
                let pos = if moon {
                    c2_core::area::TimeOfDay::Night.sun_pos()
                } else {
                    sun_pos
                };
                let nv = view.transform_vector3(Vec3::from(pos));
                let d = if moon {
                    1.5
                } else {
                    (2048.0 + (nv.x * nv.x + nv.y * nv.y).sqrt().min(812.0)) / 3048.0
                };
                SunVertex {
                    pos: (Vec3::from(v.pos) + nv * d).to_array(),
                    alpha: if nv.z >= -2024.0 {
                        0.0
                    } else if moon {
                        night
                    } else {
                        1.0 - night
                    },
                    ..*v
                }
            })
            .collect();
        if verts.is_empty() {
            return;
        }
        gpu.queue
            .write_buffer(&self.sun_buf, 0, bytemuck::cast_slice(&verts));
        pass.set_pipeline(&self.sun);
        pass.set_vertex_buffer(0, self.sun_buf.slice(..));
        pass.draw(0..verts.len() as u32, 0..1);
    }
}
