//! A message on screen, for when there is no game to show (the Android app
//! with no game copied to the phone yet). A tap, a click or Escape closes it.

use anyhow::Result;
use winit::keyboard::KeyCode;

use crate::gpu::Gpu;
use crate::hud::{Hud, Sprite};
use crate::input::Input;
use crate::scene::Scene;

struct Notice {
    hud: Hud,
    text: String,
    done: bool,
}

/// Shows `text` until the player closes it.
pub fn show(text: String) -> Result<()> {
    crate::app::run(Box::new(move |gpu| {
        Ok(Box::new(Notice {
            hud: Hud::new(gpu),
            text,
            done: false,
        }) as Box<dyn Scene>)
    }))
}

/// `text` broken into lines no wider than `width` at `px`.
fn wrap(hud: &Hud, text: &str, px: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let tried = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && hud.label_width(px, &tried) > width {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = tried;
            }
        }
        lines.push(line);
    }
    lines
}

impl Scene for Notice {
    fn title(&self) -> String {
        "c2rs".into()
    }

    fn update(&mut self, _gpu: &Gpu, _dt: f32, input: &Input) {
        self.done |= input.clicked || input.pressed(KeyCode::Escape);
    }

    fn clear_color(&self) -> [f64; 3] {
        [0.05, 0.06, 0.05]
    }

    fn handles_escape(&self) -> bool {
        true
    }

    fn wants_exit(&self) -> bool {
        self.done
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let (w, h) = (screen.0 as f32, screen.1 as f32);
        // 8-pixel font steps keep the glyphs sharp.
        let px = ((h / 24.0 / 8.0).round().max(2.0)) * 8.0;
        let margin = w * 0.06;
        let mut sprites: Vec<Sprite> = Vec::new();
        let mut y = h * 0.08;
        for line in wrap(&self.hud, &self.text, px, w - 2.0 * margin) {
            sprites.extend(self.hud.label(margin, y, px, [0.95, 0.9, 0.75, 1.0], &line));
            y += px * 1.5;
        }
        let close = "Tap to close";
        sprites.extend(
            self.hud
                .label(margin, h - px * 2.0, px, [0.6, 0.6, 0.6, 1.0], close),
        );
        self.hud.draw(gpu, pass, screen, &sprites);
    }
}
