use crate::gpu::Gpu;
use crate::input::Input;

/// Builds a scene once the GPU exists.
pub type MakeScene = Box<dyn FnOnce(&Gpu) -> anyhow::Result<Box<dyn Scene>>>;

pub trait Scene {
    fn title(&self) -> String;
    fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input);
    fn clear_color(&self) -> [f64; 3];
    /// Records the frame. Called once per frame.
    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32));
    /// Capture and hide the cursor for mouse-look.
    fn grab_cursor(&self) -> bool {
        false
    }
    /// The scene uses Escape itself (otherwise it releases the cursor / quits).
    fn handles_escape(&self) -> bool {
        false
    }
    /// The scene is done; close the window.
    fn wants_exit(&self) -> bool {
        false
    }
    /// Switch to another scene (checked after each frame is shown).
    fn next(&mut self) -> Option<MakeScene> {
        None
    }
}

/// Renders one frame of `scene` into `view`.
pub fn render(
    gpu: &Gpu,
    scene: &mut dyn Scene,
    view: &wgpu::TextureView,
    frame: &crate::gpu::Frame,
) {
    render_with(gpu, scene, view, frame, &mut |_, _, _| {});
}

/// Drawn over a scene in its pass (`render_with`).
pub type Overlay<'a> = dyn FnMut(&Gpu, &mut wgpu::RenderPass<'_>, (u32, u32)) + 'a;

/// [`render`], then `overlay` on top in the same pass (the gamepad pointer).
pub fn render_with(
    gpu: &Gpu,
    scene: &mut dyn Scene,
    view: &wgpu::TextureView,
    frame: &crate::gpu::Frame,
    overlay: &mut Overlay<'_>,
) {
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    {
        let [r, g, b] = scene.clear_color();
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &frame.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        scene.draw(gpu, &mut pass, frame.size);
        overlay(gpu, &mut pass, frame.size);
    }
    gpu.queue.submit([enc.finish()]);
}
