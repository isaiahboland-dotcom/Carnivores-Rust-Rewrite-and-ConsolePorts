//! Android's event loop, on `android-activity` directly. winit's Android
//! layer passes pad buttons as unnamed keys and turns analog sticks into
//! touches (its left stick only), and `gilrs` has no Android backend, so
//! the app reads the activity itself: touches, keys, the soft keyboard and
//! pads (both sticks, triggers, the D-pad) into the same [`Input`] the PC
//! fills.

use std::time::{Duration, Instant};

use android_activity::input::{
    Axis, InputEvent, KeyAction, KeyEvent, KeyMapChar, Keycode, MotionAction, MotionEvent,
};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use anyhow::{Context, Result};
use c2_pad::Button;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::event::TouchPhase;
use winit::keyboard::KeyCode;

use crate::gpu::{Frame, Gpu, Target};
use crate::input::Input;
use crate::pad_cursor::PadCursor;
use crate::scene::{render_with, MakeScene, Scene};

/// The activity's window, as wgpu takes a window.
struct Window(android_activity::ndk::native_window::NativeWindow);

impl Window {
    fn size(&self) -> (u32, u32) {
        (self.0.width().max(1) as u32, self.0.height().max(1) as u32)
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

/// The game once it has a GPU.
struct Running {
    gpu: Gpu,
    /// None while Android has taken the window away (paused).
    target: Option<Target>,
    frame: Frame,
    scene: Box<dyn Scene>,
    pad_cursor: PadCursor,
}

#[derive(Default)]
struct State {
    input: Input,
    pads: c2_pad::Pads,
    /// The window is (re)made or resized: make a surface for it.
    window_changed: bool,
    window_gone: bool,
    quit: bool,
    keyboard: bool,
    /// Analog triggers past half way, as ZL / ZR (some pads send only axes).
    triggers: [bool; 2],
    /// The hat (D-pad as axes): x, y.
    hat: [i8; 2],
}

pub fn run(app: AndroidApp, make: MakeScene) -> Result<()> {
    let mut make = Some(make);
    let mut running: Option<Running> = None;
    let mut st = State::default();
    let mut last = Instant::now();
    while !st.quit {
        // Block while there is nothing to draw; otherwise just drain events.
        let wait = running
            .as_ref()
            .is_some_and(|r| r.target.is_some())
            .then_some(Duration::ZERO);
        app.poll_events(wait, |ev| {
            if let PollEvent::Main(m) = ev {
                match m {
                    MainEvent::InitWindow { .. } | MainEvent::WindowResized { .. } => {
                        st.window_changed = true;
                    }
                    MainEvent::TerminateWindow { .. } => st.window_gone = true,
                    MainEvent::LostFocus | MainEvent::Pause => st.input.release_all(),
                    MainEvent::Destroy => st.quit = true,
                    _ => {}
                }
            }
        });
        read_input(&app, &mut st);
        if st.window_gone {
            st.window_gone = false;
            if let Some(r) = &mut running {
                r.target = None;
            }
        }
        if std::mem::take(&mut st.window_changed) {
            if let Some(w) = app.native_window().map(Window) {
                let size = w.size();
                match &mut running {
                    // The same window resized: the surface takes the new size.
                    Some(Running {
                        gpu,
                        target: Some(Target::Window { surface, config }),
                        frame,
                        ..
                    }) => {
                        (config.width, config.height) = size;
                        surface.configure(&gpu.device, config);
                        *frame = Frame::new(gpu, size);
                    }
                    // A new window after a pause (the old surface is gone).
                    Some(r) => {
                        r.target = Some(r.gpu.new_surface(w, size)?);
                        r.frame = Frame::new(&r.gpu, size);
                    }
                    None => {
                        let (gpu, target) = Gpu::for_native(w, size)?;
                        let make = make.take().context("the game started twice")?;
                        let scene = make(&gpu)?;
                        running = Some(Running {
                            frame: Frame::new(&gpu, size),
                            pad_cursor: PadCursor::new(&gpu),
                            target: Some(target),
                            gpu,
                            scene,
                        });
                    }
                }
                last = Instant::now();
            }
        }
        let Some(r) = &mut running else { continue };
        if r.target.is_none() {
            continue;
        }
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.25);
        last = now;
        if frame(&app, r, &mut st, dt)? {
            break;
        }
    }
    Ok(())
}

/// Updates and draws one frame; true when the game is over.
fn frame(app: &AndroidApp, r: &mut Running, st: &mut State, dt: f32) -> Result<bool> {
    // No cursor to capture, but the hunt owns the pointer as on PC (so the
    // pad's menu pointer stays out of it).
    st.input.grabbed = r.scene.grab_cursor();
    st.input.pad = st.pads.poll();
    if st.input.pad.active() {
        st.input.touch_mode = false;
    }
    r.scene.before_update(&mut st.input);
    r.pad_cursor.apply(&mut st.input, r.frame.size, dt);
    r.scene.update(&r.gpu, dt, &st.input);
    if r.scene.wants_text() != st.keyboard {
        st.keyboard = r.scene.wants_text();
        if st.keyboard {
            app.show_soft_input(true);
        } else {
            app.hide_soft_input(true);
        }
    }
    st.input.end_frame();
    r.pad_cursor.after_frame(&mut st.input);
    let Some(Target::Window { surface, config }) = &r.target else {
        return Ok(false);
    };
    let tex = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            surface.configure(&r.gpu.device, config);
            return Ok(false);
        }
        _ => return Ok(false),
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
    r.gpu.queue.present(tex);
    if r.scene.wants_exit() {
        return Ok(true);
    }
    if let Some(make) = r.scene.next() {
        r.scene = make(&r.gpu)?;
        st.input.release_all();
    }
    Ok(false)
}

