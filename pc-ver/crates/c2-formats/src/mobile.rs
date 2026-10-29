//! The mobile games' character files (Carnivores: Dinosaur Hunter and Ice
//! Age for Android, from an APK the user supplies; read locally, never
//! redistributed). Layouts were worked out from the files themselves; the
//! shared characters match the PC `.CAR`s field for field.
//!
//! A character is a text descriptor (`characters/<name>.cat`) naming its
//! pieces, which together hold what one `.CAR` holds:
//! - `models/characters/<model>.3dn`: `u32 VCount, FCount, 0; char name[32];
//!   u32 0;` then `VCount` PC vertices (16 bytes) and `FCount` faces of 52
//!   bytes: `i32 v[3]; u32 uv[3]` (`u` low 16 bits, `v` high, in texels),
//!   `u16 flags, dmask; i32 distant, next, group; u8 reserved[12]`.
//! - `animations/characters/<file>.ani`: `u32 KPS, Frames, VCount;` then
//!   `i16 data[Frames x VCount x 3]`, as a `.CAR` animation.
//! - `sounds/characters/<file>.wav`: IMA ADPCM (or PCM) mono WAV.
//! - `textures/characters/<model stem>`: `.pkm` (ETC1), `.crt` (`u16 w, h;
//!   u8 bpp (32);` then RGBA rows, top first) or `.tga` (32-bit), 256 wide.

use crate::car::{Animation, Car, Sound, ANIFX_SLOTS};
use crate::error::{FormatError, FormatErrorKind};
use crate::model::{Face, Mesh, Vertex, TEXTURE_WIDTH};
use crate::reader::Reader;
use alloc::{format, string::String, string::ToString, vec, vec::Vec};

/// A character descriptor (`.cat`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cat {
    pub title: String,
    pub model: String,
    /// Each animation's file (no extension) and the sound it plays.
    pub animations: Vec<(String, Option<usize>)>,
    pub sounds: Vec<String>,
}

fn syntax(what: &str) -> FormatError {
    FormatError {
        offset: 0,
        context: what.into(),
        kind: FormatErrorKind::Syntax("not a carnivores_character file".into()),
    }
}

/// A `.cat` file: `[general] title, model`, then `animation_id N` and
/// `sound_id N` entries with `file` (and `sound_index`) under them.
pub fn parse_cat(text: &str) -> Result<Cat, FormatError> {
    if !text.trim_start().starts_with("carnivores_character") {
        return Err(syntax("cat"));
    }
    let mut cat = Cat::default();
    #[derive(PartialEq)]
    enum In {
        None,
        Anim,
        Sound,
    }
    let mut item = In::None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("animation_id") {
            item = In::Anim;
            let _ = rest;
            cat.animations.push((String::new(), None));
            continue;
        }
        if l.starts_with("sound_id") {
            item = In::Sound;
            cat.sounds.push(String::new());
            continue;
        }
        if l.starts_with('[') {
            item = In::None;
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        match (k.trim(), &item) {
            ("title", In::None) => cat.title = v,
            ("model", In::None) => cat.model = v,
            ("file", In::Anim) => {
                if let Some(a) = cat.animations.last_mut() {
                    a.0 = v;
                }
            }
            ("sound_index", In::Anim) => {
                if let Some(a) = cat.animations.last_mut() {
                    a.1 = v.parse().ok();
                }
            }
            ("file", In::Sound) => {
                if let Some(s) = cat.sounds.last_mut() {
                    *s = v;
                }
            }
            _ => {}
        }
    }
    Ok(cat)
}

