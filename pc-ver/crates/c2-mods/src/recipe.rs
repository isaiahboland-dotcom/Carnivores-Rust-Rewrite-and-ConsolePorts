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

mod base;
pub use base::*;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recipe {
    pub recipe: Info,
    /// Expansion variant authoring data; omitted for ordinary recipes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<crate::variants::Group>,
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
    /// Everything else chosen before playing (the launcher's instances):
    /// with it a recipe rebuilds the same game later.
    #[serde(skip_serializing_if = "Settings::is_empty")]
    pub settings: Settings,
}

/// Which version a feature comes from where the PC and phone games each
/// have their own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The default for the game being built (see [`Recipe::mobile_rules`];
    /// pictures: the phone's where the game has them).
    #[default]
    Auto,
    Pc,
    Mobile,
}

impl Source {
    pub fn is_auto(&self) -> bool {
        *self == Source::Auto
    }

    /// Whether the phone's version is used, `mobile` being the default.
    pub fn mobile(self, mobile: bool) -> bool {
        match self {
            Source::Auto => mobile,
            Source::Pc => false,
            Source::Mobile => true,
        }
    }
}

/// `[settings]`: a recipe's choices beyond its files. Empty or missing
/// values are the launcher's defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// PC menu: `mod`, `original`, `ice-age` or `mobile`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub menu: String,
    /// Hunt pictures: the phone's or the PC's.
    #[serde(skip_serializing_if = "Pictures::is_auto")]
    pub pictures: Pictures,
    /// Game rules: the phone's or the PC's.
    #[serde(skip_serializing_if = "Tuning::is_auto")]
    pub tuning: Tuning,
    /// 3DS: `o3ds` or `n3ds`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub three_ds_target: String,
    /// 3DS: Ice Age's title art.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ice_age_title: Option<bool>,
    /// Prices and scores changed from the game's (the launcher's balance
    /// editor, `crate::balance`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<crate::balance::Balance>,
    /// Carnivores hunt herbivores (an advanced setting, `crate::predation`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predation: Option<crate::predation::Predation>,
    /// Times of day, settings fixed by the instance and spawn rules (the
    /// Advanced tab, `crate::rules`).
    #[serde(skip_serializing_if = "crate::rules::Rules::is_empty")]
    pub rules: crate::rules::Rules,
}

impl Settings {
    pub fn is_empty(&self) -> bool {
        *self == Settings::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pictures {
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub scope: Source,
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub weapons: Source,
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub bullets: Source,
    /// Icons of the animals both games have.
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub animals: Source,
}

impl Pictures {
    pub fn is_auto(&self) -> bool {
        *self == Pictures::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tuning {
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub shotgun: Source,
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub scoring: Source,
    /// The animals' values (speed, health, senses).
    #[serde(skip_serializing_if = "Source::is_auto")]
    pub animals: Source,
}

impl Tuning {
    pub fn is_auto(&self) -> bool {
        *self == Tuning::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Expected base game: `bases/<base>.toml` next to the recipe lists its hashes.
    pub base: String,
    /// The base game's own areas, huntables and weapons left out
    /// (`crate::standalone::leave_out`), numbered in the base's menu order
    /// (the game with its overlays); applied over the built game, add-ons
    /// and standalones combined beside it included.
    #[serde(skip_serializing_if = "crate::standalone::Skip::is_empty")]
    pub skip: crate::standalone::Skip,
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
    /// A standalone's areas, huntables and weapons left out when it is
    /// combined (`crate::standalone::Skip`).
    #[serde(skip_serializing_if = "crate::standalone::Skip::is_empty")]
    pub skip: crate::standalone::Skip,
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

    /// Whether the phone's game rules (shotgun, scoring, animals' values)
    /// are the default: an APK base with no PC game beside it, neither an
    /// overlay (the Modder's Edition) nor a standalone (owner, 2026-11-01).
    pub fn mobile_rules(&self, base: Option<&Base>) -> bool {
        base.is_some_and(|b| !b.base.apk.is_empty())
            && self.overlays.is_empty()
            && self.standalones.is_empty()
    }
}

/// Every recipe under `dir`, any folder deep (`recipes/instances/...`),
/// sorted by path: `.toml` files with a `[recipe]` table, so the base
/// hash lists (`bases/`) and other files are passed over.
pub fn find_recipes(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let hidden = p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if hidden {
                continue;
            }
            if p.is_dir() {
                if depth < 8 {
                    walk(&p, depth + 1, out);
                }
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
                && std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|t| t.parse::<toml::Table>().ok())
                    .is_some_and(|t| t.get("recipe").is_some_and(|r| r.is_table()))
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, 0, &mut out);
    out.sort();
    out
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

/// A folder that may hold a recipe entry: `(name, root, files, loose)`.
type Cand = (String, PathBuf, Box<dyn Backend>, bool);

fn dir_name(d: &Path) -> String {
    if d.is_file() {
        d.file_stem()
    } else {
        d.file_name()
    }
    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// How many folders down a downloads folder is looked through.
pub const SEARCH_DEPTH: usize = 4;

/// `dir` and the folders (and `.zip`s, when `zips`) inside it, up to
/// [`SEARCH_DEPTH`] folders down. A folder with a `HUNTDAT` in it is a game
/// or mod: it is listed but not looked into (nor is `HUNTDAT` itself).
pub fn dir_and_subs(dir: &Path, zips: bool) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, zips: bool, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir() || (zips && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
            })
            .collect();
        subs.sort();
        for p in subs {
            let huntdat = p
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("huntdat"));
            if huntdat {
                continue;
            }
            out.push(p.clone());
            if p.is_dir() && depth < SEARCH_DEPTH && !has_huntdat(&p) {
                walk(&p, depth + 1, zips, out);
            }
        }
    }
    let mut out = vec![dir.to_path_buf()];
    if !has_huntdat(dir) {
        walk(dir, 1, zips, &mut out);
    }
    out
}

/// A `HUNTDAT` folder right inside `dir`.
fn has_huntdat(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten().any(|e| {
            e.file_name().eq_ignore_ascii_case("huntdat") && e.file_type().is_ok_and(|t| t.is_dir())
        })
    })
}

