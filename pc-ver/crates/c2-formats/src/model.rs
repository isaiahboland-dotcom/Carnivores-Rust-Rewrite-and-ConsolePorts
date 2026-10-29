//! Mesh data shared by `.CAR`, `.3DF` and the models embedded in `.RSC`.
//!
//! Layouts follow `TPoint3d`, `TFace` and `TObj` in `Hunt.h`.

use crate::error::FormatError;
use crate::reader::{c_str, Reader, WriteLe};
use alloc::{string::String, vec::Vec};

/// Width in pixels of every model texture. Height is `bytes / 512`
/// (`Resources.cpp: LoadModel`, `TextureHeight = TextureSize >> 9`).
pub const TEXTURE_WIDTH: usize = 256;

/// `TPoint3d`, 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Index of the bone/object this vertex belongs to.
    pub owner: i16,
    pub hide: i16,
}

impl Vertex {
    pub const SIZE: usize = 16;

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        Ok(Self {
            x: r.f32("vertex.x")?,
            y: r.f32("vertex.y")?,
            z: r.f32("vertex.z")?,
            owner: r.i16("vertex.owner")?,
            hide: r.i16("vertex.hide")?,
        })
    }

    pub(crate) fn write(&self, w: &mut Vec<u8>) {
        w.put_f32(self.x);
        w.put_f32(self.y);
        w.put_f32(self.z);
        w.put_i16s(&[self.owner, self.hide]);
    }
}

/// Face flags (`sf*` in `Hunt.h`).
pub mod face_flags {
    pub const DOUBLE_SIDE: u16 = 0x0001;
    pub const DARK_BACK: u16 = 0x0002;
    pub const OPACITY: u16 = 0x0004;
    pub const TRANSPARENT: u16 = 0x0008;
    pub const MORTAL: u16 = 0x0010;
    pub const PHONG: u16 = 0x0030;
    pub const ENV_MAP: u16 = 0x0050;
    pub const NEED_VC: u16 = 0x0080;
    pub const DARK: u16 = 0x8000;
}

/// `TFace`, 64 bytes. Texture coordinates are stored on disk as integer
/// texel positions in the 256-wide model texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub v: [i32; 3],
    /// `[tax, tbx, tcx]`
    pub tu: [i32; 3],
    /// `[tay, tby, tcy]`
    pub tv: [i32; 3],
    pub flags: u16,
    pub dmask: u16,
    pub distant: i32,
    pub next: i32,
    pub group: i32,
    pub reserved: [u8; 12],
}

impl Face {
    pub const SIZE: usize = 64;

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        let mut i3 =
            |c: &str| -> Result<[i32; 3], FormatError> { Ok([r.i32(c)?, r.i32(c)?, r.i32(c)?]) };
        let v = i3("face.v")?;
        let tu = i3("face.tu")?;
        let tv = i3("face.tv")?;
        Ok(Self {
            v,
            tu,
            tv,
            flags: r.u16("face.flags")?,
            dmask: r.u16("face.dmask")?,
            distant: r.i32("face.distant")?,
            next: r.i32("face.next")?,
            group: r.i32("face.group")?,
            reserved: r.array("face.reserved")?,
        })
    }

    pub(crate) fn write(&self, w: &mut Vec<u8>) {
        for x in self.v.iter().chain(&self.tu).chain(&self.tv) {
            w.put_i32(*x);
        }
        w.extend_from_slice(&self.flags.to_le_bytes());
        w.extend_from_slice(&self.dmask.to_le_bytes());
        w.put_i32(self.distant);
        w.put_i32(self.next);
        w.put_i32(self.group);
        w.extend_from_slice(&self.reserved);
    }
}

/// `TObj`, 48 bytes: a named bone / sub-object of a `.3DF` model.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name_raw: [u8; 32],
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub owner: i16,
    pub hide: i16,
}

impl Bone {
    pub const SIZE: usize = 48;

    pub fn name(&self) -> String {
        c_str(&self.name_raw)
    }

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        Ok(Self {
            name_raw: r.array("bone.name")?,
            x: r.f32("bone.x")?,
            y: r.f32("bone.y")?,
            z: r.f32("bone.z")?,
            owner: r.i16("bone.owner")?,
            hide: r.i16("bone.hide")?,
        })
    }

    pub(crate) fn write(&self, w: &mut Vec<u8>) {
        w.extend_from_slice(&self.name_raw);
        w.put_f32(self.x);
        w.put_f32(self.y);
        w.put_f32(self.z);
        w.put_i16s(&[self.owner, self.hide]);
    }
}

/// Geometry plus its 16-bit (`xRRRRRGGGGGBBBBB`, 1-5-5-5) texture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
    /// Row-major pixels, [`TEXTURE_WIDTH`] per row.
    pub texture: Vec<u16>,
}

impl Mesh {
    pub fn texture_height(&self) -> usize {
        self.texture.len() / TEXTURE_WIDTH
    }

    pub(crate) fn read_faces_vertices(
        r: &mut Reader<'_>,
        faces: usize,
        vertices: usize,
    ) -> Result<(Vec<Face>, Vec<Vertex>), FormatError> {
        let f = (0..faces)
            .map(|_| Face::read(r))
            .collect::<Result<_, _>>()?;
        let v = (0..vertices)
            .map(|_| Vertex::read(r))
            .collect::<Result<_, _>>()?;
        Ok((f, v))
    }

    /// Texture size field is in bytes.
    pub(crate) fn read_texture(r: &mut Reader<'_>, bytes: usize) -> Result<Vec<u16>, FormatError> {
        r.u16_vec(bytes / 2, "texture")
    }

    pub(crate) fn texture_bytes(&self) -> usize {
        self.texture.len() * 2
    }

    pub(crate) fn write_faces_vertices(&self, w: &mut Vec<u8>) {
        for f in &self.faces {
            f.write(w);
        }
        for v in &self.vertices {
            v.write(w);
        }
    }
}

/// `.3DF`-style model block: `VCount, FCount, OCount, TextureSize`, faces,
/// vertices, bones, texture. Used by `.3DF` files and by `.RSC` objects
/// (`Resources.cpp: LoadModel` / `LoadModelEx`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StaticModel {
    pub mesh: Mesh,
    pub bones: Vec<Bone>,
}

impl StaticModel {
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        let vc = r.count_of(Vertex::SIZE, "model.vcount")?;
        let fc = r.count_of(Face::SIZE, "model.fcount")?;
        let oc = r.count_of(Bone::SIZE, "model.ocount")?;
        let ts = r.count_of(1, "model.texture_size")?;
        let (faces, vertices) = Mesh::read_faces_vertices(r, fc, vc)?;
        let bones = (0..oc).map(|_| Bone::read(r)).collect::<Result<_, _>>()?;
        let texture = Mesh::read_texture(r, ts)?;
        Ok(Self {
            mesh: Mesh {
                vertices,
                faces,
                texture,
            },
            bones,
        })
    }

    pub(crate) fn write(&self, w: &mut Vec<u8>) {
        w.put_len(self.mesh.vertices.len());
        w.put_len(self.mesh.faces.len());
        w.put_len(self.bones.len());
        w.put_len(self.mesh.texture_bytes());
        self.mesh.write_faces_vertices(w);
        for b in &self.bones {
            b.write(w);
        }
        w.put_u16s(&self.mesh.texture);
    }
}
