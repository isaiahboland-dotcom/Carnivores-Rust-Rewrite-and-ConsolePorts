//! Mip levels for the world's textures, built as the original builds its
//! smaller textures (`Resources.cpp: CreateMipMapMT`): each texel of the
//! next level averages a 2x2 block, a clear top-left texel keeps the block
//! clear, and clear texels among the other three count as the top-left one.
//! Taking the coverage from one texel, not the average, keeps far foliage
//! as dense as near.
//!
//! The original has one smaller level and picks it by distance (past 10
//! cells for the ground, 12 for models, `renderd3d.cpp`); here the GPU picks
//! from the full chain per pixel.

/// `w x h` RGBA8 at half the size per side (at least 1).
pub fn half(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; hw * hh * 4];
    let at = |x: usize, y: usize| {
        let i = (y.min(h - 1) * w + x.min(w - 1)) * 4;
        [src[i], src[i + 1], src[i + 2], src[i + 3]]
    };
    for y in 0..hh {
        for x in 0..hw {
            let c1 = at(x * 2, y * 2);
            let o = (y * hw + x) * 4;
            if c1[3] < 128 {
                continue; // clear, as the top-left texel
            }
            let others = [
                at(x * 2 + 1, y * 2),
                at(x * 2, y * 2 + 1),
                at(x * 2 + 1, y * 2 + 1),
            ];
            for k in 0..3 {
                let sum: u32 = std::iter::once(c1)
                    .chain(others.iter().map(|c| if c[3] < 128 { c1 } else { *c }))
                    .map(|c| u32::from(c[k]))
                    .sum();
                out[o + k] = ((sum + 2) / 4) as u8;
            }
            out[o + 3] = c1[3];
        }
    }
    out
}

/// Mip levels below a `w x h` texture, down to 1x1.
pub fn levels(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(c: [u8; 4], n: usize) -> Vec<u8> {
        c.iter().copied().cycle().take(n * 4).collect()
    }

    #[test]
    fn chain_reaches_one_texel() {
        assert_eq!(levels(256, 256), 9);
        assert_eq!(levels(128, 128), 8);
        assert_eq!(levels(1, 1), 1);
    }

    #[test]
    fn opaque_blocks_average() {
        // 2x2: two black, two white -> mid grey.
        let src = [
            0, 0, 0, 255, 255, 255, 255, 255, //
            255, 255, 255, 255, 0, 0, 0, 255,
        ];
        assert_eq!(half(&src, 2, 2), vec![128, 128, 128, 255]);
    }

    #[test]
    fn coverage_comes_from_the_top_left_texel() {
        let clear = [0, 0, 0, 0];
        let red = [200, 0, 0, 255];
        // Top-left clear: the block is clear, whatever the others are.
        let mut src = px(red, 4);
        src[..4].copy_from_slice(&clear);
        assert_eq!(half(&src, 2, 2), vec![0, 0, 0, 0]);
        // Top-left opaque, the rest clear: the block is the top-left's colour.
        let mut src = px(clear, 4);
        src[..4].copy_from_slice(&red);
        assert_eq!(half(&src, 2, 2), red.to_vec());
    }

    #[test]
    fn odd_and_thin_sizes_stay_in_bounds() {
        assert_eq!(half(&px([9, 9, 9, 255], 3), 3, 1).len(), 4);
        assert_eq!(half(&px([9, 9, 9, 255], 1), 1, 1).len(), 4);
    }

    /// `Resources.cpp: GenerateMipMap` on RGB555 texels, for reference.
    fn original_half(a: &[u16], l: usize) -> Vec<u16> {
        let mut d = vec![0u16; l * l];
        for y in 0..l {
            for x in 0..l {
                let t = |dx: usize, dy: usize| a[x * 2 + dx + (y * 2 + dy) * 2 * l];
                let avg = |sh: u32| {
                    let s: u16 = [t(0, 0), t(1, 0), t(0, 1), t(1, 1)]
                        .iter()
                        .map(|c| (c >> sh) & 31)
                        .sum();
                    (s + 2) >> 2
                };
                d[x + y * l] = avg(0) | (avg(5) << 5) | (avg(10) << 10);
            }
        }
        d
    }

    /// T4 (docs/TEXTURE-DETAIL.md): the RGBA8 mip, requantised to 5 bits per
    /// channel, stays within one level of the original's 5-bit mip, and
    /// within 8 as 8-bit values.
    #[test]
    fn matches_original_generate_mip_map_within_one_level() {
        let l = 64usize;
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 16) as u16
        };
        let a: Vec<u16> = (0..4 * l * l).map(|_| next() & 0x7fff | 0x0400).collect();
        let rgba = c2_core::color::image_to_rgba8(&a, false);
        let mine = half(&rgba, 2 * l, 2 * l);
        let theirs = original_half(&a, l);
        let (mut worst5, mut worst8, mut sum8) = (0i32, 0i32, 0i64);
        for (i, t) in theirs.iter().enumerate() {
            let want = c2_core::color::rgb555_to_rgba8(*t, 255);
            for k in 0..3 {
                let m = i32::from(mine[i * 4 + k]);
                worst8 = worst8.max((m - i32::from(want[k])).abs());
                sum8 += i64::from((m - i32::from(want[k])).abs());
                let c5 = i32::from((*t >> [10, 5, 0][k]) & 31);
                worst5 = worst5.max(((m >> 3) - c5).abs());
            }
        }
        eprintln!(
            "T4 mip: worst 5-bit diff {worst5}, worst 8-bit diff {worst8}, mean 8-bit diff {:.3}",
            sum8 as f64 / (theirs.len() * 3) as f64
        );
        assert!(worst5 <= 1, "5-bit diff {worst5}");
        assert!(worst8 <= 8, "8-bit diff {worst8}");
    }
}
