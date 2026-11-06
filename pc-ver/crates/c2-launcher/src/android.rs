//! An instance for the Android app: the engine APK beside the merged game,
//! laid out as the app's files folder, and its install over adb.
//!
//! The APK holds no game data (docs/DEVELOPMENT.md §2); the game goes to
//! `Android/data/<package>/files/game` on the device (android-ver/README.md).
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::jobs::Step;

const MARKER: &str = ".c2rs-android-package";
/// The app's package name (android-ver/AndroidManifest.xml).
pub const PACKAGE: &str = "io.github.c2rs";
/// The engine APK, as `android-ver/build-apk.sh` names it.
pub const APK: &str = "c2rs.apk";
/// What the app reads from its files folder, beside `game/`.
const BALANCE: &str = "balance.toml";

/// The app's files folder on the device.
pub fn device_files() -> String {
    format!("/sdcard/Android/data/{PACKAGE}/files")
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
                 Android/data/{PACKAGE}/files/ (start the app once first so the folder \
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
pub fn install_steps(adb: &Path, package: &Path) -> Vec<Step> {
    let r = device_files();
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
        let sdk = std::env::var_os("ANDROID_HOME")
            .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Android/Sdk")))?;
        let name = if cfg!(windows) { "adb.exe" } else { "adb" };
        Some(sdk.join("platform-tools").join(name)).filter(|p| p.is_file())
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
        package(&game, &base, &apk, Some(&bal), &out).unwrap();
        assert!(out.join(APK).is_file());
        assert!(out.join("files/game/HUNTDAT/_RES.TXT").is_file());
        assert!(out
            .join("files/game")
            .join(c2_mods::mobile_base::SOURCE_APK)
            .is_file());
        assert!(out.join("files/balance.toml").is_file());
        // A rebuild replaces it; a folder it did not make is left alone.
        package(&game, &base, &apk, None, &out).unwrap();
        assert!(!out.join("files/balance.toml").exists());
        assert!(package(&game, &base, &apk, None, &game).is_err());
        fs::write(&apk, b"ELF").unwrap();
        assert!(package(&game, &base, &apk, None, &out).is_err());
        let steps = install_steps(Path::new("adb"), &out);
        assert_eq!(steps.len(), 5);
    }
}
