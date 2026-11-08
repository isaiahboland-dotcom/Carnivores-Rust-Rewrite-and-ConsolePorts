//! An instance for the Android app: the engine APK beside the merged game,
//! laid out as the app's files folder, and its install over adb.
//!
//! The APK holds no game data (docs/DEVELOPMENT.md §2); the game goes to
//! `Android/data/<package>/files/game` on the device (android-ver/README.md).
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::jobs::Step;

pub mod rename;

const MARKER: &str = ".c2rs-android-package";
/// The app's package name (android-ver/AndroidManifest.xml).
pub const PACKAGE: &str = "io.github.c2rs";
/// Its name on the phone.
pub const LABEL: &str = "c2rs";
/// The engine APK, as `android-ver/build-apk.sh` names it.
pub const APK: &str = "c2rs.apk";
/// What the app reads from its files folder, beside `game/`.
const BALANCE: &str = "balance.toml";

/// The package of the app named `name` on the phone: the shared app when
/// empty, else one of its own (`io.github.c2rs.<name>`), so that instances
/// given different names install side by side, each with its own files.
pub fn app_package(name: &str) -> String {
    let mut seg = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            // A package segment starts with a letter.
            if !(seg.is_empty() && c.is_ascii_digit()) {
                seg.push(c.to_ascii_lowercase());
            }
        } else if !seg.is_empty() && !seg.ends_with('_') {
            seg.push('_');
        }
    }
    let seg = seg.trim_end_matches('_');
    match (name.trim().is_empty(), seg.is_empty()) {
        (true, _) => PACKAGE.to_string(),
        (false, true) => format!("{PACKAGE}.app"),
        (false, false) => format!("{PACKAGE}.{seg}"),
    }
}

/// Another saved instance whose own app is also `package` (names such as
/// "+++" and "!!!" both sanitise to `io.github.c2rs.app`), so installing
/// one would replace the other. The shared app (an empty name) is meant to
/// be shared and never clashes.
pub fn package_clash<'a>(
    package: &str,
    own: &Path,
    builds: &'a std::collections::BTreeMap<PathBuf, crate::config::InstanceBuilds>,
) -> Option<&'a Path> {
    if package == PACKAGE {
        return None;
    }
    builds
        .iter()
        .find(|(p, b)| {
            p.as_path() != own
                && !b.android_name.trim().is_empty()
                && app_package(&b.android_name) == package
        })
        .map(|(p, _)| p.as_path())
}

/// The files folder on the device of the app `package`.
pub fn device_files(package: &str) -> String {
    format!("/sdcard/Android/data/{package}/files")
}

pub fn validate_apk(path: &Path) -> Result<(), String> {
    let mut head = [0; 4];
    let ok = fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head == *b"PK\x03\x04";
    if ok {
        Ok(())
    } else {
        Err(format!("{} is not an Android APK.", path.display()))
    }
}

/// A game the Android app takes: one built from a mobile APK.
pub fn validate_base(base: &Path) -> Result<(), String> {
    if base.join(c2_mods::mobile_base::SOURCE_APK).is_file() {
        Ok(())
    } else {
        Err(
            "The Android version needs a game built from your mobile APK as the instance's \
             base game (Base game > Use my APK as the game)."
                .into(),
        )
    }
}

/// Writes `out`: `c2rs.apk` and `files/` (`game/` from the merged `game`,
/// with the base's APK marker and Survive waves, and `balance.toml`).
pub fn package(
    game: &Path,
    base: &Path,
    apk: &Path,
    balance: Option<&Path>,
    app: &str,
    out: &Path,
) -> io::Result<()> {
    validate_apk(apk).map_err(io::Error::other)?;
    crate::windows::replace_generated(out, &[game, base], MARKER, "Android", |stage| {
        let files = stage.join("files");
        let dest = files.join("game");
        crate::windows::copy_tree(game, &dest)?;
        // The app goes by the marker; the merge leaves it out.
        for name in [
            c2_mods::mobile_base::SOURCE_APK,
            c2_mods::mobile::MOBILE_SURVIVAL,
        ] {
            let src = base.join(name);
            if src.is_file() && !dest.join(name).exists() {
                fs::copy(&src, dest.join(name))?;
            }
        }
        if let Some(b) = balance {
            fs::copy(b, files.join(BALANCE))?;
        }
        fs::copy(apk, stage.join(APK))?;
        fs::write(
            stage.join("LICENSE-engine.txt"),
            include_str!("../../../../LICENSE"),
        )?;
        fs::write(
            stage.join("README.txt"),
            format!(
                "Install {APK} on the device, then copy the contents of files/ to \
                 Android/data/{app}/files/ (start the app once first so the folder \
                 exists). Saves and settings stay in files/c2rs/ on the device. This package \
                 contains your own game/mod data; engine downloads do not include it.\n"
            ),
        )?;
        Ok(())
    })
}

