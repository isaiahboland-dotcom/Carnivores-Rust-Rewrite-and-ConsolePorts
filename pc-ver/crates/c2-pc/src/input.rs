use std::collections::HashSet;

use winit::keyboard::KeyCode;

#[derive(Default)]
pub struct Input {
    held: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    /// Raw mouse motion this frame, while dragging or while the cursor is grabbed.
    pub mouse_delta: (f32, f32),
    pub scroll: f32,
    pub dragging: bool,
    /// Cursor is captured for mouse-look (hunt mode).
    pub grabbed: bool,
    /// Left / right mouse button is held.
    pub mouse_down: bool,
    pub right_down: bool,
    /// The left / right button went down this frame.
    pub clicked: bool,
    pub right_clicked: bool,
    /// Cursor position in window pixels.
    pub cursor: Option<(f32, f32)>,
    /// Touches that began this frame, in window pixels. A captured pointer
    /// gives clicks no place, so the hunt's on-screen buttons read these.
    pub taps: Vec<(f32, f32)>,
    /// Text typed this frame (name entry).
    pub typed: String,
    /// The last key that went down this frame (key binding).
    pub last_pressed: Option<KeyCode>,
    /// The gamepads this frame (`c2_pad`; the hunt reads them).
    pub pad: c2_pad::Frame,
    /// Android: the finger standing in for the mouse, and where it last was.
    #[cfg(target_os = "android")]
    touch: Option<(u64, (f32, f32))>,
}

/// Android: the first finger down is the mouse (left button held while it
/// stays down; moving it drags, as the mouse does in the menus and to look).
#[cfg(target_os = "android")]
pub fn touch_as_mouse(input: &mut Input, id: u64, phase: winit::event::TouchPhase, at: (f32, f32)) {
    use winit::event::TouchPhase;
    match (phase, input.touch) {
        (TouchPhase::Started, None) => {
            input.touch = Some((id, at));
            input.cursor = Some(at);
            input.mouse_down = true;
            input.dragging = true;
            input.clicked = true;
        }
        (TouchPhase::Moved, Some((finger, last))) if finger == id => {
            input.mouse_delta.0 += at.0 - last.0;
            input.mouse_delta.1 += at.1 - last.1;
            input.touch = Some((id, at));
            input.cursor = Some(at);
        }
        (TouchPhase::Ended | TouchPhase::Cancelled, Some((finger, _))) if finger == id => {
            input.touch = None;
            input.cursor = Some(at);
            input.mouse_down = false;
            input.dragging = false;
        }
        _ => {}
    }
}

impl Input {
    pub fn held(&self, k: KeyCode) -> bool {
        self.held.contains(&k)
    }

    /// True only on the frame the key went down.
    pub fn pressed(&self, k: KeyCode) -> bool {
        self.pressed.contains(&k)
    }

    pub fn key(&mut self, k: KeyCode, down: bool) {
        if down {
            if self.held.insert(k) {
                self.pressed.insert(k);
                self.last_pressed = Some(k);
            }
        } else {
            self.held.remove(&k);
        }
    }

    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.mouse_delta = (0.0, 0.0);
        self.scroll = 0.0;
        self.clicked = false;
        self.right_clicked = false;
        self.taps.clear();
        self.typed.clear();
        self.last_pressed = None;
    }

    /// Forget held keys (after switching scenes).
    pub fn release_all(&mut self) {
        self.held.clear();
        self.pressed.clear();
        self.mouse_down = false;
        self.right_down = false;
        self.dragging = false;
        #[cfg(target_os = "android")]
        {
            self.touch = None;
        }
    }
}
