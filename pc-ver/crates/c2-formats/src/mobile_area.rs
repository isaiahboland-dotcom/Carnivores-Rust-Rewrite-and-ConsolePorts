//! The mobile games' areas (Carnivores: Dinosaur Hunter and Ice Age for
//! Android, from an APK the user supplies; read locally, never
//! redistributed), rebuilt as a PC `.MAP` and `.RSC`. Layouts were worked out
//! from the files and the game's loader (`Terrain::Load`,
//! `FilesManagerAndroid::ReadCompressed`); the mobile Dinosaur Hunter's first
//! three areas are the PC's, layer for layer.
//!
//! - `areas/<name>.man`: 1024x1024 byte grids. Raw: height, the three light
//!   maps (in the PC's order), the flags' low byte; then a raw 512x512 grid
//!   (mobile only); then run-length grids (`u32` byte count, then
//!   `(value, run)` byte pairs): texture, a mobile-only 512x512 grid, objects,
//!   object heights, water, fog zones (512x512), ambient zones (512x512). No
//!   second texture and no flags' high byte (the PC leaves both unused).
//! - `areas/<name>.rst`: text (`carnivores_area_resources`): `[section]`s of
//!   `key = value` lines and `<kind>_id N` items, nested by tabs.
//! - `models/objects/<model>.3dn`: `u32 VCount, FCount, OCount; char
//!   name[32]; u32 sprites;` `sprites x char[32]`, then the PC's vertices,
//!   faces and `OCount` bones.

use crate::error::{FormatError, FormatErrorKind};
use crate::map::{Map, CELLS, HALF_CELLS};
use crate::mobile::{decode_texture, decode_wav, read_mesh, Assets, Rgba};
use crate::model::{Bone, StaticModel, TEXTURE_WIDTH};
use crate::reader::Reader;
use crate::rsc::{
    object_flags, Ambient, Fog, Object, ObjectInfo, RandomSoundRef, Rsc, Water,
    AMBIENT_RANDOM_SLOTS, SKY_MAP_SIZE, SKY_SIZE, SPRITE_SIZE, TERRAIN_TEX_SIZE,
};
use alloc::{format, string::String, string::ToString, vec, vec::Vec};

fn syntax(what: &str) -> FormatError {
    FormatError {
        offset: 0,
        context: what.into(),
        kind: FormatErrorKind::Syntax("not a carnivores_area_resources file".into()),
    }
}

fn missing(what: String) -> FormatError {
    FormatError {
        offset: 0,
        context: what,
        kind: FormatErrorKind::UnexpectedEof { needed: 0 },
    }
}

/// One run-length grid of `cells` bytes.
fn rle(r: &mut Reader<'_>, cells: usize, ctx: &str) -> Result<Vec<u8>, FormatError> {
    let n = r.i32(ctx)?;
    let n = usize::try_from(n)
        .ok()
        .filter(|&n| n <= r.remaining())
        .ok_or_else(|| r.err(ctx, FormatErrorKind::BadCount(i64::from(n))))?;
    let pairs = r.bytes(n, ctx)?;
    let mut out = Vec::with_capacity(cells);
    for p in pairs.as_chunks::<2>().0 {
        let run = usize::from(p[1]).min(cells - out.len());
        out.resize(out.len() + run, p[0]);
    }
    if out.len() != cells {
        return Err(r.err(ctx, FormatErrorKind::BadCount(out.len() as i64)));
    }
    Ok(out)
}

/// A `.man` as a PC map. The second texture is the first (the PC draws
/// neither from it); the flags' high byte is clear.
pub fn parse_man(data: &[u8]) -> Result<Map, FormatError> {
    read_man(data).map(|(map, _, _)| map)
}