/// The steps that put a package on the device `adb` sees: the APK, then
/// its `game/` pushed beside the old one and swapped in only once it is
/// all there (saves and other folders are left alone).
pub fn install_steps(adb: &Path, package: &Path, app: &str) -> Vec<Step> {
    let r = device_files(app);
    let run = |args: Vec<String>| Step::Run {
        program: adb.to_path_buf(),
        args,
    };
    let shell = |cmd: String| run(vec!["shell".into(), cmd]);
    let mut steps = vec![
        run(vec![
            "install".into(),
            "-r".into(),
            package.join(APK).display().to_string(),
        ]),
        shell(format!("mkdir -p '{r}' && rm -rf '{r}/game.new'")),
        run(vec![
            "push".into(),
            package.join("files/game").display().to_string(),
            format!("{r}/game.new"),
        ]),
    ];
    let balance = package.join("files").join(BALANCE);
    steps.push(if balance.is_file() {
        run(vec![
            "push".into(),
            balance.display().to_string(),
            format!("{r}/{BALANCE}"),
        ])
    } else {
        shell(format!("rm -f '{r}/{BALANCE}'"))
    });
    steps.push(shell(format!(
        "rm -rf '{r}/game.old' && if [ -d '{r}/game' ]; then mv '{r}/game' '{r}/game.old'; fi \
         && mv '{r}/game.new' '{r}/game' && rm -rf '{r}/game.old'"
    )));
    steps
}

/// `adb`: the tools folder, PATH, or the Android SDK's platform tools.
pub fn find_adb(tools: Option<&Path>) -> Option<PathBuf> {
    crate::find_tool("adb", tools).or_else(|| {
        let name = if cfg!(windows) { "adb.exe" } else { "adb" };
        Some(sdk_dir()?.join("platform-tools").join(name)).filter(|p| p.is_file())
    })
}

/// The Android SDK: `ANDROID_HOME`, `ANDROID_SDK_ROOT`, else its usual
/// place (`~/Android/Sdk`, `%LOCALAPPDATA%\Android\Sdk`).
pub fn sdk_dir() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    var("ANDROID_HOME")
        .or_else(|| var("ANDROID_SDK_ROOT"))
        .or_else(|| {
            if cfg!(windows) {
                var("LOCALAPPDATA").map(|d| d.join("Android").join("Sdk"))
            } else if cfg!(target_os = "macos") {
                var("HOME").map(|h| h.join("Library/Android/sdk"))
            } else {
                var("HOME").map(|h| h.join("Android/Sdk"))
            }
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_package_holds_the_apk_the_game_and_its_marker() {
        let t = crate::publish::tests::Temp::new();
        let (base, game) = (t.0.join("base"), t.0.join("merged"));
        fs::create_dir_all(game.join("HUNTDAT")).unwrap();
        fs::create_dir_all(&base).unwrap();
        fs::write(game.join("HUNTDAT/_RES.TXT"), "synthetic").unwrap();
        fs::write(base.join(c2_mods::mobile_base::SOURCE_APK), "sha256:0").unwrap();
        let apk = t.0.join("engine.apk");
        fs::write(&apk, b"PK\x03\x04rest").unwrap();
        let bal = t.0.join("b.toml");
        fs::write(&bal, "").unwrap();
        let out = t.0.join("out/instance");
        assert!(validate_base(&game).is_err());
        assert!(validate_base(&base).is_ok());
        package(&game, &base, &apk, Some(&bal), PACKAGE, &out).unwrap();
        assert!(out.join(APK).is_file());
        assert!(out.join("files/game/HUNTDAT/_RES.TXT").is_file());
        assert!(out
            .join("files/game")
            .join(c2_mods::mobile_base::SOURCE_APK)
            .is_file());
        assert!(out.join("files/balance.toml").is_file());
        // A rebuild replaces it; a folder it did not make is left alone.
        package(&game, &base, &apk, None, PACKAGE, &out).unwrap();
        assert!(!out.join("files/balance.toml").exists());
        assert!(package(&game, &base, &apk, None, PACKAGE, &game).is_err());
        fs::write(&apk, b"ELF").unwrap();
        assert!(package(&game, &base, &apk, None, PACKAGE, &out).is_err());
        let steps = install_steps(Path::new("adb"), &out, PACKAGE);
        assert_eq!(steps.len(), 5);
    }

    #[test]
    fn a_name_makes_an_app_of_its_own() {
        assert_eq!(app_package(""), PACKAGE);
        assert_eq!(app_package("  "), PACKAGE);
        assert_eq!(app_package("Vanilla"), "io.github.c2rs.vanilla");
        assert_eq!(
            app_package("JPHL: Special Ed."),
            "io.github.c2rs.jphl_special_ed"
        );
        assert_eq!(app_package("2 Ice Age"), "io.github.c2rs.ice_age");
        assert_eq!(app_package("+++"), "io.github.c2rs.app");
        assert_eq!(
            device_files(&app_package("Vanilla")),
            "/sdcard/Android/data/io.github.c2rs.vanilla/files"
        );
    }

    #[test]
    fn names_that_meet_in_one_package_clash() {
        use crate::config::InstanceBuilds;
        let named = |n: &str| InstanceBuilds {
            android_name: n.into(),
            ..Default::default()
        };
        let builds = [
            (PathBuf::from("a.toml"), named("+++")),
            (PathBuf::from("b.toml"), named("")),
            (PathBuf::from("c.toml"), named("Ice Age")),
        ]
        .into_iter()
        .collect();
        let own = Path::new("me.toml");
        let clash = |name: &str| package_clash(&app_package(name), own, &builds);
        assert_eq!(clash("!!!"), Some(Path::new("a.toml")));
        assert_eq!(clash("ice-age"), Some(Path::new("c.toml")));
        assert_eq!(clash("Vanilla"), None);
        // The shared app, and an instance's own saved name, never clash.
        assert_eq!(clash(""), None);
        assert_eq!(
            package_clash(&app_package("+++"), Path::new("a.toml"), &builds),
            None
        );
    }
}
