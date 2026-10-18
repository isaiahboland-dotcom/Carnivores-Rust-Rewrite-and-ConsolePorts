//! Combining vanilla and mods into one game ("pack"): layering, the merged
//! `_RES.TXT`, the merge report, and writing the result out as a folder the
//! engine can run from.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use c2_formats::res_txt::ResTxt;
use c2_vfs::{key, Backend, DirBackend, MemBackend, Vfs, VfsError};

use crate::detect::{detect, is_data, read_res, RES_TXT};
use crate::manifest::{slug, Manifest};
use crate::res_merge::{merge, Change, Merged};
use crate::resolve::order;
use crate::ModError;

/// Marks a folder written by [`materialize`] (and so safe to replace).
pub const PACK_MARKER: &str = ".c2rs-pack";
pub const REPORT_FILE: &str = "MERGE_REPORT.md";

/// The folder holding `HUNTDAT` (`dir` itself, or a single subfolder).
pub fn find_root(dir: &Path) -> Option<PathBuf> {
    let has = |d: &Path| {
        std::fs::read_dir(d).ok().is_some_and(|rd| {
            rd.flatten().any(|e| {
                e.file_name().eq_ignore_ascii_case("huntdat")
                    && e.file_type().is_ok_and(|t| t.is_dir())
            })
        })
    };
    if has(dir) {
        return Some(dir.to_path_buf());
    }
    let subs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && has(p))
        .collect();
    match subs.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// Only some files of a directory.
struct Subset {
    dir: DirBackend,
    keep: Vec<String>,
}

impl Backend for Subset {
    fn files(&self) -> Vec<String> {
        self.keep.clone()
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        self.dir.read(path)
    }
    fn disk_path(&self, path: &str) -> Option<PathBuf> {
        self.dir.disk_path(path)
    }
}

/// A mod ready to layer.
pub struct ModSource {
    pub manifest: Manifest,
    pub root: PathBuf,
    /// Data files it adds or replaces (its spelling, relative to `root`).
    pub files: Vec<String>,
    pub res_changes: Vec<Change>,
    /// Files identical to vanilla that were skipped.
    pub identical: usize,
    /// The manifest was inferred (no `mod.toml`).
    pub inferred: bool,
}

/// The vanilla game data as a one-layer VFS.
pub fn vanilla(base: &Path) -> Result<Vfs, ModError> {
    let root = find_root(base).ok_or_else(|| ModError::NoHuntdat(base.to_path_buf()))?;
    // Only the game data: not the executables, DLLs or wrapper files next to it.
    let dir = DirBackend::new(root)?;
    let keep = dir
        .files()
        .into_iter()
        .filter(|f| is_data(&key(f)))
        .collect();
    let mut v = Vfs::new();
    v.push("vanilla", Box::new(Subset { dir, keep }));
    Ok(v)
}

/// Reads a mod folder: its `mod.toml` if any (an empty `[files]` is filled
/// in by detection), and its `_RES.TXT` changes.
pub fn load_mod(vanilla: &Vfs, dir: &Path) -> Result<ModSource, ModError> {
    let root = find_root(dir).ok_or_else(|| ModError::NoHuntdat(dir.to_path_buf()))?;
    let backend = DirBackend::new(&root)?;
    let name = dir
        .file_name()
        .map_or_else(|| "mod".into(), |n| n.to_string_lossy().into_owned());
    let d = detect(vanilla, &backend, &name)?;
    let toml_path = ["mod.toml", "MOD.TOML"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file());
    let (mut manifest, inferred) = match toml_path {
        Some(p) => {
            let text = std::fs::read_to_string(&p).map_err(|source| ModError::Io {
                path: p.clone(),
                source,
            })?;
            let m = Manifest::parse(&text).map_err(|e| ModError::Manifest {
                path: p,
                message: e.to_string(),
            })?;
            (m, false)
        }
        None => (d.manifest.clone(), true),
    };
    if manifest.info.id.is_empty() {
        manifest.info.id = slug(&name);
    }
    if manifest.files.is_empty() {
        manifest.files = d.manifest.files.clone();
    }
    // Resolve the manifest's paths to the mod's own spelling.
    let all = backend.files();
    let mut files = Vec::new();
    for f in manifest.files.adds.iter().chain(&manifest.files.replaces) {
        let k = key(f);
        match all.iter().find(|a| key(a) == k) {
            Some(a) if is_data(&k) && k != RES_TXT => files.push(a.clone()),
            Some(_) => {}
            None => return Err(ModError::MissingFile(root.join(f))),
        }
    }
    Ok(ModSource {
        manifest,
        root,
        files,
        res_changes: d.res_changes,
        identical: d.identical,
        inferred,
    })
}

/// Vanilla plus mods, merged.
pub struct Pack {
    pub vfs: Vfs,
    pub vanilla_res: ResTxt,
    pub merged: Merged,
    /// Mods in load order.
    pub mods: Vec<ModSource>,
}