/// `d` as a standalone or overlay game: its game folder, possibly a few
/// folders down.
fn game_cands(d: &Path, cands: &mut Vec<Cand>) {
    let name = dir_name(d);
    if let Some(root) = crate::pack::find_game_root(d) {
        if let Ok(b) = open_root(&root) {
            cands.push((name.clone(), root.clone(), b, false));
        }
        // Keep the extracted package as a second lookup view. Recipes
        // pinned from an unpacked download can include its wrapper
        // directory in every path, while `open_root` starts at HUNTDAT's
        // parent and omits that prefix. `root` remains the actual game
        // directory used later when building the recipe.
        if let Ok(b) = DirBackend::new(d) {
            cands.push((name, root, Box::new(b), false));
        }
    }
}

/// `d` as a mod: its game-data root, and the folder as a loose add-on.
fn mod_cands(d: &Path, cands: &mut Vec<Cand>) {
    let name = dir_name(d);
    if let Some(root) = mod_root(d) {
        if let Ok(b) = open_root(&root) {
            cands.push((name.clone(), root, b, false));
        }
    }
    // Recipe loose-file paths are relative to the extracted download,
    // which can contain a nested HUNTDAT (or only map files, with no
    // .car/.txt for `is_loose` to detect). Keep the download folder as
    // a candidate even when `mod_root` also found a game-data root.
    if d.is_dir() {
        if let Ok(b) = DirBackend::new(d) {
            cands.push((name, d.to_path_buf(), Box::new(b), true));
        }
    }
}

/// A `--mods` value: a folder of downloads (or a mod's own folder), or
/// `ID=FOLDER`, the folder given for the entry `ID` (the launcher's).
pub fn mods_arg(p: &Path) -> (Option<String>, PathBuf) {
    if !p.exists() {
        if let Some((id, dir)) = p.to_str().and_then(|s| s.split_once('=')) {
            if !id.is_empty() && !id.contains(['/', '\\']) {
                return (Some(id.to_string()), PathBuf::from(dir));
            }
        }
    }
    (None, p.to_path_buf())
}

/// The candidates from `--mods` values: an `ID=FOLDER` one is that folder
/// alone, named `ID`; a plain folder is itself and the folders in it
/// ([`dir_and_subs`]).
fn cands_from(
    list: &[RecipeMod],
    dirs: &[PathBuf],
    zips: bool,
    add: fn(&Path, &mut Vec<Cand>),
) -> Vec<Cand> {
    let mut cands = Vec::new();
    for arg in dirs {
        match mods_arg(arg) {
            (Some(id), d) => {
                let from = cands.len();
                add(&d, &mut cands);
                // The folder the launcher remembers for the entry is the
                // game folder; its paths may start above it.
                if let Some(m) = list.iter().find(|m| m.id == id) {
                    let mut own = cands.split_off(from);
                    above_view(m, &d, &mut own);
                    cands.extend(own);
                }
                for c in &mut cands[from..] {
                    c.0 = id.clone();
                }
            }
            (None, dir) => {
                for d in dir_and_subs(&dir, zips) {
                    add(&d, &mut cands);
                }
            }
        }
    }
    cands
}

