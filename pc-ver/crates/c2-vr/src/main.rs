//! `c2-vr`: Carnivores 2 in a VR headset (first draft).
//!
//! - `c2-vr check`: the OpenXR runtime.
//! - `c2-vr scene [--seconds N]`: the test scene (no game data).
//! - `c2-vr area PATH/AREA1.MAP [--seconds N]`: that area's terrain.
//! - `c2-vr play GAME [--overlay DIR].. [--mod DIR].. [--standalone DIR]..`
//!   (what the launcher's "Play in VR" passes, the same as `c2-pc play`):
//!   checks the runtime and the game, then the main menu in the first
//!   area's camp (`game`). Other `c2-pc play` options are accepted and
//!   ignored.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use c2_vr::runtime;

fn has_huntdat(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("HUNTDAT")
            })
        })
        .unwrap_or(false)
}

fn check_runtime() -> Result<runtime::Runtime> {
    match runtime::active() {
        Ok(r) => {
            println!("VR runtime: {} ({})", r.name, r.manifest.display());
            match &r.library {
                Some(l) if r.library_found() => println!("  library: {}", l.display()),
                Some(l) => println!(
                    "  library: {} (not found: reinstall the runtime?)",
                    l.display()
                ),
                None => println!("  library: none named in the manifest"),
            }
            Ok(r)
        }
        Err(looked) => {
            for p in looked {
                println!("  looked in {}", p.display());
            }
            bail!(
                "No OpenXR runtime is set up. Start SteamVR (or Monado / WiVRn) and make it \
                 the OpenXR runtime (SteamVR: Settings > OpenXR > Set SteamVR as OpenXR \
                 Runtime), then try again."
            )
        }
    }
}

/// The game folders `play` was given: (base, overlays, add-ons,
/// standalones).
fn play_folders(args: &[String]) -> (Option<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let (mut base, mut overlays, mut mods, mut standalones) = (None, vec![], vec![], vec![]);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let slot = match a.as_str() {
            "--overlay" => &mut overlays,
            "--mod" => &mut mods,
            "--standalone" => &mut standalones,
            // An option of `c2-pc play` with a value (`--balance FILE` ...).
            s if s.starts_with("--") => {
                if !matches!(s, "--mobile-menu") {
                    it.next();
                }
                continue;
            }
            _ => {
                base.get_or_insert_with(|| PathBuf::from(a));
                continue;
            }
        };
        if let Some(v) = it.next() {
            slot.push(PathBuf::from(v));
        }
    }
    (base, overlays, mods, standalones)
}

fn play(args: &[String]) -> Result<()> {
    let (base, overlays, mods, standalones) = play_folders(args);
    let Some(base) = base else {
        bail!("c2-vr play GAME: no game folder given");
    };
    if !has_huntdat(&base) {
        bail!(
            "{}: no HUNTDAT folder, so not a game folder",
            base.display()
        );
    }
    println!("Game: {}", base.display());
    if let Some(o) = overlays.iter().find(|o| !has_huntdat(o)) {
        bail!("overlay {}: no HUNTDAT folder", o.display());
    }
    let addons: usize = mods.iter().map(|m| c2_mods::expand_addons(m).len()).sum();
    println!(
        "Overlays: {}, add-ons: {addons}, standalone games: {}",
        overlays.len(),
        standalones.len()
    );
    check_runtime()?;
    if !c2_vr::PLAYABLE {
        println!(
            "The hunt isn't in VR yet (first draft): the main menu, then the first area to \
             walk around, or the trophy room (add-ons are not loaded yet)."
        );
    }
    let game = c2_vr::game::Game::new(&base, &overlays)?;
    run(c2_vr::game::menu(game)?, seconds(args)?)
}

fn area(map: &Path, seconds: Option<f32>) -> Result<()> {
    let area = c2_vr::terrain::load(map)?;
    let sun = c2_vr::sky::load_sun(map, area.time);
    if sun.is_none() {
        println!("No SUN2.3DF beside the areas: no sun.");
    }
    println!("Area: {}", map.display());
    run(
        Box::new(move |gpu, format| {
            Ok(Box::new(c2_vr::terrain::TerrainScene::new(gpu, format, area, sun)?) as _)
        }),
        seconds,
    )
}

fn scene(seconds: Option<f32>) -> Result<()> {
    run(
        Box::new(|gpu, format| Ok(Box::new(c2_vr::test_scene::TestScene::new(gpu, format)?) as _)),
        seconds,
    )
}

/// Runs a scene (for `seconds`, if given) and prints what it measured.
fn run(make: c2_vr::scene::MakeScene, seconds: Option<f32>) -> Result<()> {
    let limit = seconds.map(std::time::Duration::from_secs_f32);
    let r = c2_vr::xr::run(make, limit)?;
    let fps = if r.seconds > 0.0 {
        r.frames as f32 / r.seconds
    } else {
        0.0
    };
    println!(
        "Done: {} on {}, {} x {} per eye, {} frames in {:.0} s ({fps:.1} fps), slowest frame \
         {:.1} ms on the CPU, {} not drawn (headset off or dashboard open)",
        r.system,
        r.runtime,
        r.resolution.0,
        r.resolution.1,
        r.frames,
        r.seconds,
        r.worst_cpu_ms,
        r.skipped
    );
    Ok(())
}

/// `--seconds N`, if given.
fn seconds(args: &[String]) -> Result<Option<f32>> {
    match args.iter().position(|a| a == "--seconds") {
        Some(i) => Ok(Some(
            args.get(i + 1)
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|s| *s > 0.0)
                .ok_or_else(|| anyhow!("--seconds N: a number of seconds"))?,
        )),
        None => Ok(None),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check_runtime().map(|_| ()),
        Some("play") => play(&args[1..]),
        Some("scene") => {
            let s = seconds(&args)?;
            check_runtime()?;
            scene(s)
        }
        Some("area") => {
            let Some(map) = args.get(1).filter(|a| !a.starts_with("--")) else {
                bail!("c2-vr area PATH/AREA1.MAP");
            };
            let s = seconds(&args)?;
            check_runtime()?;
            area(Path::new(map), s)
        }
        _ => bail!(
            "usage: c2-vr check | c2-vr scene [--seconds N] | c2-vr area MAP [--seconds N] | \
             c2-vr play GAME [--overlay DIR].. [--mod DIR].."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_args_as_the_launcher_sends_them() {
        let a: Vec<String> = [
            "/g",
            "--overlay",
            "/me",
            "--mod",
            "/a",
            "--balance",
            "/b.toml",
            "--mobile-menu",
            "--standalone",
            "/s",
            "--survival-modes",
            "classic",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (base, o, m, s) = play_folders(&a);
        assert_eq!(base, Some(PathBuf::from("/g")));
        assert_eq!(o, vec![PathBuf::from("/me")]);
        assert_eq!(m, vec![PathBuf::from("/a")]);
        assert_eq!(s, vec![PathBuf::from("/s")]);
    }
}
