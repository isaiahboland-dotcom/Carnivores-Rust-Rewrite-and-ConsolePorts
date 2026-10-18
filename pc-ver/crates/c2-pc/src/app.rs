//! winit event loop hosting one [`Scene`].

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::gpu::{Frame, Gpu, Target};
use crate::input::Input;
use crate::scene::{render, Scene};

pub type MakeScene = Box<dyn FnOnce(&Gpu) -> Result<Box<dyn Scene>>>;

struct Running {
    window: Arc<Window>,
    gpu: Gpu,
    target: Target,
    frame: Frame,
    scene: Box<dyn Scene>,
    input: Input,
    last: Instant,
}

/// Captures (or releases) the cursor; returns whether it is now captured.
fn grab(window: &Window, on: bool) -> bool {
    use winit::window::CursorGrabMode;
    if !on {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        return false;
    }
    let ok = window
        .set_cursor_grab(CursorGrabMode::Locked)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
        .is_ok();
    window.set_cursor_visible(!ok);
    ok
}

struct App {
    make: Option<MakeScene>,
    running: Option<Running>,
    error: Option<anyhow::Error>,
}

pub fn run(make: MakeScene) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        make: Some(make),
        running: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) -> Result<()> {
        let Some(make) = self.make.take() else {
            return Ok(());
        };
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("c2rs")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
            )?,
        );
        let (gpu, target) = Gpu::for_window(el.owned_display_handle(), window.clone())?;
        let size = window.inner_size();
        let frame = Frame::new(&gpu, (size.width, size.height));
        let scene = make(&gpu)?;
        window.set_title(&scene.title());
        let mut input = Input::default();
        if scene.grab_cursor() {
            input.grabbed = grab(&window, true);
        }
        self.running = Some(Running {
            window,
            gpu,
            target,
            frame,
            scene,
            input,
            last: Instant::now(),
        });
        Ok(())
    }

    fn fail(&mut self, el: &ActiveEventLoop, e: anyhow::Error) {
        self.error = Some(e);
        el.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if let Err(e) = self.start(el) {
            self.fail(el, e);
        }
    }

    fn device_event(
        &mut self,
        _el: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let (Some(r), DeviceEvent::MouseMotion { delta }) = (&mut self.running, event) {
            if r.input.dragging || r.input.grabbed {
                r.input.mouse_delta.0 += delta.0 as f32;
                r.input.mouse_delta.1 += delta.1 as f32;
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = &mut self.running else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    let down = event.state == ElementState::Pressed;
                    // Escape releases a captured cursor first, then quits.
                    if k == KeyCode::Escape && down && !event.repeat && !r.scene.handles_escape() {
                        if r.input.grabbed {
                            r.input.grabbed = grab(&r.window, false);
                        } else {
                            el.exit();
                        }
                    }
                    r.input.key(k, down);
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                let down = state == ElementState::Pressed;
                r.input.dragging = down;
                r.input.mouse_down = down;
                if down && !r.input.grabbed && r.scene.grab_cursor() {
                    r.input.grabbed = grab(&r.window, true);
                }
            }
            WindowEvent::Focused(false) if r.input.grabbed => {
                r.input.grabbed = grab(&r.window, false);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                r.input.scroll += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                }
            }
            WindowEvent::Resized(size) => {
                if let Target::Window { surface, config } = &mut r.target {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(&r.gpu.device, config);
                }
                r.frame = Frame::new(&r.gpu, (size.width, size.height));
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - r.last).as_secs_f32().min(0.25);
                r.last = now;
                r.scene.update(&r.gpu, dt, &r.input);
                r.input.end_frame();
                let Target::Window { surface, config } = &r.target else {
                    return;
                };
                let tex = match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        surface.configure(&r.gpu.device, config);
                        return;
                    }
                    _ => return,
                };
                let view = tex.texture.create_view(&Default::default());
                render(&r.gpu, r.scene.as_mut(), &view, &r.frame);
                if r.scene.wants_exit() {
                    el.exit();
                }
                r.gpu.queue.present(tex);
                r.window.set_title(&r.scene.title());
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(r) = &self.running {
            r.window.request_redraw();
        }
    }
}

/// Renders a single frame offscreen and writes it as PNG.
pub fn screenshot(size: (u32, u32), out: &std::path::Path, make: MakeScene) -> Result<()> {
    let (gpu, target) = Gpu::offscreen(size)?;
    let Target::Offscreen { texture } = &target else {
        anyhow::bail!("offscreen target expected")
    };
    let frame = Frame::new(&gpu, size);
    let mut scene = make(&gpu)?;
    scene.update(&gpu, 0.0, &Input::default());
    let view = texture.create_view(&Default::default());
    render(&gpu, scene.as_mut(), &view, &frame);
    let rgba = gpu.read_rgba(texture)?;
    std::fs::write(out, crate::png::encode_rgba(size.0, size.1, &rgba))?;
    println!("{}: {}", out.display(), scene.title());
    Ok(())
}
