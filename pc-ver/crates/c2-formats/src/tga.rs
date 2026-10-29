//! `.TGA` images (menus, HUD, bullet icons, call icons).
//! Loaded by `Resources.cpp: LoadPictureTGA`.
//!
//! The original reads width/height at offset 12, then raw 16-bit pixels from
//! offset 18, always treating rows as bottom-up. Every vanilla file is
//! uncompressed 16-bit with no ID field, which that path handles exactly. We
//! also honour the header (ID length, origin bit, RLE, 24/32-bit) so the few
//! mod images in other encodings decode correctly instead of as garbage.

use crate::error::{FormatError, FormatErrorKind};
use crate::reader::{Reader, WriteLe};
use alloc::{format, string::String, vec, vec::Vec};

/// Decoded image: `width * height` pixels, top row first, `ARRRRRGGGGGBBBBB`.
/// The top bit is kept as stored for 16-bit files (the engine ignores it),
/// and set from alpha >= 128 for 32-bit files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u16>,
}

fn pack_555(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 10) | (u16::from(g >> 3) << 5) | u16::from(b >> 3)
}

pub fn parse(data: &[u8]) -> Result<Picture, FormatError> {
    let mut r = Reader::new(data);
    let id_len = usize::from(r.array::<1>("tga.id_length")?[0]);
    let [cmap_type, image_type] = r.array("tga.types")?;
    r.bytes(9, "tga.colormap_origin")?;
    let width = usize::from(r.u16("tga.width")?);
    let height = usize::from(r.u16("tga.height")?);
    let [bpp, descriptor] = r.array("tga.bpp_descriptor")?;
    r.bytes(id_len, "tga.id")?;

    let unsupported = |what: String| r_err(&r, what);
    if cmap_type != 0 {
        return Err(unsupported("colour-mapped TGA".into()));
    }
    let rle = match image_type {
        2 => false,
        10 => true,
        t => return Err(unsupported(format!("TGA image type {t}"))),
    };
    let px_bytes = match bpp {
        16 | 24 | 32 => usize::from(bpp / 8),
        b => return Err(unsupported(format!("{b}-bit TGA"))),
    };
    let decode = |p: &[u8]| -> u16 {
        match p.len() {
            2 => u16::from_le_bytes([p[0], p[1]]),
            3 => pack_555(p[2], p[1], p[0]),
            _ => pack_555(p[2], p[1], p[0]) | if p[3] >= 128 { 0x8000 } else { 0 },
        }
    };

    let count = width * height;
    let mut pixels = Vec::with_capacity(count.min(r.remaining()));
    if rle {
        while pixels.len() < count {
            let [head] = r.array("tga.rle.header")?;
            let n = usize::from(head & 0x7f) + 1;
            if head & 0x80 != 0 {
                let p = decode(r.bytes(px_bytes, "tga.rle.pixel")?);
                pixels.extend(core::iter::repeat_n(p, n));
            } else {
                for _ in 0..n {
                    pixels.push(decode(r.bytes(px_bytes, "tga.rle.raw")?));
                }
            }
        }
        pixels.truncate(count);
    } else {
        let raw = r.bytes(count * px_bytes, "tga.pixels")?;
        pixels.extend(raw.chunks_exact(px_bytes).map(decode));
    }

    // Descriptor bit 5 set = top-left origin; otherwise rows are bottom-up.
    if descriptor & 0x20 == 0 && width > 0 {
        let rows: Vec<&[u16]> = pixels.chunks_exact(width).rev().collect();
        pixels = rows.concat();
    }
    Ok(Picture {
        width,
        height,
        pixels,
    })
}

fn r_err(r: &Reader<'_>, what: String) -> FormatError {
    r.err("tga", FormatErrorKind::Syntax(what))
}

/// Writes the vanilla encoding: uncompressed 16-bit, bottom-up, no ID.
pub fn write(pic: &Picture) -> Vec<u8> {
    let mut w = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let dim = |n: usize| u16::try_from(n).unwrap_or(u16::MAX).to_le_bytes();
    w.extend_from_slice(&dim(pic.width));
    w.extend_from_slice(&dim(pic.height));
    w.extend_from_slice(&[16, 0]);
    if pic.width > 0 {
        for row in pic.pixels.chunks_exact(pic.width).rev() {
            w.put_u16s(row);
        }
    }
    w
}
