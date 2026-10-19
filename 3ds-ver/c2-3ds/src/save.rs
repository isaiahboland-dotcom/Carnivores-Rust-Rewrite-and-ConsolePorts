//! The hunter's save on the SD card: `sdmc:/3ds/c2rs/trophy00.sav`, in the
//! original's format (score, rank, records, trophies, options), so it can be
//! copied to and from the PC version.

use c2_formats::sav::{self, Save};

use crate::DIR;

fn path() -> String {
    format!("{DIR}/trophy00.sav")
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
