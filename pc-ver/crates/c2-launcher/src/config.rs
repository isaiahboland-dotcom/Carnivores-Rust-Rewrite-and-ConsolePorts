//! What the launcher remembers, in `launcher.toml` in the game's data folder
//! (beside the saves): the game folders, the add-ons and the 3DS build.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The data folder `c2-pc` uses (`c2-pc/src/save.rs`: `data_dir`).
pub fn data_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("c2rs")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddOn {
    /// A folder (archives are unpacked into the data folder first).
    pub path: PathBuf,
    pub enabled: bool,
}

impl AddOn {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Target {
    /// Old 3DS / 2DS: smaller textures, shorter view.
    #[default]
    Old,
    New,
}

impl Target {
    pub fn arg(self) -> &'static str {
        match self {
            Target::Old => "o3ds",
            Target::New => "n3ds",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThreeDs {
    pub target: Target,
    /// The install's name: its SD folder (`3ds/<name>/`) and app file.
    pub name: String,
    /// The SD card (or any folder to copy from by hand).
    pub out: Option<PathBuf>,
    pub vanilla_pack: bool,
    pub addons_pack: bool,
}

impl Default for ThreeDs {
    fn default() -> Self {
        Self {
            target: Target::Old,
            name: "c2rs".into(),
            out: None,
            vanilla_pack: true,
            addons_pack: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    /// The original game folder (with `HUNTDAT`).
    pub base: Option<PathBuf>,
    /// Folders copied over it first (the Modder's Edition, which most
    /// add-ons are made for).
    pub overlays: Vec<PathBuf>,
    /// In load order: later ones win.
    pub addons: Vec<AddOn>,
    /// Full launcher: the 3DS section shown (asked on first launch).
    pub show_3ds: Option<bool>,
    pub three_ds: ThreeDs,
    /// Where `c2-pc`, `c2-cook` and `c2-3ds.3dsx` are, if not beside the
    /// launcher.
    pub tools: Option<PathBuf>,
}

fn path() -> PathBuf {
    data_dir().join("launcher.toml")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn store(&self) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
        std::fs::write(path(), text).map_err(|e| e.to_string())
    }

    pub fn enabled_addons(&self) -> Vec<&Path> {
        self.addons
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.path.as_path())
            .collect()
    }
}

/// `name` in `dir`, any case.
pub fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

/// What a dropped game folder is, for the user: `None` if it has no
/// `HUNTDAT`.
pub fn describe_game(dir: &Path) -> Option<&'static str> {
    let huntdat = find_ci(dir, "HUNTDAT")?;
    Some(if find_ci(&huntdat, "_MENU.TXT").is_some() {
        "Modder's Edition style game"
    } else {
        "Carnivores 2"
    })
}

/// A folder name for the 3DS install: letters, digits, `-` and `_`.
pub fn install_name(s: &str) -> String {
    let n: String = s
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if n.is_empty() {
        "c2rs".into()
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_names_are_folder_safe() {
        assert_eq!(install_name("Ice Age + C2"), "Ice-Age---C2");
        assert_eq!(install_name("  "), "c2rs");
        assert_eq!(install_name("c2_ice-age"), "c2_ice-age");
    }

    #[test]
    fn config_round_trips() {
        let c = Config {
            base: Some("/games/c2".into()),
            addons: vec![AddOn {
                path: "/mods/a".into(),
                enabled: false,
            }],
            show_3ds: Some(true),
            ..Default::default()
        };
        let t = toml::to_string_pretty(&c).unwrap_or_default();
        let back: Config = toml::from_str(&t).unwrap_or_default();
        assert_eq!(back, c);
    }
}
