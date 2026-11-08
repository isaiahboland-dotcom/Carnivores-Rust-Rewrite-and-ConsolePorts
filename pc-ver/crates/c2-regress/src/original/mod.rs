//! The scenario in the original engine (`run --engine original`): the
//! shipped hunt executable under Wine, driven by the hook in
//! `pc-ver/crates/c2-regress/original/` (docs/REGRESSION.md). Recorded in
//! the same files as our engine's runs, with the same checkpoint names.

pub mod checks;
pub mod script;
pub mod wine;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use c2_core::scenario::Scenario;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::game::Game;
use crate::metrics::unavailable;
use crate::output::{build_info, write_json, write_jsonl};
use crate::scenario_file::{GameDef, ScenarioFile};
use checks::{changed, load_bmp, AMMO, CALL_ICON, WHOLE};

pub struct Original<'a> {
    pub file: &'a ScenarioFile,
    pub def: &'a GameDef,
    pub scenario: &'a Scenario,
    pub game: &'a Game,
    pub game_dir: &'a Path,
    /// Where the game copy and the Wine prefix are kept between runs.
    pub work: &'a Path,
    pub hook_dir: &'a Path,
    pub repeat: usize,
}

/// `target/original-hook` beside the `target/<profile>` this runs from.
pub fn default_hook_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.parent()?.join("original-hook"))
}

fn stage(name: &str, status: &str, reason: String, frame: u32) -> Value {
    json!({ "name": name, "source": "original", "status": status, "reason": reason,
            "start_tick": Value::Null, "end_tick": frame })
}

fn event(frame: u32, kind: &str, stage: &str, name: &str, ok: Option<bool>, detail: &str) -> Value {
    json!({ "tick": Value::Null, "frame": frame, "time_ms": frame * script::FRAME_MS,
            "source": "original", "kind": kind, "stage": stage, "name": name, "ok": ok,
            "detail": detail })
}

fn sha_file(p: &Path) -> String {
    std::fs::read(p).map_or_else(|_| "missing".into(), |d| format!("{:x}", Sha256::digest(d)))
}

/// Runs the hunt `repeat` times into `dir`; returns whether it passed.
pub fn run(o: &Original, dir: &Path) -> Result<bool> {
    for f in ["c2launch.exe", "c2hook.dll"] {
        if !o.hook_dir.join(f).exists() {
            bail!(
                "{} has no {f}: build it with pc-ver/crates/c2-regress/original/build.sh",
                o.hook_dir.display()
            );
        }
    }
    let area = o
        .def
        .area
        .as_deref()
        .context("the game section names no area")?;
    let weapons = o.game.weapon_names();
    let args = script::command_line(area, o.scenario, weapons.len())?;
    let plan = script::plan(o.scenario, &weapons);
    std::fs::create_dir_all(o.work)?;
    let game_dir = wine::game_copy(o.game_dir, o.work, &o.game.content_hash)?;
    let prefix = wine::prefix(o.work)?;
    let exe = wine::find_ci(&game_dir, "v_soft.ren")
        .context("no v_soft.ren (the software-rendered hunt) in the game folder")?;
    let shots = dir.join("screenshots");
    std::fs::create_dir_all(&shots)?;
    let script_path = dir.join("original-script.txt");
    std::fs::write(&script_path, &plan.text)?;

    let mut log = String::new();
    let mut runs: Vec<(Option<i32>, Vec<String>)> = Vec::new();
    for r in 0..o.repeat.max(1) {
        for e in std::fs::read_dir(&game_dir)?.flatten() {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            if n.ends_with(".bmp") || n == "c2hook.log" {
                std::fs::remove_file(e.path())?;
            }
        }
        // A fresh hunter each run: 5000 points, 800x600 (OptRes 4).
        let mut save = c2_core::profile::new_player("regress", 0, 5000);
        save.options.res = 4;
        std::fs::write(game_dir.join("trophy00.sav"), c2_formats::sav::write(&save))?;
        let code = wine::run(&wine::Launch {
            prefix: &prefix,
            hook_dir: o.hook_dir,
            game_dir: &game_dir,
            exe: &exe,
            args: &args,
            script: &script_path,
            exit_frame: plan.exit_frame,
            timeout: Duration::from_secs(300),
        })?;
        let hook_log = std::fs::read_to_string(game_dir.join("c2hook.log")).unwrap_or_default();
        log.push_str(&format!(
            "--- original run {} (exit {code:?})\n{hook_log}",
            r + 1
        ));
        let hashes = plan
            .shots
            .iter()
            .map(|s| sha_file(&game_dir.join(format!("{}.bmp", s.name))))
            .collect();
        if r == 0 {
            if let Ok(t) = std::fs::read_to_string(game_dir.join("render.log")) {
                log.push_str("--- render.log (the game's own)\n");
                log.push_str(&t);
            }
            for s in &plan.shots {
                if let Ok(img) = load_bmp(&game_dir.join(format!("{}.bmp", s.name))) {
                    let png = crate::image_diff::encode_rgb(img.width, img.height, &img.rgb)?;
                    std::fs::write(shots.join(format!("{}.png", s.name)), png)?;
                }
            }
            // Keep the BMPs for the checks below.
            let keep = dir.join(".bmp");
            std::fs::create_dir_all(&keep)?;
            for s in &plan.shots {
                let src = game_dir.join(format!("{}.bmp", s.name));
                if src.exists() {
                    std::fs::copy(&src, keep.join(format!("{}.bmp", s.name)))?;
                }
            }
        }
        runs.push((code, hashes));
    }
    let passed = record(o, dir, &plan, &runs, &args, &exe, log)?;
    let _ = std::fs::remove_dir_all(dir.join(".bmp"));
    Ok(passed)
}

