//! The menu panel's picture, drawn on the CPU (`panel` uploads it when it
//! changes): the game's 800x600 menu screens, their lit regions, text in
//! the PC's 8x8 font (`c2-pc hud.rs`, proportional as there), pictures and
//! boxes. Coordinates are the menu canvas's (800x600, `c2-pc menu.rs: CW,
//! CH`); the pixels are `SCALE` times that, so text stays sharp in the
//! headset.

use font8x8::{UnicodeFonts, BASIC_FONTS};

pub const W: usize = 800;
pub const H: usize = 600;
pub const SCALE: usize = 2;
/// The pixels' size.
pub const PW: usize = W * SCALE;
pub const PH: usize = H * SCALE;

/// An RGBA picture (a menu screen, a view picture).
#[derive(Clone, Default)]
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Rgba {
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        self.px
            .get(i..i + 4)
            .map_or([0; 4], |p| [p[0], p[1], p[2], p[3]])
    }
}

/// `(x, y, w, h)` in canvas units.
pub type Rect = (f32, f32, f32, f32);

fn to_u8(c: [f32; 4]) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Per glyph (ASCII 32..=127): first lit column and advance
/// (`c2-pc hud.rs: font_metrics`).
fn metrics(c: char) -> (u8, u8) {
    let bits = BASIC_FONTS
        .get(c)
        .map_or(0u8, |g| g.iter().fold(0, |m, b| m | b));
    if bits == 0 {
        return (0, 4);
    }
    let first = bits.trailing_zeros() as u8;
    let last = 7 - bits.leading_zeros() as u8;
    (first, last - first + 2)
}

fn glyph(c: char) -> char {
    let code = (c as u32).clamp(32, 127);
    char::from_u32(code).unwrap_or(' ')
}

/// The width of `text` at `px` (`c2-pc hud.rs: label_width`).
pub fn text_width(px: f32, text: &str) -> f32 {
    text.chars()
        .map(|c| f32::from(metrics(glyph(c)).1))
        .sum::<f32>()
        * px
        / 8.0
}

/// Description text re-flowed for a panel `width` wide (`c2-pc menu.rs:
/// wrap`): long lines join into paragraphs, short ones stay.
pub fn wrap(px: f32, width: f32, lines: &[String]) -> Vec<String> {
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
            if text_width(px, &next) > width && !cur.is_empty() {
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

pub struct Canvas {
    /// `PW` x `PH` RGBA.
    pub px: Vec<u8>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            px: vec![0; PW * PH * 4],
        }
    }
}

impl Canvas {
    fn put(&mut self, x: usize, y: usize, c: [u8; 4]) {
        if x < PW && y < PH {
            let i = (y * PW + x) * 4;
            self.px[i..i + 4].copy_from_slice(&c);
        }
    }

