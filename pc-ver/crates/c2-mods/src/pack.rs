//! Combining vanilla and mods into one game ("pack"): layering, the merged
//! `_RES.TXT`, the merge report, and writing the result out as a folder the
//! engine can run from.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use c2_formats::res_txt::{Block, ResTxt};
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

/// `rel` (either slash, any case) under `base`, if it exists.
pub fn find_path(base: &Path, rel: &str) -> Option<PathBuf> {
    rel.split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
        .try_fold(base.to_path_buf(), |d, c| {
            std::fs::read_dir(&d)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(c))
                })
        })
}

fn is_zip(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// The add-ons a folder given as one stands for: a collection (no files
/// of its own, and three or more add-on folders in it, or two not named
/// like versions of one add-on: "MEE", "Legacy"...) is its add-ons, each
/// on its own; anything else is itself. Read as one, a collection's add-ons
/// would trample each other's menu files.
pub fn expand_addons(dir: &Path) -> Vec<PathBuf> {
    let itself = || vec![dir.to_path_buf()];
    if !dir.is_dir() || find_root(dir).as_deref() == Some(dir) {
        return itself();
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return itself();
    };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    let hidden = |p: &PathBuf| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
    };
    if entries.iter().any(|p| p.is_file() && !hidden(p)) {
        return itself();
    }
    let mut subs: Vec<PathBuf> = entries
        .into_iter()
        .filter(|p| p.is_dir() && !hidden(p))
        .filter(|p| mod_root(p).is_some() || crate::loose::is_loose(p))
        .collect();
    subs.sort();
    let versions = subs.iter().any(|p| {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        [
            "mee",
            "modder",
            "legacy",
            "classic",
            "vanilla",
            "speed patch",
            "version",
        ]
        .iter()
        .any(|w| n.contains(w))
    });
    if subs.len() >= 3 || (subs.len() == 2 && !versions) {
        subs
    } else {
        itself()
    }
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
#[derive(Default)]
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
    /// A loose add-on (`crate::loose`): its files' game paths -> disk
    /// (`files` holds the game paths then).
    pub map: Vec<(String, PathBuf)>,
    /// A loose add-on's hunter model, merged at build time.
    pub hunter: Option<PathBuf>,
    /// How a loose add-on was read, for the report.
    pub notes: Vec<String>,
    /// Loose add-on entries that get a menu slot of their own.
    pub slots: Vec<crate::loose::SlotAdd>,
    /// A loose add-on's text files its entries came from.
    pub texts: Vec<PathBuf>,
}

impl ModSource {
    /// Read as a loose add-on (not from a `HUNTDAT` layout).
    pub fn is_loose(&self) -> bool {
        !self.map.is_empty() || self.hunter.is_some() || !self.texts.is_empty()
    }

    /// The files on disk a loose add-on was built from.
    pub fn loose_inputs(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.map.iter().map(|(_, p)| p.clone()).collect();
        v.extend(self.hunter.iter().cloned());
        v.extend(self.texts.iter().cloned());
        v.sort();
        v.dedup();
        v
    }
}

/// Reads a loose add-on (`crate::loose`) as a mod, from the files `keep`
/// allows.
pub(crate) fn load_loose(
    vanilla: &Vfs,
    dir: &Path,
    keep: &dyn Fn(&Path) -> bool,
) -> Result<ModSource, ModError> {
    let l = crate::loose::read_only(dir, vanilla, keep)?;
    let name = dir
        .file_name()
        .map_or_else(|| "mod".into(), |n| n.to_string_lossy().into_owned());
    let mut manifest = Manifest::default();
    manifest.info.id = slug(&name);
    manifest.info.name = name;
    for (p, _) in &l.files {
        if vanilla.resolve(&key(p)).is_some() {
            manifest.files.replaces.push(p.clone());
        } else {
            manifest.files.adds.push(p.clone());
        }
    }
    for c in &l.res_changes {
        let (section, b, added) = match c {
            Change::Add { section, block } => (section, block, true),
            Change::Modify { section, block, .. } => (section, block, false),
            Change::Entry { .. } => continue,
        };
        let id = block_id(b).unwrap_or_else(|| "?".into());
        let r = &mut manifest.res;
        match (section.as_str(), added) {
            ("characters", true) => r.adds_dinos.push(id),
            ("characters", false) => r.modifies_dinos.push(id),
            ("weapons", true) => r.adds_weapons.push(id),
            _ => r.modifies_weapons.push(id),
        }
    }
    Ok(ModSource {
        manifest,
        root: dir.to_path_buf(),
        files: l.files.iter().map(|(p, _)| p.clone()).collect(),
        res_changes: l.res_changes,
        identical: 0,
        inferred: true,
        map: l.files,
        hunter: l.hunter,
        notes: l.notes,
        slots: l.slots,
        texts: l.texts,
    })
}