fn read_input(app: &AndroidApp, st: &mut State) {
    let Ok(mut events) = app.input_events_iter() else {
        return;
    };
    while events.next(|ev| match ev {
        InputEvent::KeyEvent(k) => key(app, st, k),
        InputEvent::MotionEvent(m) => motion(st, m),
        _ => InputStatus::Unhandled,
    }) {}
}

fn key(app: &AndroidApp, st: &mut State, k: &KeyEvent) -> InputStatus {
    let down = match k.action() {
        KeyAction::Down => true,
        KeyAction::Up => false,
        _ => return InputStatus::Handled,
    };
    let code = k.key_code();
    if matches!(
        code,
        Keycode::VolumeUp | Keycode::VolumeDown | Keycode::VolumeMute
    ) {
        return InputStatus::Unhandled;
    }
    if let Some(b) = pad_button(code) {
        st.input.touch_mode = false;
        st.pads.button(b, down);
        return InputStatus::Handled;
    }
    if let Some(kc) = key_code(code) {
        if k.repeat_count() == 0 || !down {
            st.input.key(kc, down);
        }
    }
    if down {
        let ch = app
            .device_key_character_map(k.device_id())
            .ok()
            .and_then(|map| map.get(code, k.meta_state()).ok());
        if let Some(KeyMapChar::Unicode(c)) = ch {
            if !c.is_control() {
                st.input.typed.push(c);
            }
        }
    }
    InputStatus::Handled
}

