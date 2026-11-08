//! Base game hash lists (`recipes/bases/*.toml`): which game a recipe is
//! built on, and the checks that keep a wrong game from being played or
//! built silently (launch protection).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use c2_vfs::Vfs;

use super::{sha256, Recipe};
use crate::ModError;

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
    /// Who made the game and where it is downloaded (a standalone game as
    /// the base).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub credit: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// `sha256:<hex>` of the APK the base is built from (README M17): the
    /// game folder `mobile_base::build` made from it records the same in
    /// [`crate::mobile_base::SOURCE_APK`]. `files` is then empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub apk: String,
}

impl Base {
    /// A hash list's text. Fails when it does not parse, or when it pins
    /// no files and no APK (a misspelt `[files]` table would otherwise
    /// check as "ok (0 files)").
    pub fn parse(text: &str) -> Result<Self, String> {
        let b: Base = toml::from_str(text).map_err(|e| e.to_string())?;
        if b.files.is_empty() && b.base.apk.is_empty() {
            return Err("it lists no files and no APK".into());
        }
        Ok(b)
    }
}

/// The base profile `bases/<base>.toml` beside the recipe or a folder
/// above it: `Ok(None)` for a recipe without a base (any game), an error
/// when the recipe names a base whose hash list is missing or broken.
/// Launch protection goes by this; it never passes a game unchecked.
pub fn read_base(recipe_path: &Path, r: &Recipe) -> Result<Option<Base>, String> {
    if r.recipe.base.is_empty() {
        return Ok(None);
    }
    // Beside the recipe, or beside a folder above it (a recipe in
    // `recipes/instances/...` uses `recipes/bases/`).
    let name = format!("{}.toml", r.recipe.base);
    let file = recipe_path
        .ancestors()
        .skip(1)
        .take(8)
        .map(|d| d.join("bases").join(&name))
        .find(|f| f.is_file())
        .ok_or_else(|| format!("hash list bases/{name} not found beside the recipe"))?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("bases/{name}: {e}"))?;
    Base::parse(&text)
        .map(Some)
        .map_err(|e| format!("bases/{name} is broken: {e}"))
}

/// [`read_base`] for callers that only want the base's details (its APK,
/// its file count), not launch protection: a missing or broken list is `None`.
pub fn load_base(recipe_path: &Path, r: &Recipe) -> Option<Base> {
    read_base(recipe_path, r).ok().flatten()
}

/// For a base built from an APK (`apk` set): whether the game folder `game`
/// was built from that APK (`None` for other bases).
pub fn check_apk(base: &Base, game: &Path) -> Option<bool> {
    if base.base.apk.is_empty() {
        return None;
    }
    let got = std::fs::read_to_string(game.join(crate::mobile_base::SOURCE_APK)).ok();
    Some(got.is_some_and(|g| g.trim().eq_ignore_ascii_case(base.base.apk.trim())))
}

/// `sha256:<hex>` of a file, read in pieces (APKs are hundreds of MiB).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let mut s = String::from("sha256:");
    for b in h.finalize() {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
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
            ..Default::default()
        },
        files,
    })
}
