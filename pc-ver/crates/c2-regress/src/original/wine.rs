//! Running the original under Wine: a working copy of the game folder, a
//! Wine prefix of its own with an 800x600 virtual desktop, and one hooked
//! hunt (`c2launch.exe`, `c2hook.dll`; built by `original/build.sh`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// Files in a game install that are not the game: renderer wrappers and
/// their settings, logs, saves.
fn skipped(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "d3d9.dll" | "dxgi.dll" | "opengl32.dll" | "reshade-shaders"
    ) || n.ends_with(".ini")
        || n.ends_with(".log")
        || n.ends_with(".sav")
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let name = e.file_name();
        if skipped(&name.to_string_lossy()) {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        if e.file_type()?.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).with_context(|| format!("copying {}", src.display()))?;
        }
    }
    Ok(())
}

/// `work/game` holds a copy of `game` (made once per content hash), so
/// the game's own files (logs, saves) are never written in the install.
pub fn game_copy(game: &Path, work: &Path, content_hash: &str) -> Result<PathBuf> {
    let dir = work.join("game");
    let stamp = work.join("game.content");
    if std::fs::read_to_string(&stamp).ok().as_deref() != Some(content_hash) {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {}", dir.display()))?;
        }
        copy_tree(game, &dir)?;
        std::fs::write(&stamp, content_hash)?;
    }
    pin_config(&dir)?;
    Ok(dir)
}

/// Modder's Edition forks read `config.cfg`, whose `resolution` overrides
/// the profile's: pin the copy to the 800x600 virtual desktop (exclusive
/// fullscreen, a popup window with no title bar), the software renderer
/// and no frame limit. Other keys are kept.
fn pin_config(dir: &Path) -> Result<()> {
    let Some(p) = find_ci(dir, "config.cfg") else {
        return Ok(());
    };
    let pins = [
        ("resolution", "800x600"),
        ("display_mode", "1"),
        ("renderer", "0"),
        ("fps_limit", "0"),
    ];
    let text = std::fs::read_to_string(&p)?;
    let mut out: Vec<String> = text
        .lines()
        .filter(|l| {
            let key = l.trim_start_matches('#').split_whitespace().next();
            !pins.iter().any(|(k, _)| Some(*k) == key)
        })
        .map(String::from)
        .collect();
    out.extend(pins.iter().map(|(k, v)| format!("{k} {v}")));
    std::fs::write(&p, out.join("\n") + "\n")?;
    Ok(())
}

/// A file in `dir`, ignoring case.
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

fn wine(prefix: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("wine");
    c.env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .args(args);
    c.stdout(Stdio::null()).stderr(Stdio::null());
    c
}