    fn blend(&mut self, x: usize, y: usize, c: [f32; 4]) {
        if x >= PW || y >= PH {
            return;
        }
        let i = (y * PW + x) * 4;
        for k in 0..3 {
            let d = f32::from(self.px[i + k]) / 255.0;
            self.px[i + k] = ((d + (c[k] - d) * c[3]).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        self.px[i + 3] = 255;
    }

    /// The canvas rectangle `r` in pixels, clipped.
    fn span(r: Rect) -> (usize, usize, usize, usize) {
        let s = SCALE as f32;
        let x0 = (r.0 * s).max(0.0) as usize;
        let y0 = (r.1 * s).max(0.0) as usize;
        let x1 = (((r.0 + r.2) * s).max(0.0) as usize).min(PW);
        let y1 = (((r.1 + r.3) * s).max(0.0) as usize).min(PH);
        (x0, y0, x1, y1)
    }

    /// A menu screen: `base`, with the regions in `lit` from `on`, those in
    /// `dim` darkened (`hit`: 400x300 region ids; `base`, `on`: 800x600).
    pub fn board(&mut self, base: &Rgba, on: &Rgba, hit: &[u8], lit: &[u8], dim: &[u8]) {
        for y in 0..PH {
            for x in 0..PW {
                let (cx, cy) = (x / SCALE, y / SCALE);
                let r = hit.get(cy / 2 * 400 + cx / 2).copied().unwrap_or(0);
                let mut c = if r != 0 && lit.contains(&r) && on.w == W {
                    on.at(cx, cy)
                } else {
                    base.at(cx, cy)
                };
                if r != 0 && dim.contains(&r) {
                    for v in &mut c[..3] {
                        *v = (f32::from(*v) * 0.35) as u8;
                    }
                }
                c[3] = 255;
                self.put(x, y, c);
            }
        }
    }

    /// `img` at its own size with its top left at (`x`, `y`).
    pub fn image(&mut self, img: &Rgba, x: f32, y: f32) {
        let (x0, y0, x1, y1) = Self::span((x, y, img.w as f32, img.h as f32));
        for py in y0..y1 {
            for px in x0..x1 {
                let sx = (px / SCALE).saturating_sub(x as usize);
                let sy = (py / SCALE).saturating_sub(y as usize);
                let mut c = img.at(sx.min(img.w - 1), sy.min(img.h - 1));
                c[3] = 255;
                self.put(px, py, c);
            }
        }
    }

    /// A rectangle of `color` over what's there (by its alpha).
    pub fn fill(&mut self, r: Rect, color: [f32; 4]) {
        let (x0, y0, x1, y1) = Self::span(r);
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, color);
            }
        }
    }

    /// A frame `t` wide around the outside of `r`.
    pub fn outline(&mut self, r: Rect, t: f32, color: [f32; 4]) {
        let (x, y, w, h) = r;
        for side in [
            (x - t, y - t, w + 2.0 * t, t),
            (x - t, y + h, w + 2.0 * t, t),
            (x - t, y, t, h),
            (x + w, y, t, h),
        ] {
            self.fill(side, color);
        }
    }

    /// `text` at `px` tall (the 8x8 font scaled), top left at (`x`, `y`).
    pub fn text(&mut self, x: f32, y: f32, px: f32, color: [f32; 4], text: &str) {
        let k = px / 8.0 * SCALE as f32;
        let c = to_u8(color);
        let mut cx = x * SCALE as f32;
        let top = (y * SCALE as f32).round();
        for ch in text.chars() {
            let ch = glyph(ch);
            let (first, adv) = metrics(ch);
            if let Some(g) = BASIC_FONTS.get(ch) {
                let w = (f32::from(adv - 1) * k).ceil() as usize;
                let h = px as usize * SCALE;
                for dy in 0..h {
                    let row = ((dy as f32 + 0.5) / k) as usize;
                    let Some(bits) = g.get(row) else { continue };
                    for dx in 0..w {
                        let col = ((dx as f32 + 0.5) / k) as usize + usize::from(first);
                        if col < 8 && bits & (1 << col) != 0 {
                            self.put(cx.round() as usize + dx, top as usize + dy, c);
                        }
                    }
                }
            }
            cx += f32::from(adv) * k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_proportional_and_wraps() {
        assert!(text_width(8.0, "i") < text_width(8.0, "W"));
        assert_eq!(text_width(16.0, "ab"), 2.0 * text_width(8.0, "ab"));
        let lines = wrap(
            12.0,
            200.0,
            &["a long line of description text that wraps".to_string()],
        );
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| text_width(12.0, l) <= 200.0));
    }

    #[test]
    fn boards_light_and_dim_regions() {
        let solid = |v: u8| Rgba {
            w: W,
            h: H,
            px: [v, v, v, 255].repeat(W * H),
        };
        let mut hit = vec![0u8; 400 * 300];
        hit[0] = 1; // canvas (0..2, 0..2)
        hit[1] = 2;
        let mut c = Canvas::default();
        c.board(&solid(100), &solid(200), &hit, &[1], &[2]);
        let at = |x: usize, y: usize| c.px[(y * PW + x) * 4];
        assert_eq!(at(0, 0), 200, "lit");
        assert_eq!(at(2 * SCALE, 0), 35, "dimmed");
        assert_eq!(at(0, 4 * SCALE), 100, "plain");
    }

    #[test]
    fn text_draws_inside_its_box() {
        let mut c = Canvas::default();
        c.text(10.0, 10.0, 8.0, [1.0; 4], "H");
        let lit: Vec<(usize, usize)> = (0..PH)
            .flat_map(|y| (0..PW).map(move |x| (x, y)))
            .filter(|&(x, y)| c.px[(y * PW + x) * 4] == 255)
            .collect();
        assert!(!lit.is_empty());
        assert!(lit
            .iter()
            .all(|&(x, y)| (20..40).contains(&x) && (20..36).contains(&y)));
    }
}
