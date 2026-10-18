//! Inferring a mod's manifest by diffing its files against vanilla.
//!
//! Only files under `HUNTDAT/` count (the game's data); executables, DLLs,
//! logs, saves and wrapper folders next to it are ignored. A file identical
//! to vanilla is ignored too, so a mod shipped as a full copy of the game
//! reduces to what it really changes.

use c2_formats::res_txt::ResTxt;
use c2_vfs::{key, Backend, Vfs};

use crate::manifest::{slug, Manifest};
use crate::res_merge::{block_id, diff, Change};
use crate::ModError;

pub const RES_TXT: &str = "huntdat/_res.txt";

/// Files inside `HUNTDAT` that are never game data: logs, backups, and the
/// map editor's side files (`.grp .lai .mtc .prj`, not read by the game).
fn ignored(k: &str) -> bool {
    let name = k.rsplit('/').next().unwrap_or(k);
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    matches!(
        ext,
        "log" | "bak" | "tmp" | "db" | "ini" | "url" | "grp" | "lai" | "mtc" | "prj"
    )
}

/// Is this key part of the game data?
pub fn is_data(k: &str) -> bool {
    k.starts_with("huntdat/") && !ignored(k)
}

#[derive(Debug, Clone, Default)]
pub struct Detected {
    pub manifest: Manifest,
    /// Data files identical to vanilla (ignored).
    pub identical: usize,
    /// The mod's `_RES.TXT` changes.
    pub res_changes: Vec<Change>,
}

/// Parses a `_RES.TXT` the way the game reads it (lossy text).
pub fn read_res(data: &[u8]) -> Result<ResTxt, ModError> {
    Ok(ResTxt::parse(&String::from_utf8_lossy(data))?)
}

/// Infers the manifest of the mod in `m` (rooted where its `HUNTDAT` is)
/// against `vanilla`.
pub fn detect(vanilla: &Vfs, m: &dyn Backend, name: &str) -> Result<Detected, ModError> {
    let mut d = Detected::default();
    d.manifest.info.id = slug(name);
    d.manifest.info.name = name.to_string();
    let mut files = m.files();
    files.sort_by_key(|f| key(f));
    for f in files {
        let k = key(&f);
        if !is_data(&k) || k == RES_TXT {
            continue;
        }
        match vanilla.resolve(&k) {
            None => d.manifest.files.adds.push(f),
            Some(_) => {
                if vanilla.read(&k)? == m.read(&f)? {
                    d.identical += 1;
                } else {
                    d.manifest.files.replaces.push(f);
                }
            }
        }
    }
    if let Some(f) = m.files().into_iter().find(|f| key(f) == RES_TXT) {
        let v = read_res(&vanilla.read(RES_TXT)?)?;
        let r = read_res(&m.read(&f)?)?;
        d.res_changes = diff(&v, &r);
        let res = &mut d.manifest.res;
        for c in &d.res_changes {
            let (section, id, added) = match c {
                Change::Add { section, block } => (section, block_id(block), true),
                Change::Modify { section, block, .. } => (section, block_id(block), false),
                Change::Entry { .. } => continue,
            };
            let id = id.unwrap_or_else(|| "?".into());
            let list = match (section.to_ascii_lowercase().as_str(), added) {
                ("characters", true) => &mut res.adds_dinos,
                ("characters", false) => &mut res.modifies_dinos,
                ("weapons", true) => &mut res.adds_weapons,
                ("weapons", false) => &mut res.modifies_weapons,
                _ => continue,
            };
            list.push(id);
        }
    }
    Ok(d)
}