/// Creates the prefix once, set to an 800x600 virtual desktop so the game
/// never changes the display mode.
pub fn prefix(work: &Path) -> Result<PathBuf> {
    // Wine refuses a relative WINEPREFIX (a relative `--out`).
    let p = std::path::absolute(work.join("prefix"))?;
    if !p.join("system.reg").exists() {
        std::fs::create_dir_all(&p)?;
        let output = Command::new("wineboot")
            .arg("-i")
            .env("WINEPREFIX", &p)
            .env("WINEDEBUG", "-all")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("running wineboot (is Wine installed?)")?;
        if !output.status.success() {
            bail!(
                "wineboot failed: {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    }
    for (key, value, data) in [
        ("HKCU\\Software\\Wine\\Explorer", "Desktop", "Default"),
        (
            "HKCU\\Software\\Wine\\Explorer\\Desktops",
            "Default",
            "800x600",
        ),
    ] {
        let output = wine(&p, &["reg", "add", key, "/v", value, "/d", data, "/f"])
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            bail!(
                "configuring Wine desktop failed: {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    }
    // `wine reg` started the desktop before the setting existed; a game
    // started while it lingers would get the real screen.
    stop(&p);
    Ok(p)
}

/// Stops every Wine process of the prefix and waits for them.
fn stop(prefix: &Path) {
    let _ = Command::new("wineserver")
        .arg("-k")
        .env("WINEPREFIX", prefix)
        .status();
    let _ = Command::new("wineserver")
        .arg("-w")
        .env("WINEPREFIX", prefix)
        .status();
}

/// A Unix path as Wine sees it.
pub fn win_path(prefix: &Path, p: &Path) -> Result<String> {
    let o = Command::new("winepath")
        .arg("-w")
        .arg(p)
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .output()?;
    if !o.status.success() || o.stdout.is_empty() {
        bail!(
            "winepath failed for {}: {}: {}",
            p.display(),
            o.status,
            String::from_utf8_lossy(&o.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub struct Launch<'a> {
    pub prefix: &'a Path,
    pub hook_dir: &'a Path,
    pub game_dir: &'a Path,
    pub exe: &'a Path,
    pub args: &'a str,
    pub script: &'a Path,
    pub exit_frame: u32,
    pub timeout: Duration,
}

/// Keep the hook input below Win32's MAX_PATH even when the audit's output
/// directory and stage name make the archived script's full path longer.
fn stage_script(game_dir: &Path, script: &Path) -> Result<&'static str> {
    const NAME: &str = "c2-regression-script.txt";
    std::fs::copy(script, game_dir.join(NAME)).context("staging original-engine script")?;
    Ok(NAME)
}

/// Runs one hunt from a clean slate: the last run's pictures and hook log
/// removed, a fresh hunter (5000 points, 800x600: `OptRes` 4). Returns the
/// exit code and the hook's log.
pub fn run_fresh(l: &Launch) -> Result<(Option<i32>, String)> {
    for e in std::fs::read_dir(l.game_dir)?.flatten() {
        let n = e.file_name().to_string_lossy().to_ascii_lowercase();
        if n.ends_with(".bmp") || n == "c2hook.log" {
            std::fs::remove_file(e.path())?;
        }
    }
    let mut save = c2_core::profile::new_player("regress", 0, 5000);
    save.options.res = 4;
    std::fs::write(
        l.game_dir.join("trophy00.sav"),
        c2_formats::sav::write(&save),
    )?;
    let code = run(l)?;
    let log = std::fs::read_to_string(l.game_dir.join("c2hook.log")).unwrap_or_default();
    require_hooks(&log)?;
    Ok((code, log))
}

/// The hooks every capture depends on: the virtual clock, the frame count
/// that drives the script, and the presented-frame capture. Others
/// (`GetAsyncKeyState`, `CreateProcessA`) are absent from some executables.
const REQUIRED_HOOKS: [&str; 3] = ["timeGetTime", "PeekMessageA", "BitBlt"];

/// Fails when the hook log reports a required import as not found: such a
/// run is untimed, unscripted or uncaptured, which is an infrastructure
/// error and not an engine result.
pub fn require_hooks(hook_log: &str) -> Result<()> {
    let missing: Vec<&str> = REQUIRED_HOOKS
        .into_iter()
        .filter(|name| {
            hook_log
                .lines()
                .any(|l| l.trim() == format!("{name} NOT FOUND"))
        })
        .collect();
    if !missing.is_empty() {
        bail!(
            "the original executable does not import {}; the hook cannot drive or capture it",
            missing.join(", ")
        );
    }
    Ok(())
}

/// Runs one hunt; returns its exit code (`None`: killed at the timeout).
pub fn run(l: &Launch) -> Result<Option<i32>> {
    let script = stage_script(l.game_dir, l.script)?;
    let mut cmd = wine(l.prefix, &[]);
    cmd.arg(l.hook_dir.join("c2launch.exe"))
        .arg(win_path(l.prefix, l.exe)?)
        .args(l.args.split(' '))
        .current_dir(l.game_dir)
        .env("C2H_SCRIPT", script)
        .env("C2H_EXIT", (l.exit_frame + 500).to_string());
    stop(l.prefix);
    let mut child = cmd.spawn().context("starting wine")?;
    let start = Instant::now();
    let code = loop {
        if let Some(s) = child.try_wait()? {
            break s.code();
        }
        if start.elapsed() > l.timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    // Nothing of the run may linger in the prefix.
    stop(l.prefix);
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_required_hooks_fail_and_optional_ones_do_not() {
        // The log every supported executable produces: optional imports absent.
        let usual = "timeGetTime hooked\r\nPeekMessageA hooked\r\nBitBlt hooked\r\n\
                     GetAsyncKeyState NOT FOUND\r\nCreateProcessA NOT FOUND\r\n";
        assert!(require_hooks(usual).is_ok());
        // A run that never reached the hook has no verdict to give here.
        assert!(require_hooks("").is_ok());
        let unclocked = usual.replace("timeGetTime hooked", "timeGetTime NOT FOUND");
        let error = require_hooks(&unclocked).unwrap_err().to_string();
        assert!(error.contains("timeGetTime") && !error.contains("CreateProcessA"));
        let blind = usual.replace("BitBlt hooked", "BitBlt NOT FOUND");
        assert!(require_hooks(&blind)
            .unwrap_err()
            .to_string()
            .contains("BitBlt"));
    }

    #[test]
    fn long_archived_script_is_staged_verbatim_under_a_short_relative_name() -> Result<()> {
        let root = std::env::temp_dir().join(format!("c2-hook-script-{}", std::process::id()));
        let archived = root
            .join("a".repeat(90))
            .join("b".repeat(90))
            .join("c".repeat(90));
        let game = root.join("game");
        std::fs::create_dir_all(&archived)?;
        std::fs::create_dir_all(&game)?;
        let script = archived.join("original-script-kill_01_wooly_rhinoceros.txt");
        let text = b"101 release 13 600\nD+301 exit\n";
        std::fs::write(&script, text)?;
        assert!(script.to_string_lossy().len() > 260);
        let name = stage_script(&game, &script)?;
        assert!(!Path::new(name).is_absolute());
        assert_eq!(std::fs::read(game.join(name))?, text);
        assert_eq!(std::fs::read(&script)?, text);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }
}
