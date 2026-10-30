//! Recipes (docs/DEVELOPMENT.md §6): a mod combination described by credits, URLs and
//! file hashes, never by the mods' content. A user downloads the listed mods,
//! `verify` checks their copies against the hashes, `prepare` turns the recipe
//! into a build (load order, resolutions, patches).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use c2_vfs::{key, Backend, DirBackend, Vfs};

use crate::detect::{is_data, read_res, RES_TXT};
use crate::manifest::{slug, Manifest};
use crate::pack::{load_loose, mod_root, open_root, BuildOptions, ModSource};
use crate::res_merge::diff;
use crate::ModError;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recipe {
    pub recipe: Info,
    /// Releases copied over the base game first (the Modder's Edition):
    /// pinned like mods, files from the release's folder.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<RecipeMod>,
    /// Standalone games combined beside the base (README M16), files from
    /// each game's folder; `mods` are then add-ons over the combined game.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub standalones: Vec<RecipeMod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combine: Option<Combine>,
    pub mods: Vec<RecipeMod>,
    pub load_order: LoadOrder,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resolve: Vec<Resolve>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patches: Vec<PatchRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Expected base game: `bases/<base>.toml` next to the recipe lists its hashes.
    pub base: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeMod {
    pub id: String,
    pub credit: String,
    pub url: String,
    /// Folder name hint under `--mods` (defaults to a folder named like `id`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub folder: String,
    /// A loose add-on (`crate::loose`): `files` are paths from the add-on's
    /// folder (its models, menu files, sounds and the texts holding its
    /// entries), and it is installed from those files only.
    #[serde(skip_serializing_if = "is_false")]
    pub loose: bool,
    /// `path = "sha256:<hex>"` for every file used (paths from the mod's
    /// `HUNTDAT` parent, or a loose add-on's folder). Only these files are
    /// taken from the mod.
    pub files: BTreeMap<String, String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How standalones are combined.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Combine {
    /// `tied` (each game's ambient animals on its own maps, the default)
    /// or `random` (on every map).
    pub ambients: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoadOrder {
    pub order: Vec<String>,
}

/// An explicit conflict resolution: a file (`path`) or a `_RES.TXT` entry
/// (`entry = "characters:T-Rex"`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resolve {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub entry: String,
    pub winner: String,
}

/// One of the recipe's own text patches (a unified diff, path relative to the recipe).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchRef {
    pub target: String,
    pub patch: String,
}

/// A base game's file list (`recipes/bases/*.toml`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Base {
    pub base: BaseInfo,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BaseInfo {
    pub id: String,
    pub note: String,
}

impl Recipe {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn load(path: &Path) -> Result<Self, ModError> {
        let text = std::fs::read_to_string(path).map_err(|source| ModError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text).map_err(|e| ModError::Manifest {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

pub fn sha256(data: &[u8]) -> String {
    let d = Sha256::digest(data);
    let mut s = String::with_capacity(71);
    s.push_str("sha256:");
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// How one recipe mod matched a folder.
#[derive(Debug, Clone, Default)]
pub struct Located {
    pub id: String,
    pub url: String,
    /// The mod's root (where its `HUNTDAT` is), if a folder was found.
    pub root: Option<PathBuf>,
    /// Listed files: `path -> spelling in the folder`, for those that match.
    pub ok: BTreeMap<String, String>,
    pub missing: Vec<String>,
    /// `(path, expected, found)`.
    pub mismatched: Vec<(String, String, String)>,
}

impl Located {
    pub fn complete(&self) -> bool {
        self.root.is_some() && self.missing.is_empty() && self.mismatched.is_empty()
    }
}

fn check(m: &RecipeMod, root: &Path, dir: &dyn Backend) -> Located {
    let index: BTreeMap<String, String> = dir.files().into_iter().map(|f| (key(&f), f)).collect();
    let mut l = Located {
        id: m.id.clone(),
        url: m.url.clone(),
        root: Some(root.to_path_buf()),
        ..Default::default()
    };
    for (path, want) in &m.files {
        match index.get(&key(path)) {
            None => l.missing.push(path.clone()),
            Some(f) => match dir.read(f) {
                Ok(data) => {
                    let got = sha256(&data);
                    if got.eq_ignore_ascii_case(want) {
                        l.ok.insert(path.clone(), f.clone());
                    } else {
                        l.mismatched.push((path.clone(), want.clone(), got));
                    }
                }
                Err(_) => l.missing.push(path.clone()),
            },
        }
    }
    l
}

/// A whole game folder (a standalone or an overlay release) as a recipe
/// entry: every data file under its game folder (the one holding
/// `HUNTDAT`), credit and URL to fill in.
pub fn pin_game(dir: &Path) -> Result<RecipeMod, ModError> {
    let root =
        crate::pack::find_game_root(dir).ok_or_else(|| ModError::NoHuntdat(dir.to_path_buf()))?;
    let b = open_root(&root)?;
    let mut files = BTreeMap::new();
    for f in b.files() {
        if is_data(&key(&f)) {
            files.insert(f.clone(), sha256(&b.read(&f)?));
        }
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(RecipeMod {
        id: slug(&name),
        folder: name,
        files,
        ..Default::default()
    })
}

/// Finds each of `games` (standalones or overlays) under `mods_dir`, as
/// [`locate`] does mods, its game folder possibly a few folders down.
pub fn locate_games(games: &[RecipeMod], mods_dir: &Path) -> Vec<Located> {
    let mut cands: Vec<(String, PathBuf, Box<dyn Backend>, bool)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(mods_dir) {
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        subs.sort();
        for d in subs {
            let name = d
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            if let Some(root) = crate::pack::find_game_root(&d) {
                if let Ok(b) = open_root(&root) {
                    cands.push((name.clone(), root.clone(), b, false));
                }
                // Keep the extracted package as a second lookup view. Recipes
                // pinned from an unpacked download can include its wrapper
                // directory in every path, while `open_root` starts at HUNTDAT's
                // parent and omits that prefix. `root` remains the actual game
                // directory used later when building the recipe.
                if let Ok(b) = DirBackend::new(&d) {
                    cands.push((name, root, Box::new(b), false));
                }
            }
        }
    }
    locate_among(games, &cands)
}

/// Finds each recipe mod under `mods_dir`: the folder named by `folder` or
/// like the id, else the folder where most listed files match their hashes.
pub fn locate(r: &Recipe, mods_dir: &Path) -> Vec<Located> {
    // (folder name, root, files, loose add-on)
    let mut cands: Vec<(String, PathBuf, Box<dyn Backend>, bool)> = Vec::new();
    let mut dirs = vec![mods_dir.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(mods_dir) {
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
            .collect();
        subs.sort();
        dirs.extend(subs);
    }
    for d in dirs {
        let name = if d.is_file() {
            d.file_stem()
        } else {
            d.file_name()
        }
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if let Some(root) = mod_root(&d) {
            if let Ok(b) = open_root(&root) {
                cands.push((name.clone(), root, b, false));
            }
        }
        // Recipe loose-file paths are relative to the extracted download,
        // which can contain a nested HUNTDAT (or only map files, with no
        // .car/.txt for `is_loose` to detect). Keep the download folder as
        // a candidate even when `mod_root` also found a game-data root.
        if d.is_dir() {
            if let Ok(b) = DirBackend::new(&d) {
                cands.push((name, d, Box::new(b), true));
            }
        }
    }
    locate_among(&r.mods, &cands)
}

/// Each of `list` among the candidate folders: `(name, root, files, loose)`.
fn locate_among(
    list: &[RecipeMod],
    cands: &[(String, PathBuf, Box<dyn Backend>, bool)],
) -> Vec<Located> {
    list.iter()
        .map(|m| {
            let kind = cands.iter().filter(|c| c.3 == m.loose);
            let is_named = |n: &str| {
                (!m.folder.is_empty() && n.eq_ignore_ascii_case(&m.folder)) || slug(n) == m.id
            };
            let named: Vec<_> = kind.clone().filter(|(n, ..)| is_named(n)).collect();
            let named_checks: Vec<_> = named
                .iter()
                .map(|(_, root, b, _)| check(m, root, b.as_ref()))
                .collect();
            if let Some(found) = named_checks.iter().find(|l| l.complete()) {
                return found.clone();
            }
            if let Some(found) = named_checks
                .into_iter()
                .filter(|l| !l.ok.is_empty() || !l.mismatched.is_empty())
                .max_by_key(|l| l.ok.len())
            {
                return found;
            }
            kind.map(|(_, root, b, _)| check(m, root, b.as_ref()))
                .filter(|l| !l.ok.is_empty() || !l.mismatched.is_empty())
                .max_by_key(|l| (l.ok.len(), l.mismatched.len()))
                .unwrap_or_else(|| Located {
                    id: m.id.clone(),
                    url: m.url.clone(),
                    missing: m.files.keys().cloned().collect(),
                    ..Default::default()
                })
        })
        .collect()
}

/// The base profile `bases/<base>.toml` beside the recipe, if any.
pub fn load_base(recipe_path: &Path, r: &Recipe) -> Option<Base> {
    if r.recipe.base.is_empty() {
        return None;
    }
    let p = recipe_path
        .parent()?
        .join("bases")
        .join(format!("{}.toml", r.recipe.base));
    toml::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// Base files missing or different in the user's game: `(missing, mismatched)`.
pub fn check_base(base: &Base, vanilla: &Vfs) -> (Vec<String>, Vec<String>) {
    let (mut missing, mut mismatched) = (Vec::new(), Vec::new());
    for (path, want) in &base.files {
        match vanilla.read(path) {
            Err(_) => missing.push(path.clone()),
            Ok(d) => {
                if !sha256(&d).eq_ignore_ascii_case(want) {
                    mismatched.push(path.clone());
                }
            }
        }
    }
    (missing, mismatched)
}

/// A base profile listing every data file of `vanilla`.
pub fn base_profile(id: &str, note: &str, vanilla: &Vfs) -> Result<Base, ModError> {
    let mut files = BTreeMap::new();
    for (_, r) in vanilla.files() {
        files.insert(r.path.clone(), sha256(&vanilla.read(&r.path)?));
    }
    Ok(Base {
        base: BaseInfo {
            id: id.into(),
            note: note.into(),
        },
        files,
    })
}

/// A recipe for these mods (as loaded), pinning every file they use.
pub fn new_recipe(
    id: &str,
    name: &str,
    base: &str,
    mods: &[ModSource],
) -> Result<Recipe, ModError> {
    let mut r = Recipe {
        recipe: Info {
            id: id.into(),
            name: name.into(),
            version: "0.1".into(),
            base: base.into(),
        },
        ..Default::default()
    };
    for m in mods {
        let mut files = BTreeMap::new();
        if m.is_loose() {
            for p in m.loose_inputs() {
                let rel = p.strip_prefix(&m.root).unwrap_or(&p);
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                let data = std::fs::read(&p).map_err(|source| ModError::Io {
                    path: p.clone(),
                    source,
                })?;
                files.insert(rel, sha256(&data));
            }
        } else {
            let dir = open_root(&m.root)?;
            let mut used = m.files.clone();
            if !m.res_changes.is_empty() {
                if let Some(f) = dir.files().into_iter().find(|f| key(f) == RES_TXT) {
                    used.push(f);
                }
            }
            for f in used {
                files.insert(f.clone(), sha256(&dir.read(&f)?));
            }
        }
        let info = &m.manifest.info;
        r.mods.push(RecipeMod {
            id: info.id.clone(),
            credit: info.authors.join(", "),
            url: info.source_url.clone(),
            folder: if m.root.is_file() {
                m.root.file_stem()
            } else {
                m.root.file_name()
            }
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| slug(n) != info.id && !n.eq_ignore_ascii_case("huntdat"))
            .unwrap_or_default(),
            loose: m.is_loose(),
            files,
        });
        r.load_order.order.push(info.id.clone());
    }
    Ok(r)
}

/// A patch path inside the recipe's folder (no absolute paths or `..`).
fn patch_path(recipe_path: &Path, p: &str) -> Result<PathBuf, ModError> {
    let rel = Path::new(p);
    if rel
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(ModError::Recipe(format!(
            "patch path {p:?} must stay inside the recipe folder"
        )));
    }
    Ok(recipe_path.parent().unwrap_or(Path::new(".")).join(rel))
}

/// Turns a verified recipe into mods (in the recipe's order) and build options.
pub fn prepare(
    recipe_path: &Path,
    r: &Recipe,
    located: &[Located],
    vanilla: &Vfs,
) -> Result<(Vec<ModSource>, BuildOptions), ModError> {
    if let Some(bad) = located.iter().find(|l| !l.complete()) {
        return Err(ModError::Recipe(format!(
            "mod '{}' is missing or doesn't match (get it from {})",
            bad.id,
            if bad.url.is_empty() { "?" } else { &bad.url }
        )));
    }
    let vres = read_res(&vanilla.read(RES_TXT)?)?;
    let mut sources = Vec::new();
    for (m, l) in r.mods.iter().zip(located) {
        let Some(root) = &l.root else { continue };
        if m.loose {
            // Installed as the loose reader would, from the pinned files only.
            let pinned: Vec<PathBuf> = l.ok.values().map(|f| root.join(f)).collect();
            let mut s = load_loose(vanilla, root, &|p| pinned.iter().any(|q| q == p))?;
            let info = &mut s.manifest.info;
            info.id = m.id.clone();
            info.authors = vec![m.credit.clone()];
            info.source_url = m.url.clone();
            s.inferred = false;
            sources.push(s);
            continue;
        }
        let dir = open_root(root)?;
        let mut files = Vec::new();
        let mut res_changes = Vec::new();
        for f in l.ok.values() {
            let k = key(f);
            if k == RES_TXT {
                res_changes = diff(&vres, &read_res(&dir.read(f)?)?);
            } else if is_data(&k) {
                files.push(f.clone());
            }
        }
        let mut manifest = Manifest::default();
        manifest.info.id = m.id.clone();
        manifest.info.name = m.id.clone();
        manifest.info.authors = vec![m.credit.clone()];
        manifest.info.source_url = m.url.clone();
        sources.push(ModSource {
            manifest,
            root: root.clone(),
            files,
            res_changes,
            identical: 0,
            inferred: false,
            ..Default::default()
        });
    }
    if !r.load_order.order.is_empty() {
        let ids: Vec<&String> = sources.iter().map(|s| &s.manifest.info.id).collect();
        for id in &r.load_order.order {
            if !ids.contains(&id) {
                return Err(ModError::Recipe(format!(
                    "load_order names unknown mod '{id}'"
                )));
            }
        }
        for id in ids {
            if !r.load_order.order.contains(id) {
                return Err(ModError::Recipe(format!(
                    "mod '{id}' is missing from load_order"
                )));
            }
        }
        sources.sort_by_key(|s| {
            r.load_order
                .order
                .iter()
                .position(|o| *o == s.manifest.info.id)
        });
    }
    let mut opts = BuildOptions::default();
    for res in &r.resolve {
        if !res.path.is_empty() {
            opts.file_winners
                .push((res.path.clone(), res.winner.clone()));
        } else if let Some((section, name)) = res.entry.split_once(':') {
            opts.res_winners.push((
                section.trim().into(),
                name.trim().into(),
                res.winner.clone(),
            ));
        } else {
            return Err(ModError::Recipe(
                "a [[resolve]] needs `path` or `entry = \"section:name\"`".into(),
            ));
        }
    }
    for p in &r.patches {
        let path = patch_path(recipe_path, &p.patch)?;
        let text = std::fs::read_to_string(&path).map_err(|source| ModError::Io {
            path: path.clone(),
            source,
        })?;
        opts.patches.push((p.target.clone(), p.patch.clone(), text));
    }
    Ok((sources, opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_recipe_parses() {
        let r = Recipe::parse(
            r#"
[recipe]
id = "ultimate-c2"
name = "Ultimate Carnivores 2"
version = "0.1"
base = "c2-vanilla"

[[mods]]
id = "example-raptors"
credit = "Original Author"
url = "https://example.com/mod-page"
files = { "HUNTDAT/DINO/NEWRAPTOR.CAR" = "sha256:00" }

[load_order]
order = ["example-raptors"]

[[resolve]]
path = "HUNTDAT/MENU/MENUM.TGA"
winner = "example-textures"

[[patches]]
target = "_RES.TXT"
patch = "patches/ultimate-c2/res-balance.patch"
"#,
        );
        let r = r.unwrap_or_default();
        assert_eq!(r.mods[0].files.len(), 1);
        assert_eq!(r.resolve[0].winner, "example-textures");
        assert_eq!(Recipe::parse(&r.to_toml()).ok(), Some(r));
        assert_eq!(
            sha256(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(patch_path(Path::new("r/x.toml"), "../evil.patch").is_err());
    }
}