fn read_man(data: &[u8]) -> Result<(Map, Vec<u8>, Vec<u8>), FormatError> {
    let mut r = Reader::new(data);
    let mut raw = |ctx: &str| r.bytes(CELLS, ctx).map(<[u8]>::to_vec);
    let height = raw("man.height")?;
    let light = [raw("man.light0")?, raw("man.light1")?, raw("man.light2")?];
    let flags = raw("man.flags")?;
    let lod_flags = r.bytes(HALF_CELLS, "man.lod_flags")?.to_vec();
    let texture1: Vec<u16> = rle(&mut r, CELLS, "man.texture")?
        .into_iter()
        .map(u16::from)
        .collect();
    let lod_tiles = rle(&mut r, HALF_CELLS, "man.lod_tiles")?;
    let objects = rle(&mut r, CELLS, "man.objects")?;
    let object_height = rle(&mut r, CELLS, "man.object_height")?;
    let water = rle(&mut r, CELLS, "man.water")?;
    let fogs = rle(&mut r, HALF_CELLS, "man.fogs")?;
    let ambient = rle(&mut r, HALF_CELLS, "man.ambient")?;
    if r.remaining() != 0 {
        return Err(r.err("man", FormatErrorKind::TrailingData(r.remaining())));
    }
    Ok((
        Map {
            height,
            texture2: texture1.clone(),
            texture1,
            objects,
            flags: flags.into_iter().map(u16::from).collect(),
            light,
            water,
            object_height,
            fogs,
            ambient,
        },
        lod_flags,
        lod_tiles,
    ))
}

/// Convert both area files together: second-LOD maps use one tile across
/// four PC cells, with rotations from the separate half-resolution grid.
pub fn area_from(data: &[u8], rst: &Rst, assets: &dyn Assets) -> Result<(Map, Rsc), FormatError> {
    let (mut map, flags, tiles) = read_man(data)?;
    let mut rsc = rsc_from(rst, assets)?;
    if rst
        .section("general")
        .is_some_and(|g| g.flag("use_second_lod"))
    {
        expand_second_lod(&mut map, &mut rsc, &flags, &tiles)?;
    }
    Ok((map, rsc))
}

fn expand_second_lod(
    map: &mut Map,
    rsc: &mut Rsc,
    flags: &[u8],
    tiles: &[u8],
) -> Result<(), FormatError> {
    let base = rsc.textures.len();
    // Leave original tiles in place: water definitions still reference them.
    for index in 0..base {
        for qy in 0..2 {
            for qx in 0..2 {
                let mut tile = Vec::with_capacity(TERRAIN_TEX_SIZE * TERRAIN_TEX_SIZE);
                for y in 0..TERRAIN_TEX_SIZE {
                    for x in 0..TERRAIN_TEX_SIZE {
                        let sx = qx * TERRAIN_TEX_SIZE / 2 + x / 2;
                        let sy = qy * TERRAIN_TEX_SIZE / 2 + y / 2;
                        tile.push(rsc.textures[index][sy * TERRAIN_TEX_SIZE + sx]);
                    }
                }
                rsc.textures.push(tile);
            }
        }
    }
    for y in 0..1024 {
        for x in 0..1024 {
            let i = y * 1024 + x;
            let li = (y / 2) * 512 + x / 2;
            let tile = usize::from(tiles[li]);
            if tile >= base {
                return Err(syntax("second LOD tile outside atlas"));
            }
            let dir = u16::from(flags[li] & 3);
            let (qx, qy) = (x % 2, y % 2);
            let (qx, qy) = match dir {
                1 => (qy, 1 - qx),
                2 => (1 - qx, 1 - qy),
                3 => (1 - qy, qx),
                _ => (qx, qy),
            };
            map.texture1[i] = (base + tile * 4 + qy * 2 + qx) as u16;
            map.texture2[i] = map.texture1[i];
            map.flags[i] = (map.flags[i] & !3) | dir;
        }
    }
    Ok(())
}

/// A `key = value` line's value: quotes dropped.
fn unquote(v: &str) -> &str {
    v.trim().trim_matches('"')
}

