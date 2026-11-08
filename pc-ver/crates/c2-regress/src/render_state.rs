//! The render pass's checkpoint state checked against the headless run's.
//! Both tick the same scenario, so a checkpoint both reach must have the
//! same `state_hash`; a difference means the renderer's loop changed the
//! simulation.
//!
//! The render pass also runs stages headless can't (the weapon and call
//! pickers), so its checkpoints are matched by name: those only it has are
//! listed, not compared, and `end` is left out once it has any (it then
//! ends later, at another tick).

use serde_json::{json, Value};

use crate::headless::RunData;
use crate::metrics::unavailable;

/// `render_state` in `metrics.json`: `identical` is false when a headless
/// checkpoint is missing from the render pass or reached with another tick
/// or state.
pub fn agreement(first: Option<&RunData>, render: Option<&Value>) -> Value {
    let Some(render) = render else {
        return unavailable("render pass not run");
    };
    let Some(first) = first else {
        return unavailable("no headless run");
    };
    let headless: Vec<(&str, u64, String)> = first
        .checkpoints
        .iter()
        .map(|c| {
            (
                c.name.as_str(),
                u64::from(c.tick),
                format!("{:016x}", c.hash),
            )
        })
        .collect();
    let rendered: Vec<(&str, u64, &str)> = render["checkpoints"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap_or_default(),
                c["tick"].as_u64().unwrap_or(u64::MAX),
                c["state_hash"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    let render_only: Vec<&str> = rendered
        .iter()
        .map(|r| r.0)
        .filter(|n| !headless.iter().any(|h| h.0 == *n))
        .collect();
    let mut compared = 0;
    let mut differences = Vec::new();
    for (name, tick, hash) in &headless {
        if *name == "end" && !render_only.is_empty() {
            continue;
        }
        compared += 1;
        match rendered.iter().find(|r| r.0 == *name) {
            Some(&(_, t, h)) if t == *tick && h == hash => {}
            found => differences.push(json!({
                "name": name,
                "headless": { "tick": tick, "state_hash": hash },
                "render": found.map(|r| json!({ "tick": r.1, "state_hash": r.2 })),
            })),
        }
    }
    json!({
        "identical": differences.is_empty(),
        "compared": compared,
        "render_only": render_only,
        "differences": differences,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::Checkpoint;

    fn run(cps: &[(&str, u32, u64)]) -> RunData {
        let mut r = RunData::empty();
        r.checkpoints = cps
            .iter()
            .map(|&(name, tick, hash)| Checkpoint {
                name: name.into(),
                tick,
                hash,
            })
            .collect();
        r
    }

    fn render(cps: &[(&str, u32, u64)]) -> Value {
        let v: Vec<Value> = cps
            .iter()
            .map(|(n, t, h)| json!({ "name": n, "tick": t, "state_hash": format!("{h:016x}") }))
            .collect();
        json!({ "checkpoints": v })
    }

    #[test]
    fn render_only_stages_and_a_later_end_still_agree() {
        let h = run(&[("warmup", 500, 1), ("map_open", 1539, 2), ("end", 1749, 3)]);
        let r = render(&[
            ("warmup", 500, 1),
            ("map_open", 1539, 2),
            ("weapon_picker_open", 1780, 3),
            ("end", 1813, 4),
        ]);
        let a = agreement(Some(&h), Some(&r));
        assert_eq!(a["identical"], true, "{a}");
        assert_eq!(a["compared"], 2);
        assert_eq!(a["render_only"], json!(["weapon_picker_open"]));
    }

    #[test]
    fn another_state_or_a_missing_checkpoint_disagrees() {
        let h = run(&[("warmup", 500, 1), ("map_open", 1539, 2), ("end", 1600, 3)]);
        let other = agreement(
            Some(&h),
            Some(&render(&[
                ("warmup", 500, 9),
                ("map_open", 1539, 2),
                ("end", 1600, 3),
            ])),
        );
        assert_eq!(other["identical"], false);
        assert_eq!(other["differences"][0]["name"], "warmup");
        let missing = agreement(
            Some(&h),
            Some(&render(&[("warmup", 500, 1), ("end", 1600, 3)])),
        );
        assert_eq!(missing["identical"], false);
        assert_eq!(missing["differences"][0]["render"], Value::Null);
        // Same stages: `end` is compared too.
        let end = agreement(
            Some(&h),
            Some(&render(&[
                ("warmup", 500, 1),
                ("map_open", 1539, 2),
                ("end", 1601, 3),
            ])),
        );
        assert_eq!(end["identical"], false);
        assert_eq!(agreement(Some(&h), None)["identical"], Value::Null);
    }
}
