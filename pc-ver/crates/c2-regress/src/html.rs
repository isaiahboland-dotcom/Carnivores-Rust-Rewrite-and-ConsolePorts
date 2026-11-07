//! `report.html`: one run's stages, metrics, screenshots and comparison,
//! readable without any tool. Built from the run's own files.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::output::read_json;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "-".into(),
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.fract() != 0.0)
            .map_or_else(|| n.to_string(), |f| format!("{f:.3}")),
        Value::Object(o) if o.contains_key("unavailable") => {
            format!("unavailable ({})", text(&o["unavailable"]))
        }
        other => other.to_string(),
    }
}

/// Rows of `key: value` for every leaf under `v`, with dotted keys.
fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(o) if !o.contains_key("unavailable") => {
            for (k, x) in o {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, x, out);
            }
        }
        _ => out.push((prefix.into(), text(v))),
    }
}

fn table(out: &mut String, rows: &[(String, String)]) {
    out.push_str("<table>");
    for (k, v) in rows {
        let _ = write!(out, "<tr><th>{}</th><td>{}</td></tr>", esc(k), esc(v));
    }
    out.push_str("</table>");
}

pub fn write(dir: &Path) -> Result<()> {
    let manifest = read_json(&dir.join("manifest.json"))?;
    let metrics = read_json(&dir.join("metrics.json"))?;
    let comparison = read_json(&dir.join("comparison.json")).ok();
    let events = std::fs::read_to_string(dir.join("events.jsonl")).unwrap_or_default();
    let result = text(&metrics["result"]);
    let mut h = String::new();
    let _ = write!(
        h,
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{} {} {}</title><style>\
         body{{font-family:sans-serif;margin:2em;max-width:1200px}}\
         table{{border-collapse:collapse;margin:.5em 0}}td,th{{border:1px solid #ccc;padding:2px 8px;text-align:left;font-size:13px}}\
         .passed{{color:#060}}.failed{{color:#b00;font-weight:bold}}.unsupported{{color:#a60}}\
         figure{{display:inline-block;margin:6px}}img{{width:380px;border:1px solid #888}}</style></head><body>",
        esc(&text(&manifest["scenario"]["id"])),
        esc(&text(&manifest["game"]["id"])),
        esc(&text(&manifest["platform"])),
    );
    let _ = write!(
        h,
        "<h1>{} &middot; {} &middot; {}</h1><p class=\"{r}\">Result: {r}</p>",
        esc(&text(&manifest["scenario"]["id"])),
        esc(&text(&manifest["game"]["name"])),
        esc(&text(&manifest["platform"])),
        r = esc(&result),
    );
    h.push_str("<h2>Stages</h2><table><tr><th>Stage</th><th>Source</th><th>Status</th><th>Ticks</th><th>Reason</th></tr>");
    for line in events.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if e["kind"] != "stage_end" {
            continue;
        }
        let st = text(&e["name"]);
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}</td><td class=\"{st}\">{st}</td><td>{}</td><td>{}</td></tr>",
            esc(&text(&e["stage"])),
            esc(&text(&e["source"])),
            e["tick"],
            esc(&text(&e["detail"])),
        );
    }
    h.push_str("</table>");
    for (title, v) in [
        ("Functional", &metrics["functional"]),
        ("UI screens", &metrics["ui"]),
        ("Simulation", &metrics["simulation"]),
        ("Render", &metrics["render"]),
        ("Memory", &metrics["memory"]),
        ("Determinism", &metrics["determinism"]),
        ("Manifest", &manifest),
    ] {
        let mut rows = Vec::new();
        flatten("", v, &mut rows);
        let _ = write!(h, "<h2>{title}</h2>");
        table(&mut h, &rows);
    }
    if let Some(c) = &comparison {
        h.push_str("<h2>Comparison</h2>");
        let mut rows = Vec::new();
        flatten("", c, &mut rows);
        table(&mut h, &rows);
    }
    let mut shots: Vec<String> = std::fs::read_dir(dir.join("screenshots"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".png"))
                .collect()
        })
        .unwrap_or_default();
    shots.sort();
    if !shots.is_empty() {
        h.push_str("<h2>Screenshots</h2>");
    }
    for s in shots {
        let diff = dir.join("comparison").join(&s);
        let _ = write!(
            h,
            "<figure><img src=\"screenshots/{n}\"><figcaption>{n}</figcaption></figure>",
            n = esc(&s)
        );
        if diff.exists() {
            let _ = write!(
                h,
                "<figure><img src=\"comparison/{n}\"><figcaption>diff: {n}</figcaption></figure>",
                n = esc(&s)
            );
        }
    }
    h.push_str("</body></html>\n");
    std::fs::write(dir.join("report.html"), h)?;
    Ok(())
}
