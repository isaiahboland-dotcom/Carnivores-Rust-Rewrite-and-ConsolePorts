//! A cooked pack (`c2-cook` output) as a layer, read from disk on demand.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use c2_formats::cooked::{parse_pack_header, parse_pack_index, PackEntry, PACK_HEADER_LEN};

use crate::{Backend, VfsError};

pub struct PackBackend {
    path: PathBuf,
    file: Mutex<File>,
    entries: Vec<PackEntry>,
    /// Entry indices sorted by name, for a binary search (a combined
    /// game's pack has thousands; a map of names would cost the 3DS RAM).
    by_name: Vec<u32>,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> VfsError + '_ {
    move |source| VfsError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn bad(path: &Path, e: impl std::fmt::Display) -> VfsError {
    VfsError::Io {
        path: path.display().to_string(),
        source: std::io::Error::other(e.to_string()),
    }
}

impl PackBackend {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, VfsError> {
        let path = path.into();
        let mut file = File::open(&path).map_err(io(&path))?;
        let mut head = [0u8; PACK_HEADER_LEN];
        file.read_exact(&mut head).map_err(io(&path))?;
        let (count, index_len) = parse_pack_header(&head).map_err(|e| bad(&path, e))?;
        let mut index = vec![0u8; index_len];
        file.read_exact(&mut index).map_err(io(&path))?;
        let entries = parse_pack_index(&index, count).map_err(|e| bad(&path, e))?;
        // Every entry range must sit inside the file, past the index — the
        // same check `RangePackBackend::open` applies — so a corrupt or
        // truncated pack cannot make read() reserve `e.size` bytes (up to
        // 4 GiB, the u32 size field) for a range that can never be read
        // (audit F4).
        let len = file.metadata().map_err(io(&path))?.len();
        let index_end = (PACK_HEADER_LEN as u64) + index_len as u64;
        for e in &entries {
            let end = u64::from(e.offset) + u64::from(e.size);
            if u64::from(e.offset) < index_end || end > len {
                return Err(VfsError::InvalidPack {
                    path: path.display().to_string(),
                    reason: "entry range outside pack data".into(),
                });
            }
        }
        let mut by_name: Vec<u32> = (0..entries.len() as u32).collect();
        by_name.sort_by(|&a, &b| entries[a as usize].name.cmp(&entries[b as usize].name));
        Ok(Self {
            path,
            file: Mutex::new(file),
            entries,
            by_name,
        })
    }

    pub fn entries(&self) -> &[PackEntry] {
        &self.entries
    }
}

impl Backend for PackBackend {
    fn files(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let e = self
            .by_name
            .binary_search_by(|&i| self.entries[i as usize].name.as_str().cmp(path))
            .ok()
            .map(|j| &self.entries[self.by_name[j] as usize])
            .ok_or_else(|| VfsError::NotFound { path: path.into() })?;
        let mut f = self
            .file
            .lock()
            .map_err(|_| bad(&self.path, "pack lock poisoned"))?;
        f.seek(SeekFrom::Start(u64::from(e.offset)))
            .map_err(io(&self.path))?;
        let mut out = vec![0u8; e.size as usize];
        f.read_exact(&mut out).map_err(io(&self.path))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vfs;

    #[test]
    fn reads_entries_from_a_pack_file() {
        let p = std::env::temp_dir().join(format!("c2-vfs-pack-{}.c2pk", std::process::id()));
        let bytes = c2_formats::cooked::write_pack(&[
            ("HUNTDAT/_RES.TXT".into(), b"res".to_vec()),
            ("HUNTDAT/AREAS/AREA1.MAP".into(), vec![1, 2, 3]),
        ]);
        std::fs::write(&p, bytes).ok();
        let mut v = Vfs::new();
        if let Ok(b) = PackBackend::open(&p) {
            v.push("pack", Box::new(b));
        }
        std::fs::remove_file(&p).ok();
        assert_eq!(v.read("huntdat/areas/area1.map").ok(), Some(vec![1, 2, 3]));
        assert_eq!(v.read("HUNTDAT/_res.txt").ok(), Some(b"res".to_vec()));
    }

    #[test]
    fn open_rejects_entry_ranges_outside_the_file() {
        let valid = c2_formats::cooked::write_pack(&[("x".into(), vec![1, 2, 3])]);
        let p =
            std::env::temp_dir().join(format!("c2-vfs-pack-invalid-{}.c2pk", std::process::id()));
        // One-byte name: the entry's offset and size follow its u16 length.
        let offset = PACK_HEADER_LEN + 3;
        for case in 0..3 {
            let mut bytes = valid.clone();
            match case {
                0 => {
                    bytes.pop();
                } // Truncated payload.
                1 => bytes[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes()), // Inside header.
                _ => bytes[offset + 4..offset + 8].copy_from_slice(&u32::MAX.to_le_bytes()),
            }
            std::fs::write(&p, bytes).unwrap_or_else(|e| panic!("{e}"));
            let opened = PackBackend::open(&p);
            assert!(
                matches!(opened, Err(VfsError::InvalidPack { .. })),
                "case {case}"
            );
        }
        std::fs::remove_file(&p).unwrap_or_else(|e| panic!("{e}"));
    }
}