/// The base game data: the game folder, then any overlays over it (a
/// release shipped as changed files only, such as the Modder's Edition,
/// copied over the original game). A later layer's file wins, `_RES.TXT`
/// included.
pub fn vanilla(bases: &[PathBuf]) -> Result<Vfs, ModError> {
    let mut v = Vfs::new();
    for (i, base) in bases.iter().enumerate() {
        let root = find_root(base).ok_or_else(|| ModError::NoHuntdat(base.to_path_buf()))?;
        // Only the game data: not the executables, DLLs or wrapper files next to it.
        let dir: Box<dyn Backend> = Box::new(DirBackend::new(root)?);
        let keep = dir
            .files()
            .into_iter()
            .filter(|f| is_data(&key(f)))
            .collect();
        let name = if i == 0 {
            "vanilla".to_string()
        } else {
            format!("base overlay {}", base.display())
        };
        v.push(name, Box::new(Subset { dir, keep }));
    }
    // Carnivores: Ice Age plays as a dialect game (its rules rewritten).
    if let Some(l) = crate::ice_age::layer(&v)? {
        v.push(crate::ice_age::LAYER, Box::new(l));
    }
    Ok(v)
}

/// Reads a mod folder or `.zip`: its `mod.toml` if any (an empty `[files]`
/// is filled in by detection), and its `_RES.TXT` changes.
pub fn load_mod(vanilla: &Vfs, dir: &Path) -> Result<ModSource, ModError> {
    let Some(root) = mod_root(dir) else {
        if crate::loose::is_loose(dir) {
            return load_loose(vanilla, dir, &|_| true);
        }
        return Err(ModError::NoHuntdat(dir.to_path_buf()));
    };
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
        ..Default::default()
    })
}

