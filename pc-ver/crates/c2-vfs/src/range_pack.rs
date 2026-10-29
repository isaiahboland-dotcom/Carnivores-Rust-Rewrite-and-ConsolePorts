//! Portable on-demand cooked-pack reader. Platform sources supply exact ranges.
use crate::{Backend, VfsError};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use c2_formats::cooked::{parse_pack_header, parse_pack_index, PackEntry, PACK_HEADER_LEN};

/// A stable, random-access file. Reads must either fill the slice or return an
/// error, including on short reads. Implementations must support concurrent calls.
pub trait PackSource: Send + Sync {
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read_exact_at(&self, offset: u64, out: &mut [u8]) -> Result<(), VfsError>;
}

pub struct RangePackBackend<S> {
    source: S,
    entries: Vec<PackEntry>,
    by_name: Vec<usize>,
}

impl<S: PackSource> RangePackBackend<S> {
    /// Parse the header/index once; file payloads are read only when requested.
    pub fn open(source: S, label: &str) -> Result<Self, VfsError> {
        let invalid = |reason: String| VfsError::InvalidPack {
            path: label.into(),
            reason,
        };
        let mut header = [0; PACK_HEADER_LEN];
        source.read_exact_at(0, &mut header)?;
        let (count, index_len) = parse_pack_header(&header).map_err(|e| invalid(e.to_string()))?;
        let index_end = (PACK_HEADER_LEN as u64) + index_len as u64;
        if index_end > source.len() || count > index_len / 10 {
            return Err(invalid(
                "index exceeds file or cannot contain the declared entries".into(),
            ));
        }
        let mut index = vec![0; index_len];
        if !index.is_empty() {
            source.read_exact_at(PACK_HEADER_LEN as u64, &mut index)?;
        }
        let entries = parse_pack_index(&index, count).map_err(|e| invalid(e.to_string()))?;
        for e in &entries {
            let end = u64::from(e.offset) + u64::from(e.size);
            if u64::from(e.offset) < index_end || end > source.len() {
                return Err(invalid("entry range outside pack data".into()));
            }
        }
        let mut by_name: Vec<usize> = (0..entries.len()).collect();
        by_name.sort_by(|&a, &b| entries[a].name.cmp(&entries[b].name));
        Ok(Self {
            source,
            entries,
            by_name,
        })
    }
    pub fn entries(&self) -> &[PackEntry] {
        &self.entries
    }
}
impl<S: PackSource> Backend for RangePackBackend<S> {
    fn files(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let e = self
            .by_name
            .binary_search_by(|&i| self.entries[i].name.as_str().cmp(path))
            .ok()
            .map(|i| &self.entries[self.by_name[i]])
            .ok_or_else(|| VfsError::NotFound { path: path.into() })?;
        let mut bytes = vec![0; e.size as usize];
        if !bytes.is_empty() {
            self.source.read_exact_at(u64::from(e.offset), &mut bytes)?;
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vfs;
    use alloc::{boxed::Box, sync::Arc};
    use std::sync::Mutex;
    struct Source {
        bytes: Vec<u8>,
        reads: Arc<Mutex<Vec<(u64, usize)>>>,
    }
    impl PackSource for Source {
        fn len(&self) -> u64 {
            self.bytes.len() as u64
        }
        fn read_exact_at(&self, offset: u64, out: &mut [u8]) -> Result<(), VfsError> {
            self.reads.lock().unwrap().push((offset, out.len()));
            let start = offset as usize;
            let bytes = self
                .bytes
                .get(start..start.saturating_add(out.len()))
                .ok_or_else(|| VfsError::PlatformIo {
                    path: "fixture".into(),
                    code: -1,
                })?;
            out.copy_from_slice(bytes);
            Ok(())
        }
    }
    #[test]
    fn reads_only_the_requested_payload_and_resolves_case() {
        let data = c2_formats::cooked::write_pack(&[
            ("A/ONE".into(), b"hello".to_vec()),
            ("A/TWO".into(), vec![7; 4097]),
        ]);
        let reads = Arc::new(Mutex::new(Vec::new()));
        let pack = RangePackBackend::open(
            Source {
                bytes: data,
                reads: reads.clone(),
            },
            "test",
        )
        .unwrap();
        assert_eq!(reads.lock().unwrap().len(), 2);
        let mut vfs = Vfs::new();
        vfs.push("test", Box::new(pack));
        assert_eq!(vfs.read("a/one").unwrap(), b"hello");
        assert_eq!(reads.lock().unwrap().last().map(|r| r.1), Some(5));
        assert_eq!(vfs.read("a/two").unwrap(), vec![7; 4097]);
    }
    #[test]
    fn rejects_entry_outside_file_and_truncated_index() {
        let original = c2_formats::cooked::write_pack(&[("x".into(), vec![1, 2, 3])]);
        let reads = Arc::new(Mutex::new(Vec::new()));
        let source = |bytes| Source {
            bytes,
            reads: reads.clone(),
        };
        let mut bad = original.clone();
        bad[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            RangePackBackend::open(source(bad), "bad"),
            Err(VfsError::InvalidPack { .. })
        ));
        assert!(matches!(
            RangePackBackend::open(source(original[..20].to_vec()), "bad"),
            Err(VfsError::InvalidPack { .. })
        ));
    }
}