fn motion(st: &mut State, m: &MotionEvent) -> InputStatus {
    if m.source().is_joystick_class() {
        let p = m.pointer_at_index(0);
        let a = |axis| p.axis_value(axis);
        // Android's y axes point down; ours point up.
        st.pads
            .sticks([a(Axis::X), -a(Axis::Y)], [a(Axis::Z), -a(Axis::Rz)]);
        for (i, (t, b)) in [
            (a(Axis::Ltrigger).max(a(Axis::Brake)), Button::L2),
            (a(Axis::Rtrigger).max(a(Axis::Gas)), Button::R2),
        ]
        .into_iter()
        .enumerate()
        {
            // Half way to press, a quarter to let go.
            let on = if st.triggers[i] { t > 0.25 } else { t > 0.5 };
            if on != st.triggers[i] {
                st.triggers[i] = on;
                st.pads.button(b, on);
            }
        }
        let hat = [a(Axis::HatX).round() as i8, a(Axis::HatY).round() as i8];
        for (i, (neg, pos)) in [
            (Button::DPadLeft, Button::DPadRight),
            (Button::DPadUp, Button::DPadDown),
        ]
        .into_iter()
        .enumerate()
        {
            if hat[i] != st.hat[i] {
                st.pads.button(neg, hat[i] < 0);
                st.pads.button(pos, hat[i] > 0);
            }
        }
        // A stick pushed, the hat or a trigger: pad play hides the buttons.
        let pushed = [Axis::X, Axis::Y, Axis::Z, Axis::Rz]
            .into_iter()
            .any(|x| a(x).abs() > 0.3);
        if pushed || st.hat != hat || st.triggers.contains(&true) {
            st.input.touch_mode = false;
        }
        st.hat = hat;
        return InputStatus::Handled;
    }
    let phase = match m.action() {
        MotionAction::Down | MotionAction::PointerDown => TouchPhase::Started,
        MotionAction::Up | MotionAction::PointerUp => TouchPhase::Ended,
        MotionAction::Move => TouchPhase::Moved,
        MotionAction::Cancel => TouchPhase::Cancelled,
        _ => return InputStatus::Unhandled,
    };
    let mut touch = |p: android_activity::input::Pointer<'_>| {
        st.input.touch(p.pointer_id() as u64, phase, (p.x(), p.y()));
    };
    match phase {
        TouchPhase::Started | TouchPhase::Ended => touch(m.pointer_at_index(m.pointer_index())),
        _ => m.pointers().for_each(touch),
    }
    InputStatus::Handled
}

/// A pad's button (by position: Android's A is the bottom one).
fn pad_button(k: Keycode) -> Option<Button> {
    Some(match k {
        Keycode::ButtonA => Button::South,
        Keycode::ButtonB => Button::East,
        Keycode::ButtonX => Button::West,
        Keycode::ButtonY => Button::North,
        Keycode::ButtonL1 => Button::L1,
        Keycode::ButtonR1 => Button::R1,
        Keycode::ButtonL2 => Button::L2,
        Keycode::ButtonR2 => Button::R2,
        Keycode::ButtonSelect => Button::Select,
        Keycode::ButtonStart => Button::Start,
        Keycode::ButtonThumbl => Button::LeftThumb,
        Keycode::ButtonThumbr => Button::RightThumb,
        Keycode::DpadUp => Button::DPadUp,
        Keycode::DpadDown => Button::DPadDown,
        Keycode::DpadLeft => Button::DPadLeft,
        Keycode::DpadRight => Button::DPadRight,
        _ => return None,
    })
}

/// A keyboard key (the soft keyboard's, Back) as the PC's key.
fn key_code(k: Keycode) -> Option<KeyCode> {
    use KeyCode as K;
    const LETTERS: [KeyCode; 26] = [
        K::KeyA,
        K::KeyB,
        K::KeyC,
        K::KeyD,
        K::KeyE,
        K::KeyF,
        K::KeyG,
        K::KeyH,
        K::KeyI,
        K::KeyJ,
        K::KeyK,
        K::KeyL,
        K::KeyM,
        K::KeyN,
        K::KeyO,
        K::KeyP,
        K::KeyQ,
        K::KeyR,
        K::KeyS,
        K::KeyT,
        K::KeyU,
        K::KeyV,
        K::KeyW,
        K::KeyX,
        K::KeyY,
        K::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        K::Digit0,
        K::Digit1,
        K::Digit2,
        K::Digit3,
        K::Digit4,
        K::Digit5,
        K::Digit6,
        K::Digit7,
        K::Digit8,
        K::Digit9,
    ];
    let n = u32::from(k);
    let (a, zero) = (u32::from(Keycode::A), u32::from(Keycode::Keycode0));
    if let Some(&l) = n.checked_sub(a).and_then(|i| LETTERS.get(i as usize)) {
        return Some(l);
    }
    if let Some(&d) = n.checked_sub(zero).and_then(|i| DIGITS.get(i as usize)) {
        return Some(d);
    }
    Some(match k {
        Keycode::Back | Keycode::Escape => K::Escape,
        Keycode::Enter | Keycode::NumpadEnter => K::Enter,
        Keycode::Del => K::Backspace,
        Keycode::ForwardDel => K::Delete,
        Keycode::Space => K::Space,
        Keycode::Tab => K::Tab,
        Keycode::ShiftLeft => K::ShiftLeft,
        Keycode::ShiftRight => K::ShiftRight,
        _ => return None,
    })
}
