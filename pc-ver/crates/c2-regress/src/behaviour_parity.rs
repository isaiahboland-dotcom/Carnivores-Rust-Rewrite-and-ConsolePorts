//! Behaviour parity: each behaviour stage of two runs (`metrics.json`
//! `functional.behaviours_detail`), side by side. Meant for the original
//! engine's run as the baseline and ours as the candidate. Differences are
//! listed in `comparison.json` `behaviour_parity`; like kill parity, they
//! do not fail the comparison (each stage's own checks do that).

use c2_core::scenario::BehaviourReport;
use serde_json::{json, Value};

/// Speeds differing by more than this share are listed. An animal's
/// speed is compared per unit of its size (each engine watches its own
/// animal, of its own random size).
const SPEED_SHARE: f64 = 0.05;
/// Bodies whose drift differs by more than this many units are listed.
const DRIFT_SLACK: f64 = 64.0;

pub fn report_json(r: &BehaviourReport) -> Value {
    json!({
        "stage": r.stage,
        "kind": r.kind,
        "speed": (f64::from(r.speed) * 10.0).round() / 10.0,
        "scale": (f64::from(r.scale) * 1000.0).round() / 1000.0,
        "speed_per_scale": (f64::from(r.speed / r.scale.max(0.01)) * 10.0).round() / 10.0,
        "clips": r.clips,
        "held_ms": r.held_ms,
        "drift": r.drift.round(),
    })
}

fn note(stage: &str, what: &str, base: &Value, cand: &Value) -> Value {
    json!({ "stage": stage, "what": what, "baseline": base, "candidate": cand })
}

fn differ(b: &Value, c: &Value, key: &str, f: impl Fn(f64, f64) -> bool) -> bool {
    match (b[key].as_f64(), c[key].as_f64()) {
        (Some(x), Some(y)) => f(x, y),
        _ => b[key] != c[key],
    }
}

/// The differences between the two runs' behaviour stages, by stage.
pub fn compare(base: &Value, cand: &Value) -> Value {
    let list = |m: &Value| -> Vec<Value> {
        m["functional"]["behaviours_detail"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let (b, c) = (list(base), list(cand));
    if b.is_empty() && c.is_empty() {
        return Value::Null;
    }
    let mut notes = Vec::new();
    for bs in &b {
        let stage = bs["stage"].as_str().unwrap_or("");
        let Some(cs) = c.iter().find(|x| x["stage"] == bs["stage"]) else {
            notes.push(note(
                stage,
                "stage missing in the candidate",
                &json!(true),
                &json!(false),
            ));
            continue;
        };
        // A death's speed is its slide, already covered by its drift.
        let speed = if bs["kind"] == "animal_run" {
            "speed_per_scale"
        } else {
            "speed"
        };
        if bs["kind"] != "death"
            && differ(bs, cs, speed, |x, y| (x - y).abs() > x.abs() * SPEED_SHARE)
        {
            notes.push(note(stage, speed, &bs[speed], &cs[speed]));
        }
        // The hunter has no clips. A runner ends on its running clip, a
        // dying animal on its death clip (the original's first sample is
        // taken before the frame that starts it).
        let clip = |v: &Value| match v["kind"].as_str() {
            Some("animal_run" | "death") => v["clips"].as_array().and_then(|c| c.last()).cloned(),
            _ => None,
        };
        if clip(bs) != clip(cs) {
            notes.push(note(stage, "clip", &json!(clip(bs)), &json!(clip(cs))));
        }
        if bs["kind"] == "death" && differ(bs, cs, "drift", |x, y| (x - y).abs() > DRIFT_SLACK) {
            notes.push(note(stage, "drift", &bs["drift"], &cs["drift"]));
        }
    }
    for cs in c
        .iter()
        .filter(|x| !b.iter().any(|y| y["stage"] == x["stage"]))
    {
        let stage = cs["stage"].as_str().unwrap_or("");
        notes.push(note(
            stage,
            "stage missing in the baseline",
            &json!(false),
            &json!(true),
        ));
    }
    json!({ "same": notes.is_empty(), "differences": notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(name: &str, kind: &'static str, speed: f32, clips: Vec<usize>, drift: f32) -> Value {
        report_json(&BehaviourReport {
            stage: name.into(),
            kind,
            speed,
            scale: 1.0,
            clips,
            held_ms: 1000,
            drift,
        })
    }

    fn run(stages: Vec<Value>) -> Value {
        json!({ "functional": { "behaviours_detail": stages } })
    }

    #[test]
    fn differences_are_listed() {
        let orig = run(vec![
            stage("hunter_walk", "walk", 300.0, vec![0], 900.0),
            stage("run_01_rex", "animal_run", 1200.0, vec![0, 1, 2], 3000.0),
            stage("die_01_rex", "death", 0.0, vec![1, 5], 10.0),
        ]);
        let ours = run(vec![
            stage("hunter_walk", "walk", 290.0, vec![0], 870.0),
            stage("run_01_rex", "animal_run", 1210.0, vec![0, 3, 2], 3010.0),
            stage("die_01_rex", "death", 0.0, vec![6], 200.0),
        ]);
        let p = compare(&orig, &ours);
        let what: Vec<(&str, &str)> = p["differences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| (n["stage"].as_str().unwrap(), n["what"].as_str().unwrap()))
            .collect();
        assert_eq!(
            what,
            [("die_01_rex", "clip"), ("die_01_rex", "drift")],
            "{p:#}"
        );
        assert_eq!(compare(&orig, &orig)["same"], true);
        let slow = run(vec![stage("hunter_walk", "walk", 250.0, vec![0], 750.0)]);
        let p = compare(&orig, &slow);
        assert_eq!(p["differences"][0]["what"], "speed");
        assert_eq!(
            p["differences"][1]["what"],
            "stage missing in the candidate"
        );
        assert!(compare(&json!({}), &json!({})).is_null());
    }
}
