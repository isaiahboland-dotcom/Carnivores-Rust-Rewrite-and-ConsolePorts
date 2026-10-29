//! `.RSC` area resources: terrain textures, objects, sky, fog, sounds, water.
//! Loaded by `Resources.cpp: LoadResources`.

use crate::error::FormatError;
use crate::model::StaticModel;
use crate::reader::{Reader, WriteLe};
use alloc::vec::Vec;

/// Terrain textures are 128x128, 16-bit (`LoadTexture`).
pub const TERRAIN_TEX_SIZE: usize = 128;
/// Object billboard sprites are 128x128, 16-bit (`LoadBMPModel`).
pub const SPRITE_SIZE: usize = 128;
/// Sky textures are 256x256, 16-bit, one per time of day (`LoadSky`).
pub const SKY_SIZE: usize = 256;
/// Sky cloud-shadow map is 128x128, 8-bit (`LoadSkyMap`).
pub const SKY_MAP_SIZE: usize = 128;
/// `TAmbient::rdata[16]`.
pub const AMBIENT_RANDOM_SLOTS: usize = 16;

/// Object flags (`of*` in `Hunt.h`).
pub mod object_flags {
    pub const PLACE_WATER: i32 = 1;
    pub const PLACE_GROUND: i32 = 2;
    pub const PLACE_USER: i32 = 4;
    pub const CIRCLE: i32 = 8;
    pub const BOUND: i32 = 16;
    pub const NO_BMP: i32 = 32;
    pub const NO_LIGHT: i32 = 64;
    pub const DEF_LIGHT: i32 = 128;
    pub const GROUND_LIGHT: i32 = 256;
    pub const NO_SOFT: i32 = 512;
    pub const NO_SOFT2: i32 = 1024;
    pub const ANIMATED: i32 = 0x8000_0000_u32 as i32;
}

/// `TObjInfo`, 64 bytes, as stored (before the engine's `*2` scaling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectInfo {
    pub radius: i32,
    pub y_lo: i32,
    pub y_hi: i32,
    pub line_length: i32,
    pub line_intensity: i32,
    pub circle_radius: i32,
    pub circle_intensity: i32,
    pub flags: i32,
    pub grass_radius: i32,
    pub default_light: i32,
    pub last_ani_time: i32,
    pub bound_r: f32,
    pub reserved: [u8; 16],
}

/// `TVTL`: per-object vertex animation. Stored `frames` is one less than
/// the number of keyframes in `data` (`LoadAnimation` does `FramesCount++`).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectAnimation {
    /// First of two vertex-count fields; the original reads and discards it.
    pub vcount_a: i32,
    pub vcount: usize,
    pub kps: i32,
    pub frames_stored: usize,
    /// `(frames_stored + 1) * vcount * 3` values.
    pub data: Vec<i16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub info: ObjectInfo,
    pub model: StaticModel,
    /// 128x128 billboard used at distance.
    pub sprite: Vec<u16>,
    pub animation: Option<ObjectAnimation>,
}

/// `TFogEntity`, 20 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    /// `0x00RRGGBB` as stored.
    pub rgb: i32,
    pub y_begin: f32,
    pub mortal: i32,
    pub transp: f32,
    pub f_limit: f32,
}

/// `TRD`, 16 bytes: one random sound attached to an ambient zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RandomSoundRef {
    pub number: i32,
    pub volume: i32,
    pub freq: i32,
    pub envir: u16,
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ambient {
    pub pcm: Vec<u8>,
    pub random: [RandomSoundRef; AMBIENT_RANDOM_SLOTS],
    pub random_count: i32,
    pub volume: i32,
}

/// `TWaterEntity`, 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Water {
    pub texture_index: i32,
    pub level: i32,
    pub transp: f32,
    /// Overwritten by the engine at load time.
    pub fog_rgb: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rsc {
    /// `FadeRGB[time_of_day][r,g,b]`: sky fade colour.
    pub fade_rgb: [[i32; 3]; 3],
    /// `TransRGB[time_of_day][r,g,b]`.
    pub trans_rgb: [[i32; 3]; 3],
    /// 128x128 16-bit textures.
    pub textures: Vec<Vec<u16>>,
    pub objects: Vec<Object>,
    /// One 256x256 sky per time of day.
    pub skies: [Vec<u16>; 3],
    pub sky_map: Vec<u8>,
    /// Fog zones `1..=n` (zone 0 is "no fog" and not stored).
    pub fogs: Vec<Fog>,
    /// Raw PCM random sounds.
    pub random_sounds: Vec<Vec<u8>>,
    pub ambients: Vec<Ambient>,
    pub waters: Vec<Water>,
}

