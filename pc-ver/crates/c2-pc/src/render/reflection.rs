//! Planar water reflections (PC option `modern_reflections`, not in the
//! original): the world mirrored in the water's plane, drawn into a
//! half-size target before the frame, and read back by the water's fragment
//! shader (`shaders/terrain.wgsl: fs_water`).

use super::*;
use glam::Vec4;

/// The mirror's clip plane lies this far under the water (world units), so
/// the rippled lookup finds the shore's foot instead of a gap.
pub const CLIP_BIAS: f32 = 8.0;

/// Below this height over the water the mirror is not drawn: the mirrored
/// camera would sit in the clip plane.
pub const MIN_EYE_HEIGHT: f32 = 4.0;

/// `view` looking at the world mirrored in the horizontal plane at `level`.
/// A point of that plane keeps its place on the screen.
pub fn mirrored(view: Mat4, level: f32) -> Mat4 {
    view * Mat4::from_translation(Vec3::new(0.0, 2.0 * level, 0.0))
        * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
}

/// `eye` mirrored in the plane at `level`.
pub fn mirrored_eye(eye: Vec3, level: f32) -> Vec3 {
    Vec3::new(eye.x, 2.0 * level - eye.y, eye.z)
}

/// `proj` with its near plane swung onto the horizontal plane at `level`
/// (Lengyel's oblique near plane, for a 0..1 depth range), so the mirror
/// pass draws only what stands above the water. `view` is the mirrored
/// view; its camera is below the plane. The screen positions are `proj`'s.
pub fn oblique(proj: Mat4, view: Mat4, level: f32) -> Mat4 {
    // The plane `y >= level` in view space.
    let plane = view.inverse().transpose() * Vec4::new(0.0, 1.0, 0.0, -level);
    // The frustum's corner farthest along the plane's normal.
    let corner = proj.inverse() * Vec4::new(plane.x.signum(), plane.y.signum(), 1.0, 1.0);
    let along = plane.dot(corner);
    if along.abs() < 1e-9 {
        return proj;
    }
    let mut rows = proj.transpose();
    rows.z_axis = plane * (rows.w_axis.dot(corner) / along);
    rows.transpose()
}

/// The mirror pass's target and the group the water's shader reads it by.
pub struct Reflection {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    group: wgpu::BindGroup,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    size: (u32, u32),
}

impl Reflection {
    /// A one-pixel target: the water's pipeline needs the group whether or
    /// not the option is on.
    pub fn new(gpu: &Gpu) -> Self {
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("reflection"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let sampler = gpu.sampler(true, false);
        let size = (1, 1);
        let (color, depth) = Self::views(gpu, size);
        let group = Self::bind(gpu, &layout, &color, &sampler);
        Self {
            layout,
            sampler,
            group,
            color,
            depth,
            size,
        }
    }

    fn views(gpu: &Gpu, size: (u32, u32)) -> (wgpu::TextureView, wgpu::TextureView) {
        let texture = |label, format, usage| {
            gpu.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        (
            texture(
                "reflection",
                gpu.color_format,
                attachment | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            texture("reflection depth", DEPTH_FORMAT, attachment),
        )
    }

    fn bind(
        gpu: &Gpu,
        layout: &wgpu::BindGroupLayout,
        color: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reflection"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub(super) fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub(super) fn group(&self) -> &wgpu::BindGroup {
        &self.group
    }

    /// The colour and depth views to draw the mirror into, half the size of
    /// `screen` (the mirror is rippled and seen at a slant: full size would
    /// cost four times the fill for no visible gain).
    pub fn target(
        &mut self,
        gpu: &Gpu,
        screen: (u32, u32),
    ) -> (wgpu::TextureView, wgpu::TextureView) {
        let size = ((screen.0 / 2).max(1), (screen.1 / 2).max(1));
        if size != self.size {
            (self.color, self.depth) = Self::views(gpu, size);
            self.group = Self::bind(gpu, &self.layout, &self.color, &self.sampler);
            self.size = size;
        }
        (self.color.clone(), self.depth.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL: f32 = 1000.0;

    fn camera() -> (Mat4, Mat4, Vec3) {
        let eye = Vec3::new(300.0, LEVEL + 250.0, -700.0);
        let forward = Vec3::new(0.3, -0.2, -1.0).normalize();
        let view = glam::camera::rh::view::look_to_mat4(eye, forward, Vec3::Y);
        (view, crate::camera::projection(1.6, 20_000.0, 1.0), eye)
    }

    fn screen(m: Mat4, p: Vec3) -> (Vec3, f32) {
        let c = m * p.extend(1.0);
        (c.truncate() / c.w, c.w)
    }

    #[test]
    fn a_point_on_the_water_keeps_its_place_on_the_screen() {
        let (view, proj, _) = camera();
        let mirror = mirrored(view, LEVEL);
        let both = oblique(proj, mirror, LEVEL) * mirror;
        for p in [
            Vec3::new(900.0, LEVEL, -4000.0),
            Vec3::new(-200.0, LEVEL, -1500.0),
        ] {
            let (main, _) = screen(proj * view, p);
            let (seen, _) = screen(both, p);
            assert!((main.truncate() - seen.truncate()).length() < 1e-4, "{p}");
            // On the plane: on the mirror's near plane.
            assert!(seen.z.abs() < 1e-3, "{p}: depth {}", seen.z);
        }
    }

    #[test]
    fn the_mirror_shows_what_is_above_the_water_and_clips_what_is_below() {
        let (view, proj, eye) = camera();
        let mirror = mirrored(view, LEVEL);
        let both = oblique(proj, mirror, LEVEL) * mirror;
        // A treetop over the water ahead: drawn where its mirror image is.
        let top = Vec3::new(900.0, LEVEL + 400.0, -4000.0);
        let (seen, w) = screen(both, top);
        let image = Vec3::new(top.x, 2.0 * LEVEL - top.y, top.z);
        let (main, _) = screen(proj * view, image);
        assert!((main.truncate() - seen.truncate()).length() < 1e-4);
        assert!(w > 0.0 && seen.z > 0.0 && seen.z < 1.0, "depth {}", seen.z);
        // The bed under it: behind the swung near plane.
        let (bed, w) = screen(both, Vec3::new(900.0, LEVEL - 300.0, -4000.0));
        assert!(w > 0.0 && bed.z < 0.0, "depth {}", bed.z);
        assert_eq!(mirrored_eye(eye, LEVEL).y, LEVEL - 250.0);
    }

    #[test]
    fn nearer_points_have_less_depth() {
        let (view, proj, _) = camera();
        let mirror = mirrored(view, LEVEL);
        let both = oblique(proj, mirror, LEVEL) * mirror;
        // Two points on one line of sight of the mirrored camera.
        let eye = mirrored_eye(camera().2, LEVEL);
        let dir = (Vec3::new(900.0, LEVEL + 400.0, -4000.0) - eye).normalize();
        let (near, _) = screen(both, eye + dir * 3000.0);
        let (far, _) = screen(both, eye + dir * 6000.0);
        assert!(near.z > 0.0 && near.z < far.z && far.z < 1.0);
    }
}
