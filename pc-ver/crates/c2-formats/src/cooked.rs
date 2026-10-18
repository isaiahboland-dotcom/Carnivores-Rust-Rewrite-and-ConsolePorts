//! Cooked packs for the 3DS (written by `c2-cook`, read by `c2-3ds`).
//!
//! A pack is one file: a header, an index of named entries, then the entry
//! data. Entries are game files (`HUNTDAT/...`, as the engine reads them) and
//! GPU-ready textures (`C2TX`, already in the PICA200's tiled layout).
//!
//! ```text
//! "C2PK" | version u32 | count u32 | index_len u32
//! index: count x (name_len u16 | name (UTF-8, '/'-separated) | offset u32 | size u32)
//! data:  entries, offsets from the start of the file
//!
//! "C2TX" | width u16 | height u16 | format u8 | 3 x pad | tiled texels
//! ```
//! See `docs/FORMATS.md` ("Cooked packs").

use crate::error::{FormatError, FormatErrorKind};
use crate::reader::Reader;

pub const PACK_MAGIC: &[u8; 4] = b"C2PK";
pub const PACK_VERSION: u32 = 1;
/// Bytes before the index.
pub const PACK_HEADER_LEN: usize = 16;
pub const TEX_MAGIC: &[u8; 4] = b"C2TX";
const TEX_HEADER_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackEntry {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

fn bad(offset: usize, context: &str, kind: FormatErrorKind) -> FormatError {
    FormatError {
        offset,
        context: context.into(),
        kind,
    }
}

/// From the first [`PACK_HEADER_LEN`] bytes: (entry count, index length).
pub fn parse_pack_header(data: &[u8]) -> Result<(usize, usize), FormatError> {
    let mut r = Reader::new(data);
    if &r.array::<4>("pack.magic")? != PACK_MAGIC {
        return Err(bad(
            0,
            "pack.magic",
            FormatErrorKind::Syntax("not a C2PK pack".into()),
        ));
    }
    let version = r.i32("pack.version")?;
    if version != PACK_VERSION as i32 {
        return Err(bad(
            4,
            "pack.version",
            FormatErrorKind::BadCount(version.into()),
        ));
    }
    let count = r.count(1 << 20, "pack.count")?;
    let index_len = r.count(64 << 20, "pack.index_len")?;
    Ok((count, index_len))
}

/// The index (the `index_len` bytes after the header).
pub fn parse_pack_index(data: &[u8], count: usize) -> Result<Vec<PackEntry>, FormatError> {
    let mut r = Reader::new(data);
    let mut out = Vec::with_capacity(count.min(data.len() / 10));
    for i in 0..count {
        let ctx = format!("pack.entry[{i}]");
        let n = usize::from(r.u16(&ctx)?);
        let name = String::from_utf8_lossy(r.bytes(n, &ctx)?).into_owned();
        let offset = r.i32(&ctx)? as u32;
        let size = r.i32(&ctx)? as u32;
        out.push(PackEntry { name, offset, size });
    }
    r.finish("pack.index")?;
    Ok(out)
}

/// A whole pack in memory: its entries and their bytes.
pub fn parse_pack(data: &[u8]) -> Result<Vec<(PackEntry, &[u8])>, FormatError> {
    let (count, index_len) = parse_pack_header(data)?;
    let end = PACK_HEADER_LEN.saturating_add(index_len);
    let index = data.get(PACK_HEADER_LEN..end).ok_or_else(|| {
        bad(
            data.len(),
            "pack.index",
            FormatErrorKind::UnexpectedEof {
                needed: end - data.len(),
            },
        )
    })?;
    parse_pack_index(index, count)?
        .into_iter()
        .map(|e| {
            let (s, n) = (e.offset as usize, e.size as usize);
            match data.get(s..s.saturating_add(n)) {
                Some(d) => Ok((e, d)),
                None => Err(bad(
                    s,
                    &e.name,
                    FormatErrorKind::UnexpectedEof {
                        needed: (s + n).saturating_sub(data.len()),
                    },
                )),
            }
        })
        .collect()
}

/// Writes a pack. Entry names should be unique; offsets are assigned here.
/// Data is aligned to 16 bytes.
pub fn write_pack(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let index_len: usize = entries.iter().map(|(n, _)| 2 + n.len() + 8).sum();
    let mut offset = (PACK_HEADER_LEN + index_len).next_multiple_of(16);
    let mut out = Vec::new();
    out.extend_from_slice(PACK_MAGIC);
    out.extend_from_slice(&PACK_VERSION.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&(index_len as u32).to_le_bytes());
    for (name, data) in entries {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        offset = (offset + data.len()).next_multiple_of(16);
    }
    for (_, data) in entries {
        out.resize(out.len().next_multiple_of(16), 0);
        out.extend_from_slice(data);
    }
    out
}

/// PICA200 texel formats (`GPU_TEXCOLOR` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexFormat {
    /// `RRRRRGGGGGBBBBBA`: models with see-through texels.
    Rgba5551 = 2,
    /// `RRRRRGGGGGGBBBBB`: opaque (terrain).
    Rgb565 = 3,
}

impl TexFormat {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            2 => Some(Self::Rgba5551),
            3 => Some(Self::Rgb565),
            _ => None,
        }
    }

    pub fn bytes_per_texel(self) -> usize {
        2
    }

    fn pack(self, p: &[u8]) -> u16 {
        let (r, g, b, a) = (u16::from(p[0]), u16::from(p[1]), u16::from(p[2]), p[3]);
        match self {
            Self::Rgba5551 => (r >> 3) << 11 | (g >> 3) << 6 | (b >> 3) << 1 | u16::from(a >= 128),
            Self::Rgb565 => (r >> 3) << 11 | (g >> 2) << 5 | (b >> 3),
        }
    }
}