fn record(
    o: &Original,
    dir: &Path,
    plan: &script::Plan,
    runs: &[(Option<i32>, Vec<String>)],
    args: &str,
    exe: &Path,
    log: String,
) -> Result<bool> {
    let bmp = |name: &str| load_bmp(&dir.join(".bmp").join(format!("{name}.bmp"))).ok();
    let frame_of = |name: &str| {
        plan.shots
            .iter()
            .find(|s| s.name == name)
            .map_or(0, |s| s.frame)
    };
    let (code, _) = &runs[0];
    let ended = log.contains(" exit\r\n") || log.contains(" exit\n");
    let message = log
        .lines()
        .find(|l| l.contains("message box"))
        .map(str::to_string);
    let mut stages = Vec::new();
    let mut events = Vec::new();
    let mut fired = 0;
    let check = |name: &str, ok: bool, why: String, frame: u32, st: &mut Vec<Value>| {
        let (status, reason) = if ok {
            ("passed", String::new())
        } else {
            ("failed", why)
        };
        st.push(stage(name, status, reason, frame));
    };
    let warm = bmp("warmup");
    // 800x600 is the virtual desktop; anything else means the game ran on
    // the real screen (and its pictures cannot be trusted).
    let (warm_ok, why) = match &warm {
        Some(w) if (w.width, w.height) == (800, 600) => (true, String::new()),
        Some(w) => (
            false,
            format!(
                "{}x{}: not inside the 800x600 virtual desktop",
                w.width, w.height
            ),
        ),
        None => (false, "no warm-up screenshot".into()),
    };
    check("warmup", warm_ok, why, frame_of("warmup"), &mut stages);
    for st in plan.stages.iter().filter(|s| s.starts_with("weapon_")) {
        let (ready, after) = (bmp(&format!("{st}_ready")), bmp(&format!("{st}_after")));
        let (ok, why) = match (&warm, &ready, &after) {
            (Some(w), Some(r), Some(a)) => {
                let drawn = changed(w, r, WHOLE);
                let ammo = changed(r, a, AMMO);
                let detail = format!("view changed {drawn:.3} on drawing, ammo display {ammo:.3}");
                events.push(event(
                    frame_of(&format!("{st}_after")),
                    "verify",
                    st,
                    "fired",
                    Some(ammo > 0.002),
                    &detail,
                ));
                (drawn > 0.01 && ammo > 0.002, detail)
            }
            _ => (false, "screenshots missing".into()),
        };
        fired += usize::from(ok);
        check(st, ok, why, frame_of(&format!("{st}_after")), &mut stages);
    }
    for (name, region, min) in [
        ("binoculars", WHOLE, 0.05),
        ("map", WHOLE, 0.02),
        ("call_icon", CALL_ICON, 0.01),
    ] {
        let (open, closed) = (bmp(&format!("{name}_open")), bmp(&format!("{name}_closed")));
        let d = match (&open, &closed) {
            (Some(a), Some(b)) => changed(a, b, region),
            _ => 0.0,
        };
        let detail = format!("open vs closed: {d:.3} of the region differs");
        events.push(event(
            frame_of(&format!("{name}_closed")),
            "verify",
            name,
            &format!("{name}_open"),
            Some(d > min),
            &detail,
        ));
        check(
            name,
            d > min,
            detail,
            frame_of(&format!("{name}_closed")),
            &mut stages,
        );
    }
    for name in ["weapon_picker", "call_picker"] {
        stages.push(stage(
            name,
            "unsupported",
            "the original has no picker screens".into(),
            0,
        ));
    }
    let end_ok = bmp("end").is_some() && *code == Some(0) && ended;
    let why = match (code, &message) {
        (_, Some(m)) => m.clone(),
        (None, _) => "timeout: the game was stopped".into(),
        (c, _) => format!("the game ended with {c:?} before the script's end"),
    };
    check("end", end_ok, why, plan.exit_frame, &mut stages);
    for s in &stages {
        events.push(event(
            s["end_tick"].as_u64().unwrap_or(0) as u32,
            "stage_end",
            s["name"].as_str().unwrap_or(""),
            s["status"].as_str().unwrap_or(""),
            Some(s["status"] != "failed"),
            s["reason"].as_str().unwrap_or(""),
        ));
    }
    let count = |k: &str| stages.iter().filter(|s| s["status"] == k).count();
    let identical = runs.iter().all(|r| r.1 == runs[0].1);
    let failed = count("failed") > 0 || !identical;
    let ui = |n: &str| {
        stages
            .iter()
            .find(|s| s["name"] == n)
            .map_or(json!("not_run"), |s| s["status"].clone())
    };
    let checkpoints: Vec<Value> = plan
        .shots
        .iter()
        .zip(&runs[0].1)
        .filter(|(s, _)| {
            !s.name.ends_with("_ready")
                && !s.name.ends_with("_after")
                && !s.name.ends_with("_closed")
        })
        .map(|(s, h)| json!({ "name": s.name, "frame": s.frame, "screenshot_sha256": h }))
        .collect();
    let why_none = "the original's state is not read; its clock is virtual (20 ms a frame), so wall times say nothing";
    let metrics = json!({
        "result": if failed { "failed" } else { "passed" },
        "engine": "original",
        "functional": {
            "map": o.game.area_name,
            "animals_expected": o.scenario.species.len(),
            "animals_placed": unavailable("the original places its own animals; the four are the hunt's targets"),
            "animals_end": unavailable("the original's state is not read"),
            "weapon_registry": o.game.weapons.len(),
            "weapon_registry_expected": o.def.weapons,
            "weapons_fired_once": fired,
            "stages": { "passed": count("passed"), "failed": count("failed"), "unsupported": count("unsupported") },
            "timeouts": usize::from(code.is_none()),
            "panics": 0,
            "setup_error": message,
        },
        "ui": { "binoculars": ui("binoculars"), "map": ui("map"), "call_icon": ui("call_icon"),
                "weapon_picker": ui("weapon_picker"), "call_picker": ui("call_picker") },
        "simulation": unavailable(why_none),
        "render": unavailable(why_none),
        "memory": { "process_bytes": unavailable(why_none), "peak_bytes": unavailable(why_none) },
        "determinism": if runs.len() < 2 {
            json!({ "repeats": 1, "identical": unavailable("one run: use --repeat 2 or more") })
        } else {
            json!({ "repeats": runs.len(), "identical": identical, "compared": "every screenshot, byte for byte" })
        },
        "checkpoints": checkpoints,
    });
    let files: Vec<Value> = o
        .game
        .files
        .iter()
        .map(|(p, h)| json!({ "path": p, "sha256": h }))
        .collect();
    let wine_version = std::process::Command::new("wine")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let manifest = json!({
        "scenario": { "id": o.file.id(), "version": o.file.version(), "file_sha256": o.file.hash },
        "game": { "id": o.def.id, "name": o.def.name, "area": o.game.area_name },
        "engine": "original",
        "platform": "original-wine",
        "mode": "original",
        "build": build_info(),
        "original": {
            "hunt_executable": exe.file_name().map(|n| n.to_string_lossy().into_owned()),
            "hunt_executable_sha256": sha_file(exe),
            "command_line": args,
            "frame_ms": script::FRAME_MS,
            "wine": wine_version,
            "hook": "c2hook.dll: virtual timeGetTime, scripted input, growable heap",
        },
        "content": { "sha256": o.game.content_hash, "files": files },
        "seed": unavailable("the original seeds rand() from its clock, which starts at a fixed value"),
        "settings": { "tick_ms": Value::Null, "frame_ms": script::FRAME_MS,
                      "warmup_ticks": o.scenario.warmup_ticks, "time_of_day": "day",
                      "repeats": runs.len(), "inject": Value::Null },
        "hardware": crate::output::hardware(),
        "renderer": { "backend": "original software renderer (v_soft.ren)", "resolution": [800, 600] },
    });
    write_json(&dir.join("metrics.json"), &metrics)?;
    write_json(&dir.join("manifest.json"), &manifest)?;
    write_jsonl(&dir.join("events.jsonl"), &events)?;
    std::fs::write(dir.join("engine.log"), log)?;
    Ok(!failed)
}
