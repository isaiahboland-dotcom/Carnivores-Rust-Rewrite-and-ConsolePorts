//! Layered virtual filesystem.
//!
//! Layers stack `base game → mod 1 → … → user overrides`; a later layer's
//! file hides an earlier one with the same path. Lookups are
//! case-insensitive (the original game ran on Windows) and every resolved
//! file reports which layer it came from (provenance, for merge reports).
//!
//! Paths are relative, `/`-separated (`HUNTDAT/AREAS/AREA1.MAP`); `\` is
//! accepted and normalised.

mod cooked;
pub use cooked::PackBackend;

#[cfg(feature = "zip")]
mod zip;
#[cfg(feature = "zip")]
pub use zip::ZipBackend;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("{path}: not found")]
    NotFound { path: String },
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Normalised lookup key: `/` separators, lower case, no leading `./` or `/`.
pub fn key(path: &str) -> String {
    // Empty and `.` parts dropped: Windows (the games' home) reads
    // `ANIMALS//X.car` as `ANIMALS/X.car`, and some games write it so.
    path.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
        .to_ascii_lowercase()
}

/// Where a layer's files come from.
pub trait Backend: Send + Sync {
    /// Every file, as the backend spells it (`/`-separated, relative).
    fn files(&self) -> Vec<String>;
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError>;
    /// The file on disk, if the backend has one (lets callers link instead of copy).
    fn disk_path(&self, _path: &str) -> Option<PathBuf> {
        None
    }
}

/// A directory on disk, indexed once.
pub struct DirBackend {
    root: PathBuf,
    files: Vec<String>,
}

impl DirBackend {
    /// Indexes `root` recursively. Unreadable entries are skipped.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, VfsError> {
        let root = root.into();
        let mut files = Vec::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(rel) = stack.pop() {
            let dir = root.join(&rel);
            let rd = std::fs::read_dir(&dir).map_err(|source| VfsError::Io {
                path: dir.display().to_string(),
                source,
            })?;
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let r = rel.join(&name);
                match e.file_type() {
                    Ok(t) if t.is_dir() => stack.push(r),
                    Ok(_) => files.push(
                        r.components()
                            .map(|c| c.as_os_str().to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("/"),
                    ),
                    Err(_) => {}
                }
            }
        }
        files.sort();
        Ok(Self { root, files })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Backend for DirBackend {
    fn files(&self) -> Vec<String> {
        self.files.clone()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let p = self.root.join(path);
        std::fs::read(&p).map_err(|source| VfsError::Io {
            path: p.display().to_string(),
            source,
        })
    }

    fn disk_path(&self, path: &str) -> Option<PathBuf> {
        Some(self.root.join(path))
    }
}

/// Files held in memory (generated files such as a merged `_RES.TXT`, tests).
#[derive(Default)]
pub struct MemBackend {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: &str, data: Vec<u8>) {
        self.files.insert(path.replace('\\', "/"), data);
    }
}

impl Backend for MemBackend {
    fn files(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| VfsError::NotFound { path: path.into() })
    }
}

pub struct Layer {
    pub name: String,
    pub backend: Box<dyn Backend>,
}

/// A file as resolved through the layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// Index of the winning layer.
    pub layer: usize,
    /// The path as the winning layer spells it.
    pub path: String,
    /// Every layer that has this file, bottom to top (the last one wins).
    pub providers: Vec<usize>,
}

#[derive(Default)]
pub struct Vfs {
    layers: Vec<Layer>,
    index: BTreeMap<String, Resolved>,
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a layer on top.
    pub fn push(&mut self, name: impl Into<String>, backend: Box<dyn Backend>) {
        let li = self.layers.len();
        for f in backend.files() {
            let k = key(&f);
            match self.index.get_mut(&k) {
                Some(r) => {
                    r.layer = li;
                    r.path = f;
                    if r.providers.last() != Some(&li) {
                        r.providers.push(li);
                    }
                }
                None => {
                    self.index.insert(
                        k,
                        Resolved {
                            layer: li,
                            path: f,
                            providers: vec![li],
                        },
                    );
                }
            }
        }
        self.layers.push(Layer {
            name: name.into(),
            backend,
        });
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn resolve(&self, path: &str) -> Option<&Resolved> {
        self.index.get(&key(path))
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let r = self
            .resolve(path)
            .ok_or_else(|| VfsError::NotFound { path: path.into() })?;
        self.layers[r.layer].backend.read(&r.path)
    }

    /// The winning file on disk, if its layer is a directory.
    pub fn disk_path(&self, path: &str) -> Option<PathBuf> {
        let r = self.resolve(path)?;
        self.layers[r.layer].backend.disk_path(&r.path)
    }

    /// Every visible file (by normalised key), with provenance.
    pub fn files(&self) -> impl Iterator<Item = (&str, &Resolved)> {
        self.index.iter().map(|(k, r)| (k.as_str(), r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_doubled_and_dot_parts() {
        assert_eq!(key("ANIMALS//A/./x.CAR"), "animals/a/x.car");
        assert_eq!(key("./HUNTDAT\\Menu\\m.tga"), "huntdat/menu/m.tga");
    }

    fn mem(files: &[(&str, &str)]) -> Box<dyn Backend> {
        let mut m = MemBackend::new();
        for (p, d) in files {
            m.insert(p, d.as_bytes().to_vec());
        }
        Box::new(m)
    }

    #[test]
    fn later_layers_win_case_insensitively() {
        let mut v = Vfs::new();
        v.push(
            "base",
            mem(&[("HUNTDAT/_RES.TXT", "base"), ("HUNTDAT/A.CAR", "a")]),
        );
        v.push(
            "mod",
            mem(&[("huntdat/_res.txt", "mod"), ("HUNTDAT/B.CAR", "b")]),
        );
        assert_eq!(v.read("HuntDat\\_Res.Txt").ok(), Some(b"mod".to_vec()));
        let r = v.resolve("HUNTDAT/_RES.TXT").cloned();
        assert_eq!(
            r,
            Some(Resolved {
                layer: 1,
                path: "huntdat/_res.txt".into(),
                providers: vec![0, 1],
            })
        );
        assert_eq!(v.read("huntdat/a.car").ok(), Some(b"a".to_vec()));
        assert_eq!(v.files().count(), 3);
        assert!(v.read("nope").is_err());
    }

    #[test]
    fn dir_backend_indexes_recursively() {
        let dir = std::env::temp_dir().join(format!("c2-vfs-test-{}", std::process::id()));
        let sub = dir.join("HUNTDAT").join("AREAS");
        std::fs::create_dir_all(&sub).ok();
        std::fs::write(sub.join("AREA1.MAP"), b"m").ok();
        let b = DirBackend::new(&dir);
        let files = b.as_ref().map(|b| b.files()).unwrap_or_default();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(files, vec!["HUNTDAT/AREAS/AREA1.MAP".to_string()]);
    }
}