/// An `.rst` item (`object_id 3`) or section: its lines and items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RstBlock {
    /// `object` for `object_id`; the section's name for a section.
    pub kind: String,
    pub id: usize,
    pub entries: Vec<(String, String)>,
    pub items: Vec<RstBlock>,
}

impl RstBlock {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| unquote(v))
    }

    pub fn num(&self, key: &str) -> Option<f32> {
        self.get(key)?.parse().ok()
    }

    pub fn int(&self, key: &str) -> Option<i32> {
        self.num(key).map(|v| v as i32)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.int(key).is_some_and(|v| v != 0)
    }

    /// `r, g, b`.
    pub fn rgb(&self, key: &str) -> Option<[i32; 3]> {
        let mut it = self.get(key)?.split(',').map(|c| c.trim().parse().ok());
        Some([it.next()??, it.next()??, it.next()??])
    }
}

/// An `.rst` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rst {
    pub sections: Vec<RstBlock>,
}

impl Rst {
    pub fn section(&self, name: &str) -> Option<&RstBlock> {
        self.sections
            .iter()
            .find(|s| s.kind.eq_ignore_ascii_case(name))
    }

    /// A section's items (none if it is missing).
    pub fn items(&self, name: &str) -> &[RstBlock] {
        self.section(name).map_or(&[], |s| &s.items)
    }
}

pub fn parse_rst(text: &str) -> Result<Rst, FormatError> {
    if !text.trim_start().starts_with("carnivores_area_resources") {
        return Err(syntax("rst"));
    }
    let mut rst = Rst::default();
    // The open items, outermost first, with their depth in tabs.
    let mut open: Vec<(usize, RstBlock)> = Vec::new();
    let close_to = |open: &mut Vec<(usize, RstBlock)>, rst: &mut Rst, depth: usize| {
        while open.last().is_some_and(|(d, _)| *d >= depth) {
            let Some((_, b)) = open.pop() else { break };
            match open.last_mut() {
                Some((_, parent)) => parent.items.push(b),
                None => {
                    if let Some(s) = rst.sections.last_mut() {
                        s.items.push(b);
                    }
                }
            }
        }
    };
    for line in text.lines().skip(1) {
        let depth = line.chars().take_while(|&c| c == '\t').count();
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Some(name) = l.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
            close_to(&mut open, &mut rst, 0);
            rst.sections.push(RstBlock {
                kind: name.trim().to_string(),
                ..RstBlock::default()
            });
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            // A line belongs to the innermost open item shallower than it.
            close_to(&mut open, &mut rst, depth);
            let entry = (k.trim().to_string(), v.trim().to_string());
            match open.last_mut() {
                Some((_, b)) => b.entries.push(entry),
                None => match rst.sections.last_mut() {
                    Some(s) => s.entries.push(entry),
                    None => return Err(syntax("rst: a line before any [section]")),
                },
            }
            continue;
        }
        let item = l.split_once(char::is_whitespace).and_then(|(kind, id)| {
            Some((kind.strip_suffix("_id")?, id.trim().parse::<usize>().ok()?))
        });
        let Some((kind, id)) = item else {
            return Err(syntax("rst: not a line, item or section"));
        };
        close_to(&mut open, &mut rst, depth);
        open.push((
            depth,
            RstBlock {
                kind: kind.to_string(),
                id,
                ..RstBlock::default()
            },
        ));
    }
    close_to(&mut open, &mut rst, 0);
    Ok(rst)
}