/// Finds each of `games` (standalones or overlays) in `dirs` or up to
/// [`SEARCH_DEPTH`] folders down (see [`mods_arg`]), as [`locate`] does mods, its game folder
/// possibly a few folders down.
pub fn locate_games(games: &[RecipeMod], dirs: &[PathBuf]) -> Vec<Located> {
    let add: fn(&Path, &mut Vec<Cand>) = |d, c| {
        if d.is_dir() {
            game_cands(d, c);
        }
    };
    locate_among(games, &cands_from(games, dirs, false, add))
}

/// Finds each recipe mod in `dirs` (downloads folders, mod folders, or
/// `ID=FOLDER`: see [`mods_arg`]): the folder named by `folder` or like the
/// id, else the folder where most listed files match their hashes.
pub fn locate(r: &Recipe, dirs: &[PathBuf]) -> Vec<Located> {
    locate_among(&r.mods, &cands_from(&r.mods, dirs, true, mod_cands))
}

/// One recipe entry checked against the one folder the player gave for it
/// (the launcher's recipe page): `game` for an overlay or standalone.
pub fn check_folder(m: &RecipeMod, dir: &Path, game: bool) -> Located {
    let mut cands = Vec::new();
    if game {
        game_cands(dir, &mut cands);
    } else {
        mod_cands(dir, &mut cands);
    }
    above_view(m, dir, &mut cands);
    let l = locate_among(std::slice::from_ref(m), &cands)
        .pop()
        .unwrap_or_default();
    if l.root.is_none() {
        // Nothing of it there: no root, so the folder isn't taken.
        return Located {
            id: m.id.clone(),
            url: m.url.clone(),
            missing: m.files.keys().cloned().collect(),
            ..Default::default()
        };
    }
    l
}

/// When paths of `m` start with the last folder names of `dir` (a recipe
/// pinned from the unpacked download, `dir` the game folder in it, as the
/// launcher remembers it; a download can hold more than one game), `dir`
/// seen from the folder above those, so those paths are found. Its root
/// stays `dir`'s, as found.
fn above_view(m: &RecipeMod, dir: &Path, cands: &mut Vec<Cand>) {
    let names: Vec<String> = dir
        .components()
        .rev()
        .take_while(|c| matches!(c, Component::Normal(_)))
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    // The most of `dir`'s last names any path starts with.
    let k = m
        .files
        .keys()
        .filter_map(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            (1..parts.len().min(names.len() + 1))
                .rev()
                .find(|&k| (0..k).all(|i| names[k - 1 - i].eq_ignore_ascii_case(parts[i])))
        })
        .max();
    let Some(above) = k.and_then(|k| dir.ancestors().nth(k)) else {
        return;
    };
    let root = cands
        .iter()
        .find(|c| c.3 == m.loose)
        .map_or_else(|| dir.to_path_buf(), |c| c.1.clone());
    if let Ok(b) = DirBackend::new(above) {
        cands.push((dir_name(dir), root, Box::new(b), m.loose));
    }
}

/// Every entry of a recipe, its kind and whether it is a whole game
/// (overlay or standalone): `("overlay" | "standalone" | "mod", entry, game)`.
pub fn entries(r: &Recipe) -> impl Iterator<Item = (&'static str, &RecipeMod, bool)> {
    r.overlays
        .iter()
        .map(|m| ("overlay", m, true))
        .chain(r.standalones.iter().map(|m| ("standalone", m, true)))
        .chain(r.mods.iter().map(|m| ("mod", m, false)))
}

