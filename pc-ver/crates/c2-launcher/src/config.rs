//! What the launcher remembers, in `launcher.toml` in the game's data folder
//! (beside the saves): the game folders, the add-ons and the 3DS build.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use c2_mods::recipe::{Pictures, Settings, Tuning};
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

impl MenuStyle {
    /// Its name in a recipe's `[settings]`.
    pub fn name(self) -> &'static str {
        match self {
            MenuStyle::Mod => "mod",
            MenuStyle::Original => "original",
            MenuStyle::IceAge => "ice-age",
            MenuStyle::Mobile => "mobile",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        [
            MenuStyle::Mod,
            MenuStyle::Original,
            MenuStyle::IceAge,
            MenuStyle::Mobile,
        ]
        .into_iter()
        .find(|m| m.name().eq_ignore_ascii_case(s))
    }
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

/// The Wii U build (`crate::wiiu`): which installs, each its own Wii U
/// Menu entry and SD folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WiiU {
    /// The SD card (or any folder to copy from by hand).
    pub out: Option<PathBuf>,
    /// RPX for Homebrew Launcher or WUHB for Aroma's Wii U Menu.
    pub format: WiiUFormat,
    /// The original game (`carnivores2`).
    pub original: bool,
    /// The game with the overlays and add-ons (`carnivores2-addons`).
    pub addons: bool,
    /// One install per standalone game.
    pub standalones: bool,
    /// The user's own Wii U Menu art, by install name (its SD folder); any
    /// not picked is cut from the game's menu picture.
    pub art: std::collections::BTreeMap<String, WiiUArt>,
}

/// A Wii U install's own art: icon (128x128), and the start screens shown
/// while it loads, the TV's (1280x720) and the GamePad's (854x480); PNGs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct WiiUArt {
    pub icon: Option<PathBuf>,
    pub tv: Option<PathBuf>,
    pub drc: Option<PathBuf>,
}

impl WiiUArt {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for WiiU {
    fn default() -> Self {
        WiiU {
            out: None,
            format: WiiUFormat::default(),
            original: true,
            addons: true,
            standalones: true,
            art: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum WiiUFormat {
    Rpx,
    #[default]
    Wuhb,
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
    /// A recipe set up on the Add-ons tab (`crate::recipe_page`).
    pub recipe: Option<RecipeUse>,
    /// The folders each recipe was last given, so an instance plays again
    /// without being set up again.
    pub remembered: Vec<RecipeUse>,
    /// Local output paths and packaging choices, separate from shareable recipes.
    pub instance_builds: BTreeMap<PathBuf, ConsoleBuilds>,
    /// Where an instance's downloads are looked for, besides the ones it
    /// was last given; empty: `source/`, `source/mods`, `addons/` and
    /// `mods/` beside the recipes folder, those that exist.
    pub mod_dirs: Vec<PathBuf>,
    /// The 3DS and Wii U hunt pictures: the phone's (from a game built
    /// from the APK) or the game's own, each.
    pub pictures: Pictures,
    /// The phone game's rules (shotgun, scoring, animals' health) over a PC
    /// game, each (`c2_mods::balance::Balance::phone_tuning`).
    pub tuning: Tuning,
}

/// Console output choices last used for one saved instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ConsoleBuilds {
    pub three_ds: ThreeDs,
    pub wiiu: WiiU,
}

/// A recipe and the folder given for each of its entries; while `on`, play
/// and the builds go by it instead of the add-on and standalone lists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct RecipeUse {
    pub path: PathBuf,
    /// Its own game folder (a standalone game, or one built from an APK);
    /// `None`: the Game tab's, with its overlays.
    pub game: Option<PathBuf>,
    /// Base overlays resolved for this instance; None for older configs.
    /// Recipe-pinned overlays are still loaded from their own entries.
    pub overlays: Option<Vec<PathBuf>>,
    /// Entry id -> its folder.
    pub folders: BTreeMap<String, PathBuf>,
    pub on: bool,
}

impl RecipeUse {
    /// `--mods ID=FOLDER` for each entry (`c2_mods::recipe::mods_arg`).
    pub fn mods_args(&self) -> Vec<String> {
        self.folders
            .iter()
            .flat_map(|(id, dir)| ["--mods".to_string(), format!("{id}={}", dir.display())])
            .collect()
    }

