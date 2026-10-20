//! The hunter's save on the SD card: `sdmc:/3ds/c2rs/trophy00.sav`, in the
//! original's format (score, rank, records, trophies, options), so it can be
//! copied to and from the PC version. Packs other than `c2rs.c2pk` keep
//! their own (`trophy00-modded.sav`): their trophies are other animals.

use c2_formats::sav::{self, Save};

use crate::DIR;

static PACK: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Saves go with this pack from now on.
pub fn use_pack(pack: &std::path::Path) {
    let stem = pack
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = PACK.set(stem);
}

fn path() -> String {
    match PACK.get().map(String::as_str) {
        None | Some("c2rs") => format!("{DIR}/trophy00.sav"),
        Some(p) => format!("{DIR}/trophy00-{p}.sav"),
    }
}

/// The save, or a new hunter with `start_score` points (`_RES.TXT`
/// `prices.start`) if there is none or it can't be read.
pub fn load(start_score: i32) -> Save {
    std::fs::read(path())
        .ok()
        .and_then(|d| sav::parse(&d).ok())
        .unwrap_or_else(|| c2_core::profile::new_player("Hunter", 0, start_score))
}

pub fn store(s: &Save) -> std::io::Result<()> {
    std::fs::write(path(), sav::write(s))
}
