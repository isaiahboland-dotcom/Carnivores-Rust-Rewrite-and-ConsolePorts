//! Kill parity: how each kill went in two runs (`metrics.json`
//! `functional.kills_detail`), side by side. Meant for the original
//! engine's run as the baseline and ours as the candidate. Differences are
//! listed in `comparison.json` `kill_parity`; they do not fail the
//! comparison, since a pass/fail kill check already does that.

use c2_core::scenario::KillReport;
use serde_json::{json, Value};

/// A body within this many units of its killer's origin lies on it
/// (carried, or synchronized to the killer's clip).
const ON_KILLER: f64 = 1.0;
/// Times to kill differing by more than this share, and by more than
/// `MS_SLACK`, are listed.
const TIME_SHARE: f64 = 0.25;
const MS_SLACK: f64 = 500.0;

pub fn report_json(r: &KillReport) -> Value {
    json!({
        "stage": r.stage,
        "ms_to_kill": r.ms_to_kill,
        "body_clips": r.body_clips,
        "killer_clips": r.killer_clips,
        "body_on_killer": f64::from(r.start_gap) <= ON_KILLER,
        "start_gap": r.start_gap.round(),
        "max_stray": r.max_stray.round(),
    })
}

fn note(stage: &str, what: &str, base: &Value, cand: &Value) -> Value {
    json!({ "stage": stage, "what": what, "baseline": base, "candidate": cand })
}

/// The differences between the two runs' kills, by stage.
pub fn compare(base: &Value, cand: &Value) -> Value {
    let list = |m: &Value| -> Vec<Value> {
        m["functional"]["kills_detail"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let (b, c) = (list(base), list(cand));
    if b.is_empty() && c.is_empty() {
        return Value::Null;
    }
    let mut notes = Vec::new();
    for bk in &b {
        let stage = bk["stage"].as_str().unwrap_or("");
        let Some(ck) = c.iter().find(|k| k["stage"] == bk["stage"]) else {
            notes.push(note(
                stage,
                "kill missing in the candidate",
                &json!(true),
                &json!(false),
            ));
            continue;
        };
        for key in ["body_clips", "killer_clips", "body_on_killer"] {
            if bk[key] != ck[key] {
                notes.push(note(stage, key, &bk[key], &ck[key]));
            }
        }
        let (tb, tc) = (bk["ms_to_kill"].as_f64(), ck["ms_to_kill"].as_f64());
        if let (Some(tb), Some(tc)) = (tb, tc) {
            if (tb - tc).abs() > MS_SLACK && (tb - tc).abs() > tb * TIME_SHARE {
                notes.push(note(
                    stage,
                    "ms_to_kill",
                    &bk["ms_to_kill"],
                    &ck["ms_to_kill"],
                ));
            }
        }
    }
    for ck in c
        .iter()
        .filter(|k| !b.iter().any(|x| x["stage"] == k["stage"]))
    {
        let stage = ck["stage"].as_str().unwrap_or("");
        notes.push(note(
            stage,
            "kill missing in the baseline",
            &json!(false),
            &json!(true),
        ));
    }
    json!({ "same": notes.is_empty(), "differences": notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kill(stage: &str, body: usize, gap: f32, ms: u32) -> Value {
        report_json(&KillReport {
            stage: stage.into(),
            ms_to_kill: ms,
            body_clips: vec![body],
            killer_clips: vec![9],
            start_gap: gap,
            max_stray: 0.0,
        })
    }

    fn run(kills: Vec<Value>) -> Value {
        json!({ "functional": { "kills_detail": kills } })
    }

    #[test]
    fn differences_are_listed() {
        let orig = run(vec![
            kill("kill_01_rhino", 6, 0.0, 840),
            kill("kill_02_rex", 3, 0.0, 7940),
        ]);
        let ours = run(vec![
            kill("kill_01_rhino", 0, 150.0, 900),
            kill("kill_02_rex", 3, 0.0, 8020),
        ]);
        let p = compare(&orig, &ours);
        assert_eq!(p["same"], false);
        let what: Vec<&str> = p["differences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["what"].as_str().unwrap())
            .collect();
        assert_eq!(what, ["body_clips", "body_on_killer"]);
        assert_eq!(compare(&orig, &orig)["same"], true);
        let slow = run(vec![
            kill("kill_01_rhino", 6, 0.0, 4000),
            kill("kill_02_rex", 3, 0.0, 7940),
        ]);
        assert_eq!(
            compare(&orig, &slow)["differences"][0]["what"],
            "ms_to_kill"
        );
        assert!(compare(&json!({}), &json!({})).is_null());
    }
}
