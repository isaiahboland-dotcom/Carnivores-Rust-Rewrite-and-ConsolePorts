//! Comparing a candidate run against an approved baseline: functional
//! (fails), visual (fails past the tolerance), performance (warns past the
//! thresholds, and only between matching configurations).

use std::path::Path;

use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::image_diff::{self, Mask};
use crate::output::{read_json, write_json};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Performance {
    pub median_frame_time_pct: f64,
    pub p95_frame_time_pct: f64,
    pub peak_memory_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NamedMask {
    /// A checkpoint name, or `*` for all.
    pub checkpoint: String,
    #[serde(flatten)]
    pub rect: Mask,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Visual {
    pub tolerance: u8,
    pub max_differing_ratio: f64,
    pub mask: Vec<NamedMask>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    pub performance: Performance,
    pub visual: Visual,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            median_frame_time_pct: 10.0,
            p95_frame_time_pct: 15.0,
            peak_memory_pct: 15.0,
        }
    }
}

impl Default for Visual {
    fn default() -> Self {
        Self {
            tolerance: 24,
            max_differing_ratio: 0.01,
            mask: Vec::new(),
        }
    }
}

impl Thresholds {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
}

pub struct Verdict {
    pub functional_ok: bool,
    pub visual_ok: bool,
    pub perf_warnings: usize,
    pub report: Value,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// Why two runs' performance is not comparable, if it is not.
fn perf_mismatch(a: &Value, b: &Value) -> Option<String> {
    let keys = [
        "/platform",
        "/mode",
        "/hardware/os",
        "/hardware/arch",
        "/hardware/cpu",
        "/hardware/logical_cpus",
        "/renderer/adapter",
        "/renderer/resolution",
        "/build/profile",
        "/scenario/version",
        "/content/sha256",
        "/settings/tick_ms",
        "/settings/warmup_ticks",
        "/settings/settle_ticks",
        "/settings/time_of_day",
        // A run with a deliberate fault is no performance candidate.
        "/settings/inject",
    ];
    keys.iter()
        .find(|k| a.pointer(k).unwrap_or(&Value::Null) != b.pointer(k).unwrap_or(&Value::Null))
        .map(|k| format!("{k} differs"))
}

fn pct_check(name: &str, base: Option<f64>, cand: Option<f64>, limit: f64) -> Value {
    match (base, cand) {
        (Some(b), Some(c)) if b > 0.0 => {
            let pct = (c - b) / b * 100.0;
            json!({ "metric": name, "baseline": b, "candidate": c, "change_pct": pct,
                    "threshold_pct": limit, "warning": pct > limit })
        }
        _ => json!({ "metric": name, "unavailable": "missing in one of the runs" }),
    }
}

fn functional(base: &Value, cand: &Value, bman: &Value, cman: &Value) -> (bool, Vec<String>) {
    let mut problems = Vec::new();
    for k in ["/scenario/id", "/scenario/version", "/game/id"] {
        if bman.pointer(k) != cman.pointer(k) {
            problems.push(format!("{k}: runs are of different scenarios or games"));
        }
    }
    if cand["result"] != "passed" {
        problems.push("candidate run failed".into());
    }
    let f = &cand["functional"];
    if bman.pointer("/game/area") != cman.pointer("/game/area") {
        problems.push("map differs from the baseline".into());
    }
    if f["animals_end"] != f["animals_expected"] {
        problems.push(format!(
            "{} animals at the end, {} expected",
            f["animals_end"], f["animals_expected"]
        ));
    }
    if f["weapons_fired_once"] != f["weapon_registry"] {
        problems.push(format!(
            "{} of {} weapons fired once",
            f["weapons_fired_once"], f["weapon_registry"]
        ));
    }
    for k in ["timeouts", "panics"] {
        if f[k].as_u64().unwrap_or(1) > 0 {
            problems.push(format!("{k}: {}", f[k]));
        }
    }
    if !f["setup_error"].is_null() {
        problems.push(format!("set-up: {}", f["setup_error"]));
    }
    // A screen that worked in the baseline must still work.
    if let (Some(bu), Some(cu)) = (base["ui"].as_object(), cand["ui"].as_object()) {
        for (screen, st) in bu {
            if st == "passed" && cu.get(screen) != Some(st) {
                problems.push(format!(
                    "{screen}: passed in the baseline, now {:?}",
                    cu.get(screen)
                ));
            }
        }
    }
    (problems.is_empty(), problems)
}

fn visual(base: &Path, cand: &Path, out: &Path, th: &Visual) -> Result<(bool, Vec<Value>)> {
    let bdir = base.join("screenshots");
    let mut rows = Vec::new();
    let mut ok = true;
    let Ok(entries) = std::fs::read_dir(&bdir) else {
        return Ok((
            true,
            vec![json!({ "unavailable": "baseline has no screenshots" })],
        ));
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".png"))
        .collect();
    names.sort();
    std::fs::create_dir_all(out)?;
    for name in names {
        let cp = name.trim_end_matches(".png");
        let masks: Vec<Mask> = th
            .mask
            .iter()
            .filter(|m| m.checkpoint == "*" || m.checkpoint == cp)
            .map(|m| m.rect)
            .collect();
        let row = match (
            image_diff::load_png(&bdir.join(&name)),
            image_diff::load_png(&cand.join("screenshots").join(&name)),
        ) {
            (Ok(a), Ok(b)) => match image_diff::diff(&a, &b, th.tolerance, &masks) {
                Ok(d) => {
                    let pass = d.ratio() <= th.max_differing_ratio;
                    let file = out.join(&name);
                    std::fs::write(&file, image_diff::encode_rgb(a.width, a.height, &d.image)?)?;
                    ok &= pass;
                    json!({ "checkpoint": cp, "differing_ratio": d.ratio(),
                            "max_channel_delta": d.max_delta, "masks": masks.len(),
                            "passed": pass, "diff_image": format!("comparison/{name}") })
                }
                Err(e) => {
                    ok = false;
                    json!({ "checkpoint": cp, "passed": false, "error": e.to_string() })
                }
            },
            (_, Err(e)) | (Err(e), _) => {
                ok = false;
                json!({ "checkpoint": cp, "passed": false, "error": e.to_string() })
            }
        };
        rows.push(row);
    }
    Ok((ok, rows))
}

