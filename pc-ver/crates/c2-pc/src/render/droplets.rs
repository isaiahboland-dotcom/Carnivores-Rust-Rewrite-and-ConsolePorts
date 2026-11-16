//! Water running off the screen after surfacing (PC option
//! `modern_underwater`, not in the original): `shaders/droplets.wgsl` over
//! the world, and the state that starts and ages it.

use super::*;

/// Screen drops are a proof of concept held back for a later release: the
/// state and shader stay, but nothing is drawn while this is `false`.
pub const DROPLETS_ENABLED: bool = false;

/// Seconds the drops stay on the screen.
pub const DROPLET_SECONDS: f32 = 3.5;

/// A dip shorter than this (seconds) leaves no drops: bobbing at the
/// surface crosses the game's underwater line again and again.
const MIN_DIVE_SECONDS: f32 = 0.4;

/// Whether the screen is wet: set off by coming up after a dive.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wet {
    /// Seconds under water, 0 out of it.
    dive: f32,
    /// Seconds since surfacing and that surfacing's seed, while the drops
    /// last.
    drops: Option<(f32, f32)>,
    surfacings: u32,
}

impl Wet {
    /// A frame of `dt` seconds with the hunter `underwater` or not.
    pub fn tick(&mut self, underwater: bool, dt: f32) {
        if underwater {
            self.dive += dt;
            // Back under: the water takes the drops.
            self.drops = None;
            return;
        }
        if self.dive >= MIN_DIVE_SECONDS {
            self.surfacings = self.surfacings.wrapping_add(1);
            self.drops = Some((0.0, (self.surfacings % 512) as f32 * 3.7));
        } else if let Some((age, _)) = &mut self.drops {
            *age += dt;
            if *age >= DROPLET_SECONDS {
                self.drops = None;
            }
        }
        self.dive = 0.0;
    }

    /// Wet as `age` seconds after a surfacing (screenshots).
    pub fn surfaced(age: f32) -> Self {
        Self {
            drops: Some((age, 3.7)),
            ..Self::default()
        }
    }

    /// The drops to draw: seconds since surfacing and their seed.
    pub fn drops(&self) -> Option<(f32, f32)> {
        self.drops
    }
}

pub struct DropletRenderer {
    pipeline: wgpu::RenderPipeline,
    buf: wgpu::Buffer,
    group: wgpu::BindGroup,
}

impl DropletRenderer {
    pub fn new(gpu: &Gpu) -> Self {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("droplets"),
            size: std::mem::size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("droplets"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
            });
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("droplets"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        // Its own module: it reads no frame globals.
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("droplets"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/droplets.wgsl").into()),
            });
        let pipeline = pipeline(
            gpu,
            PipelineDesc {
                label: "droplets",
                module: &module,
                layout: &pipeline_layout(gpu, "droplets", &[&layout]),
                vs: "vs",
                fs: "fs",
                buffers: &[],
                blend: Blend::Background,
            },
        );
        Self {
            pipeline,
            buf,
            group,
        }
    }

    /// The drops `age` seconds after surfacing, over what the pass holds.
    pub fn draw(
        &self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        aspect: f32,
        (age, seed): (f32, f32),
    ) {
        let data = [age, aspect, seed, DROPLET_SECONDS];
        gpu.queue
            .write_buffer(&self.buf, 0, bytemuck::cast_slice(&data));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_follow_a_dive_and_dry() {
        let mut w = Wet::default();
        w.tick(false, 0.1);
        assert_eq!(w.drops(), None);
        for _ in 0..10 {
            w.tick(true, 0.1);
        }
        assert_eq!(w.drops(), None, "no drops under water");
        w.tick(false, 0.1);
        let (age, seed) = w.drops().expect("drops on surfacing");
        assert_eq!(age, 0.0);
        w.tick(false, 1.0);
        assert_eq!(w.drops(), Some((1.0, seed)));
        w.tick(false, DROPLET_SECONDS);
        assert_eq!(w.drops(), None, "dried");
    }

    #[test]
    fn a_bob_at_the_surface_leaves_none_and_diving_again_clears_them() {
        let mut w = Wet::default();
        w.tick(true, 0.1);
        w.tick(false, 0.1);
        assert_eq!(w.drops(), None);
        for _ in 0..10 {
            w.tick(true, 0.1);
        }
        w.tick(false, 0.1);
        let first = w.drops().expect("drops").1;
        w.tick(true, 0.1);
        assert_eq!(w.drops(), None);
        for _ in 0..10 {
            w.tick(true, 0.1);
        }
        w.tick(false, 0.1);
        // Another surfacing, other drops.
        assert_ne!(w.drops().expect("drops").1, first);
    }
}
