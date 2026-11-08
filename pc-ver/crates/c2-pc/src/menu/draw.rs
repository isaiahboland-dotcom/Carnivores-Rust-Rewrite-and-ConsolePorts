//! Drawing helpers shared by the screens: text, images, boards, the canvas.

use super::*;

/// Re-flows description text for a panel. The files are broken for a wider
/// font: long lines are joined into paragraphs, short ones ("Name : ...",
/// blank lines) kept, then everything is wrapped to `width`.
pub(super) fn wrap(hud: &Hud, px: f32, width: f32, lines: &[String]) -> Vec<String> {
    let mut paras: Vec<String> = Vec::new();
    let mut joining = false;
    for l in lines {
        let l = l.trim();
        match paras.last_mut() {
            Some(p) if joining && !l.is_empty() => {
                p.push(' ');
                p.push_str(l);
            }
            _ => paras.push(l.to_string()),
        }
        joining = l.len() > 30;
    }
    let mut out = Vec::new();
    for l in &paras {
        let mut cur = String::new();
        for w in l.split(' ') {
            let next = if cur.is_empty() {
                w.to_string()
            } else {
                format!("{cur} {w}")
            };
            if hud.label_width(px, &next) > width && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
                cur = w.to_string();
            } else {
                cur = next;
            }
        }
        out.push(cur);
    }
    out
}

impl Menu {
    /// The canvas rectangle a 4:3 picture covers the whole window with
    /// (cropped to it, centred).
    pub(super) fn cover(&self) -> Rect {
        let (s, _, _) = self.canvas();
        if self.screen_size.0 == 0 || self.screen_size.1 == 0 {
            return (0.0, 0.0, CW, CH);
        }
        let (w, h) = (self.screen_size.0 as f32 / s, self.screen_size.1 as f32 / s);
        let k = (w / CW).max(h / CH);
        let (cw, ch) = (CW * k, CH * k);
        ((CW - cw) / 2.0, (CH - ch) / 2.0, cw, ch)
    }

    /// Canvas mapping for the last drawn window size: (scale, x0, y0).
    pub(super) fn canvas(&self) -> (f32, f32, f32) {
        // Not drawn yet (a test script): the canvas's own size.
        if self.screen_size.0 == 0 || self.screen_size.1 == 0 {
            return (1.0, 0.0, 0.0);
        }
        let (w, h) = (self.screen_size.0 as f32, self.screen_size.1 as f32);
        let s = (w / CW).min(h / CH);
        (s, (w - CW * s) / 2.0, (h - CH * s) / 2.0)
    }

    pub(super) fn text(
        &self,
        out: &mut Vec<Sprite>,
        x: f32,
        y: f32,
        px: f32,
        color: [f32; 4],
        t: &str,
    ) {
        let (s, ox, oy) = self.canvas();
        let (x, y) = (ox + x * s, oy + y * s);
        out.extend(self.hud.label(x + s, y + s, px * s, SHADOW, t));
        out.extend(self.hud.label(x, y, px * s, color, t));
    }

    pub(super) fn text_width(&self, px: f32, t: &str) -> f32 {
        self.hud.label_width(px, t)
    }

    pub(super) fn image(&self, out: &mut Vec<Sprite>, id: Option<usize>, r: Rect) {
        let Some(id) = id else { return };
        let (s, ox, oy) = self.canvas();
        out.push(Sprite {
            size: Some([r.2 * s, r.3 * s]),
            ..Sprite::new(id, ox + r.0 * s, oy + r.1 * s, 1.0)
        });
    }

    pub(super) fn image_native(&self, out: &mut Vec<Sprite>, id: Option<usize>, x: f32, y: f32) {
        let Some((id, w, h)) = id.and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h))) else {
            return;
        };
        self.image(out, Some(id), (x, y, w as f32, h as f32));
    }

    /// The board, with `lit` regions shown lit.
    pub(super) fn board(&self, out: &mut Vec<Sprite>, b: &Board, lit: &[u8]) {
        if b.back.is_some() {
            let r = self.cover();
            self.image(out, b.back, r);
            if b.dim {
                // Around the board (it darkens its own part).
                let (x1, y1) = (r.0 + r.2, r.1 + r.3);
                for s in [
                    (r.0, r.1, -r.0, r.3),
                    (CW, r.1, x1 - CW, r.3),
                    (0.0, r.1, CW, -r.1),
                    (0.0, CH, CW, y1 - CH),
                ] {
                    self.fill(out, s, [0.0, 0.0, 0.0, 150.0 / 255.0]);
                }
            }
        }
        self.image(out, b.base, (0.0, 0.0, CW, CH));
        for &r in lit {
            if let Some(Some((id, rect))) = b.lit.get(r as usize) {
                self.image(out, Some(*id), *rect);
            }
        }
        for l in &b.labels {
            self.kit_words(out, l);
        }
    }

    /// A composed board's words, in the phone game's lettering.
    pub(super) fn kit_words(&self, out: &mut Vec<Sprite>, l: &kit::Label) {
        self.kit_words_sized(out, l, if l.title { 24.0 } else { 17.0 });
    }

    pub(super) fn kit_words_sized(&self, out: &mut Vec<Sprite>, l: &kit::Label, px: f32) {
        let font = self.kit.as_ref().and_then(|k| {
            if l.title {
                k.title.as_ref()
            } else {
                k.body.as_ref()
            }
        });
        let (x, y, color) = if l.centred && l.rect.2 > 0.0 {
            let w = font.map_or_else(|| self.text_width(px, &l.text), |f| f.width(px, &l.text));
            (
                l.rect.0 + (l.rect.2 - w) / 2.0,
                l.rect.1 + (l.rect.3 - px) / 2.0,
                kit::STONE,
            )
        } else {
            (
                l.rect.0,
                l.rect.1,
                if l.title { kit::GOLD } else { kit::LIGHT },
            )
        };
        match font {
            Some(f) => {
                let c = self.canvas();
                f.sprites(
                    out,
                    (c.0, c.1 + c.0, c.2 + c.0),
                    (x, y),
                    px,
                    SHADOW,
                    &l.text,
                );
                f.sprites(out, c, (x, y), px, color, &l.text);
            }
            None => self.text(out, x, y, px, color, &l.text),
        }
    }

    pub(super) fn frame_time(&self) -> f32 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| (d.as_millis() % 100_000) as f32 / 1000.0)
    }

    pub(super) fn draw_loading(&self, out: &mut Vec<Sprite>) {
        if let (None, Some(b)) = (self.loading, &self.kit_loading) {
            self.board(out, b, &[]);
            return;
        }
        let Some((id, w, h)) = self
            .loading
            .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
        else {
            return;
        };
        // The top half is the picture; the bottom half its filled progress bar.
        let (s, ox, oy) = self.canvas();
        let (x, y) = ((CW - w) / 2.0, (CH - h / 2.0) / 2.0);
        out.push(Sprite {
            uv: [0.0, 0.0, 1.0, 0.5],
            size: Some([w * s, h / 2.0 * s]),
            ..Sprite::new(id, ox + x * s, oy + y * s, 1.0)
        });
    }
}
