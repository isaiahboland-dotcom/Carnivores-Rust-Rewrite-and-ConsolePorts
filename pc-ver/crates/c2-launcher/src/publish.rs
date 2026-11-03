//! Recoverable publication for generated directories in launcher-owned storage.
//! A reserved transaction directory also excludes concurrent writers to the same
//! destination. Interrupted transactions are retained for inspection on restart.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) struct Staged {
    transaction: PathBuf,
    pub(crate) path: PathBuf,
    destination: PathBuf,
    backup: PathBuf,
}

fn real_directory(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_dir() => Ok(true),
        Ok(_) => Err(io::Error::other(format!(
            "{} is not a real directory",
            path.display()
        ))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

impl Staged {
    /// `destination` must be generated from a safe single filename in app-owned
    /// storage. Never use this helper to replace a user-selected game folder.
    pub(crate) fn new(destination: &Path) -> Result<Self, String> {
        let parent = destination.parent().ok_or("output has no parent")?;
        // Refuse symlink parents, including ancestors, before writing anything.
        for ancestor in parent.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            real_directory(ancestor).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        real_directory(destination).map_err(|e| e.to_string())?;
        let name = destination.file_name().ok_or("output has no filename")?;
        let transaction = parent.join(format!(".c2rs-install-{}", name.to_string_lossy()));
        fs::create_dir(&transaction).map_err(|e| format!(
            "Cannot reserve {}: {e}. An interrupted install may be present; inspect its data and previous folders before retrying.", transaction.display()
        ))?;
        let path = transaction.join("data");
        if let Err(e) = fs::create_dir(&path) {
            let _ = fs::remove_dir(&transaction);
            return Err(e.to_string());
        }
        Ok(Self {
            backup: transaction.join("previous"),
            transaction,
            path,
            destination: destination.to_path_buf(),
        })
    }

    pub(crate) fn abort(self, error: String) -> String {
        match fs::remove_dir_all(&self.transaction) {
            Ok(()) => error,
            Err(e) => format!(
                "{error}; incomplete output retained at {}: {e}",
                self.transaction.display()
            ),
        }
    }

    pub(crate) fn publish(self) -> Result<Option<String>, String> {
        self.publish_with(
            |from, to| fs::rename(from, to),
            |path| fs::remove_dir_all(path),
        )
    }

    fn publish_with(
        self,
        mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
        cleanup: impl FnOnce(&Path) -> io::Result<()>,
    ) -> Result<Option<String>, String> {
        let had_old = match real_directory(&self.destination) {
            Ok(v) => v,
            Err(e) => return Err(self.abort(e.to_string())),
        };
        if had_old {
            if let Err(e) = rename(&self.destination, &self.backup) {
                return Err(self.abort(format!("Previous install preserved: {e}")));
            }
        }
        if let Err(e) = rename(&self.path, &self.destination) {
            if had_old {
                if let Err(restore) = rename(&self.backup, &self.destination) {
                    return Err(format!("Publish failed: {e}; restore failed: {restore}. Previous install retained at {}; new output at {}", self.backup.display(), self.path.display()));
                }
            }
            return Err(self.abort(format!("Publish failed; previous install preserved: {e}")));
        }
        Ok(cleanup(&self.transaction).err().map(|e| {
            format!(
                "Install published; backup/transaction retained at {}: {e}",
                self.transaction.display()
            )
        }))
    }
}

/// Discover reserved transactions without reading paths from recovery records.
/// Do not delete or rename them automatically: another launcher may be running.
pub(crate) fn pending(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for parent in [
        root.to_path_buf(),
        root.join("mobile"),
        root.join("apk-base"),
        root.join("addons"),
    ] {
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".c2rs-install-")
                {
                    found.push(entry.path());
                }
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    pub(crate) struct Temp(pub PathBuf);
    impl Temp {
        pub(crate) fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "c2-install-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            // Canonicalization avoids platform temp directory aliases.
            Self(p.canonicalize().unwrap())
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn prepared(root: &Path) -> Staged {
        let out = root.join("game");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("old"), b"old").unwrap();
        let stage = Staged::new(&out).unwrap();
        fs::write(stage.path.join("new"), b"new").unwrap();
        stage
    }

    #[test]
    fn build_failure_preserves_install_and_success_replaces_it() {
        let t = Temp::new();
        let stage = prepared(&t.0);
        assert_eq!(stage.abort("build failed".into()), "build failed");
        assert!(t.0.join("game/old").exists());
        let stage = Staged::new(&t.0.join("game")).unwrap();
        fs::write(stage.path.join("new"), b"new").unwrap();
        assert!(stage.publish().unwrap().is_none());
        assert!(t.0.join("game/new").exists());
        assert!(!t.0.join("game/old").exists());
        assert!(pending(&t.0).is_empty());
    }

    #[test]
    fn rename_failures_preserve_or_restore_previous_install() {
        for fail_at in [1, 2] {
            let t = Temp::new();
            let stage = prepared(&t.0);
            let mut calls = 0;
            assert!(stage
                .publish_with(
                    |a, b| {
                        calls += 1;
                        if calls == fail_at {
                            Err(io::Error::other("injected"))
                        } else {
                            fs::rename(a, b)
                        }
                    },
                    |path| fs::remove_dir_all(path)
                )
                .is_err());
            assert_eq!(fs::read(t.0.join("game/old")).unwrap(), b"old");
            assert!(pending(&t.0).is_empty());
        }
    }

    #[test]
    fn failed_restoration_retains_backup_and_blocks_restart_overwrite() {
        let t = Temp::new();
        let stage = prepared(&t.0);
        let backup = stage.backup.clone();
        let mut calls = 0;
        let error = stage
            .publish_with(
                |a, b| {
                    calls += 1;
                    if calls > 1 {
                        Err(io::Error::other("injected"))
                    } else {
                        fs::rename(a, b)
                    }
                },
                |path| fs::remove_dir_all(path),
            )
            .unwrap_err();
        assert!(error.contains("restore failed"));
        assert_eq!(fs::read(backup.join("old")).unwrap(), b"old");
        assert_eq!(pending(&t.0).len(), 1);
        assert!(Staged::new(&t.0.join("game")).is_err());
    }

    #[test]
    fn cleanup_failure_reports_success_and_keeps_backup() {
        let t = Temp::new();
        let stage = prepared(&t.0);
        let backup = stage.backup.clone();
        let warning = stage
            .publish_with(
                |a, b| fs::rename(a, b),
                |_| Err(io::Error::other("injected cleanup failure")),
            )
            .unwrap();
        assert!(warning.is_some());
        assert_eq!(fs::read(t.0.join("game/new")).unwrap(), b"new");
        assert_eq!(fs::read(backup.join("old")).unwrap(), b"old");
        assert_eq!(pending(&t.0).len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_destinations() {
        let t = Temp::new();
        fs::create_dir(t.0.join("original")).unwrap();
        std::os::unix::fs::symlink(t.0.join("original"), t.0.join("game")).unwrap();
        assert!(Staged::new(&t.0.join("game")).is_err());
    }
}