fn rgb3x3(r: &mut Reader<'_>, ctx: &str) -> Result<[[i32; 3]; 3], FormatError> {
    let mut out = [[0; 3]; 3];
    for row in &mut out {
        for c in row {
            *c = r.i32(ctx)?;
        }
    }
    Ok(out)
}

fn read_object(r: &mut Reader<'_>) -> Result<Object, FormatError> {
    let info = ObjectInfo {
        radius: r.i32("rsc.obj.radius")?,
        y_lo: r.i32("rsc.obj.ylo")?,
        y_hi: r.i32("rsc.obj.yhi")?,
        line_length: r.i32("rsc.obj.linelength")?,
        line_intensity: r.i32("rsc.obj.lintensity")?,
        circle_radius: r.i32("rsc.obj.circlerad")?,
        circle_intensity: r.i32("rsc.obj.cintensity")?,
        flags: r.i32("rsc.obj.flags")?,
        grass_radius: r.i32("rsc.obj.grrad")?,
        default_light: r.i32("rsc.obj.deflight")?,
        last_ani_time: r.i32("rsc.obj.lastanitime")?,
        bound_r: r.f32("rsc.obj.boundr")?,
        reserved: r.array("rsc.obj.res")?,
    };
    let model = StaticModel::read(r)?;
    let sprite = r.u16_vec(SPRITE_SIZE * SPRITE_SIZE, "rsc.obj.sprite")?;
    let animation = if info.flags & object_flags::ANIMATED != 0 {
        let vcount_a = r.i32("rsc.obj.anim.vcount_a")?;
        let vcount = r.count_of(6, "rsc.obj.anim.vcount")?;
        let kps = r.i32("rsc.obj.anim.kps")?;
        let frames_stored = r.count_of(vcount.max(1) * 6, "rsc.obj.anim.frames")?;
        let data = r.i16_vec((frames_stored + 1) * vcount * 3, "rsc.obj.anim.data")?;
        Some(ObjectAnimation {
            vcount_a,
            vcount,
            kps,
            frames_stored,
            data,
        })
    } else {
        None
    };
    Ok(Object {
        info,
        model,
        sprite,
        animation,
    })
}

pub fn parse(data: &[u8]) -> Result<Rsc, FormatError> {
    let mut r = Reader::new(data);
    let tc = r.count_of(TERRAIN_TEX_SIZE * TERRAIN_TEX_SIZE * 2, "rsc.texture_count")?;
    let mc = r.count_of(64, "rsc.model_count")?;
    let fade_rgb = rgb3x3(&mut r, "rsc.fade_rgb")?;
    let trans_rgb = rgb3x3(&mut r, "rsc.trans_rgb")?;
    let textures = (0..tc)
        .map(|_| r.u16_vec(TERRAIN_TEX_SIZE * TERRAIN_TEX_SIZE, "rsc.texture"))
        .collect::<Result<_, _>>()?;
    let objects = (0..mc)
        .map(|_| read_object(&mut r))
        .collect::<Result<_, _>>()?;
    let skies = [
        r.u16_vec(SKY_SIZE * SKY_SIZE, "rsc.sky.day")?,
        r.u16_vec(SKY_SIZE * SKY_SIZE, "rsc.sky.dusk")?,
        r.u16_vec(SKY_SIZE * SKY_SIZE, "rsc.sky.night")?,
    ];
    let sky_map = r
        .bytes(SKY_MAP_SIZE * SKY_MAP_SIZE, "rsc.sky_map")?
        .to_vec();

    let fc = r.count_of(20, "rsc.fog_count")?;
    let fogs = (0..fc)
        .map(|_| {
            Ok(Fog {
                rgb: r.i32("rsc.fog.rgb")?,
                y_begin: r.f32("rsc.fog.ybegin")?,
                mortal: r.i32("rsc.fog.mortal")?,
                transp: r.f32("rsc.fog.transp")?,
                f_limit: r.f32("rsc.fog.flimit")?,
            })
        })
        .collect::<Result<_, FormatError>>()?;

    let rc = r.count_of(4, "rsc.random_sound_count")?;
    let random_sounds = (0..rc)
        .map(|_| {
            let n = r.count_of(1, "rsc.random_sound.length")?;
            Ok(r.bytes(n, "rsc.random_sound.pcm")?.to_vec())
        })
        .collect::<Result<_, FormatError>>()?;

    let ac = r.count_of(4 + AMBIENT_RANDOM_SLOTS * 16 + 8, "rsc.ambient_count")?;
    let mut ambients = Vec::with_capacity(ac);
    for _ in 0..ac {
        let n = r.count_of(1, "rsc.ambient.length")?;
        let pcm = r.bytes(n, "rsc.ambient.pcm")?.to_vec();
        let mut random = [RandomSoundRef::default(); AMBIENT_RANDOM_SLOTS];
        for slot in &mut random {
            *slot = RandomSoundRef {
                number: r.i32("rsc.ambient.rd.number")?,
                volume: r.i32("rsc.ambient.rd.volume")?,
                freq: r.i32("rsc.ambient.rd.freq")?,
                envir: r.u16("rsc.ambient.rd.envir")?,
                flags: r.u16("rsc.ambient.rd.flags")?,
            };
        }
        ambients.push(Ambient {
            pcm,
            random,
            random_count: r.i32("rsc.ambient.rsfx_count")?,
            volume: r.i32("rsc.ambient.volume")?,
        });
    }

    let wc = r.count_of(16, "rsc.water_count")?;
    let waters = (0..wc)
        .map(|_| {
            Ok(Water {
                texture_index: r.i32("rsc.water.tindex")?,
                level: r.i32("rsc.water.level")?,
                transp: r.f32("rsc.water.transp")?,
                fog_rgb: r.i32("rsc.water.fogrgb")?,
            })
        })
        .collect::<Result<_, FormatError>>()?;

    r.finish("rsc")?;
    Ok(Rsc {
        fade_rgb,
        trans_rgb,
        textures,
        objects,
        skies,
        sky_map,
        fogs,
        random_sounds,
        ambients,
        waters,
    })
}