/// An object's `.3dn`: its model (untextured), its name (its texture's:
/// objects share textures) and its sprite's name, if any.
pub fn parse_object(data: &[u8]) -> Result<(StaticModel, String, Option<String>), FormatError> {
    let mut r = Reader::new(data);
    let vc = r.i32("3dn.vcount")?;
    let fc = r.i32("3dn.fcount")?;
    let oc = r.i32("3dn.ocount")?;
    let name = crate::reader::c_str(r.bytes(32, "3dn.name")?);
    let sprites = r.i32("3dn.sprites")?;
    let mut sprite = None;
    for _ in 0..sprites.clamp(0, 16) {
        let s = crate::reader::c_str(r.bytes(32, "3dn.sprite")?);
        sprite.get_or_insert(s);
    }
    let mesh = read_mesh(&mut r, vc, fc)?;
    let oc = usize::try_from(oc)
        .ok()
        .filter(|&n| {
            n.checked_mul(Bone::SIZE)
                .is_some_and(|b| b <= r.remaining())
        })
        .ok_or_else(|| r.err("3dn.ocount", FormatErrorKind::BadCount(i64::from(oc))))?;
    let bones = (0..oc)
        .map(|_| Bone::read(&mut r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((StaticModel { mesh, bones }, name, sprite))
}

/// RGBA as the PC's `RGB555`; see-through texels (alpha under half) as 0,
/// the PC's colour key.
fn rgb555(p: &[u8]) -> u16 {
    if p.get(3).is_some_and(|&a| a < 128) {
        return 0;
    }
    let c = |i: usize| u16::from(p.get(i).copied().unwrap_or(0) >> 3);
    // A kept texel never reads as the key.
    ((c(0) << 10) | (c(1) << 5) | c(2)).max(1)
}

/// The `w` x `h` image at (`x0`, `y0`) of `img` scaled to `size` square,
/// bilinear (see-through texels keep the key: nearest for alpha).
fn crop_scaled(img: &Rgba, x0: usize, y0: usize, w: usize, h: usize, size: usize) -> Vec<u16> {
    let (iw, ih) = (img.width.max(1), img.height.max(1));
    let px = |x: usize, y: usize| -> [u8; 4] {
        let i = (y.min(ih - 1) * iw + x.min(iw - 1)) * 4;
        let p = img.pixels.get(i..i + 4).unwrap_or(&[0, 0, 0, 0]);
        [p[0], p[1], p[2], p[3]]
    };
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            // Texel centres, in source pixels.
            let fx = ((x as f32 + 0.5) * w as f32 / size as f32 - 0.5).max(0.0);
            let fy = ((y as f32 + 0.5) * h as f32 / size as f32 - 0.5).max(0.0);
            let (ix, iy) = (fx as usize, fy as usize);
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let (x1, y1) = ((ix + 1).min(w - 1), (iy + 1).min(h - 1));
            let q = [
                px(x0 + ix, y0 + iy),
                px(x0 + x1, y0 + iy),
                px(x0 + ix, y0 + y1),
                px(x0 + x1, y0 + y1),
            ];
            let mut c = [0u8; 4];
            for (k, ck) in c.iter_mut().enumerate().take(3) {
                let top = f32::from(q[0][k]) * (1.0 - tx) + f32::from(q[1][k]) * tx;
                let bot = f32::from(q[2][k]) * (1.0 - tx) + f32::from(q[3][k]) * tx;
                *ck = (top * (1.0 - ty) + bot * ty + 0.5) as u8;
            }
            let near = q[usize::from(tx >= 0.5) + 2 * usize::from(ty >= 0.5)];
            c[3] = near[3];
            out.push(rgb555(&c));
        }
    }
    out
}

/// A texture under `assets/textures/<dir>/<stem>`, whichever of the
/// mobile formats it is in.
fn texture(assets: &dyn Assets, dir: &str, stem: &str) -> Option<Result<Rgba, FormatError>> {
    ["pkm", "crt", "tga"].iter().find_map(|ext| {
        let p = format!("assets/textures/{dir}/{stem}.{ext}");
        assets.get(&p).map(|d| decode_texture(&p, &d))
    })
}