    /// The recipe's id (its pack's name), or the file's name.
    pub fn id(&self) -> String {
        c2_mods::recipe::Recipe::load(&self.path)
            .ok()
            .map(|r| r.recipe.id)
            .filter(|id| !id.is_empty())
            .or_else(|| {
                self.path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "recipe".into())
    }
}

fn path() -> PathBuf {
    data_dir().join("launcher.toml")
}

/// Games built from the player's APKs (`apk-base/<apk>/` from the Game
/// tab, `builds/apk-base/<id>/` from a recipe), which have the phone's
/// interface art (`HUNTDAT/MOBILE`).
pub fn phone_games() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        data_dir().join("apk-base"),
        data_dir().join("builds/apk-base"),
    ]
    .iter()
    .filter_map(|d| std::fs::read_dir(d).ok())
    .flat_map(|rd| rd.flatten().map(|e| e.path()))
    .filter(|p| is_phone_game(p))
    .collect();
    v.sort();
    v
}

/// A game folder built from an APK.
pub fn is_phone_game(dir: &Path) -> bool {
    find_ci(dir, "HUNTDAT").is_some_and(|h| find_ci(&h, "MOBILE").is_some())
}

/// The recipes folder (instances, any folder deep): beside the launcher,
/// or (working from the source tree) the tree's `recipes/`; a new one is
/// made beside the launcher.
pub fn recipes_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    // pc-ver/target/release -> the tree's top folder
    let tree = exe_dir.ancestors().nth(3).map(|r| r.join("recipes"));
    std::iter::once(exe_dir.join("recipes"))
        .chain(tree)
        .find(|d| d.is_dir())
        .unwrap_or_else(|| exe_dir.join("recipes"))
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

    /// The folders an instance's downloads are looked for in.
    pub fn search_dirs(&self) -> Vec<PathBuf> {
        if !self.mod_dirs.is_empty() {
            return self.mod_dirs.clone();
        }
        let top = recipes_dir()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        ["source", "source/mods", "addons", "mods"]
            .iter()
            .map(|d| top.join(d))
            .filter(|d| d.is_dir())
            .collect()
    }

    /// The folders last given for the recipe at `path`.
    pub fn remembered(&self, path: &Path) -> Option<&RecipeUse> {
        self.recipe
            .iter()
            .chain(&self.remembered)
            .find(|u| u.path == path)
    }

    /// Remembers a recipe's folders (the newest kept).
    pub fn remember(&mut self, u: &RecipeUse) {
        self.remembered.retain(|r| r.path != u.path);
        self.remembered.push(u.clone());
    }

    pub fn remember_builds(&mut self) {
        if let Some(r) = self.recipe.as_ref().filter(|r| r.on) {
            self.instance_builds.insert(
                r.path.clone(),
                ConsoleBuilds {
                    three_ds: self.three_ds.clone(),
                    wiiu: self.wiiu.clone(),
                },
            );
        }
    }

    /// Restore this instance's outputs; first builds get their own install
    /// name and only the recipe pack, using the last chosen destination.
    pub fn restore_builds(&mut self, path: &Path, id: &str) {
        let saved = self
            .instance_builds
            .get(path)
            .cloned()
            .unwrap_or_else(|| ConsoleBuilds {
                three_ds: ThreeDs {
                    name: install_name(id),
                    out: self.three_ds.out.clone(),
                    target: self.three_ds.target,
                    cia: self.three_ds.cia,
                    ice_age_title: self.three_ds.ice_age_title,
                    vanilla_pack: false,
                    addons_pack: true,
                    standalone_packs: false,
                    ..Default::default()
                },
                wiiu: WiiU {
                    out: self.wiiu.out.clone(),
                    format: self.wiiu.format,
                    original: false,
                    addons: true,
                    standalones: false,
                    ..Default::default()
                },
            });
        self.three_ds = saved.three_ds;
        self.wiiu = saved.wiiu;
    }

    /// Takes in a recipe's choices: those it leaves unset stay as they are.
    pub fn apply(&mut self, s: &Settings) {
        if let Some(m) = MenuStyle::from_name(&s.menu) {
            self.menu_style = m;
        }
        if let Some(v) = &s.survival {
            let has = |k: &str| v.iter().any(|x| x.eq_ignore_ascii_case(k));
            self.survival_modes = SurvivalModes {
                classic: has("classic"),
                modded: has("modded"),
            };
        }
        match s.three_ds_target.as_str() {
            "o3ds" => self.three_ds.target = Target::Old,
            "n3ds" => self.three_ds.target = Target::New,
            _ => {}
        }
        if let Some(t) = s.ice_age_title {
            self.three_ds.ice_age_title = t;
        }
        if !s.pictures.is_auto() {
            self.pictures = s.pictures;
        }
        if !s.tuning.is_auto() {
            self.tuning = s.tuning;
        }
    }