pub fn write(rsc: &Rsc) -> Vec<u8> {
    let mut w = Vec::new();
    w.put_len(rsc.textures.len());
    w.put_len(rsc.objects.len());
    for row in rsc.fade_rgb.iter().chain(&rsc.trans_rgb) {
        for c in row {
            w.put_i32(*c);
        }
    }
    for t in &rsc.textures {
        w.put_u16s(t);
    }
    for o in &rsc.objects {
        let i = &o.info;
        for v in [
            i.radius,
            i.y_lo,
            i.y_hi,
            i.line_length,
            i.line_intensity,
            i.circle_radius,
            i.circle_intensity,
            i.flags,
            i.grass_radius,
            i.default_light,
            i.last_ani_time,
        ] {
            w.put_i32(v);
        }
        w.put_f32(i.bound_r);
        w.extend_from_slice(&i.reserved);
        o.model.write(&mut w);
        w.put_u16s(&o.sprite);
        if let Some(a) = &o.animation {
            w.put_i32(a.vcount_a);
            w.put_len(a.vcount);
            w.put_i32(a.kps);
            w.put_len(a.frames_stored);
            w.put_i16s(&a.data);
        }
    }
    for s in &rsc.skies {
        w.put_u16s(s);
    }
    w.extend_from_slice(&rsc.sky_map);
    w.put_len(rsc.fogs.len());
    for f in &rsc.fogs {
        w.put_i32(f.rgb);
        w.put_f32(f.y_begin);
        w.put_i32(f.mortal);
        w.put_f32(f.transp);
        w.put_f32(f.f_limit);
    }
    w.put_len(rsc.random_sounds.len());
    for s in &rsc.random_sounds {
        w.put_len(s.len());
        w.extend_from_slice(s);
    }
    w.put_len(rsc.ambients.len());
    for a in &rsc.ambients {
        w.put_len(a.pcm.len());
        w.extend_from_slice(&a.pcm);
        for rd in &a.random {
            w.put_i32(rd.number);
            w.put_i32(rd.volume);
            w.put_i32(rd.freq);
            w.extend_from_slice(&rd.envir.to_le_bytes());
            w.extend_from_slice(&rd.flags.to_le_bytes());
        }
        w.put_i32(a.random_count);
        w.put_i32(a.volume);
    }
    w.put_len(rsc.waters.len());
    for wt in &rsc.waters {
        w.put_i32(wt.texture_index);
        w.put_i32(wt.level);
        w.put_f32(wt.transp);
        w.put_i32(wt.fog_rgb);
    }
    w
}
