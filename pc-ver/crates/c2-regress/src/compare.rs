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

/// What the visual comparison did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualCheck {
    /// `compared` checkpoints, all within the tolerance.
    Passed {
        compared: usize,
    },
    Failed,
    /// No pictures compared. `by_design`: they cannot be (a published
    /// baseline has no screenshots; another engine draws differently).
    /// Otherwise they could have been and were not: one of the runs was
    /// recorded without the render pass.
    NotRun {
        reason: String,
        by_design: bool,
    },
}

impl VisualCheck {
    pub fn failed(&self) -> bool {
        *self == Self::Failed
    }

    /// Not compared, though it could have been (exit code 5).
    pub fn missing(&self) -> bool {
        matches!(
            self,
            Self::NotRun {
                by_design: false,
                ..
            }
        )
    }
}

pub struct Verdict {
    pub functional_ok: bool,
    pub visual: VisualCheck,
    /// `Some` when the comparison was asked to require identical
    /// checkpoint state (`--same-state`): whether it was.
    pub same_state: Option<bool>,
    pub perf_warnings: usize,
    pub report: Value,
}

impl Verdict {
    /// Functional and visual passed, and the state matched if required.
    /// A visual check that did not run is no failure here: see
    /// [`VisualCheck::missing`].
    pub fn passed(&self) -> bool {
        self.functional_ok && !self.visual.failed() && self.same_state != Some(false)
    }
}

/// Where the checkpoint state of two runs differs: names, ticks, hashes,
/// or the number of checkpoints. Empty when identical.
pub fn state_differences(base: &Value, cand: &Value) -> Vec<String> {
    let (Some(b), Some(c)) = (
        base["checkpoints"].as_array(),
        cand["checkpoints"].as_array(),
    ) else {
        return vec!["checkpoint state missing in one of the runs".into()];
    };
    let mut out: Vec<String> = b
        .iter()
        .zip(c)
        .filter(|(b, c)| b != c)
        .map(|(b, c)| {
            format!(
                "{}: tick {} hash {} -> {}: tick {} hash {}",
                b["name"], b["tick"], b["state_hash"], c["name"], c["tick"], c["state_hash"]
            )
        })
        .collect();
    if b.len() != c.len() {
        out.push(format!("{} checkpoints -> {}", b.len(), c.len()));
    }
    out
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
    // Same id and version but another text: the scenario was edited
    // without a version bump, so the runs did not do the same thing.
    let hash = "/scenario/file_sha256";
    if bman.pointer(hash) != cman.pointer(hash) {
        problems.push(format!(
            "{hash}: the scenario file differs from the baseline's \
             (edited without a version bump?)"
        ));
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
    // Older recordings have no `weapons_to_fire`: they fired them all.
    let to_fire = f.get("weapons_to_fire").unwrap_or(&f["weapon_registry"]);
    if &f["weapons_fired_once"] != to_fire {
        problems.push(format!(
            "{} of {} weapons fired once",
            f["weapons_fired_once"], to_fire
        ));
    }
    let k = &f["kills"];
    if !k.is_null() && k["passed"] != k["expected"] {
        problems.push(format!("{} of {} kills passed", k["passed"], k["expected"]));
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

/// The `.png` names in `dir`, sorted; empty if there is no such folder.
fn pngs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".png"))
        .collect();
    names.sort();
    names
}