/// A texture ready for upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookedTexture {
    pub width: u16,
    pub height: u16,
    pub format: TexFormat,
    /// Tiled texels (see [`tile`]).
    pub data: Vec<u8>,
}

/// Index of texel `(x, y)` inside its 8x8 tile: Morton (Z) order.
fn morton(x: usize, y: usize) -> usize {
    let mut m = 0;
    for b in 0..3 {
        m |= ((x >> b) & 1) << (2 * b) | ((y >> b) & 1) << (2 * b + 1);
    }
    m
}

/// Converts a top-row-first RGBA8 image to the PICA200 layout: 8x8 tiles in
/// rows, Morton order inside each tile, the bottom image row first. The
/// GPU then samples it with `t` = the image's `v` (0 = top).
/// `width` and `height` must be powers of two from 8 to 1024.
pub fn tile(rgba: &[u8], width: usize, height: usize, format: TexFormat) -> CookedTexture {
    let bpp = format.bytes_per_texel();
    let mut data = vec![0u8; width * height * bpp];
    for y in 0..height {
        // Memory row `ty` holds image row `height - 1 - ty`.
        let ty = height - 1 - y;
        for x in 0..width {
            let src = (y * width + x) * 4;
            let Some(p) = rgba.get(src..src + 4) else {
                continue;
            };
            let tile = (ty / 8) * (width / 8) + x / 8;
            let i = (tile * 64 + morton(x & 7, ty & 7)) * bpp;
            data[i..i + 2].copy_from_slice(&format.pack(p).to_le_bytes());
        }
    }
    CookedTexture {
        width: width as u16,
        height: height as u16,
        format,
        data,
    }
}

fn valid_side(n: u16) -> bool {
    n.is_power_of_two() && (8..=1024).contains(&n)
}

pub fn write_texture(t: &CookedTexture) -> Vec<u8> {
    let mut out = Vec::with_capacity(TEX_HEADER_LEN + t.data.len());
    out.extend_from_slice(TEX_MAGIC);
    out.extend_from_slice(&t.width.to_le_bytes());
    out.extend_from_slice(&t.height.to_le_bytes());
    out.extend_from_slice(&[t.format as u8, 0, 0, 0]);
    out.extend_from_slice(&t.data);
    out
}

pub fn parse_texture(data: &[u8]) -> Result<CookedTexture, FormatError> {
    let mut r = Reader::new(data);
    if &r.array::<4>("tex.magic")? != TEX_MAGIC {
        return Err(bad(
            0,
            "tex.magic",
            FormatErrorKind::Syntax("not a C2TX texture".into()),
        ));
    }
    let width = r.u16("tex.width")?;
    let height = r.u16("tex.height")?;
    let [f, ..] = r.array::<4>("tex.format")?;
    if !valid_side(width) || !valid_side(height) {
        return Err(bad(
            4,
            "tex.size",
            FormatErrorKind::BadCount(i64::from(width.max(height))),
        ));
    }
    let format = TexFormat::from_u8(f)
        .ok_or_else(|| bad(8, "tex.format", FormatErrorKind::BadCount(i64::from(f))))?;
    let n = usize::from(width) * usize::from(height) * format.bytes_per_texel();
    let texels = r.bytes(n, "tex.data")?.to_vec();
    r.finish("tex")?;
    Ok(CookedTexture {
        width,
        height,
        format,
        data: texels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trip() {
        let entries = vec![
            ("HUNTDAT/_RES.TXT".to_string(), b"res".to_vec()),
            ("tex/terrain.tx".to_string(), vec![7; 33]),
            ("empty".to_string(), vec![]),
        ];
        let bytes = write_pack(&entries);
        let got = parse_pack(&bytes).map(|v| {
            v.into_iter()
                .map(|(e, d)| (e.name, d.to_vec()))
                .collect::<Vec<_>>()
        });
        assert_eq!(got.ok(), Some(entries));
        assert!(parse_pack(&bytes[..20]).is_err());
        assert!(parse_pack(b"C2PKxxxx").is_err());
    }

    #[test]
    fn tiling_follows_the_pica_layout() {
        // 8x16 image: texel value = its (x, y) so we can find it again.
        let (w, h) = (8, 16);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 8) as u8, (y * 8) as u8, 0, 255]);
            }
        }
        let t = tile(&rgba, w, h, TexFormat::Rgb565);
        let at = |i: usize| u16::from_le_bytes([t.data[2 * i], t.data[2 * i + 1]]);
        let rgb = |x: u16, y: u16| ((x * 8) >> 3) << 11 | ((y * 8) >> 2) << 5;
        // First texel in memory: bottom-left of the image.
        assert_eq!(at(0), rgb(0, 15));
        // Morton: index 1 is x+1, index 2 is the row above (image y - 1).
        assert_eq!(at(1), rgb(1, 15));
        assert_eq!(at(2), rgb(0, 14));
        assert_eq!(at(3), rgb(1, 14));
        // Second tile row holds the top half of the image.
        assert_eq!(at(64), rgb(0, 7));
        assert_eq!(at(127), rgb(7, 0));

        let bytes = write_texture(&t);
        assert_eq!(parse_texture(&bytes).ok(), Some(t));
        assert!(parse_texture(&bytes[..bytes.len() - 1]).is_err());
    }
}