/// Vanilla plus mods, merged.
pub struct Pack {
    pub vfs: Vfs,
    pub vanilla_res: ResTxt,
    pub merged: Merged,
    /// Base layers (the game folder and its overlays): VFS layers
    /// `0..bases`.
    pub bases: usize,
    /// Mods in load order (VFS layers `bases..bases + mods.len()`; later
    /// layers are generated files).
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

const MENU_TXT: &str = "huntdat/_menu.txt";

/// `CALL7_A.WAV` with prefix `call`, from 7 to 11 -> `CALL11_A.WAV`.
fn renumber(name: &str, prefixes: &[&str], from: usize, to: usize) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let p = prefixes.iter().find(|p| lower.starts_with(*p))?;
    let rest = &name[p.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.parse::<usize>().ok()? != from {
        return None;
    }
    Some(format!("{}{to}{}", &name[..p.len()], &rest[digits.len()..]))
}

/// `overwrite char6 area2` -> `overwrite char10 area2` (menu-slot
/// conditions follow an entry to its new slot).
fn retarget_conditions(b: &mut Block, from: usize, to: usize) {
    let (old, new) = (format!("char{from}"), format!("char{to}"));
    for s in &mut b.subs {
        s.name = s
            .name
            .split_whitespace()
            .map(|w| {
                if w.eq_ignore_ascii_case(&old) {
                    new.as_str()
                } else {
                    w
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        retarget_conditions(&mut s.block, from, to);
    }
}

/// Gives loose add-on entries made for a menu slot (`loose::SlotAdd`) a
/// slot of their own, the next free one in load order: their menu files
/// (`DINO7.TGA`, `CALL7_A.WAV`, `WEAPON1.TXT`, `BULLET1.TGA`) are renumbered,
/// their slot conditions (`overwrite char6`) and `callNo` / `pic` follow,
/// and the menu gets the entry at the price of the slot it was made for
/// (`_MENU.TXT` when the base has one, else `_RES.TXT`'s `prices`).
/// Returns each mod's `_MENU.TXT` changes.
fn assign_slots(
    vfs: &Vfs,
    res: &ResTxt,
    mods: &mut [ModSource],
    notes: &mut Vec<String>,
) -> Result<Vec<(String, Vec<Change>)>, ModError> {
    let menu_res = match vfs.resolve(MENU_TXT) {
        Some(r) => Some(read_res(&vfs.read(&r.path.clone())?)?),
        None => None,
    };
    let menu = c2_formats::menu_txt::MenuData::from_res(menu_res.as_ref().unwrap_or(res));
    let (mut dinos, mut weapons) = (menu.huntables.len(), menu.weapons.len());
    let mut out = Vec::new();
    for m in mods.iter_mut() {
        let mut mc = Vec::new();
        for s in m.slots.clone() {
            let chars = s.section == "characters";
            let n = if chars {
                dinos += 1;
                dinos
            } else {
                weapons += 1;
                weapons
            };
            let prefixes: &[&str] = if chars {
                &["dino", "call"]
            } else {
                &["weapon", "bullet"]
            };
            for (p, _) in &mut m.map {
                let (dir, name) = p.rsplit_once('/').unwrap_or(("", p.as_str()));
                if let Some(new) = renumber(name, prefixes, s.from, n) {
                    *p = format!("{dir}/{new}");
                }
            }
            m.files = m.map.iter().map(|(p, _)| p.clone()).collect();
            let slot_price = if chars {
                menu.huntables.get(s.from - 1).map(|i| i.price)
            } else {
                menu.weapons.get(s.from - 1).map(|i| i.price)
            }
            .unwrap_or(0);
            let Some(block) = m.res_changes.iter_mut().find_map(|c| match c {
                Change::Add { section, block }
                    if *section == s.section && block_id(block).as_deref() == Some(&s.id) =>
                {
                    Some(block)
                }
                _ => None,
            }) else {
                continue;
            };
            if chars {
                retarget_conditions(block, s.from - 1, n - 1);
                // It answers its own slot's call (`callNo`; an entry for the
                // original game has none, and would answer slot 0's).
                if block.get(&["callNo"]).is_none() {
                    block.entries.push(c2_formats::res_txt::Entry {
                        key: "callNo".into(),
                        value: (n - 1).to_string(),
                    });
                }
            }
            for e in &mut block.entries {
                let k = e.key.to_ascii_lowercase();
                if chars && k == "callno" {
                    e.value = (n - 1).to_string();
                }
                if !chars && k == "pic" {
                    let v = e.value.trim().trim_matches('\'');
                    if let Some(new) = renumber(v, &["bullet"], s.from, n) {
                        e.value = format!("'{new}'");
                    }
                }
            }
            // Its own `price` if the entry has one (imported mobile animals),
            // else the price of the slot it was made for.
            let price = block
                .get(&["price"])
                .map(c2_formats::res_txt::c_atoi)
                .unwrap_or(slot_price);
            let price = Change::Entry {
                section: "prices".into(),
                key: if chars { "dino" } else { "weapon" }.into(),
                n: n - 1,
                value: price.to_string(),
            };
            if menu_res.is_some() {
                // The menu's own list: the entry's name and menu stats.
                let keep: &[&str] = if chars {
                    &["name", "ai", "smell", "hear", "look", "pic"]
                } else {
                    &["name", "power", "prec", "loud", "rate"]
                };
                let entry = Block {
                    entries: block
                        .entries
                        .iter()
                        .filter(|e| keep.iter().any(|k| e.key.eq_ignore_ascii_case(k)))
                        .cloned()
                        .collect(),
                    ..Default::default()
                };
                mc.push(Change::Add {
                    section: s.section.clone(),
                    block: entry,
                });
                mc.push(price);
            } else {
                m.res_changes.push(price);
            }
            notes.push(format!(
                "`{}`: '{}' added as {} {n}, alongside '{}' (made for slot {}; its menu files renumbered)",
                m.manifest.info.id,
                s.id,
                if chars { "huntable" } else { "weapon" },
                s.alongside,
                s.from
            ));
        }
        out.push((m.manifest.info.id.clone(), mc));
    }
    Ok(out)
}

/// Merges the hunter models loose add-ons ship (`crate::hunter`) into the
/// base one, moving each add-on's kill animations to their new numbers.
/// The merged model's path and bytes, if any add-on had one.
fn merge_hunters(
    vfs: &mut Vfs,
    res: &ResTxt,
    mods: &mut [ModSource],
    notes: &mut Vec<String>,
) -> Result<Option<(String, Vec<u8>)>, ModError> {
    if mods.iter().all(|m| m.hunter.is_none()) {
        return Ok(None);
    }
    let Some(file) = res
        .section("characters")
        .and_then(|s| s.blocks.first())
        .and_then(|b| b.get(&["file"]))
        .map(|f| f.trim().trim_matches('\'').replace('\\', "/"))
    else {
        notes.push("no hunter entry in the base `_RES.TXT`: add-on hunters left out".into());
        return Ok(None);
    };
    let path = vfs
        .resolve(&format!("huntdat/{file}"))
        .map_or_else(|| format!("HUNTDAT/{file}"), |r| r.path.clone());
    let mut base = c2_formats::car::parse(&vfs.read(&path)?)?;
    for m in mods.iter_mut() {
        let Some(h) = &m.hunter else { continue };
        let data = std::fs::read(h).map_err(|source| ModError::Io {
            path: h.clone(),
            source,
        })?;
        let extra = c2_formats::car::parse(&data)?;
        let id = &m.manifest.info.id;
        match crate::hunter::merge(&mut base, std::slice::from_ref(&extra)).remove(0) {
            crate::hunter::Outcome::Merged { map } => {
                crate::hunter::remap(&mut m.res_changes, &map);
                notes.push(format!(
                    "`{id}`: its hunter model's animations merged into `{path}` (now {})",
                    base.animations.len()
                ));
            }
            crate::hunter::Outcome::Incompatible(why) => notes.push(format!(
                "`{id}`: its hunter model is left out ({why}); its kills may show the wrong animation"
            )),
        }
    }
    Ok(Some((path, c2_formats::car::write(&base))))
}

/// Layers the mods (in dependency order) over vanilla and merges `_RES.TXT`.
pub fn build(bases: &[PathBuf], mods: Vec<ModSource>) -> Result<Pack, ModError> {
    build_with(bases, mods, &BuildOptions::default())
}

/// [`build`] with a recipe's resolutions and patches.
pub fn build_with(
    bases: &[PathBuf],
    mods: Vec<ModSource>,
    opts: &BuildOptions,
) -> Result<Pack, ModError> {
    let mut vfs = vanilla(bases)?;
    let n_bases = bases.len();
    let mut notes = Vec::new();
    let vanilla_res = read_res(&vfs.read(RES_TXT)?)?;
    // Two add-ons whose ids are the same (ids made up from folder names: a
    // Dilophosaurus add-on and the mobile import's): the later one is told
    // apart, not refused.
    let mut mods = mods;
    let mut seen = std::collections::HashSet::new();
    for m in &mut mods {
        let id = m.manifest.info.id.clone();
        if !seen.insert(id.clone()) {
            let new = (2..)
                .map(|n| format!("{id}-{n}"))
                .find(|n| !seen.contains(n))
                .unwrap_or_default();
            notes.push(format!(
                "two add-ons are both `{id}`: the later one is `{new}`"
            ));
            seen.insert(new.clone());
            m.manifest.info.id = new;
        }
    }
    // Animals imported from the mobile games load first: where a PC add-on
    // brings the same animal, the PC one wins (PC is the default where both
    // exist, owner's rule).
    mods.sort_by_key(|m| !crate::mobile::is_import(&m.root));
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
    let menu_changes = assign_slots(&vfs, &vanilla_res, &mut mods, &mut notes)?;
    for m in &mods {
        if !m.notes.is_empty() {
            for n in &m.notes {
                notes.push(format!("`{}`: {n}", m.manifest.info.id));
            }
        }
        let layer: Box<dyn Backend> = if m.map.is_empty() {
            Box::new(Subset {
                dir: open_root(&m.root)?,
                keep: m.files.clone(),
            })
        } else {
            Box::new(crate::loose::MapBackend {
                map: m
                    .map
                    .iter()
                    .filter(|(p, _)| m.files.contains(p))
                    .cloned()
                    .collect(),
            })
        };
        vfs.push(m.manifest.info.id.clone(), layer);
    }
    let hunter = merge_hunters(&mut vfs, &vanilla_res, &mut mods, &mut notes)?;
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
    if menu_changes.iter().any(|(_, c)| !c.is_empty()) {
        if let Some(r) = vfs.resolve(MENU_TXT) {
            let path = r.path.clone();
            let menu = read_res(&vfs.read(&path)?)?;
            let m = merge(&menu, &menu_changes);
            let mut mem = MemBackend::new();
            mem.insert(&path, m.res.write().into_bytes());
            vfs.push("(merged _MENU.TXT)", Box::new(mem));
        }
    }
    if let Some((path, data)) = hunter {
        let mut mem = MemBackend::new();
        mem.insert(&path, data);
        vfs.push("(merged hunter)", Box::new(mem));
    }
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
        bases: n_bases,
        mods,
        notes,
    })
}

impl Pack {
    fn is_mod_layer(&self, l: usize) -> bool {
        l >= self.bases && l < self.bases + self.mods.len()
    }

    /// Files more than one mod provides: (path, mods in load order).
    pub fn file_conflicts(&self) -> Vec<(String, Vec<String>)> {
        let layers = self.vfs.layers();
        self.vfs
            .files()
            .filter_map(|(_, r)| {
                let mods: Vec<String> = r
                    .providers
                    .iter()
                    .filter(|&&l| self.is_mod_layer(l))
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
        let layers = self.vfs.layers();
        for (i, l) in layers.iter().take(self.bases).enumerate() {
            let _ = writeln!(s, "{i}. {}", l.name);
        }
        for (i, m) in self.mods.iter().enumerate() {
            let info = &m.manifest.info;
            let _ = writeln!(
                s,
                "{}. `{}` — {}{}{} ({} file{}, {} identical to vanilla skipped{})",
                i + self.bases,
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
            let _ = writeln!(s, "\n## Decisions\n");
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
            if !self.is_mod_layer(r.layer) {
                continue;
            }
            any = true;
            let how = if r.providers.first().is_some_and(|&l| l < self.bases) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_of_add_ons_is_its_add_ons_but_versions_are_one() {
        let root = std::env::temp_dir().join(format!("c2rs-expand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let addon = |p: &Path| {
            std::fs::create_dir_all(p).unwrap();
            std::fs::write(p.join("x.car"), b"").unwrap();
        };
        // A collection: three add-ons, no files of its own.
        let all = root.join("all");
        for n in ["A", "B", "C"] {
            addon(&all.join(n).join("inner"));
        }
        let got = expand_addons(&all);
        assert_eq!(got.len(), 3);
        // One add-on with two versions: itself.
        let one = root.join("one");
        addon(&one.join("Thing MEE"));
        addon(&one.join("Thing Legacy"));
        assert_eq!(expand_addons(&one), std::slice::from_ref(&one));
        // A plain add-on: itself.
        let plain = root.join("plain");
        addon(&plain);
        assert_eq!(expand_addons(&plain), std::slice::from_ref(&plain));
        let _ = std::fs::remove_dir_all(&root);
    }
}
