//! `.MAP` area terrain: fixed 1024x1024 grids.
//! Loaded by `Resources.cpp: LoadResources` ("Load MAPs file").

use crate::error::FormatError;
use crate::reader::{Reader, WriteLe};
use alloc::vec::Vec;

pub const MAP_SIZE: usize = 1024;
pub const CELLS: usize = MAP_SIZE * MAP_SIZE;
pub const HALF_SIZE: usize = 512;
pub const HALF_CELLS: usize = HALF_SIZE * HALF_SIZE;
/// Total size of a `.MAP` file in bytes.
pub const FILE_SIZE: usize = CELLS * 13 + HALF_CELLS * 2;

/// All grids are row-major (`[y][x]` in the original).
#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    /// `HMap`: terrain height, scaled by `ctHScale` (64) in-engine.
    pub height: Vec<u8>,
    /// `TMap1`: texture index per cell.
    pub texture1: Vec<u16>,
    /// `TMap2`: secondary texture index per cell.
    pub texture2: Vec<u16>,
    /// `OMap`: object (RSC model) index per cell, 255 = none.
    pub objects: Vec<u8>,
    /// `FMap`: per-cell flags.
    pub flags: Vec<u16>,
    /// `LMap`: light maps indexed by time of day (`OptDayNight`: 0 day, 1 dusk, 2 night).
    pub light: [Vec<u8>; 3],
    /// `WMap`: water index per cell.
    pub water: Vec<u8>,
    /// `HMapO`: object height map.
    pub object_height: Vec<u8>,
    /// `FogsMap` (512x512): fog zone index.
    pub fogs: Vec<u8>,
    /// `AmbMap` (512x512): ambient sound zone index.
    pub ambient: Vec<u8>,
}

pub fn parse(data: &[u8]) -> Result<Map, FormatError> {
    parse_lean(data, None)
}

/// Like [`parse`], but with `Some(light)` only that light map is kept and
/// `texture2` is skipped (left empty): what a renderer that draws one time
/// of day needs, for about 30% less memory.
pub fn parse_lean(data: &[u8], light: Option<usize>) -> Result<Map, FormatError> {
    let mut r = Reader::new(data);
    let b = |r: &mut Reader<'_>, n, c| r.bytes(n, c).map(<[u8]>::to_vec);
    let keep = |i: usize| light.is_none_or(|l| l == i);
    let height = b(&mut r, CELLS, "map.hmap")?;
    let texture1 = r.u16_vec(CELLS, "map.tmap1")?;
    let texture2 = if light.is_none() {
        r.u16_vec(CELLS, "map.tmap2")?
    } else {
        r.bytes(CELLS * 2, "map.tmap2")?;
        Vec::new()
    };
    let objects = b(&mut r, CELLS, "map.omap")?;
    let flags = r.u16_vec(CELLS, "map.fmap")?;
    let mut lights: [Vec<u8>; 3] = Default::default();
    for (i, (l, c)) in lights
        .iter_mut()
        .zip(["map.lmap.day", "map.lmap.dusk", "map.lmap.night"])
        .enumerate()
    {
        let raw = r.bytes(CELLS, c)?;
        if keep(i) {
            *l = raw.to_vec();
        }
    }
    let map = Map {
        height,
        texture1,
        texture2,
        objects,
        flags,
        light: lights,
        water: b(&mut r, CELLS, "map.wmap")?,
        object_height: b(&mut r, CELLS, "map.hmapo")?,
        fogs: b(&mut r, HALF_CELLS, "map.fogsmap")?,
        ambient: b(&mut r, HALF_CELLS, "map.ambmap")?,
    };
    r.finish("map")?;
    Ok(map)
}

pub fn write(m: &Map) -> Vec<u8> {
    let mut w = Vec::with_capacity(FILE_SIZE);
    w.extend_from_slice(&m.height);
    w.put_u16s(&m.texture1);
    w.put_u16s(&m.texture2);
    w.extend_from_slice(&m.objects);
    w.put_u16s(&m.flags);
    for l in &m.light {
        w.extend_from_slice(l);
    }
    w.extend_from_slice(&m.water);
    w.extend_from_slice(&m.object_height);
    w.extend_from_slice(&m.fogs);
    w.extend_from_slice(&m.ambient);
    w
}