/// A `.3dn` model: its name and mesh (no texture).
pub fn parse_3dn(data: &[u8]) -> Result<(String, Mesh), FormatError> {
    let mut r = Reader::new(data);
    let vc = r.i32("3dn.vcount")?;
    let fc = r.i32("3dn.fcount")?;
    r.i32("3dn.reserved")?;
    let name = crate::reader::c_str(r.bytes(32, "3dn.name")?);
    r.i32("3dn.reserved2")?;
    let count = |n: i32, size: usize, what: &str, r: &Reader<'_>| {
        usize::try_from(n)
            .ok()
            .filter(|&n| n.checked_mul(size).is_some_and(|b| b <= r.remaining()))
            .ok_or_else(|| r.err(what, FormatErrorKind::BadCount(i64::from(n))))
    };
    let vc = count(vc, Vertex::SIZE, "3dn.vcount", &r)?;
    let vertices = (0..vc)
        .map(|_| Vertex::read(&mut r))
        .collect::<Result<Vec<_>, _>>()?;
    let fc = count(fc, 52, "3dn.fcount", &r)?;
    let mut faces = Vec::with_capacity(fc);
    for _ in 0..fc {
        let v = [
            r.i32("3dn.face.v")?,
            r.i32("3dn.face.v")?,
            r.i32("3dn.face.v")?,
        ];
        let mut tu = [0; 3];
        let mut tv = [0; 3];
        for k in 0..3 {
            tu[k] = i32::from(r.u16("3dn.face.u")?);
            tv[k] = i32::from(r.u16("3dn.face.v")?);
        }
        faces.push(Face {
            v,
            tu,
            tv,
            flags: r.u16("3dn.face.flags")?,
            dmask: r.u16("3dn.face.dmask")?,
            distant: r.i32("3dn.face.distant")?,
            next: r.i32("3dn.face.next")?,
            group: r.i32("3dn.face.group")?,
            reserved: r.array("3dn.face.reserved")?,
        });
    }
    Ok((
        name,
        Mesh {
            vertices,
            faces,
            texture: Vec::new(),
        },
    ))
}

/// A `.ani` clip for a model of `vcount` vertices.
pub fn parse_ani(data: &[u8], name: &str, vcount: usize) -> Result<Animation, FormatError> {
    let mut r = Reader::new(data);
    let kps = r.i32("ani.kps")?;
    let frames = r.i32("ani.frames")?;
    let vc = r.i32("ani.vcount")?;
    if usize::try_from(vc).ok() != Some(vcount) {
        return Err(r.err("ani.vcount", FormatErrorKind::BadCount(i64::from(vc))));
    }
    let n = usize::try_from(frames)
        .ok()
        .and_then(|f| f.checked_mul(vcount * 3))
        .filter(|&n| n * 2 <= r.remaining())
        .ok_or_else(|| r.err("ani.frames", FormatErrorKind::BadCount(i64::from(frames))))?;
    let data = (0..n)
        .map(|_| r.i16("ani.data"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut name_raw = [0u8; 32];
    for (d, s) in name_raw.iter_mut().zip(name.bytes().take(31)) {
        *d = s;
    }
    Ok(Animation {
        name_raw,
        kps,
        frames: n / (vcount * 3).max(1),
        data,
    })
}

/// An RGBA8 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// ETC1 modifier tables.
const ETC1: [[i32; 2]; 8] = [
    [2, 8],
    [5, 17],
    [9, 29],
    [13, 42],
    [18, 60],
    [24, 80],
    [33, 106],
    [47, 183],
];

/// One ETC1 block into 4x4 RGB.
fn etc1_block(b: &[u8; 8]) -> [[u8; 3]; 16] {
    let diff = b[3] & 2 != 0;
    let flip = b[3] & 1 != 0;
    let mut base = [[0i32; 3]; 2];
    for c in 0..3 {
        if diff {
            let a = i32::from(b[c] >> 3);
            let d = i32::from((b[c] & 7) as i8) - if b[c] & 4 != 0 { 8 } else { 0 };
            let e = |x: i32| (x << 3) | (x >> 2);
            base[0][c] = e(a);
            base[1][c] = e((a + d) & 31);
        } else {
            base[0][c] = i32::from(b[c] >> 4) * 17;
            base[1][c] = i32::from(b[c] & 15) * 17;
        }
    }
    let tables = [usize::from(b[3] >> 5), usize::from((b[3] >> 2) & 7)];
    let bits = u32::from_be_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 3]; 16];
    for x in 0..4 {
        for y in 0..4 {
            let i = x * 4 + y;
            let msb = (bits >> (i + 16)) & 1;
            let lsb = (bits >> i) & 1;
            let sub = usize::from(if flip { y >= 2 } else { x >= 2 });
            let [small, large] = ETC1[tables[sub]];
            let m = match (msb, lsb) {
                (0, 0) => small,
                (0, _) => large,
                (_, 0) => -small,
                _ => -large,
            };
            for c in 0..3 {
                out[y * 4 + x][c] = (base[sub][c] + m).clamp(0, 255) as u8;
            }
        }
    }
    out
}

