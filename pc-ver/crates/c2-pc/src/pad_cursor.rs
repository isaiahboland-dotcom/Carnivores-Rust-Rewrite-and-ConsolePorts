//! A gamepad pointer for the menus (they point and click, and Wayland lets
//! no program move the system pointer): the left stick moves a drawn arrow,
//! A clicks where it is, B is Escape (back), + is Enter. The menus read it
//! as the mouse (`Input::cursor`, `clicked`), so every screen takes it as it
//! is. Moving the mouse hides the arrow and the pad carries on from there.
//! Off while the cursor is captured (the hunt reads the pad itself). While
//! it shows, "A Select  B Back" sits in the bottom right.

use winit::keyboard::KeyCode;

use crate::gpu::Gpu;
use crate::hud::{Hud, Sprite};
use crate::input::Input;

/// Full over, the arrow crosses this share of the window's height a second.
const SPEED: f32 = 1.1;
/// The arrow image's size and on-screen scale.
const W: usize = 12;
const H: usize = 18;
const SCALE: f32 = 2.0;

pub struct PadCursor {
    hud: Hud,
    arrow: usize,
    pos: Option<(f32, f32)>,
    shown: bool,
    /// The pad is holding the "mouse button" / a key down.
    holding: bool,
    keys: Vec<KeyCode>,
}

/// An arrow pointer, gold (the launcher's) with a dark edge; our own
/// drawing, no game art.
fn arrow() -> Vec<u8> {
    let inside = |x: i32, y: i32| {
        let (fx, fy) = (x as f32, y as f32);
        (0..W as i32).contains(&x)
            && (0..H as i32).contains(&y)
            && fx <= fy * 0.62
            && (fy < 12.0 || fx < 15.0 - fy * 0.55 || (fx > 3.0 && fx < 6.0 && fy < 17.0))
    };
    let mut px = vec![0u8; W * H * 4];
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            if !inside(x, y) {
                continue;
            }
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|(dx, dy)| !inside(x + dx, y + dy));
            let c = if edge {
                [16, 22, 16, 255]
            } else {
                [240, 200, 70, 255]
            };
            let i = (y as usize * W + x as usize) * 4;
            px[i..i + 4].copy_from_slice(&c);
        }
    }
    px
}

impl PadCursor {
    pub fn new(gpu: &Gpu) -> Self {
        let mut hud = Hud::new(gpu);
        let arrow = hud.add_image(gpu, W as u32, H as u32, &arrow());
        Self {
            hud,
            arrow,
            pos: None,
            shown: false,
            holding: false,
            keys: Vec::new(),
        }
    }

    /// The mouse moved: the arrow goes, and the pad carries on from here.
    pub fn mouse_moved(&mut self, x: f32, y: f32) {
        self.pos = Some((x, y));
        self.shown = false;
    }

    /// Before the scene's update: the pad as the mouse, `size` the window.
    pub fn apply(&mut self, input: &mut Input, size: (u32, u32), dt: f32) {
        use c2_pad::Action as Pad;
        if input.grabbed {
            self.shown = false;
            if self.holding {
                input.mouse_down = false;
                self.holding = false;
            }
            return;
        }
        let pad = input.pad;
        let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let [sx, sy] = pad.walk;
        if sx != 0.0 || sy != 0.0 || pad.pressed(Pad::Jump) {
            self.shown = true;
        }
        if !self.shown {
            return;
        }
        let (mut x, mut y) = self.pos.unwrap_or((w * 0.5, h * 0.5));
        // Curved, so small pushes place it precisely.
        let v = SPEED * h * dt;
        x = (x + sx * sx.abs() * v).clamp(0.0, w - 1.0);
        y = (y - sy * sy.abs() * v).clamp(0.0, h - 1.0);
        self.pos = Some((x, y));
        input.cursor = Some((x, y));
        if pad.pressed(Pad::Jump) {
            input.clicked = true;
            input.mouse_down = true;
            self.holding = true;
        } else if self.holding && !pad.held(Pad::Jump) {
            input.mouse_down = false;
            self.holding = false;
        }
        for (a, k) in [(Pad::Run, KeyCode::Escape), (Pad::Pause, KeyCode::Enter)] {
            if pad.pressed(a) {
                input.key(k, true);
                self.keys.push(k);
            }
        }
    }

    /// After the frame: the keys the pad pressed go up again.
    pub fn after_frame(&mut self, input: &mut Input) {
        for k in self.keys.drain(..) {
            input.key(k, false);
        }
    }

    pub fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let Some((x, y)) = self.pos.filter(|_| self.shown) else {
            return;
        };
        // "A Select  B Back" in the bottom right while the pad is in use.
        let scale = (screen.1 as f32 / 480.0).floor().max(1.0);
        let px = 8.0 * scale;
        let hints = [("A", "Select"), ("B", "Back")];
        let space = self.hud.label_width(px, " ");
        let gap = 14.0 * scale;
        let width: f32 = hints
            .iter()
            .map(|(b, a)| self.hud.label_width(px, b) + space + self.hud.label_width(px, a))
            .sum::<f32>()
            + gap;
        let pad = 8.0 * scale;
        let (mut hx, hy) = (
            screen.0 as f32 - width - 2.0 * pad,
            screen.1 as f32 - 28.0 * scale,
        );
        let mut sprites: Vec<Sprite> = self
            .hud
            .rect(
                hx - pad,
                hy - 6.0 * scale,
                width + 2.0 * pad,
                20.0 * scale,
                [0.0, 0.0, 0.0, 0.6],
            )
            .into_iter()
            .collect();
        for (b, a) in hints {
            sprites.extend(self.hud.label(hx, hy, px, [0.94, 0.78, 0.27, 1.0], b));
            hx += self.hud.label_width(px, b) + space;
            sprites.extend(self.hud.label(hx, hy, px, [0.85, 0.85, 0.8, 1.0], a));
            hx += self.hud.label_width(px, a) + gap;
        }
        sprites.push(Sprite::new(self.arrow, x, y, SCALE));
        self.hud.draw(gpu, pass, screen, &sprites);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_has_a_tip_a_body_and_an_edge() {
        let px = arrow();
        let at = |x: usize, y: usize| &px[(y * W + x) * 4..(y * W + x) * 4 + 4];
        assert_eq!(at(0, 0)[3], 255, "tip");
        assert_eq!(at(W - 1, 0)[3], 0, "clear beside the tip");
        assert!(px.chunks(4).any(|c| c == [240, 200, 70, 255]), "gold body");
        assert!(px.chunks(4).any(|c| c == [16, 22, 16, 255]), "dark edge");
    }
}