/// Layers the mods (in dependency order) over vanilla and merges `_RES.TXT`.
pub fn build(base: &Path, mods: Vec<ModSource>) -> Result<Pack, ModError> {
    let mut vfs = vanilla(base)?;
    let vanilla_res = read_res(&vfs.read(RES_TXT)?)?;
    let manifests: Vec<Manifest> = mods.iter().map(|m| m.manifest.clone()).collect();
    let ord = order(&manifests)?;
    let mut slots: Vec<Option<ModSource>> = mods.into_iter().map(Some).collect();
    let mods: Vec<ModSource> = ord.into_iter().filter_map(|i| slots[i].take()).collect();
    for m in &mods {
        vfs.push(
            m.manifest.info.id.clone(),
            Box::new(Subset {
                dir: DirBackend::new(&m.root)?,
                keep: m.files.clone(),
            }),
        );
    }
    let changes: Vec<(String, Vec<Change>)> = mods
        .iter()
        .map(|m| (m.manifest.info.id.clone(), m.res_changes.clone()))
        .collect();
    let merged = merge(&vanilla_res, &changes);
    if changes.iter().any(|(_, c)| !c.is_empty()) {
        let mut mem = MemBackend::new();
        mem.insert("HUNTDAT/_RES.TXT", merged.res.write().into_bytes());
        vfs.push("merged _RES.TXT", Box::new(mem));
    }
    Ok(Pack {
        vfs,
        vanilla_res,
        merged,
        mods,
    })
}

impl Pack {
    /// Files more than one mod provides: (path, mods in load order).
    pub fn file_conflicts(&self) -> Vec<(String, Vec<String>)> {
        let layers = self.vfs.layers();
        self.vfs
            .files()
            .filter_map(|(_, r)| {
                let mods: Vec<String> = r
                    .providers
                    .iter()
                    .filter(|&&l| l > 0 && layers[l].name != "merged _RES.TXT")
                    .map(|&l| layers[l].name.clone())
                    .collect();
                (mods.len() > 1).then(|| (r.path.clone(), mods))
            })
            .collect()
    }

    /// The human-readable merge report (Markdown).
    pub fn report(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "# Merge report\n");
        let _ = writeln!(s, "## Load order\n");
        let _ = writeln!(s, "0. vanilla");
        for (i, m) in self.mods.iter().enumerate() {
            let info = &m.manifest.info;
            let _ = writeln!(
                s,
                "{}. `{}` — {}{}{} ({} file{}, {} identical to vanilla skipped{})",
                i + 1,
                info.id,
                info.name,
                if info.version.is_empty() {
                    String::new()
                } else {
                    format!(" {}", info.version)
                },
                if info.source_url.is_empty() {
                    String::new()
                } else {
                    format!(" <{}>", info.source_url)
                },
                m.files.len(),
                if m.files.len() == 1 { "" } else { "s" },
                m.identical,
                if m.inferred {
                    ", manifest inferred"
                } else {
                    ""
                },
            );
        }
        let conflicts = self.file_conflicts();
        let _ = writeln!(s, "\n## Conflicts\n");
        if conflicts.is_empty() && self.merged.conflicts.is_empty() {
            let _ = writeln!(s, "None.");
        }
        for (p, mods) in &conflicts {
            let _ = writeln!(
                s,
                "- file `{p}`: {} — `{}` wins (last loaded)",
                mods.join(", "),
                mods.last().map_or("", String::as_str)
            );
        }
        for c in &self.merged.conflicts {
            let _ = writeln!(
                s,
                "- `_RES.TXT` {} {}: {} — `{}` wins (last loaded)",
                c.section,
                c.what,
                c.mods.join(", "),
                c.mods.last().map_or("", String::as_str)
            );
        }
        let _ = writeln!(s, "\n## `_RES.TXT` changes\n");
        if self.merged.applied.is_empty() {
            let _ = writeln!(s, "None (vanilla `_RES.TXT`).");
        }
        for (m, c) in &self.merged.applied {
            let _ = writeln!(s, "- `{m}`: {}", c.describe(&self.vanilla_res));
        }
        let _ = writeln!(s, "\n## Files from mods\n");
        let layers = self.vfs.layers();
        let mut any = false;
        for (_, r) in self.vfs.files() {
            if r.layer == 0 || layers[r.layer].name == "merged _RES.TXT" {
                continue;
            }
            any = true;
            let how = if r.providers.first() == Some(&0) {
                "replaces vanilla"
            } else {
                "new"
            };
            let _ = writeln!(s, "- `{}` ← `{}` ({how})", r.path, layers[r.layer].name);
        }
        if !any {
            let _ = writeln!(s, "None.");
        }
        s
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ModError + '_ {
    move |source| ModError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Links (or copies) a file into the pack.
fn place(src: &Path, dst: &Path) -> Result<(), ModError> {
    #[cfg(unix)]
    if std::os::unix::fs::symlink(src, dst).is_ok() {
        return Ok(());
    }
    if std::fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst).map(|_| ()).map_err(io(dst))
}

/// Writes the pack to `out`: every visible file linked (or copied) from its
/// layer, generated files written, plus the merge report. `out` must be
/// empty, missing, or an earlier pack (which is replaced).
pub fn materialize(pack: &Pack, out: &Path) -> Result<(), ModError> {
    if out.exists() {
        let empty = std::fs::read_dir(out).map_err(io(out))?.next().is_none();
        if !empty && !out.join(PACK_MARKER).exists() {
            return Err(ModError::NotAPack(out.to_path_buf()));
        }
        std::fs::remove_dir_all(out).map_err(io(out))?;
    }
    std::fs::create_dir_all(out).map_err(io(out))?;
    for (_, r) in pack.vfs.files() {
        let dst = out.join(&r.path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        match pack.vfs.disk_path(&r.path) {
            Some(src) => {
                let src = std::fs::canonicalize(&src).map_err(io(&src))?;
                place(&src, &dst)?;
            }
            None => std::fs::write(&dst, pack.vfs.read(&r.path)?).map_err(io(&dst))?,
        }
    }
    std::fs::write(out.join(REPORT_FILE), pack.report()).map_err(io(out))?;
    std::fs::write(out.join(PACK_MARKER), "built by c2rs\n").map_err(io(out))?;
    Ok(())
}
