//! Where the port keeps its own player saves (`trophy0N.sav`, one per
//! registered hunter). The game folder is never written: on first run the
//! original saves are copied as the starting point.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use c2_formats::sav::{self, Save};

/// Save slots (`trophy0%d.sav` has room for one digit).
pub const MAX_PLAYERS: i32 = 10;

/// Per-user data directory: `$XDG_DATA_HOME/c2rs` (`~/.local/share/c2rs`),
/// `%APPDATA%\c2rs` on Windows, `~/Library/Application Support/c2rs` on macOS.
pub fn data_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("c2rs")
}

pub fn slot_path(reg: i32) -> PathBuf {
    data_dir().join(format!("trophy0{reg}.sav"))
}

pub use c2_core::profile::new_player;

fn read(p: &Path) -> Option<Save> {
    sav::parse(&std::fs::read(p).ok()?).ok()
}

/// `trophy0N.sav` files in `dir` (case-insensitive), by slot.
fn find_saves(dir: &Path) -> Vec<(i32, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for p in entries.flatten().map(|e| e.path()) {
        let Some(n) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let n = n.to_ascii_lowercase();
        if let Some(d) = n
            .strip_prefix("trophy0")
            .and_then(|r| r.strip_suffix(".sav"))
            .and_then(|d| d.parse::<i32>().ok())
        {
            if (0..MAX_PLAYERS).contains(&d) {
                out.push((d, p));
            }
        }
    }
    out.sort();
    out
}

/// First run: copy the game's saves into our data dir.
fn import(game_dir: &Path) {
    if !find_saves(&data_dir()).is_empty() {
        return;
    }
    for (reg, p) in find_saves(game_dir) {
        if let Some(mut s) = read(&p) {
            s.reg_number = reg;
            match store(&s) {
                Ok(()) => tracing::info!("imported {}", p.display()),
                Err(e) => tracing::warn!("{e:#}"),
            }
        }
    }
}

/// Every registered hunter, by slot.
pub fn players(game_dir: &Path) -> Vec<Save> {
    import(game_dir);
    find_saves(&data_dir())
        .into_iter()
        .filter_map(|(reg, p)| {
            let mut s = read(&p)?;
            s.reg_number = reg;
            Some(s)
        })
        .collect()
}

/// Hunter `reg`, or a blank one.
pub fn load(game_dir: &Path, reg: i32) -> Save {
    players(game_dir)
        .into_iter()
        .find(|s| s.reg_number == reg)
        .unwrap_or_else(|| new_player("", reg, 0))
}

pub fn store(s: &Save) -> Result<()> {
    let p = slot_path(s.reg_number);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&p, sav::write(s)).with_context(|| format!("writing {}", p.display()))?;
    tracing::info!("saved {}", p.display());
    Ok(())
}

pub fn delete(reg: i32) -> Result<()> {
    let p = slot_path(reg);
    std::fs::remove_file(&p).with_context(|| format!("deleting {}", p.display()))
}
