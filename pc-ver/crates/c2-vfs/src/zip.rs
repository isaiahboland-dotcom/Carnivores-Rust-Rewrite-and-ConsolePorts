//! A zip archive as a layer, optionally rooted at a folder inside it (mods
//! are often zipped with their game folder one level down).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{key, Backend, VfsError};

pub struct ZipBackend {
    path: PathBuf,
    archive: Mutex<zip::ZipArchive<File>>,
    /// `/`-terminated prefix inside the archive ("" for its top).
    prefix: String,
    /// `(name under prefix, archive index)`.
    files: Vec<(String, usize)>,
}

fn err(path: &Path, e: impl std::fmt::Display) -> VfsError {
    VfsError::Io {
        path: path.display().to_string(),
        source: std::io::Error::other(e.to_string()),
    }
}

/// The archive's entry names, `/`-separated (folders end in `/`).
fn names(a: &zip::ZipArchive<File>) -> Vec<String> {
    a.file_names()
        .filter_map(|n| n.ok().map(|n| n.replace('\\', "/")))
        .collect()
}

impl ZipBackend {
    /// Opens `path`, rooted at `prefix` (`""` or `"Some Folder"`).
    pub fn new(path: impl Into<PathBuf>, prefix: &str) -> Result<Self, VfsError> {
        let path = path.into();
        let f = File::open(&path).map_err(|source| VfsError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let archive = zip::ZipArchive::new(f).map_err(|e| err(&path, e))?;
        let prefix = match prefix.trim_matches('/') {
            "" => String::new(),
            p => format!("{p}/"),
        };
        let pk = key(&prefix);
        let mut files = Vec::new();
        for i in 0..archive.len() {
            let Some(Ok(name)) = archive.name_for_index(i) else {
                continue;
            };
            let name = name.replace('\\', "/");
            if name.ends_with('/') || !key(&name).starts_with(&pk) {
                continue;
            }
            files.push((name[prefix.len()..].to_string(), i));
        }
        files.sort();
        Ok(Self {
            path,
            archive: Mutex::new(archive),
            prefix,
            files,
        })
    }

    /// Opens `path` at the folder holding a directory named `marker`
    /// (case-insensitive): the top, or one folder down if exactly one has it.
    pub fn find(path: impl Into<PathBuf>, marker: &str) -> Result<Option<Self>, VfsError> {
        let path = path.into();
        let f = File::open(&path).map_err(|source| VfsError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let archive = zip::ZipArchive::new(f).map_err(|e| err(&path, e))?;
        let marker = marker.to_ascii_lowercase();
        let mut roots: Vec<String> = Vec::new();
        for n in names(&archive) {
            let parts: Vec<&str> = n.split('/').collect();
            // A folder `marker` at depth 0 or 1, with something below it.
            for d in 0..2.min(parts.len().saturating_sub(1)) {
                if parts[d].eq_ignore_ascii_case(&marker) {
                    let root = parts[..d].join("/");
                    if !roots.contains(&root) {
                        roots.push(root);
                    }
                }
            }
        }
        let root = if roots.iter().any(|r| r.is_empty()) {
            String::new()
        } else {
            match roots.as_slice() {
                [one] => one.clone(),
                _ => return Ok(None),
            }
        };
        Self::new(path, &root).map(Some)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The folder inside the archive the layer is rooted at (`""` or `"x/"`).
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

impl Backend for ZipBackend {
    fn files(&self) -> Vec<String> {
        self.files.iter().map(|(n, _)| n.clone()).collect()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let Some(&(_, i)) = self.files.iter().find(|(n, _)| n == path) else {
            return Err(VfsError::NotFound { path: path.into() });
        };
        let mut a = self
            .archive
            .lock()
            .map_err(|_| err(&self.path, "archive lock poisoned"))?;
        let mut f = a.by_index(i).map_err(|e| err(&self.path, e))?;
        let mut out = Vec::with_capacity(usize::try_from(f.size()).unwrap_or(0));
        f.read_to_end(&mut out).map_err(|e| err(&self.path, e))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_the_game_folder_inside_a_zip() {
        let p = std::env::temp_dir().join(format!("c2-vfs-zip-{}.zip", std::process::id()));
        let write = || -> zip::result::ZipResult<()> {
            let mut w = zip::ZipWriter::new(File::create(&p)?);
            let o = zip::write::SimpleFileOptions::default();
            w.add_directory("My Mod/", o)?;
            w.start_file("My Mod/HUNTDAT/_RES.TXT", o)?;
            w.write_all(b"res")?;
            w.start_file("My Mod/readme.txt", o)?;
            w.write_all(b"hi")?;
            w.start_file("other.txt", o)?;
            w.finish()?;
            Ok(())
        };
        assert!(write().is_ok());
        let z = ZipBackend::find(&p, "huntdat").ok().flatten();
        let got = z.as_ref().map(|z| {
            (
                z.prefix().to_string(),
                z.files(),
                z.read("HUNTDAT/_RES.TXT").ok(),
            )
        });
        std::fs::remove_file(&p).ok();
        assert_eq!(
            got,
            Some((
                "My Mod/".to_string(),
                vec!["HUNTDAT/_RES.TXT".to_string(), "readme.txt".to_string()],
                Some(b"res".to_vec())
            ))
        );
    }
}
