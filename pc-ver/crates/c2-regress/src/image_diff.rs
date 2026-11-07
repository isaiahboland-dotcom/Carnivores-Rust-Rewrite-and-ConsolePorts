//! Screenshot comparison with a per-channel tolerance and masked regions,
//! so small rasterisation differences between GPUs and drivers pass.

use std::io::BufReader;
use std::path::Path;

use anyhow::{bail, Context, Result};

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub fn load_png(path: &Path) -> Result<Rgba> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut dec = png::Decoder::new(BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info()?;
    let size = reader
        .output_buffer_size()
        .context("PNG too large to decode")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());
    let (w, h) = (info.width, info.height);
    let data = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => bail!("{}: palette PNG not expanded", path.display()),
    };
    Ok(Rgba {
        width: w,
        height: h,
        data,
    })
}

/// RGB (no alpha) PNG bytes.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(rgb)?;
    }
    Ok(out)
}

/// A region to ignore, in fractions of the image (0..=1).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct Mask {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Mask {
    fn covers(&self, px: u32, py: u32, w: u32, h: u32) -> bool {
        let (fx, fy) = (px as f32 / w as f32, py as f32 / h as f32);
        fx >= self.x && fx < self.x + self.w && fy >= self.y && fy < self.y + self.h
    }
}

pub struct Diff {
    /// Pixels compared (not masked) and those over the tolerance.
    pub compared: u64,
    pub differing: u64,
    pub max_delta: u8,
    /// Red where pixels differ, a dimmed copy of the baseline elsewhere,
    /// blue where masked. RGB rows.
    pub image: Vec<u8>,
}

impl Diff {
    pub fn ratio(&self) -> f64 {
        if self.compared == 0 {
            0.0
        } else {
            self.differing as f64 / self.compared as f64
        }
    }
}

/// Compares two images of the same size. A pixel differs when any channel
/// is more than `tolerance` apart.
pub fn diff(a: &Rgba, b: &Rgba, tolerance: u8, masks: &[Mask]) -> Result<Diff> {
    if (a.width, a.height) != (b.width, b.height) {
        bail!(
            "sizes differ: {}x{} vs {}x{}",
            a.width,
            a.height,
            b.width,
            b.height
        );
    }
    let (w, h) = (a.width, a.height);
    let mut d = Diff {
        compared: 0,
        differing: 0,
        max_delta: 0,
        image: Vec::with_capacity((w * h * 3) as usize),
    };
    for (i, (pa, pb)) in a
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data.as_chunks::<4>().0)
        .enumerate()
    {
        let (x, y) = (i as u32 % w, i as u32 / w);
        if masks.iter().any(|m| m.covers(x, y, w, h)) {
            d.image.extend([0, 0, 96]);
            continue;
        }
        let delta = (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap_or(0);
        d.compared += 1;
        d.max_delta = d.max_delta.max(delta);
        if delta > tolerance {
            d.differing += 1;
            d.image.extend([255, 0, 0]);
        } else {
            d.image.extend([pa[0] / 3, pa[1] / 3, pa[2] / 3]);
        }
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Rgba {
        let data = (0..w * h).flat_map(|i| f(i % w, i / w)).collect();
        Rgba {
            width: w,
            height: h,
            data,
        }
    }

    #[test]
    fn tolerance_and_masks() {
        let a = img(10, 10, |_, _| [100, 100, 100, 255]);
        let b = img(10, 10, |x, _| {
            if x < 5 {
                [104, 100, 100, 255]
            } else {
                [200, 100, 100, 255]
            }
        });
        let d = diff(&a, &b, 8, &[]).unwrap();
        assert_eq!((d.compared, d.differing, d.max_delta), (100, 50, 100));
        let right = Mask {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let d = diff(&a, &b, 8, &[right]).unwrap();
        assert_eq!((d.compared, d.differing), (50, 0));
        assert_eq!(d.image.len(), 300);
        assert!(diff(&a, &img(5, 5, |_, _| [0; 4]), 8, &[]).is_err());
    }

    #[test]
    fn png_round_trip() {
        let rgb: Vec<u8> = (0..4 * 3 * 3).map(|i| i as u8 * 7).collect();
        let bytes = encode_rgb(4, 3, &rgb).unwrap();
        let dir = std::env::temp_dir().join(format!("c2-regress-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.png");
        std::fs::write(&p, bytes).unwrap();
        let back = load_png(&p).unwrap();
        assert_eq!((back.width, back.height), (4, 3));
        assert_eq!(&back.data[..4], &[0, 7, 14, 255]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
