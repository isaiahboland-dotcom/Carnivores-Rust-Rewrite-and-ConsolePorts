//! Combining vanilla and mods into one game ("pack"): layering, the merged
//! `_RES.TXT`, the merge report, and writing the result out as a folder the
//! engine can run from.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use c2_formats::res_txt::ResTxt;
use c2_vfs::{key, Backend, DirBackend, MemBackend, Vfs, VfsError, ZipBackend};

use crate::detect::{detect, is_data, read_res, RES_TXT};
use crate::manifest::{slug, Manifest};
use crate::res_merge::{block_id, merge, Change, Merged};
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

fn is_zip(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Where a mod's game data is: a `.zip` (its `HUNTDAT` found inside when
/// opened), else the folder holding `HUNTDAT` (see [`find_root`]).
pub fn mod_root(path: &Path) -> Option<PathBuf> {
    if is_zip(path) {
        return Some(path.to_path_buf());
    }
    find_root(path)
}

/// The files of a [`mod_root`].
pub fn open_root(root: &Path) -> Result<Box<dyn Backend>, ModError> {
    if is_zip(root) {
        return match ZipBackend::find(root, "huntdat")? {
            Some(z) => Ok(Box::new(z)),
            None => Err(ModError::NoHuntdat(root.to_path_buf())),
        };
    }
    Ok(Box::new(DirBackend::new(root)?))
}

/// Only some files of a layer.
struct Subset {
    dir: Box<dyn Backend>,
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
    let dir: Box<dyn Backend> = Box::new(DirBackend::new(root)?);
    let keep = dir
        .files()
        .into_iter()
        .filter(|f| is_data(&key(f)))
        .collect();
    let mut v = Vfs::new();
    v.push("vanilla", Box::new(Subset { dir, keep }));
    Ok(v)
}

/// Reads a mod folder or `.zip`: its `mod.toml` if any (an empty `[files]`
/// is filled in by detection), and its `_RES.TXT` changes.
pub fn load_mod(vanilla: &Vfs, dir: &Path) -> Result<ModSource, ModError> {
    let root = mod_root(dir).ok_or_else(|| ModError::NoHuntdat(dir.to_path_buf()))?;
    let backend = open_root(&root)?;
    let name = if is_zip(dir) {
        dir.file_stem()
    } else {
        dir.file_name()
    }
    .map_or_else(|| "mod".into(), |n| n.to_string_lossy().into_owned());
    let d = detect(vanilla, backend.as_ref(), &name)?;
    // mod.toml beside HUNTDAT inside a zip, or in the given folder.
    let toml_text = if is_zip(&root) {
        backend
            .files()
            .into_iter()
            .find(|f| key(f) == "mod.toml")
            .map(|f| {
                let data = backend.read(&f)?;
                Ok::<_, ModError>((root.join(&f), String::from_utf8_lossy(&data).into_owned()))
            })
            .transpose()?
    } else {
        ["mod.toml", "MOD.TOML"]
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file())
            .map(|p| {
                std::fs::read_to_string(&p)
                    .map(|t| (p.clone(), t))
                    .map_err(|source| ModError::Io { path: p, source })
            })
            .transpose()?
    };
    let (mut manifest, inferred) = match toml_text {
        Some((p, text)) => {
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
    /// Mods in load order (VFS layers `1..=mods.len()`; later layers are
    /// generated files).
    pub mods: Vec<ModSource>,
    /// Recipe resolutions and patches, for the report.
    pub notes: Vec<String>,
}

/// Recipe decisions applied while building.
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// `(path, mod id)`: this mod's copy of the file wins.
    pub file_winners: Vec<(String, String)>,
    /// `(section, entry name, mod id)`: only this mod's change to the entry applies.
    pub res_winners: Vec<(String, String, String)>,
    /// `(target path, patch name, unified diff)`, applied in order after merging.
    pub patches: Vec<(String, String, String)>,
}

/// Layers the mods (in dependency order) over vanilla and merges `_RES.TXT`.
pub fn build(base: &Path, mods: Vec<ModSource>) -> Result<Pack, ModError> {
    build_with(base, mods, &BuildOptions::default())
}

/// [`build`] with a recipe's resolutions and patches.
pub fn build_with(
    base: &Path,
    mods: Vec<ModSource>,
    opts: &BuildOptions,
) -> Result<Pack, ModError> {
    let mut vfs = vanilla(base)?;
    let mut notes = Vec::new();
    let vanilla_res = read_res(&vfs.read(RES_TXT)?)?;
    let manifests: Vec<Manifest> = mods.iter().map(|m| m.manifest.clone()).collect();
    let ord = order(&manifests)?;
    let mut slots: Vec<Option<ModSource>> = mods.into_iter().map(Some).collect();
    let mut mods: Vec<ModSource> = ord.into_iter().filter_map(|i| slots[i].take()).collect();
    // File winners: drop the file from every other mod.
    for (path, winner) in &opts.file_winners {
        let k = key(path);
        let has = |m: &ModSource| m.files.iter().any(|f| key(f) == k);
        let Some(w) = mods.iter().find(|m| &m.manifest.info.id == winner) else {
            return Err(ModError::Recipe(format!(
                "resolve {path}: no mod '{winner}'"
            )));
        };
        if !has(w) {
            return Err(ModError::Recipe(format!(
                "resolve {path}: '{winner}' does not provide it"
            )));
        }
        let mut losers = Vec::new();
        for m in mods.iter_mut().filter(|m| &m.manifest.info.id != winner) {
            if has(m) {
                m.files.retain(|f| key(f) != k);
                losers.push(m.manifest.info.id.clone());
            }
        }
        notes.push(format!(
            "file `{path}`: `{winner}` chosen by the recipe over {}",
            if losers.is_empty() {
                "nobody".into()
            } else {
                losers.join(", ")
            }
        ));
    }
    for m in &mods {
        vfs.push(
            m.manifest.info.id.clone(),
            Box::new(Subset {
                dir: open_root(&m.root)?,
                keep: m.files.clone(),
            }),
        );
    }
    let mut changes: Vec<(String, Vec<Change>)> = mods
        .iter()
        .map(|m| (m.manifest.info.id.clone(), m.res_changes.clone()))
        .collect();
    // Entry winners: drop other mods' changes to that entry.
    for (section, name, winner) in &opts.res_winners {
        let sec = section.to_ascii_lowercase();
        let id = name.trim().to_ascii_lowercase();
        let mut targets = vec![(sec.clone(), format!("+{id}"))];
        if let Some(i) = vanilla_res.section(section).and_then(|s| {
            s.blocks
                .iter()
                .position(|b| block_id(b).as_deref() == Some(&id))
        }) {
            targets.push((sec.clone(), format!("#{i}")));
        }
        if !changes.iter().any(|(m, _)| m == winner) {
            return Err(ModError::Recipe(format!(
                "resolve {section}:{name}: no mod '{winner}'"
            )));
        }
        for (m, cs) in changes.iter_mut().filter(|(m, _)| m != winner) {
            let before = cs.len();
            cs.retain(|c| !targets.contains(&c.target()));
            if cs.len() != before {
                notes.push(format!(
                    "`_RES.TXT` {section} '{name}': `{winner}` chosen by the recipe over `{m}`"
                ));
            }
        }
    }
    let merged = merge(&vanilla_res, &changes);
    if changes.iter().any(|(_, c)| !c.is_empty()) {
        let mut mem = MemBackend::new();
        mem.insert("HUNTDAT/_RES.TXT", merged.res.write().into_bytes());
        vfs.push("(merged _RES.TXT)", Box::new(mem));
    }
    for (target, name, diff) in &opts.patches {
        let path = if key(target).starts_with("huntdat/") {
            target.clone()
        } else {
            format!("HUNTDAT/{target}")
        };
        let text = String::from_utf8_lossy(&vfs.read(&path)?).into_owned();
        let patched = crate::patch::apply(&text, diff)
            .map_err(|e| ModError::Recipe(format!("patch {name} on {path}: {e}")))?;
        let spelled = vfs.resolve(&path).map_or(path.clone(), |r| r.path.clone());
        let mut mem = MemBackend::new();
        mem.insert(&spelled, patched.into_bytes());
        vfs.push(format!("(patch {name})"), Box::new(mem));
        notes.push(format!("patch `{name}` applied to `{spelled}`"));
    }
    Ok(Pack {
        vfs,
        vanilla_res,
        merged,
        mods,
        notes,
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
                    .filter(|&&l| l > 0 && l <= self.mods.len())
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
        if !self.notes.is_empty() {
            let _ = writeln!(s, "\n## Recipe decisions\n");
            for n in &self.notes {
                let _ = writeln!(s, "- {n}");
            }
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
            if r.layer == 0 || r.layer > self.mods.len() {
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
