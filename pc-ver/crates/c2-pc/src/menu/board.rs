//! Screen pictures: a board's base image, lit regions and hit map.

use super::*;

/// A screen picture, its lit regions and hit map.
#[derive(Default)]
pub(super) struct Board {
    pub(super) base: Option<usize>,
    /// Lit cut-out per region id: image and its canvas rectangle.
    pub(super) lit: Vec<Option<(usize, Rect)>>,
    /// 400x300 region ids.
    pub(super) hit: Vec<u8>,
    /// Words drawn over it (a board composed from the mobile art).
    pub(super) labels: Vec<kit::Label>,
    /// The picture filling the window behind it, and whether the window
    /// around the board is darkened as the board is (the phone's).
    pub(super) back: Option<usize>,
    pub(super) dim: bool,
}

/// A menu picture drawn from the model at `file` (from `huntdat`), for
/// an animal with none (`c2_formats::portrait`).
pub(super) fn model_picture(hud: &mut Hud, gpu: &Gpu, huntdat: &Path, file: &str) -> Option<usize> {
    let car = c2_formats::car::parse(&std::fs::read(find_file(huntdat, file)?).ok()?).ok()?;
    let (w, h) = (357, 208);
    let rgba = c2_formats::portrait::render(&car.mesh, w, h, 0.8)?;
    Some(hud.add_image_filtered(gpu, w as u32, h as u32, &rgba, true))
}

pub(super) fn read_tga(dir: &Path, name: &str) -> Option<c2_formats::tga::Picture> {
    let p = find_file(dir, name)?;
    match c2_formats::tga::parse(&std::fs::read(&p).ok()?) {
        Ok(t) => Some(t),
        Err(e) => {
            tracing::warn!("{}: {e}", p.display());
            None
        }
    }
}

impl Board {
    pub(super) fn load(
        hud: &mut Hud,
        gpu: &Gpu,
        dir: &Path,
        base: &str,
        on: &str,
        map: &str,
    ) -> Self {
        let mut b = Board {
            base: read_tga(dir, base).map(|t| {
                let rgba = c2_core::color::image_to_rgba8(&t.pixels, false);
                hud.add_image_filtered(gpu, t.width as u32, t.height as u32, &rgba, true)
            }),
            lit: Vec::new(),
            labels: Vec::new(),
            back: None,
            dim: false,
            hit: find_file(dir, map)
                .and_then(|p| std::fs::read(p).ok())
                .filter(|h| h.len() == 400 * 300)
                .unwrap_or_default(),
        };
        let Some(on) = read_tga(dir, on).filter(|t| t.width == 800 && t.height == 600) else {
            return b;
        };
        if b.hit.is_empty() {
            return b;
        }
        let rgba = c2_core::color::image_to_rgba8(&on.pixels, false);
        // Bounding box of each region, in hit-map pixels.
        let mut bb = vec![(usize::MAX, usize::MAX, 0, 0); 256];
        for (i, &r) in b.hit.iter().enumerate() {
            if r != 0 {
                let (x, y) = (i % 400, i / 400);
                let e = &mut bb[r as usize];
                *e = (e.0.min(x), e.1.min(y), e.2.max(x), e.3.max(y));
            }
        }
        b.lit = vec![None; 256];
        for (r, &(x0, y0, x1, y1)) in bb.iter().enumerate() {
            if x0 == usize::MAX {
                continue;
            }
            let (px, py, w, h) = (x0 * 2, y0 * 2, (x1 - x0 + 1) * 2, (y1 - y0 + 1) * 2);
            let mut cut = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let (cx, cy) = (px + x, py + y);
                    if b.hit[cy / 2 * 400 + cx / 2] as usize == r {
                        let s = (cy * 800 + cx) * 4;
                        let d = (y * w + x) * 4;
                        cut[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
                    }
                }
            }
            let id = hud.add_image_filtered(gpu, w as u32, h as u32, &cut, true);
            b.lit[r] = Some((id, (px as f32, py as f32, w as f32, h as f32)));
        }
        b
    }

    /// The region under canvas point `p` (0 = none).
    pub(super) fn region(&self, p: Option<(f32, f32)>) -> u8 {
        let Some((x, y)) = p else { return 0 };
        if !(0.0..CW).contains(&x) || !(0.0..CH).contains(&y) || self.hit.is_empty() {
            return 0;
        }
        self.hit[(y as usize / 2) * 400 + x as usize / 2]
    }
}
