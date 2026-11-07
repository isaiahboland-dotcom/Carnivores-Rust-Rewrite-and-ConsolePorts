//! Stage checks for an original-engine run, from its screenshots (the
//! original's memory is not read): a weapon fired when its ammo display
//! changed between "ready" and "after"; a screen opened when the frame
//! with it open differs from the one after closing it.

use std::path::Path;

use anyhow::{bail, Context, Result};

pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGB rows, top first.
    pub rgb: Vec<u8>,
}

/// A 24-bit uncompressed BMP, as the hook writes them.
pub fn load_bmp(path: &Path) -> Result<Image> {
    let d = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let u32_at =
        |o: usize| -> Option<u32> { Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?)) };
    if d.get(..2) != Some(b"BM") {
        bail!("{}: not a BMP", path.display());
    }
    let (Some(off), Some(w), Some(h)) = (u32_at(10), u32_at(18), u32_at(22)) else {
        bail!("{}: truncated BMP", path.display());
    };
    let bpp = d.get(28).copied().unwrap_or(0);
    let h_signed = h as i32;
    if bpp != 24 || w == 0 || h_signed == 0 {
        bail!("{}: only 24-bit BMPs", path.display());
    }
    let (w, rows) = (w as usize, h_signed.unsigned_abs() as usize);
    let stride = (w * 3).div_ceil(4) * 4;
    let mut rgb = Vec::with_capacity(w * rows * 3);
    for y in 0..rows {
        // Positive height: bottom-up rows.
        let src = if h_signed > 0 { rows - 1 - y } else { y };
        let start = off as usize + src * stride;
        let Some(row) = d.get(start..start + w * 3) else {
            bail!("{}: truncated pixel data", path.display());
        };
        for p in row.as_chunks::<3>().0 {
            rgb.extend([p[2], p[1], p[0]]);
        }
    }
    Ok(Image {
        width: w as u32,
        height: rows as u32,
        rgb,
    })
}

/// Share of pixels in the region (fractions of the picture) where any
/// channel differs by more than 16.
pub fn changed(a: &Image, b: &Image, region: [f32; 4]) -> f64 {
    if (a.width, a.height) != (b.width, b.height) {
        return 1.0;
    }
    let (w, h) = (a.width as f32, a.height as f32);
    let x0 = (region[0] * w) as usize;
    let y0 = (region[1] * h) as usize;
    let x1 = ((region[0] + region[2]) * w).min(w) as usize;
    let y1 = ((region[1] + region[3]) * h).min(h) as usize;
    let (mut n, mut diff) = (0u64, 0u64);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * a.width as usize + x) * 3;
            n += 1;
            if (0..3).any(|c| a.rgb[i + c].abs_diff(b.rgb[i + c]) > 16) {
                diff += 1;
            }
        }
    }
    if n == 0 {
        0.0
    } else {
        diff as f64 / n as f64
    }
}

/// The ammo display (top left), the call icon (top right), everything.
pub const AMMO: [f32; 4] = [0.0, 0.0, 0.2, 0.04];
pub const CALL_ICON: [f32; 4] = [0.75, 0.0, 0.25, 0.14];
pub const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let stride = (w * 3).div_ceil(4) * 4;
        let mut d = b"BM".to_vec();
        d.extend((54 + stride * h).to_le_bytes());
        d.extend([0; 4]);
        d.extend(54u32.to_le_bytes());
        d.extend(40u32.to_le_bytes());
        d.extend(w.to_le_bytes());
        d.extend(h.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(24u16.to_le_bytes());
        d.extend([0; 24]);
        for y in (0..h).rev() {
            for x in 0..w {
                let [r, g, b] = f(x, y);
                d.extend([b, g, r]);
            }
            d.extend(vec![0; (stride - w * 3) as usize]);
        }
        d
    }

    #[test]
    fn reads_bottom_up_bgr_and_measures_change() {
        let dir = std::env::temp_dir().join(format!("c2-regress-bmp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pa, pb) = (dir.join("a.bmp"), dir.join("b.bmp"));
        std::fs::write(&pa, bmp(10, 5, |x, y| [x as u8 * 20, y as u8 * 40, 7])).unwrap();
        std::fs::write(
            &pb,
            bmp(10, 5, |x, y| {
                if x < 5 {
                    [x as u8 * 20, y as u8 * 40, 7]
                } else {
                    [255; 3]
                }
            }),
        )
        .unwrap();
        let (a, b) = (load_bmp(&pa).unwrap(), load_bmp(&pb).unwrap());
        assert_eq!((a.width, a.height), (10, 5));
        assert_eq!(&a.rgb[..3], &[0, 0, 7]);
        assert_eq!(
            &a.rgb[(4 * 10 + 1) * 3..(4 * 10 + 1) * 3 + 3],
            &[20, 160, 7]
        );
        assert!((changed(&a, &b, WHOLE) - 0.5).abs() < 1e-9);
        assert_eq!(changed(&a, &b, [0.0, 0.0, 0.5, 1.0]), 0.0);
        assert!(load_bmp(&dir.join("missing.bmp")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