/// Each of `list` among the candidate folders: `(name, root, files, loose)`.
fn locate_among(list: &[RecipeMod], cands: &[Cand]) -> Vec<Located> {
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
            ..Default::default()
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
            skip: Default::default(),
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
    crate::variants::validate(&r.variants)?;
    if !r.variants.is_empty() && !r.recipe.skip.is_empty() {
        return Err(ModError::Recipe(
            "variant packs cannot yet renumber base parts through recipe.skip".into(),
        ));
    }
    if let Some(bad) = located.iter().find(|l| !l.complete()) {
        return Err(ModError::Recipe(format!(
            "mod '{}' is missing or doesn't match (get it from {})",
            bad.id,
            if bad.url.is_empty() { "?" } else { &bad.url }
        )));
    }
    let vres = read_res(&vanilla.read(RES_TXT)?)?;
    let mut sources = Vec::new();
    let mut groups = BTreeMap::<String, String>::new();
    for (m, l) in r.mods.iter().zip(located) {
        let Some(root) = &l.root else { continue };
        if m.loose {
            // Installed as the loose reader would, from the pinned files only.
            let pinned: Vec<PathBuf> = l.ok.values().map(|f| root.join(f)).collect();
            let children = crate::pack::expand_addons(root);
            let collection = children.len() > 1;
            for (i, child) in children.iter().enumerate() {
                if !pinned.iter().any(|p| p.starts_with(child)) {
                    continue;
                }
                let mut s = load_loose(vanilla, child, &|p| pinned.iter().any(|q| q == p))?;
                s.variant_origin = Some((
                    m.id.clone(),
                    collection.then(|| {
                        child
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }),
                ));
                let info = &mut s.manifest.info;
                info.id = if collection {
                    format!("{}--part-{i}", m.id)
                } else {
                    m.id.clone()
                };
                groups.insert(info.id.clone(), m.id.clone());
                info.authors = vec![m.credit.clone()];
                info.source_url = m.url.clone();
                s.inferred = false;
                sources.push(s);
            }
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
            variant_origin: Some((m.id.clone(), None)),
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
        let ids: Vec<&String> = r.mods.iter().map(|m| &m.id).collect();
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
            r.load_order.order.iter().position(|o| {
                o == groups
                    .get(&s.manifest.info.id)
                    .unwrap_or(&s.manifest.info.id)
            })
        });
    }
    let mut opts = BuildOptions::default();
    opts.variants = r.variants.clone();
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
    // A collection remains one recipe choice. Resolve its winner to the
    // last child providing that file/entry, preserving collection order.
    for (path, winner) in &mut opts.file_winners {
        if let Some(source) = sources.iter().rev().find(|s| {
            groups.get(&s.manifest.info.id) == Some(winner)
                && s.files.iter().any(|p| key(p) == key(path))
        }) {
            *winner = source.manifest.info.id.clone();
        }
    }
    for (section, name, winner) in &mut opts.res_winners {
        if let Some(source) = sources.iter().rev().find(|s| {
            groups.get(&s.manifest.info.id) == Some(winner)
                && s.res_changes.iter().any(|c| match c {
                    crate::res_merge::Change::Add {
                        section: sec,
                        block,
                    }
                    | crate::res_merge::Change::Modify {
                        section: sec,
                        block,
                        ..
                    } => {
                        sec.eq_ignore_ascii_case(section)
                            && crate::res_merge::block_id(block)
                                .is_some_and(|id| id.eq_ignore_ascii_case(name))
                    }
                    _ => false,
                })
        }) {
            *winner = source.manifest.info.id.clone();
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
    fn game_folder_checked_with_the_downloads_wrapper_in_its_paths() {
        let dir = std::env::temp_dir().join(format!("c2rs-check-above-{}", std::process::id()));
        let game = dir.join("Bilewaters_1.1/bilewaters here");
        std::fs::create_dir_all(game.join("HUNTDAT")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(game.join("HUNTDAT/X.CAR"), b"bw").unwrap_or_else(|e| panic!("{e}"));
        let m = RecipeMod {
            id: "bilewaters-1-1".into(),
            files: [("bilewaters here/HUNTDAT/X.CAR".to_string(), sha256(b"bw"))].into(),
            ..Default::default()
        };
        let l = check_folder(&m, &game, true);
        // As `--mods ID=FOLDER` (the launcher's play and builds).
        let arg = PathBuf::from(format!("bilewaters-1-1={}", game.display()));
        let g = locate_games(std::slice::from_ref(&m), &[arg]).pop();
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert!(l.complete(), "{l:?}");
        assert_eq!(l.root.as_deref(), Some(game.as_path()));
        let g = g.unwrap_or_default();
        assert!(g.complete(), "{g:?}");
        assert_eq!(g.root.as_deref(), Some(game.as_path()));
    }

    #[test]
    fn downloads_looked_through_four_folders_down() {
        let dir = std::env::temp_dir().join(format!("c2rs-dir-and-subs-{}", std::process::id()));
        let mk = |p: &str| {
            std::fs::create_dir_all(dir.join(p)).unwrap_or_else(|e| panic!("{e}"));
        };
        mk("a/b/c/d/e/f");
        mk("a/game/HUNTDAT/MODELS");
        mk("a/game/inner");
        let got = dir_and_subs(&dir, false);
        let has = |p: &str| got.contains(&dir.join(p));
        let all = (has("a/b/c/d"), has("a/b/c/d/e"), has("a/game"));
        let skipped = (has("a/game/HUNTDAT"), has("a/game/inner"));
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(all, (true, false, true), "{got:?}");
        assert_eq!(skipped, (false, false), "{got:?}");
    }

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
