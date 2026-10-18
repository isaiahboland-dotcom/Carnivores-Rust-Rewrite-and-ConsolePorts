//! Where the port keeps its own trophy save. The game folder is never written:
//! on first run the original `trophy00.sav` is copied as the starting point.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use c2_formats::sav::{self, Save, TrophyItem};

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

pub fn save_path() -> PathBuf {
    data_dir().join("trophy00.sav")
}

fn blank() -> Save {
    Save {
        player_name_raw: [0; 128],
        reg_number: 0,
        score: 0,
        rank: 0,
        last: Default::default(),
        total: Default::default(),
        bodies: [TrophyItem::default(); 24],
        options: Default::default(),
    }
}

/// Our save, or (first run) a copy of the game's `trophy00.sav`, or blank.
pub fn load(game_dir: &Path) -> Save {
    let ours = save_path();
    let read = |p: &Path| std::fs::read(p).ok().and_then(|d| sav::parse(&d).ok());
    if let Some(s) = read(&ours) {
        return s;
    }
    let original = std::fs::read_dir(game_dir).ok().and_then(|d| {
        d.flatten().map(|e| e.path()).find(|p| {
            p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("trophy00.sav"))
        })
    });
    match original.as_deref().and_then(read) {
        Some(s) => {
            tracing::info!(
                "starting from the game's trophy00.sav (copied to {})",
                ours.display()
            );
            s
        }
        None => blank(),
    }
}

pub fn store(s: &Save) -> Result<()> {
    let p = save_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&p, sav::write(s)).with_context(|| format!("writing {}", p.display()))?;
    tracing::info!("saved {}", p.display());
    Ok(())
}
