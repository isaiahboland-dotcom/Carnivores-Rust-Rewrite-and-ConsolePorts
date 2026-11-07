//! c2rs for Android: the PC frontend (`c2-pc play`) in a NativeActivity.
//!
//! Nothing of any game is in the APK (docs/DEVELOPMENT.md §2). The player
//! copies their own files into the app's folder on the phone
//! (`Android/data/<package>/files/`, see android-ver/README.md):
//!
//! - `game/`: a game built from their own mobile APK (Dinosaur Hunter or Ice
//!   Age; `c2-pc mobile-base APK --out game`, or the launcher's "Build from my
//!   APK"). Only such a game is accepted: the phone's own menus and touch
//!   buttons come from it.
//! - `addons/` (optional): add-on folders, combined over the game in name
//!   order. Standalone games are not offered on Android.
//! - `balance.toml` (optional): prices and scores (`c2-pc play --balance`),
//!   as the launcher's Android package writes it.
//!
//! Saves, settings and combined games go to `files/c2rs/`, the log to
//! `files/c2rs.log`.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

/// Most animals and most weapons one hunt takes on a phone (owner,
/// 2026-11-06: more gets too hectic on a small touch screen, for now).
pub const MAX_PICKS: usize = 8;

/// The game folder, beside the add-ons, in the app's files folder.
pub const GAME_DIR: &str = "game";
pub const ADDONS_DIR: &str = "addons";
pub const BALANCE_FILE: &str = "balance.toml";

/// The `c2-pc` arguments for the files in `root`; an error says what is
/// missing.
pub fn launch_args(root: &Path) -> Result<Vec<String>> {
    let game = root.join(GAME_DIR);
    ensure!(
        game.is_dir(),
        "no game: copy a game built from your mobile APK to {}",
        game.display()
    );
    ensure!(
        game.join(c2_mods::mobile_base::SOURCE_APK).is_file(),
        "{} was not built from a mobile APK ({} missing): the Android version \
         needs the mobile Dinosaur Hunter or Ice Age as its base game",
        game.display(),
        c2_mods::mobile_base::SOURCE_APK
    );
    let mut args: Vec<String> = vec![
        "play".into(),
        path_arg(&game),
        "--mobile-menu".into(),
        "--max-picks".into(),
        MAX_PICKS.to_string(),
    ];
    let addons = root.join(ADDONS_DIR);
    if has_entries(&addons) {
        args.extend(["--mod".into(), path_arg(&addons)]);
    }
    let balance = root.join(BALANCE_FILE);
    if balance.is_file() {
        args.extend(["--balance".into(), path_arg(&balance)]);
    }
    Ok(args)
}

/// Development arguments file in the files folder (see [`run`]).
pub const DEV_ARGS_FILE: &str = "args.txt";

/// One argument per non-empty line, trimmed.
fn dev_args(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

fn path_arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn has_entries(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut rd| {
        rd.any(|e| e.is_ok_and(|e| !e.file_name().to_string_lossy().starts_with('.')))
    })
}

/// Logs to `root/c2rs.log` (adb logcat shows nothing of `tracing`).
///
/// Once per process: Android can start the activity again in the same
/// process, and a second subscriber would panic.
fn init_logging(root: &Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| start_log(root));
}

fn start_log(root: &Path) {
    let filter = tracing_subscriber::EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,naga=warn");
    match std::fs::File::create(root.join("c2rs.log")) {
        Ok(f) => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(f))
            .init(),
        Err(_) => tracing_subscriber::fmt().with_env_filter(filter).init(),
    }
    std::panic::set_hook(Box::new(|info| tracing::error!("panic: {info}")));
}

/// Runs the game from the files in `root`, with saves under `root/c2rs`.
pub fn run(root: PathBuf) -> Result<()> {
    init_logging(&root);
    // `c2-pc`'s data folder is `$XDG_DATA_HOME/c2rs`; Android sets no home.
    std::env::set_var("XDG_DATA_HOME", &root);
    std::env::set_current_dir(&root)?;
    // Development: `files/args.txt` (one argument per line) replaces the
    // arguments, e.g. a fixed `area` view for frame-time measurements.
    let dev_args = std::fs::read_to_string(root.join(DEV_ARGS_FILE))
        .ok()
        .map(|t| dev_args(&t));
    let args = match dev_args.map_or_else(|| launch_args(&root), Ok) {
        Ok(args) => args,
        // Nothing to play yet: say so on screen, not only in the log.
        Err(e) => {
            tracing::error!("{e:#}");
            return c2_pc::notice(format!(
                "c2rs can't start: {e:#}\n\nSee android-ver/README.md: build the game from your \
                 own mobile APK on a PC, then copy it to the phone as {}.",
                root.join(GAME_DIR).display()
            ));
        }
    };
    tracing::info!("c2rs {}: {args:?}", env!("CARGO_PKG_VERSION"));
    c2_pc::cli::run(args)
}

#[cfg(target_os = "android")]
#[allow(unsafe_code)] // looked up by name by android-activity
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let Some(root) = app
        .external_data_path()
        .or_else(|| app.internal_data_path())
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&root);
    c2_pc::set_android_app(app);
    if let Err(e) = run(root) {
        tracing::error!("{e:#}");
    }
    // One event loop per process (winit): end it, so the next start is new.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("c2-android-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn dev_args_take_one_argument_per_line() {
        let args = dev_args("area\n  /x/AREA2.MAP \n\n--view\n384\n");
        assert_eq!(args, ["area", "/x/AREA2.MAP", "--view", "384"]);
    }

    #[test]
    fn needs_a_game() {
        let root = temp("none");
        assert!(launch_args(&root).is_err());
    }

    #[test]
    fn refuses_a_pc_game() {
        let root = temp("pc");
        let _ = std::fs::create_dir_all(root.join(GAME_DIR));
        let err = launch_args(&root)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("mobile APK"), "{err}");
    }

    #[test]
    fn apk_game_with_and_without_addons() -> Result<()> {
        let root = temp("apk");
        std::fs::create_dir_all(root.join(GAME_DIR))?;
        std::fs::write(
            root.join(GAME_DIR).join(c2_mods::mobile_base::SOURCE_APK),
            "x",
        )?;
        let vanilla = launch_args(&root)?;
        assert!(vanilla.contains(&"--mobile-menu".to_string()));
        assert!(vanilla.windows(2).any(|w| w == ["--max-picks", "8"]));
        assert!(!vanilla.contains(&"--mod".to_string()));
        assert!(!vanilla.iter().any(|a| a == "--standalone"));
        std::fs::create_dir_all(root.join(ADDONS_DIR).join("some-addon"))?;
        let modded = launch_args(&root)?;
        assert!(modded.contains(&"--mod".to_string()));
        assert!(!modded.contains(&"--balance".to_string()));
        std::fs::write(root.join(BALANCE_FILE), "")?;
        assert!(launch_args(&root)?.contains(&"--balance".to_string()));
        Ok(())
    }
}
