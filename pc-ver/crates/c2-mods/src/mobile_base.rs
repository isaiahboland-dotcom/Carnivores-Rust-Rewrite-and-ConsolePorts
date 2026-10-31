//! The user's mobile APK (Carnivores: Dinosaur Hunter or Ice Age for
//! Android) as a base game, with no PC install: its pieces rebuilt as a PC
//! game folder (`HUNTDAT/...`), which every frontend then reads as it reads
//! the PC's. Built locally from the user's own APK; nothing is shipped.
//!
//! So far: the areas and the trophy room (`AREAS/AREAn.MAP`, `.RSC`).

use crate::mobile::Apk;
use crate::ModError;
use c2_formats::mobile::Assets;
use c2_formats::mobile_area;
use c2_vfs::Backend;
use std::path::{Path, PathBuf};

/// What [`build`] wrote.
#[derive(Debug, Default)]
pub struct Built {
    /// Each area's files, in order (the trophy room last).
    pub areas: Vec<PathBuf>,
}

fn write(path: &Path, data: &[u8]) -> Result<(), ModError> {
    std::fs::write(path, data).map_err(|source| ModError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// One area (`assets/areas/<name>.man` and `.rst`) as `<stem>.MAP` and
/// `.RSC` in `dir`.
fn area(apk: &Apk, name: &str, dir: &Path, stem: &str) -> Result<PathBuf, ModError> {
    let man = apk.0.read(&format!("assets/areas/{name}.man"))?;
    let rst = apk.0.read(&format!("assets/areas/{name}.rst"))?;
    let rst = mobile_area::parse_rst(&String::from_utf8_lossy(&rst))?;
    let map = mobile_area::parse_man(&man)?;
    let rsc = mobile_area::rsc_from(&rst, apk)?;
    let path = dir.join(format!("{stem}.MAP"));
    write(&path, &c2_formats::map::write(&map))?;
    write(
        &dir.join(format!("{stem}.RSC")),
        &c2_formats::rsc::write(&rsc),
    )?;
    Ok(path)
}

/// Rebuilds what is done so far of `apk` as a game folder at `out`.
pub fn build(apk: &Path, out: &Path) -> Result<Built, ModError> {
    let zip = Apk(c2_vfs::ZipBackend::new(apk, "")?);
    let dir = out.join("HUNTDAT").join("AREAS");
    std::fs::create_dir_all(&dir).map_err(|source| ModError::Io {
        path: dir.clone(),
        source,
    })?;
    let mut built = Built::default();
    for n in 1.. {
        if zip.get(&format!("assets/areas/area{n}.man")).is_none() {
            break;
        }
        built
            .areas
            .push(area(&zip, &format!("area{n}"), &dir, &format!("AREA{n}"))?);
    }
    if zip.get("assets/areas/trophy.man").is_some() {
        built.areas.push(area(&zip, "trophy", &dir, "TROPHY")?);
    }
    Ok(built)
}
