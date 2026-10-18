//! 16-bit `xRRRRRGGGGGBBBBB` texels to RGBA8.

/// Expands 5-bit channels to 8 bits (`c << 3 | c >> 2`).
pub fn rgb555_to_rgba8(px: u16, alpha: u8) -> [u8; 4] {
    let e = |c: u16| {
        let c = (c & 31) as u8;
        (c << 3) | (c >> 2)
    };
    [e(px >> 10), e(px >> 5), e(px), alpha]
}

/// Converts a whole image. With `zero_is_transparent`, texel value 0 gets
/// alpha 0 (`Resources.cpp: GenerateAlphaFlags` for models with opacity faces).
pub fn image_to_rgba8(pixels: &[u16], zero_is_transparent: bool) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|&p| {
            let a = if zero_is_transparent && p & 0x7fff == 0 {
                0
            } else {
                255
            };
            rgb555_to_rgba8(p, a)
        })
        .collect()
}
