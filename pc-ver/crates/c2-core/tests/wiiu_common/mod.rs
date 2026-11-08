//! The Wii U host tests' game folders: an install (`install.rs`) opened on
//! the host over a game folder, as the console opens its SD folder. Each
//! test crate declares `mod install;` (and `mod hunt_setup;`) itself.
#![allow(dead_code, clippy::expect_used)]
use c2_vfs::{Backend, Vfs, VfsError};
use std::path::{Path, PathBuf};

/// A folder's files, indexed once (paths with `/`, as spelled on disk).
pub struct Folder {
    root: PathBuf,
    files: Vec<String>,
}

impl Folder {
    pub fn new(root: &Path) -> Self {
        let mut files = Vec::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(rel) = stack.pop() {
            let Ok(dir) = std::fs::read_dir(root.join(&rel)) else {
                continue;
            };
            for e in dir.flatten() {
                let r = rel.join(e.file_name());
                if e.path().is_dir() {
                    stack.push(r);
                } else {
                    files.push(r.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        Folder {
            root: root.to_path_buf(),
            files,
        }
    }
}

impl Backend for Folder {
    fn files(&self) -> Vec<String> {
        self.files.clone()
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        std::fs::read(self.root.join(path)).map_err(|_| VfsError::NotFound { path: path.into() })
    }
}

/// The game folder `root` (holding `HUNTDAT`) as an install: its areas
/// as present, then the trophy room.
pub fn install(root: &Path) -> crate::install::Install {
    let folder = Folder::new(root);
    let has = |name: &str| {
        folder
            .files
            .iter()
            .any(|f| f.eq_ignore_ascii_case(&format!("HUNTDAT/AREAS/{name}.MAP")))
    };
    let mut areas: Vec<String> = (1..=99)
        .map(|n| format!("AREA{n}"))
        .filter(|a| has(a))
        .collect();
    if has("TROPHY") {
        areas.push("TROPHY".into());
    }
    let mut vfs = Vfs::new();
    vfs.push("game", Box::new(folder));
    let info = crate::install::Info {
        areas,
        ..Default::default()
    };
    crate::install::Install::open(vfs, info).expect("the game's rules")
}

/// The original game (C2_ORIGINAL_HUNTDAT is its `HUNTDAT`) as an install.
pub fn original() -> Option<(PathBuf, crate::install::Install)> {
    let huntdat = PathBuf::from(std::env::var("C2_ORIGINAL_HUNTDAT").ok()?);
    let root = huntdat.parent()?.to_path_buf();
    Some((huntdat, install(&root)))
}

/// The menu bits for the original's slots `slots` (bit n: AI 10 + n): the
/// menu lists animals by price.
pub fn slots(inst: &crate::install::Install, slots: u32) -> u64 {
    inst.animals
        .iter()
        .enumerate()
        .fold(0, |m, (i, a)| match a.pick {
            crate::install::Pick::Slot(k) if slots & (1 << k) != 0 => m | 1 << i,
            _ => m,
        })
}