/// The terrain atlas's tiles (row by row), each scaled to the PC's 128 and
/// transposed: the atlas keeps each tile with x and y swapped from the
/// PC's (checked on every patterned tile of both APKs' first areas against
/// the PC games' own, which share their maps cell for cell; untransposed,
/// the edge tiles between snow and ice pointed the wrong way, as squares
/// along the edges, owner 2026-11-01).
fn tiles(rst: &Rst, assets: &dyn Assets) -> Result<Vec<Vec<u16>>, FormatError> {
    let t = rst
        .section("tiles")
        .ok_or_else(|| syntax("rst: no [tiles]"))?;
    let name = t.get("atlas_texture").unwrap_or_default();
    let img = texture(assets, "tiles", name).ok_or_else(|| missing(format!("tiles/{name}")))??;
    let cols = t.int("atlas_tiles_in_row").unwrap_or(8).max(1) as usize;
    let rows = t.int("atlas_tiles_in_column").unwrap_or(8).max(1) as usize;
    let (tw, th) = ((img.width / cols).max(1), (img.height / rows).max(1));
    let mut out = Vec::with_capacity(cols * rows);
    for ty in 0..rows {
        for tx in 0..cols {
            let t = crop_scaled(&img, tx * tw, ty * th, tw, th, TERRAIN_TEX_SIZE);
            out.push(transposed(&t, TERRAIN_TEX_SIZE));
        }
    }
    Ok(out)
}

/// A `size` square texture with x and y swapped.
fn transposed(t: &[u16], size: usize) -> Vec<u16> {
    (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            t.get(x * size + y).copied().unwrap_or(0)
        })
        .collect()
}

/// The PC's object flags and lighting from an `[objects]` item.
fn object_info(o: &RstBlock) -> ObjectInfo {
    use object_flags as f;
    let half = |k: &str| o.int(k).unwrap_or(0) / 2;
    let mut flags = f::PLACE_GROUND;
    if o.flag("radial_collision_detection") {
        flags |= f::CIRCLE;
    }
    if o.flag("face_collision_detection") {
        flags |= f::BOUND;
    }
    if !o.flag("use_lod") {
        flags |= f::NO_BMP;
    }
    if o.flag("static_light") {
        flags |= f::GROUND_LIGHT;
    }
    let mut default_light = 128;
    if o.flag("use_custom_light") {
        flags |= f::DEF_LIGHT;
        default_light = o.int("custom_light_intensity").unwrap_or(128);
    }
    ObjectInfo {
        radius: half("bound_radius"),
        y_lo: half("bound_bottom"),
        y_hi: half("bound_top"),
        line_length: half("shadow_line_length"),
        line_intensity: o.int("shadow_line_intensity").unwrap_or(16),
        circle_radius: o.int("shadow_circle_radius").unwrap_or(0),
        circle_intensity: o.int("shadow_circle_intensity").unwrap_or(16),
        flags,
        grass_radius: 0,
        default_light,
        last_ani_time: 0,
        bound_r: 0.0,
        reserved: [0; 16],
    }
}

/// The sprite at `index` in the sprites atlas (128 square cells).
fn sprite(atlas: Option<&Rgba>, index: Option<i32>) -> Vec<u16> {
    let (Some(img), Some(i)) = (atlas, index.and_then(|i| usize::try_from(i).ok())) else {
        return vec![0; SPRITE_SIZE * SPRITE_SIZE];
    };
    let cols = (img.width / SPRITE_SIZE).max(1);
    let (x, y) = (i % cols * SPRITE_SIZE, i / cols * SPRITE_SIZE);
    crop_scaled(img, x, y, SPRITE_SIZE, SPRITE_SIZE, SPRITE_SIZE)
}

