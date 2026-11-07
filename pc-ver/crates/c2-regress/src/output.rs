//! Recording files: where a run goes, and the JSON shapes shared by the
//! headless runner and the PC renderer (`c2-pc regress`).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use c2_core::scenario::{Event, StageResult};
use serde_json::{json, Value};

/// The default recordings folder: `regression-sim-recordings/` at the top
/// of the git checkout (gitignored), else in the current folder.
pub fn default_root() -> PathBuf {
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
    top.unwrap_or_else(|| PathBuf::from("."))
        .join("regression-sim-recordings")
}

/// Days since 1970-01-01 to (year, month, day) (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `YYYYMMDDTHHMMSSZ`, UTC.
pub fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// The source revision this build runs from.
pub fn build_info() -> Value {
    let commit = git(&["rev-parse", "--short=12", "HEAD"]);
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty());
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": commit,
        "dirty": dirty,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
    })
}

pub fn short_commit() -> String {
    git(&["rev-parse", "--short=8", "HEAD"]).unwrap_or_else(|| "nogit".into())
}

/// The machine, for matching performance baselines. Recordings stay local
/// (gitignored); docs never quote these values.
pub fn hardware() -> Value {
    let cpu = std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
        s.lines()
            .find_map(|l| l.strip_prefix("model name"))
            .map(|v| v.trim_start_matches([' ', '\t', ':']).to_string())
    });
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpu": cpu,
        "logical_cpus": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "emulator": Value::Null,
    })
}

/// `<root>/<scenario>/<game>/<platform>/<stamp>_<commit>[_<label>]`.
pub fn run_dir(root: &Path, scenario: &str, game: &str, platform: &str, label: &str) -> PathBuf {
    let mut leaf = format!("{}_{}", utc_stamp(), short_commit());
    if !label.is_empty() {
        leaf.push('_');
        leaf.extend(label.chars().map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        }));
    }
    root.join(scenario).join(game).join(platform).join(leaf)
}

pub fn event_json(e: &Event, source: &str) -> Value {
    json!({
        "tick": e.tick,
        "time_ms": i64::from(e.tick) * i64::from(c2_core::scenario::TICK_MS),
        "source": source,
        "kind": e.kind.key(),
        "stage": e.stage,
        "name": e.name,
        "ok": e.ok,
        "detail": e.detail,
    })
}

pub fn stage_json(s: &StageResult, source: &str) -> Value {
    json!({
        "name": s.name,
        "source": source,
        "status": s.status.key(),
        "reason": s.status.reason(),
        "start_tick": s.start_tick,
        "end_tick": s.end_tick,
    })
}

pub fn write_json(path: &Path, v: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(v)?;
    std::fs::write(path, text + "\n").with_context(|| format!("writing {}", path.display()))
}

pub fn read_json(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// One JSON object per line.
pub fn write_jsonl(path: &Path, rows: &[Value]) -> Result<()> {
    let mut s = String::new();
    for r in rows {
        s.push_str(&serde_json::to_string(r)?);
        s.push('\n');
    }
    std::fs::write(path, s).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(20_764), (2026, 11, 7));
    }

    #[test]
    fn run_dirs_are_safe_paths() {
        let d = run_dir(Path::new("r"), "s", "c2", "pc", "a b/../c");
        let leaf = d.file_name().unwrap().to_string_lossy().to_string();
        assert!(leaf.ends_with("a_b____c"), "{leaf}");
        assert!(d.starts_with("r/s/c2/pc"));
    }
}
