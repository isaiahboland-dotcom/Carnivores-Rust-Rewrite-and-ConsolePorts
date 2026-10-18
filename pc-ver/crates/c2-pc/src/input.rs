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
    /// Text typed this frame (name entry).
    pub typed: String,
    /// The last key that went down this frame (key binding).
    pub last_pressed: Option<KeyCode>,
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
    }
}
