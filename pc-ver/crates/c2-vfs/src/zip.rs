//! A zip archive as a layer, optionally rooted at a folder inside it (mods
//! are often zipped with their game folder one level down).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{is_safe_relative_path, Backend, VfsError};

const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

fn read_bounded(
    reader: &mut impl Read,
    declared: u64,
    limit: u64,
    path: &str,
) -> Result<Vec<u8>, VfsError> {
    let too_large = || VfsError::EntryTooLarge {
        path: path.into(),
        limit,
    };
    if declared > limit {
        return Err(too_large());
    }
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        // Probe one byte beyond the remaining allowance, even at the limit.
        let remaining = limit.saturating_sub(out.len() as u64);
        let wanted = remaining.saturating_add(1).min(chunk.len() as u64) as usize;
        let n = match reader.read(&mut chunk[..wanted]) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            other => other.map_err(|source| VfsError::Io {
                path: path.into(),
                source,
            })?,
        };
        if n == 0 {
            return Ok(out);
        }
        if n as u64 > remaining {
            return Err(too_large());
        }
        out.try_reserve_exact(n).map_err(|e| VfsError::Io {
            path: path.into(),
            source: std::io::Error::other(e.to_string()),
        })?;
        out.extend_from_slice(&chunk[..n]);
    }
}

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
        if archive.len() > 100_000 {
            return Err(VfsError::InvalidPack {
                path: path.display().to_string(),
                reason: "archive entry count exceeds 100000".into(),
            });
        }
        let prefix = match prefix.trim_matches('/') {
            "" => String::new(),
            p => format!("{p}/"),
        };
        let pk = prefix.to_ascii_lowercase();
        let mut files = Vec::new();
        for i in 0..archive.len() {
            let Some(Ok(name)) = archive.name_for_index(i) else {
                continue;
            };
            let name = name.replace('\\', "/");
            if !is_safe_relative_path(&name) {
                return Err(VfsError::InvalidPack {
                    path: path.display().to_string(),
                    reason: format!("unsafe archive path: {name}"),
                });
            }
            if name.ends_with('/') || !name.to_ascii_lowercase().starts_with(&pk) {
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
        if archive.len() > 100_000 {
            return Err(VfsError::InvalidPack {
                path: path.display().to_string(),
                reason: "archive entry count exceeds 100000".into(),
            });
        }
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
        let declared = f.size();
        read_bounded(&mut f, declared, MAX_ENTRY_BYTES, path)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::io::Write;

    #[test]
    fn bounded_output_checks_metadata_and_actual_bytes() {
        assert!(matches!(
            read_bounded(&mut &b"x"[..], 4 * 1024 * 1024 * 1024, 8, "x"),
            Err(VfsError::EntryTooLarge { .. })
        ));
        assert_eq!(
            read_bounded(&mut &b"12345678"[..], 8, 8, "x").ok(),
            Some(b"12345678".to_vec())
        );
        assert!(matches!(
            read_bounded(&mut &b"123456789"[..], 1, 8, "x"),
            Err(VfsError::EntryTooLarge { .. })
        ));
    }

    #[test]
    fn direct_zip_layers_reject_paths_that_escape_pack_output() {
        let path =
            std::env::temp_dir().join(format!("c2-vfs-traversal-{}.zip", std::process::id()));
        for name in [
            "HUNTDAT/../../outside",
            "HUNTDAT/..\\..\\outside",
            "/HUNTDAT/x",
            "HUNTDAT/C:outside",
        ] {
            let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"bad").unwrap();
            writer.finish().unwrap();
            assert!(ZipBackend::new(&path, "").is_err(), "{name}");
        }
        std::fs::remove_file(path).unwrap();
    }

    fn synthetic_zip(bytes: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(
                "x",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn forged_metadata_and_crc_fail_through_the_archive_backend() {
        for corrupt_size in [false, true] {
            let mut bytes = synthetic_zip(b"payload");
            if corrupt_size {
                // Central-directory uncompressed size. No large payload needed.
                let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
                bytes[central + 24..central + 28].copy_from_slice(&u32::MAX.to_le_bytes());
            } else {
                let payload = bytes.windows(7).position(|b| b == b"payload").unwrap();
                bytes[payload] ^= 1;
            }
            let path = std::env::temp_dir().join(format!(
                "c2-vfs-forged-{}-{corrupt_size}.zip",
                std::process::id()
            ));
            std::fs::write(&path, bytes).unwrap();
            if let Ok(archive) = ZipBackend::new(&path, "") {
                assert!(archive.read("x").is_err());
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    /// A stored (uncompressed) zip with one entry whose name is written
    /// raw, without the UTF-8 flag: what a legacy Windows tool produces.
    /// Includes a valid CRC so the same fixture checks reads as well as names.
    fn raw_name_zip(raw_name: &[u8]) -> Vec<u8> {
        fn le16(out: &mut Vec<u8>, v: u16) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        fn le32(out: &mut Vec<u8>, v: u32) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let data = b"payload";
        let mut out = Vec::new();
        le32(&mut out, 0x0403_4b50); // local file header
        le16(&mut out, 20); // version needed
        le16(&mut out, 0); // flags: no UTF-8 bit
        le16(&mut out, 0); // stored
        le16(&mut out, 0); // time
        le16(&mut out, 0); // date
        le32(&mut out, 0x422c_6a15); // CRC32 of b"payload"
        le32(&mut out, data.len() as u32);
        le32(&mut out, data.len() as u32);
        le16(&mut out, raw_name.len() as u16);
        le16(&mut out, 0); // extra len
        out.extend_from_slice(raw_name);
        out.extend_from_slice(data);
        let central_at = out.len() as u32;
        le32(&mut out, 0x0201_4b50); // central directory header
        le16(&mut out, 20); // version made by
        le16(&mut out, 20); // version needed
        le16(&mut out, 0); // flags
        le16(&mut out, 0); // method
        le16(&mut out, 0); // time
        le16(&mut out, 0); // date
        le32(&mut out, 0x422c_6a15); // CRC32 of b"payload"
        le32(&mut out, data.len() as u32);
        le32(&mut out, data.len() as u32);
        le16(&mut out, raw_name.len() as u16);
        le16(&mut out, 0); // extra
        le16(&mut out, 0); // comment
        le16(&mut out, 0); // disk
        le16(&mut out, 0); // internal attrs
        le32(&mut out, 0); // external attrs
        le32(&mut out, 0); // local header offset
        out.extend_from_slice(raw_name);
        let cd_size = out.len() as u32 - central_at;
        le32(&mut out, 0x0605_4b50); // end of central directory
        le16(&mut out, 0); // this disk
        le16(&mut out, 0); // cd disk
        le16(&mut out, 1); // entries this disk
        le16(&mut out, 1); // entries total
        le32(&mut out, cd_size);
        le32(&mut out, central_at);
        le16(&mut out, 0); // comment len
        out
    }

    #[test]
    fn legacy_cp437_names_are_listed_not_dropped() {
        // The pinned ZIP dependency supplies CP437 fallback for legacy
        // Windows names (CP437 'é' = 0x82); keep that behavior covered.
        let bytes = raw_name_zip(b"HUNTDAT/DINO\x82.TGA");
        let path = std::env::temp_dir().join(format!("c2-vfs-cp437-{}.zip", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let z = ZipBackend::new(&path, "").unwrap();
        let listed = z.files();
        assert_eq!(z.read("HUNTDAT/DINOé.TGA").unwrap(), b"payload");
        drop(z);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(listed, vec!["HUNTDAT/DINOé.TGA".to_string()]);
    }

    #[test]
    fn legacy_cp437_root_is_discovered_and_readable() {
        let path =
            std::env::temp_dir().join(format!("c2-vfs-cp437-root-{}.zip", std::process::id()));
        std::fs::write(&path, raw_name_zip(b"Mod\x82/HUNTDAT/x")).unwrap();
        let z = ZipBackend::find(&path, "HUNTDAT").unwrap().unwrap();
        assert_eq!(z.prefix(), "Modé/");
        assert_eq!(z.read("HUNTDAT/x").unwrap(), b"payload");
        drop(z);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn cp437_decoding_does_not_bypass_path_safety() {
        // Decoding raw names must not re-open the traversal hole: the
        // decoded name still goes through is_safe_relative_path.
        let bytes = raw_name_zip(b"HUNTDAT/..\x5c..\x5coutsid\x82");
        let path =
            std::env::temp_dir().join(format!("c2-vfs-cp437-traversal-{}.zip", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let opened = ZipBackend::new(&path, "");
        assert!(matches!(opened, Err(VfsError::InvalidPack { .. })));
        drop(opened);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn normalized_prefix_does_not_slice_unrelated_unicode_names() {
        let path = std::env::temp_dir().join(format!("c2-vfs-prefix-{}.zip", std::process::id()));
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        writer
            .start_file("a/b/é", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
        // Previously key() removed the duplicate separator, then slicing used
        // the longer raw prefix and could split a UTF-8 code point.
        assert!(ZipBackend::new(&path, "a//b").unwrap().files().is_empty());
        std::fs::remove_file(path).unwrap();
    }
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