    /// The choices to save in a recipe: the launcher's, over `old` (a
    /// recipe's own, which keep what the launcher has no control for).
    pub fn settings(&self, old: &Settings) -> Settings {
        let m = self.survival_modes;
        Settings {
            menu: self.menu_style.name().into(),
            survival: Some(
                [("classic", m.classic), ("modded", m.modded)]
                    .iter()
                    .filter(|(_, on)| *on)
                    .map(|(k, _)| k.to_string())
                    .collect(),
            ),
            three_ds_target: self.three_ds.target.arg().into(),
            ice_age_title: Some(self.three_ds.ice_age_title),
            pictures: self.pictures,
            tuning: self.tuning,
            ..old.clone()
        }
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
    Some(if find_ci(&huntdat, "MOBILE").is_some() {
        "the mobile game, built from your APK"
    } else if c2_mods::ice_age::is_ice_age_dir(dir) {
        "Carnivores: Ice Age"
    } else if find_ci(&huntdat, "_MENU.TXT").is_some() {
        "Modder's Edition style game"
    } else {
        "Carnivores 2"
    })
}

/// The Wii U install of a base game: (folder, title). Ice Age and the
/// games built from an APK get their own, so they sit beside Carnivores 2
/// instead of over it.
pub fn base_install(dir: &Path) -> (&'static str, &'static str) {
    match (c2_mods::ice_age::is_ice_age_dir(dir), is_phone_game(dir)) {
        (true, true) => ("carnivores-ice-age-mobile", "Carnivores: Ice Age (phone)"),
        (true, false) => ("carnivores-ice-age", "Carnivores: Ice Age"),
        (false, true) => ("carnivores-dinosaur-hunter", "Carnivores: Dinosaur Hunter"),
        (false, false) => ("carnivores2", "Carnivores 2"),
    }
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
    fn instance_outputs_survive_switching_and_reload() {
        let mut c = Config::default();
        let first = PathBuf::from("recipes/first.toml");
        let second = PathBuf::from("recipes/second.toml");
        c.restore_builds(&first, "First Pack");
        assert_eq!(c.three_ds.name, "First-Pack");
        c.recipe = Some(RecipeUse {
            path: first.clone(),
            on: true,
            ..Default::default()
        });
        c.three_ds.out = Some("first sd".into());
        c.three_ds.cia = true;
        c.wiiu.out = Some("wii u sd".into());
        c.remember_builds();
        let expected = c.instance_builds[&first].clone();
        c.restore_builds(&second, "second");
        c.recipe = Some(RecipeUse {
            path: second.clone(),
            on: true,
            ..Default::default()
        });
        c.three_ds.out = Some("second sd".into());
        c.remember_builds();
        let text = toml::to_string(&c).expect("serialize preferences");
        let mut back: Config = toml::from_str(&text).expect("reload preferences");
        back.restore_builds(&first, "First Pack");
        assert_eq!(back.three_ds, expected.three_ds);
        assert_eq!(back.wiiu, expected.wiiu);
        back.restore_builds(&second, "second");
        assert_eq!(back.three_ds.out, Some("second sd".into()));
        let legacy: Config = toml::from_str("show_wiiu = false").expect("legacy config");
        assert!(legacy.instance_builds.is_empty());
    }

    #[test]
    fn a_game_built_from_an_apk_is_named_so() {
        let dir = std::env::temp_dir().join(format!("c2rs-apk-game-{}", std::process::id()));
        let mobile = dir.join("HUNTDAT").join("MOBILE");
        std::fs::create_dir_all(&mobile).unwrap_or_else(|e| panic!("{e}"));
        let kind = describe_game(&dir);
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(kind, Some("the mobile game, built from your APK"));
    }

    #[test]
    fn install_names_are_folder_safe() {
        assert_eq!(install_name("Ice Age + C2"), "Ice-Age---C2");
        assert_eq!(install_name("  "), "c2rs");
        assert_eq!(install_name("c2_ice-age"), "c2_ice-age");
    }

    #[test]
    fn recipe_folders_become_mods_args() {
        let u = RecipeUse {
            folders: [
                ("b".to_string(), PathBuf::from("/d/b")),
                ("a".into(), "/d/a".into()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        assert_eq!(u.mods_args(), ["--mods", "a=/d/a", "--mods", "b=/d/b"]);
    }

    #[test]
    fn recipe_settings_in_and_out() {
        let mut c = Config::default();
        let s = Settings {
            menu: "mobile".into(),
            survival: Some(vec!["classic".into()]),
            three_ds_target: "n3ds".into(),
            pictures: c2_mods::recipe::Pictures {
                scope: c2_mods::recipe::Source::Mobile,
                ..Default::default()
            },
            ..Default::default()
        };
        c.apply(&s);
        assert_eq!(c.menu_style, MenuStyle::Mobile);
        assert!(c.survival_modes.classic && !c.survival_modes.modded);
        assert_eq!(c.three_ds.target, Target::New);
        // What the launcher has no control for is kept from the recipe.
        let back = c.settings(&s);
        assert_eq!(back.pictures, s.pictures);
        assert_eq!(back.menu, "mobile");
        assert_eq!(back.survival, s.survival);
        // Unset leaves the launcher's choice alone.
        c.apply(&Settings::default());
        assert_eq!(c.menu_style, MenuStyle::Mobile);
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
