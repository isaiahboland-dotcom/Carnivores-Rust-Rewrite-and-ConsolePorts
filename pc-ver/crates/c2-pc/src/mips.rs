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
}
