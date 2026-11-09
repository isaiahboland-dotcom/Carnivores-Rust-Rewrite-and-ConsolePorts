//! Area side files past the `.MAP`'s 254 kinds of object (docs/EXPANSION.md,
//! "More objects per area"). The `.MAP` and `.RSC` stay as they are, for
//! the original engines; only this engine reads these.
//!
//! `AREAn.OBX`, the object map:
//! ```text
//! u8[8]  magic "C2RSOBX1"
//! u16    object[1024*1024]          per cell, row-major; 0xFFFF none, 0xFFFE landing
//! u8     object_height[1024*1024]   the `.MAP`'s HMapO, per cell
//! ```
//!
//! `AREAn.RSX`, more object models, numbered after the `.RSC`'s:
//! ```text
//! u8[8]  magic "C2RSRSX1"
//! i32    count
//! count x object, laid out as in the `.RSC`
//! ```

use crate::error::{FormatError, FormatErrorKind};
use crate::map::CELLS;
use crate::reader::{Reader, WriteLe};
use crate::rsc::{read_object, write_object, Object};
use alloc::vec::Vec;

pub const OBX_MAGIC: &[u8; 8] = b"C2RSOBX1";
pub const RSX_MAGIC: &[u8; 8] = b"C2RSRSX1";

/// An `.OBX`: the object grid that replaces the `.MAP`'s.
#[derive(Debug, Clone, PartialEq)]
pub struct Obx {
    pub objects: Vec<u16>,
    pub object_height: Vec<u8>,
}

fn magic(r: &mut Reader<'_>, want: &[u8; 8], ctx: &str) -> Result<(), FormatError> {
    let got = r.bytes(8, ctx)?;
    if got != want {
        return Err(r.err(ctx, FormatErrorKind::Syntax("not this side file".into())));
    }
    Ok(())
}

pub fn parse_obx(data: &[u8]) -> Result<Obx, FormatError> {
    let mut r = Reader::new(data);
    magic(&mut r, OBX_MAGIC, "obx.magic")?;
    let objects = r.u16_vec(CELLS, "obx.objects")?;
    let object_height = r.bytes(CELLS, "obx.object_height")?.to_vec();
    r.finish("obx")?;
    Ok(Obx {
        objects,
        object_height,
    })
}

pub fn write_obx(o: &Obx) -> Vec<u8> {
    let mut w = Vec::with_capacity(8 + CELLS * 3);
    w.extend_from_slice(OBX_MAGIC);
    w.put_u16s(&o.objects);
    w.extend_from_slice(&o.object_height);
    w
}

/// An `.RSX`'s models, to append to the `.RSC`'s.
pub fn parse_rsx(data: &[u8]) -> Result<Vec<Object>, FormatError> {
    let mut r = Reader::new(data);
    magic(&mut r, RSX_MAGIC, "rsx.magic")?;
    let n = r.count_of(64, "rsx.count")?;
    let objects = (0..n)
        .map(|_| read_object(&mut r))
        .collect::<Result<_, _>>()?;
    r.finish("rsx")?;
    Ok(objects)
}

pub fn write_rsx(objects: &[Object]) -> Vec<u8> {
    let mut w = Vec::new();
    w.extend_from_slice(RSX_MAGIC);
    w.put_len(objects.len());
    for o in objects {
        write_object(&mut w, o);
    }
    w
}

/// Applies an area's side files, given as their bytes when present: the
/// `.OBX` grid replaces the `.MAP`'s objects and object heights, and the
/// `.RSX` models are appended to the `.RSC`'s. On an error nothing has been
/// changed, and the error names the file (`"obx"` or `"rsx"` context).
pub fn apply(
    map: &mut crate::map::Map,
    models: &mut Vec<Object>,
    obx: Option<&[u8]>,
    rsx: Option<&[u8]>,
) -> Result<(), FormatError> {
    let grid = obx.map(parse_obx).transpose()?;
    let extra = rsx.map(parse_rsx).transpose()?;
    if let Some(g) = grid {
        map.objects = g.objects;
        map.object_height = g.object_height;
    }
    if let Some(e) = extra {
        models.extend(e);
    }
    Ok(())
}

/// What [`join_layers`] did.
#[derive(Debug, Default, PartialEq)]
pub struct JoinReport {
    /// Models in all (the `.RSC`'s plus the side file's).
    pub models: usize,
    /// Each layer's models that matched an earlier one exactly, so its
    /// index was reused instead of adding a copy.
    pub shared: Vec<usize>,
    /// Cells where a layer put a different object over one already there
    /// (the later layer wins), per layer.
    pub conflicts: Vec<usize>,
    /// Landing markers in layers, which are left out (the base's stay).
    pub landings_skipped: usize,
}

