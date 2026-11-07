//! The `c2-regress` command line: exit codes and failure handling without
//! game data, and (when `C2_ORIGINAL_DIR` names a Carnivores 2 install,
//! the folder holding `HUNTDAT/`) a headless run of the canonical scenario.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_c2-regress"))
}

fn scenario() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/vanilla_map2_four_dinos.toml")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("c2-regress-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    // Checked by the tests that use it.
    let _ = std::fs::create_dir_all(&d);
    d
}

#[test]
fn lists_games_and_their_status() {
    let o = bin()
        .args(["list", "--scenario"])
        .arg(scenario())
        .output()
        .unwrap();
    assert!(o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("vanilla_map2_four_dinos v2"));
    assert!(text.contains("c1") && text.contains("Unsupported"));
    assert!(text.contains("fallen-kings") && text.contains("Planned"));
}

#[test]
fn unsupported_games_are_reported_not_run() {
    for game in ["c1", "fallen-kings"] {
        let o = bin()
            .args(["run", "--scenario"])
            .arg(scenario())
            .args(["--game-id", game, "--game", "/nonexistent", "--no-render"])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(3), "{game}");
    }
}

#[test]
fn missing_assets_and_bad_input_are_errors() {
    let out = tmp("missing");
    let empty = tmp("empty-game");
    let o = bin()
        .args(["run", "--scenario"])
        .arg(scenario())
        .args(["--game-id", "c2", "--no-render", "--game"])
        .arg(&empty)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("_RES.TXT"));

    let bad = out.join("bad.toml");
    std::fs::write(&bad, "[scenario]\nid = 1\n").unwrap();
    let o = bin()
        .args(["run", "--scenario"])
        .arg(&bad)
        .args(["--game-id", "c2", "--game", "/x", "--no-render"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));

    let o = bin()
        .args(["compare", "/nope/a", "/nope/b"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    let o = bin().arg("frobnicate").output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    std::fs::remove_dir_all(&out).unwrap();
    std::fs::remove_dir_all(&empty).unwrap();
}

/// Needs a local Carnivores 2 install; skipped (passes) without one.
#[test]
fn canonical_scenario_runs_headless_on_carnivores_2() {
    let Ok(game) = std::env::var("C2_ORIGINAL_DIR") else {
        return;
    };
    let out = tmp("c2");
    let o = bin()
        .args(["run", "--scenario"])
        .arg(scenario())
        .args([
            "--game-id",
            "c2",
            "--no-render",
            "--repeat",
            "2",
            "--game",
            &game,
        ])
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    let runs: Vec<_> = std::fs::read_dir(out.join("vanilla_map2_four_dinos/c2"))
        .unwrap()
        .flatten()
        .flat_map(|p| std::fs::read_dir(p.path()).unwrap().flatten())
        .collect();
    assert_eq!(runs.len(), 1);
    let dir = runs[0].path();
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("metrics.json")).unwrap()).unwrap();
    assert_eq!(m["result"], "passed");
    assert_eq!(m["functional"]["weapons_fired_once"], 6);
    assert_eq!(m["functional"]["animals_end"], 4);
    assert_eq!(m["determinism"]["identical"], true);
    for f in [
        "manifest.json",
        "events.jsonl",
        "tick_times.csv",
        "engine.log",
        "report.html",
    ] {
        assert!(dir.join(f).exists(), "{f}");
    }
    std::fs::remove_dir_all(&out).unwrap();
}
