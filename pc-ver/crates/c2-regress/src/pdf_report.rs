//! The screenshot comparison PDF: runs side by side per checkpoint, each
//! picture labelled with its run, then a page of metrics per run in the
//! order given (oldest first shows trends).

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde_json::Value;

use crate::image_diff::{encode_rgb, load_png};
use crate::output::{read_json, utc_stamp};
use crate::pdf::{Page, Pdf, PAGE_H, PAGE_W};

struct Run {
    dir: PathBuf,
    tag: String,
    /// Game, platform, build (cover page and captions).
    label: String,
    short: String,
    /// The run folder's name: time, commit, label.
    when: String,
    metrics: Value,
}

/// At most `n` characters (captions must stay inside their column).
fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.into();
    }
    let mut o: String = s.chars().take(n - 3).collect();
    o.push_str("...");
    o
}

/// What a checkpoint shows, for the page heading.
pub fn describe(cp: &str) -> String {
    if let Some(rest) = cp.strip_prefix("weapon_") {
        let gun = rest.trim_end_matches("_fire");
        return format!("Weapon {gun}: mid fire animation after its one shot");
    }
    match cp {
        "warmup" => "After warm-up: the four animals at the anchor".into(),
        "binoculars_open" => "Binoculars raised".into(),
        "map_open" => "Map open".into(),
        "call_icon_open" => "Change-call icon after switching call".into(),
        "weapon_picker_open" => "Change-gun picker open".into(),
        "call_picker_open" => "Change-call picker open".into(),
        "end" => "End of the scenario: the four animals still present".into(),
        other => other.into(),
    }
}

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => "-".into(),
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.fract() != 0.0)
            .map_or_else(|| n.to_string(), |f| format!("{f:.2}")),
        Value::Object(o) if o.contains_key("unavailable") => "n/a".into(),
        other => other.to_string(),
    }
}

/// Checkpoint names with a screenshot in any run, in the first run's
/// order (then any others).
fn checkpoints(runs: &[Run]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for r in runs {
        if let Some(cps) = r.metrics["checkpoints"].as_array() {
            for c in cps {
                let n = s(&c["name"]);
                if !names.contains(&n)
                    && r.dir.join("screenshots").join(format!("{n}.png")).exists()
                {
                    names.push(n);
                }
            }
        }
    }
    names
}

/// Embeds a screenshot at half size (box filter): enough for a tile,
/// a quarter of the bytes.
fn embed(pdf: &mut Pdf, path: &Path) -> Option<crate::pdf::Image> {
    let img = load_png(path).ok()?;
    let (w, h) = ((img.width / 2).max(1), (img.height / 2).max(1));
    let px = |x: u32, y: u32, c: usize| {
        let x = x.min(img.width - 1);
        let y = y.min(img.height - 1);
        u16::from(img.data[((y * img.width + x) * 4) as usize + c])
    };
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let sum = px(2 * x, 2 * y, c)
                    + px(2 * x + 1, 2 * y, c)
                    + px(2 * x, 2 * y + 1, c)
                    + px(2 * x + 1, 2 * y + 1, c);
                rgb.push((sum / 4) as u8);
            }
        }
    }
    pdf.image(&encode_rgb(w, h, &rgb).ok()?).ok()
}

