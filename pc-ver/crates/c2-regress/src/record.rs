//! A finished run written out: `manifest.json`, `metrics.json`,
//! `events.jsonl`, `tick_times.csv`, `engine.log`. The PC renderer's own
//! files (`render.json`, `frame_times.csv`, `screenshots/`) are read back
//! and folded in when present.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use c2_core::prof::SECTIONS;
use c2_core::scenario::Scenario;
use serde_json::{json, Map, Value};

use crate::game::Game;
use crate::headless::RunData;
use crate::metrics::{process_memory, unavailable, Stats};
use crate::output::{build_info, event_json, hardware, stage_json, write_json, write_jsonl};
use crate::scenario_file::{GameDef, ScenarioFile};

pub struct RunContext<'a> {
    pub file: &'a ScenarioFile,
    pub def: &'a GameDef,
    pub game: &'a Game,
    pub scenario: &'a Scenario,
    pub platform: &'a str,
    pub runs: &'a [RunData],
    /// `render.json` from `c2-pc regress`, when the render pass ran.
    pub render: Option<Value>,
    /// Why there is no render pass, when there is none.
    pub render_missing: Option<String>,
    /// The deliberate fault this run carries (`--inject`), if any.
    pub inject: Option<String>,
    pub log: String,
}

/// Frame times (ms, the `total_ms` column) from `frame_times.csv`.
pub fn read_frame_times(path: &Path) -> Option<Vec<f64>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next()?.split(',').collect();
    let col = header.iter().position(|h| *h == "total_ms")?;
    Some(
        lines
            .filter_map(|l| l.split(',').nth(col)?.parse().ok())
            .collect(),
    )
}

fn stats_or(values: &[f64], why: &str) -> Value {
    Stats::of(values).map_or_else(|| unavailable(why), |s| s.json_ms())
}

/// Do all repeats reach the same checkpoints with the same state?
fn determinism(runs: &[RunData]) -> Value {
    let Some(first) = runs.first() else {
        return unavailable("no runs");
    };
    let mut mismatches = Vec::new();
    for (i, r) in runs.iter().enumerate().skip(1) {
        let a: Vec<(&str, u32, u64)> = first
            .checkpoints
            .iter()
            .map(|c| (c.name.as_str(), c.tick, c.hash))
            .collect();
        let b: Vec<(&str, u32, u64)> = r
            .checkpoints
            .iter()
            .map(|c| (c.name.as_str(), c.tick, c.hash))
            .collect();
        if a != b {
            let at = a.iter().zip(&b).position(|(x, y)| x != y);
            mismatches.push(json!({ "repeat": i + 1, "first_difference": at }));
        }
    }
    if runs.len() < 2 {
        return json!({
            "repeats": 1,
            "identical": unavailable("one run: use --repeat 2 or more"),
            "checkpoints": first.checkpoints.len(),
        });
    }
    json!({
        "repeats": runs.len(),
        "identical": mismatches.is_empty(),
        "checkpoints": first.checkpoints.len(),
        "mismatches": mismatches,
    })
}

/// Every stage, headless and rendered.
fn all_stages(ctx: &RunContext) -> Vec<Value> {
    let mut v: Vec<Value> = ctx
        .runs
        .first()
        .and_then(|r| r.director.as_ref())
        .map(|d| d.stages.iter().map(|s| stage_json(s, "headless")).collect())
        .unwrap_or_default();
    if let Some(Value::Array(r)) = ctx.render.as_ref().map(|r| r["stages"].clone()) {
        v.extend(r);
    }
    v
}