/// Compares `cand` with `base`; writes `comparison.json` and diff images
/// into `cand`.
pub fn compare(base: &Path, cand: &Path, th: &Thresholds) -> Result<Verdict> {
    if base == cand {
        bail!("baseline and candidate are the same run");
    }
    let (bm, cm) = (
        read_json(&base.join("metrics.json"))?,
        read_json(&cand.join("metrics.json"))?,
    );
    let (bman, cman) = (
        read_json(&base.join("manifest.json"))?,
        read_json(&cand.join("manifest.json"))?,
    );
    let (functional_ok, problems) = functional(&bm, &cm, &bman, &cman);
    // Our renderer against the original's: different pictures by design,
    // compared by eye (the PDF), not pixel by pixel.
    let engine = |m: &Value| m["engine"].as_str().unwrap_or("rust").to_string();
    let (visual_ok, shots) = if engine(&bman) != engine(&cman) {
        let why = format!(
            "{} engine against {}: compare by eye in the PDF",
            engine(&cman),
            engine(&bman)
        );
        (true, vec![json!({ "unavailable": why })])
    } else {
        visual(base, cand, &cand.join("comparison"), &th.visual)?
    };

    let p = &th.performance;
    // A published baseline names no machine: its figures are shown for
    // reference, never warned about.
    let published = bman.get("published").is_some();
    let mismatch = if published {
        None
    } else {
        perf_mismatch(&bman, &cman)
    };
    let mut perf = match mismatch {
        Some(why) => json!({ "skipped": why }),
        None => {
            let frame = |m: &Value, k: &str| num(&m["render"]["frames"]["frame_time"][k]);
            let tick = |m: &Value, k: &str| num(&m["simulation"]["tick_duration"][k]);
            let checks = vec![
                pct_check(
                    "frame_time_median_ms",
                    frame(&bm, "median_ms"),
                    frame(&cm, "median_ms"),
                    p.median_frame_time_pct,
                ),
                pct_check(
                    "frame_time_p95_ms",
                    frame(&bm, "p95_ms"),
                    frame(&cm, "p95_ms"),
                    p.p95_frame_time_pct,
                ),
                pct_check(
                    "sim_tick_median_ms",
                    tick(&bm, "median_ms"),
                    tick(&cm, "median_ms"),
                    p.median_frame_time_pct,
                ),
                pct_check(
                    "sim_tick_p95_ms",
                    tick(&bm, "p95_ms"),
                    tick(&cm, "p95_ms"),
                    p.p95_frame_time_pct,
                ),
                pct_check(
                    "peak_memory_bytes",
                    num(&bm["memory"]["peak_bytes"]),
                    num(&cm["memory"]["peak_bytes"]),
                    p.peak_memory_pct,
                ),
            ];
            json!({ "checks": checks })
        }
    };
    if published {
        if let Some(checks) = perf["checks"].as_array_mut() {
            for c in checks {
                c["warning"] = json!(false);
            }
        }
        perf["reference_only"] = json!(
            "published baseline from one unidentified reference machine: compare figures by eye"
        );
    }
    let perf_warnings = perf["checks"]
        .as_array()
        .map_or(0, |c| c.iter().filter(|c| c["warning"] == true).count());
    let same_state = bm["checkpoints"] == cm["checkpoints"] && !bm["checkpoints"].is_null();
    // Another release of the game (or edited files): results may differ
    // for that reason alone.
    let content_matches = bman.pointer("/content/sha256") == cman.pointer("/content/sha256");
    let report = json!({
        "baseline": base.display().to_string(),
        "candidate": cand.display().to_string(),
        "functional": { "passed": functional_ok, "problems": problems },
        "visual": { "passed": visual_ok, "tolerance": th.visual.tolerance,
                    "max_differing_ratio": th.visual.max_differing_ratio, "checkpoints": shots },
        "performance": perf,
        "performance_warnings": perf_warnings,
        "simulation_state_identical": same_state,
        "content_matches_baseline": content_matches,
        "baseline_has_screenshots": base.join("screenshots").is_dir(),
    });
    write_json(&cand.join("comparison.json"), &report)?;
    Ok(Verdict {
        functional_ok,
        visual_ok,
        perf_warnings,
        report,
    })
}
