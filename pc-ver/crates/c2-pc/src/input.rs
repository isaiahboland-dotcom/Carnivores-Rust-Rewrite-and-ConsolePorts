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
    /// Left mouse button is held (fire).
    pub mouse_down: bool,
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
            }
        } else {
            self.held.remove(&k);
        }
    }

    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.mouse_delta = (0.0, 0.0);
        self.scroll = 0.0;
    }
}
