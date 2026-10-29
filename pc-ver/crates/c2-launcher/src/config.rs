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

/// Which menu the PC game shows (its frames and buttons).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MenuStyle {
    /// The game's own (with a mod's frames if it brings them).
    #[default]
    Mod,
    /// The original Carnivores 2's.
    Original,
    /// Carnivores: Ice Age's (needs its folder).
    IceAge,
    /// A touch-style layout after the mobile game's (experimental).
    Mobile,
}

/// Which survivals the games offer (owner, 2026-10-23): the mobile game's
/// classic one (from a Carnivores 2 APK import; on Ice Age with its
/// stand-ins) and a dialect game's own ("modded"). With more than one, the
/// game asks which. Ours is benched (not offered).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "SurvivalModesFile")]
pub struct SurvivalModes {
    pub classic: bool,
    pub modded: bool,
}

impl Default for SurvivalModes {
    fn default() -> Self {
        Self {
            classic: true,
            modded: true,
        }
    }
}

/// As saved: the table, or an older single choice (`"Ours"`, `"Mobile"`,
/// `"Both"`).
#[derive(Deserialize)]
#[serde(untagged)]
enum SurvivalModesFile {
    Old(String),
    New {
        #[serde(default = "yes")]
        classic: bool,
        #[serde(default = "yes")]
        modded: bool,
    },
}

fn yes() -> bool {
    true
}

impl From<SurvivalModesFile> for SurvivalModes {
    fn from(f: SurvivalModesFile) -> Self {
        match f {
            SurvivalModesFile::New { classic, modded } => Self { classic, modded },
            SurvivalModesFile::Old(s) => Self {
                classic: !s.eq_ignore_ascii_case("ours"),
                modded: true,
            },
        }
    }
}

impl SurvivalModes {
    /// `--survival-modes` for `c2-pc` / `c2-cook`.
    pub fn arg(self) -> String {
        let mut v = Vec::new();
        if self.classic {
            v.push("classic");
        }
        if self.modded {
            v.push("modded");
        }
        if v.is_empty() {
            "none".into()
        } else {
            v.join(",")
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
    /// A pack per standalone game that is on (`<game>.c2pk`, each played
    /// on its own: the 3DS has no combined game).
    pub standalone_packs: bool,
    /// Also a `.cia` (needs `makerom` and `bannertool`).
    pub cia: bool,
    /// The user's home-menu icon (48x48 PNG), banner (256x128 PNG) and
    /// banner sound (WAV/OGG); plain placeholders if not set.
    pub icon: Option<PathBuf>,
    pub banner: Option<PathBuf>,
    pub banner_sound: Option<PathBuf>,
    /// The title screen: Ice Age's art instead of Carnivores 2's.
    pub ice_age_title: bool,
    /// Icon and banner not set: taken from this game's menu pictures
    /// (`ArtFrom`).
    pub art_from: ArtFrom,
}

/// Where a `.cia`'s icon and banner come from when not picked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ArtFrom {
    /// The base game's menu pictures.
    #[default]
    Base,
    /// A standalone game's (its folder).
    Game(PathBuf),
    /// Plain placeholders.
    Plain,
}

impl Default for ThreeDs {
    fn default() -> Self {
        Self {
            target: Target::Old,
            name: "c2rs".into(),
            out: None,
            vanilla_pack: true,
            addons_pack: true,
            standalone_packs: true,
            cia: false,
            icon: None,
            banner: None,
            banner_sound: None,
            ice_age_title: false,
            art_from: ArtFrom::Base,
        }
    }
}

/// The Wii U build (`crate::wiiu`): only where the SD card is, for now.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct WiiU {
    /// The SD card (or any folder to copy from by hand).
    pub out: Option<PathBuf>,
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
    /// Standalone games (whole games made from Carnivores 2), combined
    /// beside the base on PC (README M16).
    pub standalones: Vec<AddOn>,
    /// Full launcher: the 3DS section shown (asked on first launch).
    pub show_3ds: Option<bool>,
    pub three_ds: ThreeDs,
    /// Full launcher: the Wii U section shown (off until turned on in
    /// Settings).
    pub show_wiiu: bool,
    pub wiiu: WiiU,
    /// Where `c2-pc`, `c2-cook` and `c2-3ds.3dsx` are, if not beside the
    /// launcher.
    pub tools: Option<PathBuf>,
    /// The mobile game's values for this game (`mobile-values.toml` from
    /// the last APK import), offered in the balance editor.
    pub mobile_values: Option<PathBuf>,
    /// The mobile game's Survive mode (`mobile-survival.txt` from the last
    /// Carnivores 2 APK import), and which survivals are offered.
    pub mobile_survival: Option<PathBuf>,
    pub survival_modes: SurvivalModes,
    /// The PC menu style, and the Ice Age folder it (and the 3DS title)
    /// can take art from.
    pub menu_style: MenuStyle,
    pub ice_age: Option<PathBuf>,
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

    /// The add-ons to load, in order: a folder of add-ons (dropped as one)
    /// is its add-ons (`c2_mods::expand_addons`).
    /// The standalone games that are on.
    pub fn enabled_standalones(&self) -> Vec<PathBuf> {
        self.standalones
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.path.clone())
            .collect()
    }

    pub fn enabled_addons(&self) -> Vec<PathBuf> {
        self.addons
            .iter()
            .filter(|a| a.enabled)
            .flat_map(|a| c2_mods::expand_addons(&a.path))
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
    Some(if c2_mods::ice_age::is_ice_age_dir(dir) {
        "Carnivores: Ice Age"
    } else if find_ci(&huntdat, "_MENU.TXT").is_some() {
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
