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
    /// Fingers on the screen now, in the order they came down.
    pub fingers: Vec<Finger>,
    /// Fingers lifted this frame (where they left).
    pub lifted: Vec<Finger>,
    /// The last input was a touch: the phone's on-screen buttons show. A key,
    /// a pad or the mouse wheel hides them again.
    pub touch_mode: bool,
    /// Android: the finger standing in for the mouse, and where it last was.
    #[cfg(target_os = "android")]
    touch: Option<(u64, (f32, f32))>,
}

/// A finger on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finger {
    pub id: u64,
    pub at: (f32, f32),
    /// Where it came down.
    pub start: (f32, f32),
    /// Came down this frame.
    pub new: bool,
}

impl Input {
    /// A touch event: the finger list, and (Android) the first finger as the
    /// mouse (tap = click, drag = drag, as the menus and mouse-look use it).
    pub fn touch(&mut self, id: u64, phase: winit::event::TouchPhase, at: (f32, f32)) {
        use winit::event::TouchPhase;
        self.touch_mode = true;
        match phase {
            TouchPhase::Started => {
                self.taps.push(at);
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger {
                    id,
                    at,
                    start: at,
                    new: true,
                });
            }
            TouchPhase::Moved => {
                if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
                    f.at = at;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some(i) = self.fingers.iter().position(|f| f.id == id) {
                    let mut f = self.fingers.remove(i);
                    f.at = at;
                    self.lifted.push(f);
                }
            }
        }
        #[cfg(target_os = "android")]
        self.touch_as_mouse(id, phase, at);
    }

    #[cfg(target_os = "android")]
    fn touch_as_mouse(&mut self, id: u64, phase: winit::event::TouchPhase, at: (f32, f32)) {
        use winit::event::TouchPhase;
        match (phase, self.touch) {
            (TouchPhase::Started, None) => {
                self.touch = Some((id, at));
                self.cursor = Some(at);
                self.mouse_down = true;
                self.dragging = true;
                self.clicked = true;
            }
            (TouchPhase::Moved, Some((finger, last))) if finger == id => {
                self.mouse_delta.0 += at.0 - last.0;
                self.mouse_delta.1 += at.1 - last.1;
                self.touch = Some((id, at));
                self.cursor = Some(at);
            }
            (TouchPhase::Ended | TouchPhase::Cancelled, Some((finger, _))) if finger == id => {
                self.touch = None;
                self.cursor = Some(at);
                self.mouse_down = false;
                self.dragging = false;
            }
            _ => {}
        }
    }

    pub fn held(&self, k: KeyCode) -> bool {
        self.held.contains(&k)
    }

    /// True only on the frame the key went down.
    pub fn pressed(&self, k: KeyCode) -> bool {
        self.pressed.contains(&k)
    }

    pub fn key(&mut self, k: KeyCode, down: bool) {
        // A key means keyboard play (Escape too can be a phone's Back).
        if down && k != KeyCode::Escape {
            self.touch_mode = false;
        }
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
        self.lifted.clear();
        for f in &mut self.fingers {
            f.new = false;
        }
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
        self.fingers.clear();
        #[cfg(target_os = "android")]
        {
            self.touch = None;
        }
    }
}
