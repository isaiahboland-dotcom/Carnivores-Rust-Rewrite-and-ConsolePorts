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
use crate::pad_cursor::PadCursor;
use crate::scene::{render, render_with, Scene};

pub use crate::scene::MakeScene;

struct Running {
    window: Arc<Window>,
    gpu: Gpu,
    /// None while Android has taken the window away (paused).
    target: Option<Target>,
    frame: Frame,
    scene: Box<dyn Scene>,
    input: Input,
    last: Instant,
    pads: c2_pad::Pads,
    pad_cursor: PadCursor,
    /// Android: the soft keyboard is shown ([`Scene::wants_text`]).
    #[cfg(target_os = "android")]
    keyboard: bool,
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

/// The Android activity, handed over by `android_main` before [`run`].
#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<winit::platform::android::activity::AndroidApp> =
    std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_android_app(app: winit::platform::android::activity::AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

#[cfg(not(target_os = "android"))]
fn event_loop() -> Result<EventLoop<()>> {
    Ok(EventLoop::new()?)
}

#[cfg(target_os = "android")]
fn event_loop() -> Result<EventLoop<()>> {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let app = ANDROID_APP
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no Android activity (set_android_app)"))?;
    Ok(EventLoop::builder().with_android_app(app).build()?)
}

pub fn run(make: MakeScene) -> Result<()> {
    let event_loop = event_loop()?;
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
            // Android: back from a pause with a new window under the old one.
            if let Some(r) = &mut self.running {
                if r.target.is_none() {
                    r.target = Some(r.gpu.window_target(r.window.clone())?);
                    let size = r.window.inner_size();
                    r.frame = Frame::new(&r.gpu, (size.width, size.height));
                    r.last = Instant::now();
                }
            }
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
        let pad_cursor = PadCursor::new(&gpu);
        let mut input = Input::default();
        if scene.grab_cursor() {
            input.grabbed = grab(&window, true);
        }
        self.running = Some(Running {
            window,
            gpu,
            target: Some(target),
            frame,
            scene,
            input,
            last: Instant::now(),
            pad_cursor,
            #[cfg(target_os = "android")]
            keyboard: false,
            pads: {
                let pads = c2_pad::Pads::new();
                for name in pads.names() {
                    tracing::info!("gamepad: {name}");
                }
                pads
            },
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

    /// Android takes the window (and its surface) away while paused.
    fn suspended(&mut self, _el: &ActiveEventLoop) {
        if let Some(r) = &mut self.running {
            r.target = None;
            r.input.release_all();
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
                    // Android's back button is the game's Escape.
                    #[cfg(target_os = "android")]
                    let k = if k == KeyCode::BrowserBack {
                        KeyCode::Escape
                    } else {
                        k
                    };
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
                if event.state == ElementState::Pressed {
                    if let Some(t) = &event.text {
                        r.input.typed.extend(t.chars().filter(|c| !c.is_control()));
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                r.input.cursor = Some((position.x as f32, position.y as f32));
                r.pad_cursor
                    .mouse_moved(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                let down = state == ElementState::Pressed;
                r.input.right_down = down;
                r.input.right_clicked |= down;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                let down = state == ElementState::Pressed;
                r.input.dragging = down;
                r.input.mouse_down = down;
                r.input.clicked |= down;
                if down && !r.input.grabbed && r.scene.grab_cursor() {
                    r.input.grabbed = grab(&r.window, true);
                }
            }
            WindowEvent::Touch(t) => {
                let at = (t.location.x as f32, t.location.y as f32);
                if t.phase == winit::event::TouchPhase::Started {
                    r.input.taps.push(at);
                }
                // A phone has no mouse: the first finger down acts as one.
                #[cfg(target_os = "android")]
                crate::input::touch_as_mouse(&mut r.input, t.id, t.phase, at);
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
                if let Some(Target::Window { surface, config }) = &mut r.target {
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
                r.input.pad = r.pads.poll();
                r.pad_cursor.apply(&mut r.input, r.frame.size, dt);
                r.scene.update(&r.gpu, dt, &r.input);
                #[cfg(target_os = "android")]
                if r.scene.wants_text() != r.keyboard {
                    r.keyboard = r.scene.wants_text();
                    r.window.set_ime_allowed(r.keyboard);
                }
                r.input.end_frame();
                r.pad_cursor.after_frame(&mut r.input);
                let Some(Target::Window { surface, config }) = &r.target else {
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
                let pointer = &mut r.pad_cursor;
                render_with(
                    &r.gpu,
                    r.scene.as_mut(),
                    &view,
                    &r.frame,
                    &mut |gpu, pass, size| pointer.draw(gpu, pass, size),
                );
                if r.scene.wants_exit() {
                    el.exit();
                }
                r.gpu.queue.present(tex);
                if let Some(make) = r.scene.next() {
                    match make(&r.gpu) {
                        Ok(scene) => {
                            r.scene = scene;
                            r.input.release_all();
                            r.input.grabbed = grab(&r.window, r.scene.grab_cursor());
                            r.last = Instant::now();
                        }
                        Err(e) => {
                            self.error = Some(e);
                            el.exit();
                            return;
                        }
                    }
                }
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
    // Follow scene changes (menu -> loading -> hunt).
    for _ in 0..4 {
        let Some(next) = scene.next() else { break };
        scene = next(&gpu)?;
        scene.update(&gpu, 0.0, &Input::default());
        render(&gpu, scene.as_mut(), &view, &frame);
    }
    let rgba = gpu.read_rgba(texture)?;
    std::fs::write(out, crate::png::encode_rgba(size.0, size.1, &rgba))?;
    println!("{}: {}", out.display(), scene.title());
    Ok(())
}
