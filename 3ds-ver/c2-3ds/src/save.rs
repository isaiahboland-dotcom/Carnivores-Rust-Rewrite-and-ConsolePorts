//! The hunters' saves on the SD card: `sdmc:/3ds/c2rs/trophy0N.sav` (hunter
//! N, 0..=5, as the PC numbers them), in the original's format (score,
//! rank, records, trophies, options), so it can be copied to and from the
//! PC version. Packs other than `c2rs.c2pk` keep their own
//! (`trophy0N-modded.sav`): their trophies are other animals. Beside each,
//! the hunter's record (`.rec`, `c2_core::record`).

use c2_formats::sav::{self, Save};

use crate::dir;

static PACK: std::sync::OnceLock<String> = std::sync::OnceLock::new();
/// The hunter playing (save slot).
static HUNTER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// How many hunters a pack can keep.
pub const HUNTERS: i32 = 6;

/// Hunter `n` plays from now on.
pub fn use_hunter(n: i32) {
    HUNTER.store(
        n.clamp(0, HUNTERS - 1),
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub fn hunter() -> i32 {
    HUNTER.load(std::sync::atomic::Ordering::Relaxed)
}

/// Saves go with this pack from now on.
pub fn use_pack(pack: &std::path::Path) {
    let stem = pack
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = PACK.set(stem);
}

fn path_of(n: i32) -> String {
    match PACK.get().map(String::as_str) {
        None | Some("c2rs") => format!("{}/trophy0{n}.sav", dir()),
        Some(p) => format!("{}/trophy0{n}-{p}.sav", dir()),
    }
}

fn path() -> String {
    path_of(hunter())
}

/// The hunters this pack has saves for: slot and name.
pub fn hunters() -> Vec<(i32, String)> {
    (0..HUNTERS)
        .filter_map(|n| {
            let s = sav::parse(&std::fs::read(path_of(n)).ok()?).ok()?;
            Some((n, s.player_name()))
        })
        .collect()
}

/// A free slot for a new hunter.
pub fn free_slot() -> Option<i32> {
    (0..HUNTERS).find(|&n| std::fs::metadata(path_of(n)).is_err())
}

/// A new hunter in slot `n`, saved.
pub fn create(n: i32, name: &str, start_score: i32) -> std::io::Result<Save> {
    let s = c2_core::profile::new_player(name, n, start_score);
    std::fs::write(path_of(n), sav::write(&s))?;
    Ok(s)
}

/// The hunter's record.
pub fn load_record() -> c2_core::record::Record {
    std::fs::read_to_string(path().replace(".sav", ".rec"))
        .map(|t| c2_core::record::Record::parse(&t))
        .unwrap_or_default()
}

pub fn store_record(r: &c2_core::record::Record) -> std::io::Result<()> {
    std::fs::write(path().replace(".sav", ".rec"), r.write())
}

/// The save, or a new hunter with `start_score` points (`_RES.TXT`
/// `prices.start`) if there is none or it can't be read.
pub fn load(start_score: i32) -> Save {
    std::fs::read(path())
        .ok()
        .and_then(|d| sav::parse(&d).ok())
        .unwrap_or_else(|| c2_core::profile::new_player("Hunter", hunter(), start_score))
}

pub fn store(s: &Save) -> std::io::Result<()> {
    std::fs::write(path(), sav::write(s))
}