/// A `.pkm` (ETC1, `PKM 10`) texture.
pub fn decode_pkm(data: &[u8]) -> Result<Rgba, FormatError> {
    let mut r = Reader::new(data);
    if r.bytes(6, "pkm.magic")? != b"PKM 10" {
        return Err(r.err("pkm.magic", FormatErrorKind::Syntax("not PKM 10".into())));
    }
    let be = |r: &mut Reader<'_>, c: &str| -> Result<usize, FormatError> {
        let [a, b] = r.array::<2>(c)?;
        Ok(usize::from(u16::from_be_bytes([a, b])))
    };
    be(&mut r, "pkm.format")?;
    let (bw, bh) = (be(&mut r, "pkm.width")?, be(&mut r, "pkm.height")?);
    let (w, h) = (
        be(&mut r, "pkm.orig_width")?,
        be(&mut r, "pkm.orig_height")?,
    );
    let mut pixels = vec![0u8; w * h * 4];
    for by in 0..bh / 4 {
        for bx in 0..bw / 4 {
            let block = etc1_block(&r.array("pkm.block")?);
            for (k, px) in block.iter().enumerate() {
                let (x, y) = (bx * 4 + k % 4, by * 4 + k / 4);
                if x < w && y < h {
                    let d = (y * w + x) * 4;
                    pixels[d..d + 3].copy_from_slice(px);
                    pixels[d + 3] = 255;
                }
            }
        }
    }
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

/// A `.crt` texture (raw RGBA, top row first).
pub fn decode_crt(data: &[u8]) -> Result<Rgba, FormatError> {
    let mut r = Reader::new(data);
    let w = usize::from(r.u16("crt.width")?);
    let h = usize::from(r.u16("crt.height")?);
    let bpp = r.array::<1>("crt.bpp")?[0];
    if bpp != 32 {
        return Err(r.err("crt.bpp", FormatErrorKind::BadCount(i64::from(bpp))));
    }
    let pixels = r.bytes(w * h * 4, "crt.pixels")?.to_vec();
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

/// A texture by file name (`.pkm`, `.crt` or `.tga`).
pub fn decode_texture(name: &str, data: &[u8]) -> Result<Rgba, FormatError> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".pkm") {
        decode_pkm(data)
    } else if lower.ends_with(".crt") {
        decode_crt(data)
    } else {
        let p = crate::tga::parse(data)?;
        let mut pixels = Vec::with_capacity(p.pixels.len() * 4);
        for px in &p.pixels {
            let c = |s: u16| ((((px >> s) & 31) << 3) | (((px >> s) & 31) >> 2)) as u8;
            pixels.extend_from_slice(&[c(10), c(5), c(0), 255]);
        }
        Ok(Rgba {
            width: p.width,
            height: p.height,
            pixels,
        })
    }
}