pub fn write(dir: &Path, ctx: &RunContext) -> Result<bool> {
    std::fs::create_dir_all(dir)?;
    let first = ctx.runs.first();
    let stages = all_stages(ctx);
    let count = |k: &str| stages.iter().filter(|s| s["status"] == k).count();
    let director = first.and_then(|r| r.director.as_ref());
    let events = director.map_or(&[][..], |d| &d.events[..]);
    let fired = events
        .iter()
        .filter(|e| e.name == "fired_once" && e.ok == Some(true))
        .count();
    let timeouts = events.iter().filter(|e| e.name == "timeout").count();
    let setup_error = first.and_then(|r| r.setup_error.clone());
    let panics = ctx.runs.iter().filter(|r| r.panic.is_some()).count();
    let registry = ctx.game.weapons.len();
    let weapons_ok = ctx.def.weapons.is_none_or(|n| n == registry);
    // Only the tour fires; each kill or behaviour is a stage of its own.
    use c2_core::scenario::Script;
    let kill_script = matches!(ctx.scenario.script, Script::Kills(_));
    let behaviours = matches!(ctx.scenario.script, Script::Behaviours(_));
    let to_fire = if matches!(ctx.scenario.script, Script::Tour) {
        registry
    } else {
        0
    };
    let end_expected = if behaviours {
        0
    } else {
        ctx.scenario.species.len()
    };
    let kill_stages = |st: &str| {
        stages
            .iter()
            .filter(|s| s["source"] == "headless")
            .filter(|s| s["name"].as_str().is_some_and(|n| n.starts_with("kill_")))
            .filter(|s| st.is_empty() || s["status"] == st)
            .count()
    };
    let kills = kill_script.then(|| {
        json!({ "expected": ctx.scenario.species.len(), "ran": kill_stages(""),
                "passed": kill_stages("passed") })
    });
    let kills_ok = !kill_script || kill_stages("passed") == ctx.scenario.species.len();
    let determinism = determinism(ctx.runs);
    let render_state = crate::render_state::agreement(first, ctx.render.as_ref());
    // Repeats that disagree, or a render pass that reaches another state:
    // the run is not reproducible, so it fails.
    let failed = count("failed") > 0
        || determinism["identical"] == false
        || render_state["identical"] == false
        || setup_error.is_some()
        || panics > 0
        || !weapons_ok
        || fired != to_fire
        || !kills_ok
        || director.is_none();

    // UI screens: passed on any source beats unsupported; any failure fails.
    let mut ui = Map::new();
    for screen in [
        "binoculars",
        "map",
        "call_icon",
        "weapon_picker",
        "call_picker",
    ] {
        let st: Vec<&str> = stages
            .iter()
            .filter(|s| s["name"] == screen)
            .filter_map(|s| s["status"].as_str())
            .collect();
        let v = if st.contains(&"failed") {
            "failed"
        } else if st.contains(&"passed") {
            "passed"
        } else if st.is_empty() {
            "not_run"
        } else {
            "unsupported"
        };
        ui.insert(screen.into(), json!(v));
    }

    let samples: Vec<&crate::headless::TickSample> =
        ctx.runs.iter().flat_map(|r| &r.samples).collect();
    let tick_ms: Vec<f64> = samples.iter().map(|s| s.ms).collect();
    let mut sections = Map::new();
    for (i, name) in SECTIONS.iter().enumerate() {
        let v: Vec<f64> = samples.iter().map(|s| f64::from(s.sections[i])).collect();
        sections.insert((*name).into(), stats_or(&v, "no ticks"));
    }
    let ai = sections.get("dinos").cloned().unwrap_or(Value::Null);
    let (rss, peak) = process_memory();
    let render = match (&ctx.render, &ctx.render_missing) {
        (Some(r), _) => {
            let frames = read_frame_times(&dir.join("frame_times.csv")).unwrap_or_default();
            json!({
                "frames": Stats::of(&frames)
                    .map_or_else(|| unavailable("no frame_times.csv rows"), |s| s.json_fps()),
                "adapter": r["adapter"],
                "resolution": r["resolution"],
                "process_peak_bytes": r["process_peak_bytes"],
                "gpu_memory": unavailable("wgpu reports no GPU memory use"),
                "gpu_time": unavailable("no GPU timestamp queries; frame time includes the GPU wait"),
            })
        }
        (None, why) => unavailable(why.as_deref().unwrap_or("render pass not run")),
    };
    let metrics = json!({
        "result": if failed { "failed" } else { "passed" },
        "functional": {
            "map": ctx.game.area_name,
            "animals_expected": ctx.scenario.species.len(),
            "animals_placed": first.map(|r| r.placed.clone()),
            "animals_end": first.map(|r| r.animals_end),
            "animals_end_expected": end_expected,
            "weapon_registry": registry,
            "weapon_registry_expected": ctx.def.weapons,
            "weapons_fired_once": fired,
            "weapons_to_fire": to_fire,
            "kills": kills,
            "kills_detail": director.map(|d| d.kill_reports.iter()
                .map(crate::kill_parity::report_json).collect::<Vec<_>>()),
            "behaviours_detail": director.filter(|_| behaviours).map(|d| d.behaviour_reports
                .iter().map(crate::behaviour_parity::report_json).collect::<Vec<_>>()),
            "shots_total": first.map(|r| r.shots),
            "stages": { "passed": count("passed"), "failed": count("failed"),
                        "unsupported": count("unsupported") },
            "timeouts": timeouts,
            "panics": panics,
            "setup_error": setup_error,
        },
        "ui": ui,
        "simulation": {
            "ticks_per_run": first.map(|r| r.samples.len()),
            "tick_ms": c2_core::scenario::TICK_MS,
            "tick_duration": stats_or(&tick_ms, "no ticks"),
            "ai_update": ai,
            "sections": sections,
            "wall_ms_per_run": ctx.runs.iter().map(|r| r.wall_ms).collect::<Vec<_>>(),
        },
        "entities": {
            "animals_end": first.map(|r| r.animals_end),
            "effects_end": first.map(|r| r.effects_end),
        },
        "memory": { "process_bytes": rss, "peak_bytes": peak },
        "render": render,
        "determinism": determinism,
        "render_state": render_state,
        "checkpoints": first.map(|r| r.checkpoints.iter().map(|c| json!({
            "name": c.name, "tick": c.tick, "state_hash": format!("{:016x}", c.hash),
        })).collect::<Vec<_>>()),
    });
    write_json(&dir.join("metrics.json"), &metrics)?;
    write_json(&dir.join("manifest.json"), &manifest(ctx))?;

    let mut rows: Vec<Value> = events.iter().map(|e| event_json(e, "headless")).collect();
    if let Some(Value::Array(r)) = ctx.render.as_ref().map(|r| r["events"].clone()) {
        rows.extend(r);
    }
    write_jsonl(&dir.join("events.jsonl"), &rows)?;

    let mut csv = String::from("tick");
    for s in SECTIONS {
        let _ = write!(csv, ",{s}_ms");
    }
    csv.push_str(",total_ms\n");
    if let Some(r) = first {
        for s in &r.samples {
            let _ = write!(csv, "{}", s.tick);
            for v in s.sections {
                let _ = write!(csv, ",{v:.4}");
            }
            let _ = writeln!(csv, ",{:.4}", s.ms);
        }
    }
    std::fs::write(dir.join("tick_times.csv"), csv)?;
    std::fs::write(dir.join("engine.log"), &ctx.log)?;
    Ok(!failed)
}

fn manifest(ctx: &RunContext) -> Value {
    let s = ctx.scenario;
    let files: Vec<Value> = ctx
        .game
        .files
        .iter()
        .map(|(p, h)| json!({ "path": p, "sha256": h }))
        .collect();
    json!({
        "scenario": { "id": ctx.file.id(), "version": ctx.file.version(),
                      "file_sha256": ctx.file.hash },
        "game": { "id": ctx.def.id, "name": ctx.def.name, "area": ctx.game.area_name },
        "platform": ctx.platform,
        "mode": if ctx.render.is_some() { "headless+render" } else { "headless" },
        "build": build_info(),
        "content": { "sha256": ctx.game.content_hash, "files": files },
        "seed": s.seed,
        "settings": {
            "tick_ms": c2_core::scenario::TICK_MS,
            "warmup_ticks": s.warmup_ticks,
            "settle_ticks": s.settle_ticks,
            "timeout_ticks": s.timeout_ticks,
            "repeats": ctx.runs.len(),
            "time_of_day": "day",
            "inject": ctx.inject,
        },
        "hardware": hardware(),
        "renderer": ctx.render.as_ref().map(|r| json!({
            "adapter": r["adapter"], "backend": r["backend"], "resolution": r["resolution"],
        })),
    })
}
