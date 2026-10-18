use crate::gpu::Gpu;
use crate::input::Input;

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
}

/// Renders one frame of `scene` into `view`.
pub fn render(
    gpu: &Gpu,
    scene: &mut dyn Scene,
    view: &wgpu::TextureView,
    frame: &crate::gpu::Frame,
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
    }
    gpu.queue.submit([enc.finish()]);
}