/// RGBA into a `.CAR` texture: 256 wide, `RGB555`, rows of the image
/// (wider or narrower images are resampled to 256 columns); dim
/// see-through texels black.
fn car_texture(img: &Rgba) -> Vec<u16> {
    let (w, h) = (img.width.max(1), img.height);
    let rows = h * TEXTURE_WIDTH / w;
    let mut out = Vec::with_capacity(rows * TEXTURE_WIDTH);
    for y in 0..rows {
        for x in 0..TEXTURE_WIDTH {
            let (sx, sy) = (x * w / TEXTURE_WIDTH, y * w / TEXTURE_WIDTH);
            let i = (sy.min(h.saturating_sub(1)) * w + sx) * 4;
            let p = img.pixels.get(i..i + 4).unwrap_or(&[0, 0, 0, 0]);
            // A see-through texel (alpha 0: the mobile games add its colour,
            // a muzzle flash's glow) keeps its colour if bright, else goes
            // black, the `.CAR`'s colour key on see-through faces.
            let dim = (u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3 < 96;
            out.push(if p[3] < 128 && dim {
                0
            } else {
                (u16::from(p[0] >> 3) << 10) | (u16::from(p[1] >> 3) << 5) | u16::from(p[2] >> 3)
            });
        }
    }
    out
}

const IMA_INDEX: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];
const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

/// A mono WAV as 16-bit PCM bytes and its sample rate: plain PCM as it is,
/// IMA ADPCM (format 0x11) decoded block by block.
pub fn decode_wav(data: &[u8]) -> Result<(Vec<u8>, u32), FormatError> {
    let wav = crate::wav::parse(data)?;
    let Some(f) = wav.format else {
        return Ok((wav.pcm, 22050));
    };
    if f.tag != 0x11 {
        return Ok((wav.pcm, f.sample_rate));
    }
    if f.channels != 1 {
        return Err(FormatError {
            offset: 0,
            context: "wav.channels".into(),
            kind: FormatErrorKind::BadCount(i64::from(f.channels)),
        });
    }
    // Block size from the `fmt ` chunk (offset 32 of the usual layout).
    let align = data
        .get(32..34)
        .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
        .filter(|&a| a > 4)
        .unwrap_or(512);
    let mut out = Vec::with_capacity(wav.pcm.len() * 4);
    for block in wav.pcm.chunks(align) {
        if block.len() < 4 {
            break;
        }
        let mut pred = i32::from(i16::from_le_bytes([block[0], block[1]]));
        let mut index = i32::from(block[2]).clamp(0, 88);
        out.extend_from_slice(&(pred as i16).to_le_bytes());
        for &byte in &block[4..] {
            for nib in [byte & 15, byte >> 4] {
                let step = IMA_STEP[index as usize];
                let mut diff = step >> 3;
                if nib & 4 != 0 {
                    diff += step;
                }
                if nib & 2 != 0 {
                    diff += step >> 1;
                }
                if nib & 1 != 0 {
                    diff += step >> 2;
                }
                pred = if nib & 8 != 0 {
                    pred - diff
                } else {
                    pred + diff
                }
                .clamp(-32768, 32767);
                index = (index + IMA_INDEX[usize::from(nib & 7)]).clamp(0, 88);
                out.extend_from_slice(&(pred as i16).to_le_bytes());
            }
        }
    }
    Ok((out, f.sample_rate))
}

/// Faces drawn mostly on see-through texels (the mobile games blend a
/// muzzle flash by its texture's alpha) become what the PC uses for one:
/// colour-keyed (`sfOpacity`, black see-through) and double-sided.
fn mark_see_through(mesh: &mut Mesh, img: &Rgba) {
    use crate::model::face_flags::{DOUBLE_SIDE, OPACITY};
    let alpha = |u: i32, v: i32| {
        let (w, h) = (img.width.max(1), img.height.max(1));
        let x = (u.max(0) as usize * w / TEXTURE_WIDTH).min(w - 1);
        let y = (v.max(0) as usize * w / TEXTURE_WIDTH).min(h - 1);
        img.pixels.get((y * w + x) * 4 + 3).copied().unwrap_or(255)
    };
    for f in &mut mesh.faces {
        let cu = (f.tu[0] + f.tu[1] + f.tu[2]) / 3;
        let cv = (f.tv[0] + f.tv[1] + f.tv[2]) / 3;
        let samples = [
            alpha(cu, cv),
            alpha(f.tu[0], f.tv[0]),
            alpha(f.tu[1], f.tv[1]),
            alpha(f.tu[2], f.tv[2]),
        ];
        let mean = samples.iter().map(|&a| u32::from(a)).sum::<u32>() / 4;
        if mean < 160 {
            f.flags |= OPACITY | DOUBLE_SIDE;
        }
    }
}

