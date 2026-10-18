//! wgpu device + surface setup, and an offscreen target for screenshots.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use winit::window::Window;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub color_format: wgpu::TextureFormat,
    /// `max_texture_array_layers` actually granted.
    pub max_layers: u32,
}

/// Where frames go: a window surface or an offscreen texture.
pub enum Target {
    Window {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    Offscreen {
        texture: wgpu::Texture,
    },
}

pub struct Frame {
    pub size: (u32, u32),
    pub depth: wgpu::TextureView,
}

impl Frame {
    pub fn new(gpu: &Gpu, size: (u32, u32)) -> Self {
        let depth = gpu
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Self { size, depth }
    }

    pub fn aspect(&self) -> f32 {
        self.size.0.max(1) as f32 / self.size.1.max(1) as f32
    }
}

fn instance(display: Option<winit::event_loop::OwnedDisplayHandle>) -> wgpu::Instance {
    let desc = match display {
        Some(d) => wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(d)),
        None => wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
    };
    wgpu::Instance::new(desc)
}

fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    let limits = wgpu::Limits {
        max_texture_array_layers: adapter.limits().max_texture_array_layers,
        ..Default::default()
    };
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("c2rs"),
        required_limits: limits,
        ..Default::default()
    }))
    .context("requesting GPU device")
}

impl Gpu {
    pub fn for_window(
        display: winit::event_loop::OwnedDisplayHandle,
        window: Arc<Window>,
    ) -> Result<(Self, Target)> {
        let instance = instance(Some(display));
        let surface = instance
            .create_surface(window.clone())
            .context("creating window surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .context("no suitable GPU adapter")?;
        tracing::info!("GPU: {:?}", adapter.get_info().name);
        let (device, queue) = device(&adapter)?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow!("surface not supported by adapter"))?;
        // The original lights and fogs raw 16-bit colour values, so keep the maths
        // in that space: non-sRGB surface and textures, no gamma conversion.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let max_layers = device.limits().max_texture_array_layers;
        Ok((
            Self {
                device,
                queue,
                color_format: config.format,
                max_layers,
            },
            Target::Window { surface, config },
        ))
    }

    pub fn offscreen(size: (u32, u32)) -> Result<(Self, Target)> {
        let instance = instance(None);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .context("no suitable GPU adapter")?;
        let (device, queue) = device(&adapter)?;
        let color_format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let max_layers = device.limits().max_texture_array_layers;
        Ok((
            Self {
                device,
                queue,
                color_format,
                max_layers,
            },
            Target::Offscreen { texture },
        ))
    }

    /// Reads back an offscreen texture as tightly packed RGBA8 rows.
    pub fn read_rgba(&self, texture: &wgpu::Texture) -> Result<Vec<u8>> {
        let (w, h) = (texture.width(), texture.height());
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            texture.size(),
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("waiting for GPU")?;
        rx.recv()
            .context("readback channel")?
            .context("mapping readback buffer")?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| anyhow!("mapping readback buffer: {e:?}"))?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let s = (y * row) as usize;
            out.extend_from_slice(&data[s..s + (w * 4) as usize]);
        }
        Ok(out)
    }

    /// Creates a `width x height x layers` RGBA8 array texture from tightly packed layers.
    pub fn texture_array(
        &self,
        label: &str,
        width: u32,
        height: u32,
        layers: &[&[u8]],
    ) -> wgpu::TextureView {
        let n = (layers.len() as u32).clamp(1, self.max_layers);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: n,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, data) in layers.iter().take(n as usize).enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    }

    pub fn sampler(&self, linear: bool, repeat: bool) -> wgpu::Sampler {
        let f = if linear {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        };
        let a = if repeat {
            wgpu::AddressMode::Repeat
        } else {
            wgpu::AddressMode::ClampToEdge
        };
        self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: a,
            address_mode_v: a,
            mag_filter: f,
            min_filter: f,
            ..Default::default()
        })
    }
}