/// Joins copies of one area (each a `.MAP` object grid, its object heights
/// and its `.RSC` models) over the base area: each layer's models are
/// numbered after the models so far (one identical to an earlier model
/// reuses its index), and its objects are written over the grid. Returns
/// the side files' contents: the whole grid, and the models past the
/// base's `.RSC`. `None` when more than 65533 models would be needed.
pub fn join_layers(
    base_objects: &[u16],
    base_heights: &[u8],
    base_models: &[Object],
    layers: &[(&[u16], &[u8], &[Object])],
) -> Option<(Obx, Vec<Object>, JoinReport)> {
    use crate::map::{LANDING, NO_OBJECT};
    let mut objects = base_objects.to_vec();
    let mut object_height = base_heights.to_vec();
    let mut models: Vec<Object> = base_models.to_vec();
    let mut report = JoinReport::default();
    for (lo, lh, lm) in layers {
        let mut shared = 0;
        let mut remap = Vec::with_capacity(lm.len());
        for m in lm.iter() {
            let at = match models.iter().position(|e| e == m) {
                Some(i) => {
                    shared += 1;
                    i
                }
                None => {
                    models.push(m.clone());
                    models.len() - 1
                }
            };
            if at >= usize::from(LANDING) {
                return None;
            }
            remap.push(at as u16);
        }
        let mut conflicts = 0;
        for (i, &o) in lo.iter().enumerate() {
            if o == NO_OBJECT {
                continue;
            }
            if o == LANDING {
                report.landings_skipped += 1;
                continue;
            }
            let Some(&new) = remap.get(usize::from(o)) else {
                continue;
            };
            let old = objects[i];
            if old != NO_OBJECT && old != new {
                conflicts += 1;
            }
            objects[i] = new;
            object_height[i] = lh[i];
        }
        report.shared.push(shared);
        report.conflicts.push(conflicts);
    }
    report.models = models.len();
    let extra = models.split_off(base_models.len());
    Some((
        Obx {
            objects,
            object_height,
        },
        extra,
        report,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{LANDING, NO_OBJECT};

    #[test]
    fn obx_round_trips_wide_indices() {
        let mut objects = alloc::vec![NO_OBJECT; CELLS];
        objects[0] = 0;
        objects[1] = 600;
        objects[2] = LANDING;
        let o = Obx {
            objects,
            object_height: alloc::vec![7; CELLS],
        };
        let bytes = write_obx(&o);
        assert_eq!(bytes.len(), 8 + CELLS * 3);
        assert_eq!(parse_obx(&bytes).unwrap(), o);
        assert!(parse_obx(&bytes[..100]).is_err());
        let mut wrong = bytes.clone();
        wrong[0] = b'X';
        assert!(parse_obx(&wrong).is_err());
    }

    fn model(radius: i32) -> Object {
        Object {
            info: crate::rsc::ObjectInfo {
                radius,
                y_lo: 0,
                y_hi: 0,
                line_length: 0,
                line_intensity: 0,
                circle_radius: 0,
                circle_intensity: 0,
                flags: 0,
                grass_radius: 0,
                default_light: 0,
                last_ani_time: 0,
                bound_r: 0.0,
                reserved: [0; 16],
            },
            model: Default::default(),
            sprite: alloc::vec![0; crate::rsc::SPRITE_SIZE * crate::rsc::SPRITE_SIZE],
            animation: None,
        }
    }

    #[test]
    fn layers_join_with_new_indices_shared_models_and_conflicts() {
        let (a, b, c) = (model(1), model(2), model(3));
        let mut base = alloc::vec![NO_OBJECT; 8];
        base[0] = 0;
        base[1] = LANDING;
        let hb = alloc::vec![1u8; 8];
        // Layer: its model 0 is the base's (shared), its 1 and 2 are new.
        let mut l = alloc::vec![NO_OBJECT; 8];
        l[0] = 0; // same model over the same cell: no conflict
        l[2] = 1;
        l[3] = 2;
        l[1] = LANDING;
        let hl = alloc::vec![9u8; 8];
        let base_models = [a.clone()];
        let layer_models = [a, b.clone(), c.clone()];
        let (o, extra, r) =
            join_layers(&base, &hb, &base_models, &[(&l, &hl, &layer_models)]).unwrap();
        assert_eq!(&o.objects[..4], &[0, LANDING, 1, 2]);
        assert_eq!(&o.object_height[..4], &[9, 1, 9, 9]);
        assert_eq!(extra, alloc::vec![b, c]);
        assert_eq!(r.models, 3);
        assert_eq!(r.shared, alloc::vec![1]);
        assert_eq!(r.conflicts, alloc::vec![0]);
        assert_eq!(r.landings_skipped, 1);
        // A second layer putting another model on cell 2: a conflict.
        let mut l2 = alloc::vec![NO_OBJECT; 8];
        l2[2] = 0;
        let m2 = [model(4)];
        let so_far = [model(1), model(2), model(3)];
        let (o, extra, r) =
            join_layers(&o.objects, &o.object_height, &so_far, &[(&l2, &hl, &m2)]).unwrap();
        assert_eq!(o.objects[2], 3);
        assert_eq!(extra, alloc::vec![model(4)]);
        assert_eq!(r.conflicts, alloc::vec![1]);
        // Models round-trip through the side file.
        assert_eq!(parse_rsx(&write_rsx(&so_far)).unwrap(), so_far);
    }

    #[test]
    fn empty_rsx_round_trips() {
        let bytes = write_rsx(&[]);
        assert!(parse_rsx(&bytes).unwrap().is_empty());
        assert!(parse_rsx(b"C2RSRSX0\0\0\0\0").is_err());
    }
}