fn object(o: &RstBlock, assets: &dyn Assets, atlas: Option<&Rgba>) -> Result<Object, FormatError> {
    // A disabled slot (no model) stays, empty, so the map's numbers hold.
    let Some(name) = o.get("model").filter(|_| !o.flag("disabled")) else {
        return Ok(Object {
            info: ObjectInfo {
                flags: object_flags::NO_BMP,
                ..object_info(o)
            },
            model: StaticModel {
                mesh: crate::model::Mesh {
                    vertices: Vec::new(),
                    faces: Vec::new(),
                    texture: Vec::new(),
                },
                bones: Vec::new(),
            },
            sprite: vec![0; SPRITE_SIZE * SPRITE_SIZE],
            animation: None,
        });
    };
    let p = format!("assets/models/objects/{name}.3dn");
    let (mut model, tex, _) = parse_object(&assets.get(&p).ok_or_else(|| missing(p))?)?;
    let img = match texture(assets, "objects", &tex) {
        Some(t) => Some(t?),
        None => texture(assets, "objects", name).transpose()?,
    };
    if let Some(img) = img {
        // The mobile games blend by alpha; the PC keys only faces flagged.
        crate::mobile::mark_see_through(&mut model.mesh, &img);
        // 256 wide, as many rows as keep the texels square.
        let rows = img.height * TEXTURE_WIDTH / img.width.max(1);
        let mut t = Vec::with_capacity(rows * TEXTURE_WIDTH);
        for y in 0..rows {
            for x in 0..TEXTURE_WIDTH {
                let (sx, sy) = (x * img.width / TEXTURE_WIDTH, y * img.height / rows.max(1));
                let i = (sy * img.width + sx) * 4;
                t.push(rgb555(img.pixels.get(i..i + 4).unwrap_or(&[0; 4])));
            }
        }
        model.mesh.texture = t;
    }
    let use_lod = o.flag("use_lod");
    Ok(Object {
        info: object_info(o),
        model,
        sprite: sprite(atlas.filter(|_| use_lod), o.int("lod_sprite_index")),
        animation: None,
    })
}

/// A sound under `assets/sounds/ambients/` as the PC's 16-bit PCM.
fn ambient_sound(assets: &dyn Assets, file: &str) -> Result<Vec<u8>, FormatError> {
    let p = format!("assets/sounds/ambients/{file}.wav");
    match assets.get(&p) {
        Some(d) => Ok(decode_wav(&d)?.0),
        None => Ok(Vec::new()),
    }
}

/// `r, g, b` as the PC's `0x00BBGGRR`.
fn bgr(c: [i32; 3]) -> i32 {
    c[0] | (c[1] << 8) | (c[2] << 16)
}

/// The PC's time-of-day order: dawn, day, night.
const TIMES: [&str; 3] = ["dawn", "day", "night"];

