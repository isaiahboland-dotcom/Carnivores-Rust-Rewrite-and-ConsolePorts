//! `.CAR` animated characters (dinosaurs, hunter, weapons).
//! Loaded by `Resources.cpp: LoadCharacterInfo`.

use crate::error::FormatError;
use crate::model::{Face, Mesh, Vertex};
use crate::reader::{c_str, Reader, WriteLe};
use alloc::{string::String, vec::Vec};

/// Number of `Anifx` slots (`TCharacterInfo::Anifx[64]`).
pub const ANIFX_SLOTS: usize = 64;

/// One vertex-animation clip: `frames` keyframes of `(x, y, z)` `i16`
/// per vertex, played at `kps` keyframes per second.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name_raw: [u8; 32],
    pub kps: i32,
    pub frames: usize,
    /// `frames * vertex_count * 3` values, frame-major.
    pub data: Vec<i16>,
}

impl Animation {
    pub fn name(&self) -> String {
        c_str(&self.name_raw)
    }
}

/// Raw PCM sound embedded in the file (16-bit mono, no header).
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub name_raw: [u8; 32],
    pub pcm: Vec<u8>,
}

impl Sound {
    pub fn name(&self) -> String {
        c_str(&self.name_raw)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Car {
    /// 32-byte header name. Usually a 24-byte model name followed by an
    /// editor signature such as `"msc: 3"`.
    pub name_raw: [u8; 32],
    pub mesh: Mesh,
    pub animations: Vec<Animation>,
    pub sounds: Vec<Sound>,
    /// Sound index played by each animation, `-1` for none. `None` when the
    /// file ends before this table (the original then fills it with `-1`).
    pub anim_sounds: Option<[i32; ANIFX_SLOTS]>,
    /// Bytes after the table, which the original never reads (some
    /// modding tools leave a few); kept so a rewrite is byte-identical.
    pub trailing: Vec<u8>,
}

impl Car {
    pub fn name(&self) -> String {
        c_str(&self.name_raw[..24])
    }
}

pub fn parse(data: &[u8]) -> Result<Car, FormatError> {
    let mut r = Reader::new(data);
    let name_raw = r.array("car.name")?;
    let ani_count = r.count_of(40, "car.ani_count")?;
    let sfx_count = r.count_of(36, "car.sfx_count")?;
    let vc = r.count_of(Vertex::SIZE, "car.vcount")?;
    let fc = r.count_of(Face::SIZE, "car.fcount")?;
    let ts = r.count_of(1, "car.texture_size")?;
    let (faces, vertices) = Mesh::read_faces_vertices(&mut r, fc, vc)?;
    let texture = Mesh::read_texture(&mut r, ts)?;

    let mut animations = Vec::with_capacity(ani_count);
    for _ in 0..ani_count {
        let name_raw = r.array("car.anim.name")?;
        let kps = r.i32("car.anim.kps")?;
        let frames = r.count_of(vc.max(1) * 6, "car.anim.frames")?;
        let data = r.i16_vec(frames * vc * 3, "car.anim.data")?;
        animations.push(Animation {
            name_raw,
            kps,
            frames,
            data,
        });
    }

    let mut sounds = Vec::with_capacity(sfx_count);
    for _ in 0..sfx_count {
        let name_raw = r.array("car.sound.name")?;
        let len = r.count_of(1, "car.sound.length")?;
        let pcm = r.bytes(len, "car.sound.pcm")?.to_vec();
        sounds.push(Sound { name_raw, pcm });
    }

    // Resources.cpp: `ReadFile(hfile, chinfo.Anifx, 64*4, ...); if (l!=256) fill -1`.
    let anim_sounds = if r.remaining() >= ANIFX_SLOTS * 4 {
        let mut t = [0i32; ANIFX_SLOTS];
        for s in &mut t {
            *s = r.i32("car.anifx")?;
        }
        Some(t)
    } else {
        None
    };

    Ok(Car {
        name_raw,
        mesh: Mesh {
            vertices,
            faces,
            texture,
        },
        animations,
        sounds,
        anim_sounds,
        trailing: r.bytes(r.remaining(), "car.trailing")?.to_vec(),
    })
}

pub fn write(car: &Car) -> Vec<u8> {
    let mut w = Vec::new();
    w.extend_from_slice(&car.name_raw);
    w.put_len(car.animations.len());
    w.put_len(car.sounds.len());
    w.put_len(car.mesh.vertices.len());
    w.put_len(car.mesh.faces.len());
    w.put_len(car.mesh.texture.len() * 2);
    car.mesh.write_faces_vertices(&mut w);
    w.put_u16s(&car.mesh.texture);
    for a in &car.animations {
        w.extend_from_slice(&a.name_raw);
        w.put_i32(a.kps);
        w.put_len(a.frames);
        w.put_i16s(&a.data);
    }
    for s in &car.sounds {
        w.extend_from_slice(&s.name_raw);
        w.put_len(s.pcm.len());
        w.extend_from_slice(&s.pcm);
    }
    if let Some(t) = &car.anim_sounds {
        for v in t {
            w.put_i32(*v);
        }
    }
    w.extend_from_slice(&car.trailing);
    w
}