fn visual(base: &Path, cand: &Path, out: &Path, th: &Visual) -> Result<(VisualCheck, Vec<Value>)> {
    let bdir = base.join("screenshots");
    let names = pngs(&bdir);
    let not_run = |reason: String| {
        let row = json!({ "unavailable": reason });
        let check = VisualCheck::NotRun {
            reason,
            by_design: false,
        };
        Ok((check, vec![row]))
    };
    if names.is_empty() {
        return not_run(
            "the baseline has no screenshots (recorded with --no-render, \
             or without c2-pc)"
                .into(),
        );
    }
    if pngs(&cand.join("screenshots")).is_empty() {
        return not_run(format!(
            "the candidate has no screenshots (--no-render, or c2-pc missing); \
             the baseline has {}",
            names.len()
        ));
    }
    let mut rows = Vec::new();
    let mut ok = true;
    std::fs::create_dir_all(out)?;
    for name in &names {
        let cp = name.trim_end_matches(".png");
        let masks: Vec<Mask> = th
            .mask
            .iter()
            .filter(|m| m.checkpoint == "*" || m.checkpoint == cp)
            .map(|m| m.rect)
            .collect();
        let row = match (
            image_diff::load_png(&bdir.join(name)),
            image_diff::load_png(&cand.join("screenshots").join(name)),
        ) {
            (Ok(a), Ok(b)) => match image_diff::diff(&a, &b, th.tolerance, &masks) {
                Ok(d) => {
                    let pass = d.ratio() <= th.max_differing_ratio;
                    let file = out.join(name);
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
    let check = if ok {
        VisualCheck::Passed {
            compared: names.len(),
        }
    } else {
        VisualCheck::Failed
    };
    Ok((check, rows))
}

/// Compares `cand` with `base`; writes `comparison.json` and diff images
/// into `cand`. `same_state`: differing checkpoint state fails the
/// comparison (otherwise it is reported only).
pub fn compare(base: &Path, cand: &Path, th: &Thresholds, same_state: bool) -> Result<Verdict> {
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
    // A published baseline names no machine: no screenshots, and its
    // performance figures are shown for reference, never warned about.
    let published = bman.get("published").is_some();
    // Our renderer against the original's: different pictures by design,
    // compared by eye (the PDF), not pixel by pixel.
    let engine = |m: &Value| m["engine"].as_str().unwrap_or("rust").to_string();
    let by_design = |reason: String| {
        let row = json!({ "unavailable": reason });
        let check = VisualCheck::NotRun {
            reason,
            by_design: true,
        };
        (check, vec![row])
    };
    let (visual, shots) = if engine(&bman) != engine(&cman) {
        by_design(format!(
            "{} engine against {}: compare by eye in the PDF",
            engine(&cman),
            engine(&bman)
        ))
    } else if published {
        by_design("published baselines have no screenshots".into())
    } else {
        visual(base, cand, &cand.join("comparison"), &th.visual)?
    };

    let p = &th.performance;
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
    let state_diff = state_differences(&bm, &cm);
    // Another release of the game (or edited files): results may differ
    // for that reason alone.
    let content_matches = bman.pointer("/content/sha256") == cman.pointer("/content/sha256");
    let report = json!({
        "baseline": base.display().to_string(),
        "candidate": cand.display().to_string(),
        "functional": { "passed": functional_ok, "problems": problems },
        "visual": { "passed": !visual.failed(),
                    "ran": matches!(visual, VisualCheck::Passed { .. } | VisualCheck::Failed),
                    "not_run": match &visual { VisualCheck::NotRun { reason, .. } => json!(reason),
                                               _ => Value::Null },
                    "not_run_by_design": matches!(visual, VisualCheck::NotRun { by_design: true, .. }),
                    "tolerance": th.visual.tolerance,
                    "max_differing_ratio": th.visual.max_differing_ratio, "checkpoints": shots },
        "performance": perf,
        "performance_warnings": perf_warnings,
        "simulation_state_identical": state_diff.is_empty(),
        "simulation_state_required": same_state,
        "simulation_state_differences": &state_diff,
        "kill_parity": crate::kill_parity::compare(&bm, &cm),
        "content_matches_baseline": content_matches,
        "baseline_has_screenshots": base.join("screenshots").is_dir(),
    });
    write_json(&cand.join("comparison.json"), &report)?;
    Ok(Verdict {
        functional_ok,
        visual,
        same_state: same_state.then_some(state_diff.is_empty()),
        perf_warnings,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing() -> Value {
        json!({ "result": "passed", "functional": {
            "animals_end": 4, "animals_expected": 4, "weapons_fired_once": 2,
            "weapon_registry": 2, "timeouts": 0, "panics": 0, "setup_error": null } })
    }

    fn manifest(hash: &str) -> Value {
        json!({ "scenario": { "id": "s", "version": 2, "file_sha256": hash },
                "game": { "id": "c2", "area": "AREA2" } })
    }

    #[test]
    fn same_scenario_text_passes() {
        let (ok, problems) = functional(&passing(), &passing(), &manifest("a"), &manifest("a"));
        assert!(ok, "{problems:?}");
    }

    #[test]
    fn an_edited_scenario_without_a_version_bump_fails() {
        let (ok, problems) = functional(&passing(), &passing(), &manifest("a"), &manifest("b"));
        assert!(!ok);
        assert!(problems[0].contains("file_sha256"), "{problems:?}");
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("c2-regress-cmp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A run folder with the given manifest and, if `shot`, one screenshot.
    fn run_dir(root: &Path, name: &str, man: &Value, shot: bool) -> std::path::PathBuf {
        let d = root.join(name);
        std::fs::create_dir_all(d.join("screenshots")).unwrap();
        let mut m = passing();
        m["checkpoints"] = json!([{ "name": "warmup", "tick": 500, "state_hash": "aa" }]);
        write_json(&d.join("metrics.json"), &m).unwrap();
        write_json(&d.join("manifest.json"), man).unwrap();
        if shot {
            let png = image_diff::encode_rgb(2, 2, &[0; 12]).unwrap();
            std::fs::write(d.join("screenshots/warmup.png"), png).unwrap();
        }
        d
    }

    #[test]
    fn a_visual_check_that_never_ran_is_not_a_pass() {
        let root = tmp("visual");
        let th = Thresholds::default();
        let (with, without) = (
            run_dir(&root, "with", &manifest("a"), true),
            run_dir(&root, "without", &manifest("a"), false),
        );
        let v = compare(
            &with,
            &run_dir(&root, "with2", &manifest("a"), true),
            &th,
            false,
        )
        .unwrap();
        assert_eq!(v.visual, VisualCheck::Passed { compared: 1 });
        // Baseline recorded without the render pass.
        let v = compare(&without, &with, &th, false).unwrap();
        assert!(v.visual.missing() && v.passed(), "{:?}", v.visual);
        assert_eq!(v.report["visual"]["ran"], false);
        // Candidate recorded with --no-render.
        let v = compare(&with, &without, &th, false).unwrap();
        assert!(v.visual.missing(), "{:?}", v.visual);
        // A published baseline: no screenshots by design.
        let mut pubm = manifest("a");
        pubm["published"] = json!({});
        let published = run_dir(&root, "published", &pubm, false);
        let v = compare(&published, &with, &th, false).unwrap();
        assert!(matches!(
            v.visual,
            VisualCheck::NotRun {
                by_design: true,
                ..
            }
        ));
        assert!(!v.visual.missing());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn differing_state_fails_only_when_required() {
        let root = tmp("state");
        let th = Thresholds::default();
        let base = run_dir(&root, "base", &manifest("a"), false);
        let cand = run_dir(&root, "cand", &manifest("a"), false);
        let mut m = read_json(&cand.join("metrics.json")).unwrap();
        m["checkpoints"][0]["state_hash"] = json!("bb");
        write_json(&cand.join("metrics.json"), &m).unwrap();
        let v = compare(&base, &cand, &th, false).unwrap();
        assert!(v.passed() && v.same_state.is_none());
        assert_eq!(v.report["simulation_state_identical"], false);
        let v = compare(&base, &cand, &th, true).unwrap();
        assert!(!v.passed() && v.same_state == Some(false));
        assert!(v.report["simulation_state_differences"][0]
            .as_str()
            .unwrap()
            .contains("warmup"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