/// An area's `.rst` and its pieces from the APK as a PC `.RSC`.
pub fn rsc_from(rst: &Rst, assets: &dyn Assets) -> Result<Rsc, FormatError> {
    let sky = rst.section("sky").cloned().unwrap_or_default();
    let colour = |t: &str, k: &str| sky.rgb(&format!("{t}_{k}")).unwrap_or([128, 128, 128]);
    let fade_rgb = TIMES.map(|t| colour(t, "sky_color"));
    let skies = TIMES.map(|t| {
        let name = sky
            .get(&format!("{t}_sky_plane_texture"))
            .unwrap_or_default();
        let c = colour(t, "sky_color").map(|v| v.clamp(0, 255) as u8);
        match texture(assets, "sky", name) {
            // The mobile games lay the plane over the sky's colour by its
            // alpha; the PC's sky is opaque.
            Some(Ok(mut img)) => {
                for p in img.pixels.as_chunks_mut::<4>().0 {
                    let a = u16::from(p[3]);
                    for k in 0..3 {
                        p[k] = ((u16::from(p[k]) * a + u16::from(c[k]) * (255 - a)) / 255) as u8;
                    }
                    p[3] = 255;
                }
                crop_scaled(&img, 0, 0, img.width, img.height, SKY_SIZE)
            }
            _ => vec![rgb555(&[c[0], c[1], c[2], 255]); SKY_SIZE * SKY_SIZE],
        }
    });
    let clouds = sky.get("day_clouds_shadows_texture").unwrap_or_default();
    let sky_map = assets
        .get(&format!("assets/textures/sky/{clouds}.raw"))
        .filter(|d| d.len() == SKY_MAP_SIZE * SKY_MAP_SIZE)
        .unwrap_or_else(|| vec![0; SKY_MAP_SIZE * SKY_MAP_SIZE]);

    let atlas_name = rst
        .section("tiles")
        .and_then(|t| t.get("sprites_atlas_texture"))
        .unwrap_or_default();
    let atlas = texture(assets, "objects", atlas_name).transpose()?;
    let objects = rst
        .items("objects")
        .iter()
        .map(|o| object(o, assets, atlas.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;

    let fogs = rst
        .items("fog")
        .iter()
        // Zone 0 is "no fog" and not stored.
        .skip(1)
        .map(|f| Fog {
            rgb: bgr(f.rgb("color").unwrap_or([127, 127, 127])),
            y_begin: f.num("altitude").unwrap_or(0.0),
            mortal: f.int("poisonous").unwrap_or(0),
            transp: f.num("distance").unwrap_or(512.0),
            f_limit: f.num("density").unwrap_or(200.0),
        })
        .collect();
    let waters = rst
        .items("water")
        .iter()
        .map(|w| Water {
            texture_index: w.int("tile_index").unwrap_or(0),
            level: w.int("level").unwrap_or(0),
            transp: w.num("opacity").unwrap_or(1.0),
            fog_rgb: 0,
        })
        .collect();
    let random_sounds = rst
        .items("sounds")
        .iter()
        .map(|s| ambient_sound(assets, s.get("file").unwrap_or_default()))
        .collect::<Result<Vec<_>, _>>()?;
    // Rounded to the nearest (no `f32::round` without std).
    let level = |v: f32| {
        let x = v * 255.0;
        (if x < 0.0 { x - 0.5 } else { x + 0.5 }) as i32
    };
    let ambients = rst
        .items("ambients")
        .iter()
        .map(|a| {
            let mut random = [RandomSoundRef::default(); AMBIENT_RANDOM_SLOTS];
            let refs = a.items.iter().take(AMBIENT_RANDOM_SLOTS);
            for (slot, r) in random.iter_mut().zip(refs.clone()) {
                *slot = RandomSoundRef {
                    number: r.int("random_sound_index").unwrap_or(0),
                    volume: level(r.num("random_sound_volume").unwrap_or(0.25)),
                    freq: r.int("random_sound_frequency").unwrap_or(5),
                    envir: 0,
                    flags: u16::from(r.flag("random_sound_daytime_only")),
                };
            }
            Ok(Ambient {
                pcm: ambient_sound(assets, a.get("file").unwrap_or_default())?,
                random,
                random_count: refs.count() as i32,
                volume: level(a.num("volume").unwrap_or(0.5)),
            })
        })
        .collect::<Result<Vec<_>, FormatError>>()?;
    Ok(Rsc {
        fade_rgb,
        trans_rgb: fade_rgb,
        textures: tiles(rst, assets)?,
        objects,
        skies,
        sky_map,
        fogs,
        random_sounds,
        ambients,
        waters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rst_nests_items_by_tabs() {
        let t = "carnivores_area_resources\r\n[general]\r\ndetailmap_texture = \"d\"\r\n\
[ambients]\r\ncount = 1\r\n\r\nambient_id 0\r\n\tfile = \"a0\"\r\n\trandom_sounds_count = 2\r\n\r\n\
\trandom_sound_id 0\r\n\t\trandom_sound_index = 3\r\n\r\n\trandom_sound_id 1\r\n\t\t\
random_sound_index = 7\r\n\tvolume = 0.5\r\n[fog]\r\ncount = 1\r\nfog_id 0\r\n\tcolor = 79, 77, 56\r\n";
        let r = parse_rst(t).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            r.section("general")
                .and_then(|g| g.get("detailmap_texture")),
            Some("d")
        );
        let a = &r.items("ambients")[0];
        assert_eq!(a.get("file"), Some("a0"));
        assert_eq!(a.num("volume"), Some(0.5));
        let idx: Vec<_> = a
            .items
            .iter()
            .map(|i| i.int("random_sound_index"))
            .collect();
        assert_eq!(idx, [Some(3), Some(7)]);
        let f = &r.items("fog")[0];
        assert_eq!(f.rgb("color").map(bgr), Some(0x00384D4F));
    }

    #[test]
    fn second_lod_preserves_quadrants_rotations_and_water_tiles() {
        let mut map = crate::map::parse(&alloc::vec![0; crate::map::FILE_SIZE])
            .unwrap_or_else(|e| panic!("{e}"));
        map.flags.fill(0x30);
        let tile: Vec<u16> = (0..128 * 128)
            .map(|i| (i / 128 / 64 * 2 + i % 128 / 64) as u16)
            .collect();
        let mut rsc = Rsc {
            fade_rgb: [[0; 3]; 3],
            trans_rgb: [[0; 3]; 3],
            textures: alloc::vec![tile.clone()],
            objects: Vec::new(),
            skies: Default::default(),
            sky_map: Vec::new(),
            fogs: Vec::new(),
            random_sounds: Vec::new(),
            ambients: Vec::new(),
            waters: Vec::new(),
        };
        let mut flags = alloc::vec![0; HALF_CELLS];
        flags[..4].copy_from_slice(&[0, 1, 2, 3]);
        expand_second_lod(&mut map, &mut rsc, &flags, &alloc::vec![0; HALF_CELLS])
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(rsc.textures[0], tile, "water's original tile stays intact");
        for q in 0..4 {
            assert!(rsc.textures[1 + q].iter().all(|&v| v == q as u16));
        }
        for (dir, quadrants) in [[0, 1, 2, 3], [2, 0, 3, 1], [3, 2, 1, 0], [1, 3, 0, 2]]
            .iter()
            .enumerate()
        {
            for (q, &expected) in quadrants.iter().enumerate() {
                let i = q / 2 * 1024 + dir * 2 + q % 2;
                assert_eq!(map.texture1[i], 1 + expected);
                assert_eq!(map.flags[i], 0x30 | dir as u16);
            }
        }
    }

    #[test]
    fn man_layers_and_runs() {
        let mut d = Vec::new();
        for v in 1..=5u8 {
            d.extend(core::iter::repeat_n(v, CELLS));
        }
        d.extend(core::iter::repeat_n(0, HALF_CELLS));
        let mut run = |v: u8, cells: usize| {
            let pairs: Vec<u8> = (0..cells.div_ceil(255))
                .flat_map(|i| [v, (cells - i * 255).min(255) as u8])
                .collect();
            d.extend((pairs.len() as u32).to_le_bytes());
            d.extend(pairs);
        };
        run(9, CELLS);
        run(0, HALF_CELLS);
        run(255, CELLS);
        run(6, CELLS);
        run(255, CELLS);
        run(2, HALF_CELLS);
        run(3, HALF_CELLS);
        let m = parse_man(&d).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((m.height[7], m.light[2][9], m.flags[0]), (1, 4, 5));
        assert_eq!(
            (m.texture1[CELLS - 1], m.objects[3], m.object_height[0]),
            (9, 255, 6)
        );
        assert_eq!(
            (m.fogs.len(), m.fogs[0], m.ambient[HALF_CELLS - 1]),
            (HALF_CELLS, 2, 3)
        );
        assert!(parse_man(&d[..d.len() - 1]).is_err());
    }
}