/// Files a character is made of, as asked of the APK.
pub trait Assets {
    fn get(&self, path: &str) -> Option<Vec<u8>>;
}

/// The character `cat` (from `characters/<name>.cat`) as a `.CAR`: model,
/// texture, animations and sounds. Sounds at another rate than 22050 Hz
/// are kept as they are (the caller resamples if it must).
pub fn car_from(cat: &Cat, assets: &dyn Assets) -> Result<Car, FormatError> {
    car_from_in(cat, assets, "characters")
}

/// [`car_from`] for a character whose files are in `kind` folders
/// (`characters`, or `weapons` for the hunter's weapons).
pub fn car_from_in(cat: &Cat, assets: &dyn Assets, kind: &str) -> Result<Car, FormatError> {
    let missing = |what: String| FormatError {
        offset: 0,
        context: what,
        kind: FormatErrorKind::UnexpectedEof { needed: 0 },
    };
    let model_path = format!("assets/models/{kind}/{}", cat.model);
    let (name, mut mesh) = parse_3dn(&assets.get(&model_path).ok_or_else(|| missing(model_path))?)?;
    let stem = cat.model.trim_end_matches(".3dn");
    for ext in ["pkm", "crt", "tga"] {
        let p = format!("assets/textures/{kind}/{stem}.{ext}");
        if let Some(d) = assets.get(&p) {
            let img = decode_texture(&p, &d)?;
            mark_see_through(&mut mesh, &img);
            mesh.texture = car_texture(&img);
            break;
        }
    }
    let vc = mesh.vertices.len();
    let mut animations = Vec::new();
    let mut anim_sounds = [-1i32; ANIFX_SLOTS];
    for (i, (file, sound)) in cat.animations.iter().enumerate() {
        let p = format!("assets/animations/{kind}/{file}.ani");
        let d = assets.get(&p).ok_or_else(|| missing(p))?;
        animations.push(parse_ani(&d, file, vc)?);
        if let (Some(s), Some(slot)) = (sound, anim_sounds.get_mut(i)) {
            *slot = i32::try_from(*s).unwrap_or(-1);
        }
    }
    let mut sounds = Vec::new();
    for file in &cat.sounds {
        let p = format!("assets/sounds/{kind}/{file}.wav");
        let pcm = match assets.get(&p) {
            Some(d) => decode_wav(&d)?.0,
            None => Vec::new(),
        };
        let mut name_raw = [0u8; 32];
        for (d, s) in name_raw.iter_mut().zip(file.bytes().take(31)) {
            *d = s;
        }
        sounds.push(Sound { name_raw, pcm });
    }
    let mut name_raw = [0u8; 32];
    for (d, s) in name_raw.iter_mut().zip(name.bytes().take(23)) {
        *d = s;
    }
    Ok(Car {
        name_raw,
        mesh,
        animations,
        sounds,
        anim_sounds: Some(anim_sounds),
        trailing: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cat_lists_animations_and_sounds() {
        let t =
            "carnivores_character\r\n\r\n[general]\r\ntitle = \"Rex\"\r\nmodel = \"rex.3dn\"\r\n\
[animations]\r\ncount = 2\r\nanimation_id 0\r\n\tfile = \"rex_walk\"\r\n\tsound_index = 1\r\n\
animation_id 1\r\n\tfile = \"rex_die\"\r\n[sounds]\r\ncount = 2\r\nsound_id 0\r\n\tfile = \"a\"\r\n\
sound_id 1\r\n\tfile = \"b\"\r\n";
        let c = parse_cat(t).unwrap_or_default();
        assert_eq!((c.title.as_str(), c.model.as_str()), ("Rex", "rex.3dn"));
        assert_eq!(
            c.animations,
            vec![("rex_walk".into(), Some(1)), ("rex_die".into(), None)]
        );
        assert_eq!(c.sounds, vec!["a".to_string(), "b".to_string()]);
        assert!(parse_cat("hello").is_err());
    }

    fn model() -> Vec<u8> {
        let mut d = Vec::new();
        for v in [3u32, 1, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let mut name = [0u8; 32];
        name[..3].copy_from_slice(b"rex");
        d.extend_from_slice(&name);
        d.extend_from_slice(&0u32.to_le_bytes());
        for i in 0..3 {
            for f in [i as f32, 1.0, 2.0] {
                d.extend_from_slice(&f.to_le_bytes());
            }
            d.extend_from_slice(&[7, 0, 0, 0]);
        }
        for v in [0i32, 1, 2] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for (u, v) in [(10u16, 20u16), (30, 40), (50, 60)] {
            d.extend_from_slice(&u.to_le_bytes());
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&3u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        for v in [-1i32, 5, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&[0; 12]);
        d
    }

    #[test]
    fn model_reads_like_a_car_mesh() {
        let (name, m) = parse_3dn(&model()).unwrap_or_default();
        assert_eq!(name, "rex");
        assert_eq!(m.vertices.len(), 3);
        assert_eq!(m.vertices[2].x, 2.0);
        assert_eq!(m.vertices[0].owner, 7);
        let f = &m.faces[0];
        assert_eq!(
            (f.tu, f.tv, f.flags, f.next),
            ([10, 30, 50], [20, 40, 60], 3, 5)
        );
        // Truncated: an error, not a panic.
        assert!(parse_3dn(&model()[..60]).is_err());
    }

    #[test]
    fn animation_needs_the_models_vertex_count() {
        let mut d = Vec::new();
        for v in [20u32, 2, 3] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend((0..18i16).flat_map(|v| v.to_le_bytes()));
        let a = parse_ani(&d, "rex_walk", 3).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (a.kps, a.frames, a.data.len(), a.name()),
            (20, 2, 18, "rex_walk".into())
        );
        assert!(parse_ani(&d, "x", 4).is_err());
    }

    #[test]
    fn etc1_individual_and_differential_blocks() {
        // Individual mode, both halves grey 0x88, table 0: index 0 adds 2.
        let px = etc1_block(&[0x88, 0x88, 0x88, 0x00, 0, 0, 0, 0]);
        assert!(px.iter().all(|p| *p == [138, 138, 138]));
        // Differential, red 31 / delta -1 (left / right halves), msb set on
        // every pixel (index 2: subtract the small modifier, 2).
        let px = etc1_block(&[0xFF, 0, 0, 0x02, 0xFF, 0xFF, 0, 0]);
        assert_eq!(px[0], [253, 0, 0]);
        assert_eq!(px[3], [245, 0, 0]);
    }

    #[test]
    fn ima_adpcm_decodes_a_block() {
        let mut d = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        d.extend_from_slice(&20u32.to_le_bytes());
        for v in [0x11u16, 1] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&22050u32.to_le_bytes());
        d.extend_from_slice(&11025u32.to_le_bytes());
        d.extend_from_slice(&8u16.to_le_bytes()); // block align
        d.extend_from_slice(&4u16.to_le_bytes());
        d.extend_from_slice(&[2, 0, 13, 0]);
        d.extend_from_slice(b"data");
        d.extend_from_slice(&8u32.to_le_bytes());
        d.extend_from_slice(&[100, 0, 0, 0, 0x77, 0x77, 0x77, 0x77]);
        let (pcm, rate) = decode_wav(&d).unwrap_or_default();
        assert_eq!(rate, 22050);
        // Header sample, then 8 nibbles, rising.
        assert_eq!(pcm.len(), 9 * 2);
        let s: Vec<i16> = pcm
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(s[0], 100);
        assert!(s.windows(2).all(|w| w[1] > w[0]));
    }
}