pub fn write(dirs: &[PathBuf], out: &Path) -> Result<()> {
    if dirs.is_empty() {
        bail!("no runs given");
    }
    let mut runs = Vec::new();
    for (i, d) in dirs.iter().enumerate() {
        let man = read_json(&d.join("manifest.json"))?;
        let metrics = read_json(&d.join("metrics.json"))?;
        let leaf = d.file_name().map(|f| f.to_string_lossy().into_owned());
        let short = format!(
            "{} / {} / build {}{}",
            s(&man["game"]["id"]),
            s(&man["platform"]),
            s(&man["build"]["commit"]),
            if man["build"]["dirty"] == true {
                "+dirty"
            } else {
                ""
            },
        );
        let when = leaf.unwrap_or_default();
        runs.push(Run {
            dir: d.clone(),
            tag: format!("R{}", i + 1),
            label: format!("{short} / {when}"),
            short,
            when,
            metrics,
        });
    }
    let mut pdf = Pdf::new();

    let mut cover = Page::default();
    cover.text(40.0, PAGE_H - 60.0, 20.0, "Gameplay regression screenshots");
    cover.text(
        40.0,
        PAGE_H - 85.0,
        10.0,
        &format!("Generated {}", utc_stamp()),
    );
    let mut y = PAGE_H - 120.0;
    for r in &runs {
        let line = format!(
            "{}: {}  result: {}",
            r.tag,
            r.label,
            s(&r.metrics["result"])
        );
        cover.text(40.0, y, 10.0, &line);
        y -= 16.0;
    }
    cover.text(
        40.0,
        y - 10.0,
        9.0,
        "Each page is one checkpoint; pictures are labelled with their run (R1 is the first run given).",
    );
    pdf.page(cover);

    for cp in checkpoints(&runs) {
        // Pictures (and diffs against the baseline), six to a page.
        let mut tiles: Vec<(PathBuf, [String; 2])> = Vec::new();
        for r in &runs {
            let shot = r.dir.join("screenshots").join(format!("{cp}.png"));
            if shot.exists() {
                tiles.push((shot, [format!("{} {}", r.tag, r.short), r.when.clone()]));
            }
            let diff = r.dir.join("comparison").join(format!("{cp}.png"));
            if diff.exists() {
                let what = format!("{} difference from its baseline (red)", r.tag);
                tiles.push((diff, [what, r.when.clone()]));
            }
        }
        for (n, chunk) in tiles.chunks(6).enumerate() {
            let mut page = Page::default();
            let more = if n > 0 { " (continued)" } else { "" };
            page.text(
                40.0,
                PAGE_H - 40.0,
                16.0,
                &format!("Checkpoint: {cp}{more}"),
            );
            page.text(40.0, PAGE_H - 58.0, 10.0, &describe(&cp));
            page.line(40.0, PAGE_H - 66.0, PAGE_W - 40.0, PAGE_H - 66.0);
            let (cols, gap) = (3usize, 12.0);
            let tw = (PAGE_W - 80.0 - gap * (cols as f32 - 1.0)) / cols as f32;
            let th = tw * 0.75;
            for (i, (path, caption)) in chunk.iter().enumerate() {
                let (cx, cy) = (i % cols, i / cols);
                let x = 40.0 + cx as f32 * (tw + gap);
                let top = PAGE_H - 80.0 - cy as f32 * (th + 40.0);
                let used =
                    embed(&mut pdf, path).map_or(0.0, |img| page.image(img, x, top - th, tw, th));
                let mut y = top - th + (th - used) - 10.0;
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned());
                for line in caption.iter().chain(file.iter()) {
                    page.text(x, y, 6.5, &clip(line, 64));
                    y -= 8.5;
                }
            }
            pdf.page(page);
        }
    }

    let mut table = Page::default();
    table.text(40.0, PAGE_H - 40.0, 16.0, "Metrics by run");
    let cols = [
        ("Run", 40.0),
        ("Result", 70.0),
        ("vs baseline", 112.0),
        ("Tick med ms", 168.0),
        ("Tick p95 ms", 228.0),
        ("AI med ms", 288.0),
        ("Frame med ms", 345.0),
        ("Frame p95 ms", 410.0),
        ("Avg FPS", 475.0),
        ("1% low FPS", 525.0),
        ("Sim peak MB", 585.0),
        ("Render peak MB", 645.0),
        ("Stages P/F/U", 718.0),
    ];
    let mut y = PAGE_H - 70.0;
    for (name, x) in cols {
        table.text(x, y, 8.0, name);
    }
    table.line(40.0, y - 4.0, PAGE_W - 40.0, y - 4.0);
    let ms = |v: &Value| v.as_f64().map_or("n/a".into(), |f| format!("{f:.3}"));
    let mb = |v: &Value| {
        v.as_f64()
            .map_or("n/a".into(), |b| format!("{:.0}", b / 1_048_576.0))
    };
    for r in &runs {
        y -= 14.0;
        let m = &r.metrics;
        let vs = read_json(&r.dir.join("comparison.json"))
            .ok()
            .map_or("-".into(), |c| {
                let state_ok = c["simulation_state_required"] != true
                    || c["simulation_state_identical"] == true;
                let ok =
                    c["functional"]["passed"] == true && c["visual"]["passed"] == true && state_ok;
                // A visual check that could have run and did not.
                let no_visual =
                    c["visual"]["ran"] == false && c["visual"]["not_run_by_design"] == false;
                let warn = c["performance_warnings"].as_u64().unwrap_or(0);
                let pass = if no_visual {
                    "passed, no visual"
                } else {
                    "passed"
                };
                match (ok, warn) {
                    (false, _) => "FAILED".to_string(),
                    (true, 0) => pass.into(),
                    (true, n) => format!("{pass}, {n} warn"),
                }
            });
        let st = &m["functional"]["stages"];
        let row = [
            r.tag.clone(),
            s(&m["result"]),
            vs,
            ms(&m["simulation"]["tick_duration"]["median_ms"]),
            ms(&m["simulation"]["tick_duration"]["p95_ms"]),
            ms(&m["simulation"]["ai_update"]["median_ms"]),
            ms(&m["render"]["frames"]["frame_time"]["median_ms"]),
            ms(&m["render"]["frames"]["frame_time"]["p95_ms"]),
            s(&m["render"]["frames"]["average_fps"]),
            s(&m["render"]["frames"]["one_percent_low_fps"]),
            mb(&m["memory"]["peak_bytes"]),
            mb(&m["render"]["process_peak_bytes"]),
            format!(
                "{}/{}/{}",
                s(&st["passed"]),
                s(&st["failed"]),
                s(&st["unsupported"])
            ),
        ];
        for ((_, x), v) in cols.iter().zip(row) {
            table.text(*x, y, 8.0, &v);
        }
    }
    table.text(
        40.0,
        y - 24.0,
        7.0,
        "n/a: not measured on that run (for example, no render pass). Compare performance only between runs on the same machine and settings.",
    );
    pdf.page(table);
    std::fs::write(out, pdf.finish())?;
    Ok(())
}
