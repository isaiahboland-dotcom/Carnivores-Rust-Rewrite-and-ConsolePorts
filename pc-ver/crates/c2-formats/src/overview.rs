//! An area's overview picture: the map screen's and the radar's background.
//! `Hunt2.cpp: GenerateMapImage`.
//!
//! It needs only the `.MAP` and the `.RSC`'s textures, so tools that do not
//! run the game (the launcher's map editor) draw the same picture.

use alloc::{vec, vec::Vec};

use crate::map::{Map, MAP_SIZE};
use crate::rsc::Rsc;

/// The picture's side, pixels.
pub const SIZE: usize = 256;
/// Map cells per pixel.
pub const CELLS_PER_PIXEL: usize = MAP_SIZE / SIZE;
/// `FMap`'s water bit (`fmWater`).
const WATER: u16 = 0x0080;

/// `GenerateMipMap`: average 2x2 per 5-bit channel, rounded, to side `l`.
fn mip(src: &[u16], l: usize) -> Vec<u16> {
    let mut d = vec![0u16; l * l];
    for y in 0..l {
        for x in 0..l {
            let at = |dx: usize, dy: usize| {
                u32::from(
                    src.get((y * 2 + dy) * l * 2 + x * 2 + dx)
                        .copied()
                        .unwrap_or(0),
                )
            };
            let px = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
            let ch = |sh: u32| ((px.iter().map(|p| (p >> sh) & 31).sum::<u32>() + 2) >> 2) as u16;
            d[y * l + x] = (ch(10) << 10) | (ch(5) << 5) | ch(0);
        }
    }
    d
}

/// The [`SIZE`]x[`SIZE`] overview, `xRRRRRGGGGGBBBBB`, row-major (x right,
/// z down): each pixel is its cell's texture at the 32x32 mip level (16x16
/// for water). Cells or textures the files lack are black.
pub fn image(map: &Map, rsc: &Rsc) -> Vec<u16> {
    let mips: Vec<(Vec<u16>, Vec<u16>)> = rsc
        .textures
        .iter()
        .map(|t| {
            let b = mip(t, 64);
            let c = mip(&b, 32);
            let d = mip(&c, 16);
            (c, d)
        })
        .collect();
    let mut out = vec![0u16; SIZE * SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = y * CELLS_PER_PIXEL * MAP_SIZE + x * CELLS_PER_PIXEL;
            let water = map.flags.get(i).is_some_and(|f| f & WATER != 0);
            out[y * SIZE + x] = if water {
                let w = map.water.get(i).map_or(0, |&w| usize::from(w));
                let t = rsc
                    .waters
                    .get(w)
                    .map_or(0, |w| w.texture_index.max(0) as usize);
                mips.get(t).map_or(0, |(_, d)| d[(y & 15) * 16 + (x & 15)])
            } else {
                // The engine reads a missing texture (0xffff) as texture 1
                // when it loads the map.
                let t = match map.texture1.get(i) {
                    Some(0xffff) => 1,
                    Some(&t) => usize::from(t),
                    None => usize::MAX,
                };
                mips.get(t).map_or(0, |(c, _)| c[(y & 31) * 32 + (x & 31)])
            };
        }
    }
    out
}

/// [`image`] as RGBA8 (5-bit channels widened as `c << 3 | c >> 2`), opaque.
pub fn image_rgba8(map: &Map, rsc: &Rsc) -> Vec<u8> {
    let e = |c: u16| {
        let c = (c & 31) as u8;
        (c << 3) | (c >> 2)
    };
    image(map, rsc)
        .into_iter()
        .flat_map(|p| [e(p >> 10), e(p >> 5), e(p), 255])
        .collect()
}
