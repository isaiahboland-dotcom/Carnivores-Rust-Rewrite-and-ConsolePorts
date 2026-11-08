//! Publishing a run as a public reference baseline: the scores only
//! (`manifest.json`, `metrics.json`, `events.jsonl`), checked against a
//! base game's hash list first. No screenshots (renders of game assets),
//! no logs, and nothing that identifies the machine.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::output::{read_json, write_json};
use crate::scenario_file::ScenarioFile;

/// The published baselines in this checkout:
/// `pc-ver/crates/c2-regress/baselines/<scenario>/<game>/`.
pub fn published_dir(scenario: &str, game: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("baselines")
        .join(scenario)
        .join(game)
}

/// `(path, sha256)` pairs of a base hash list (`recipes/bases/*.toml`).
fn hash_list(path: &Path) -> Result<Vec<(String, String)>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let v: toml::Value = toml::from_str(&text).with_context(|| path.display().to_string())?;
    let files = v
        .get("files")
        .and_then(toml::Value::as_table)
        .context("the hash list has no [files] table")?;
    Ok(files
        .iter()
        .filter_map(|(k, h)| {
            let h = h.as_str()?.strip_prefix("sha256:")?;
            Some((k.to_ascii_uppercase(), h.to_string()))
        })
        .collect())
}

/// Every game file the run read must be in the list with the same hash.
pub fn verify_vanilla(manifest: &Value, list: &Path) -> Result<usize> {
    let known = hash_list(list)?;
    let files = manifest["content"]["files"]
        .as_array()
        .context("manifest without content files")?;
    let mut problems = Vec::new();
    for f in files {
        let (p, h) = (
            f["path"].as_str().unwrap_or_default().to_ascii_uppercase(),
            f["sha256"].as_str().unwrap_or_default(),
        );
        match known.iter().find(|(k, _)| *k == p) {
            Some((_, k)) if k == h => {}
            Some(_) => problems.push(format!("{p}: differs from the list")),
            None => problems.push(format!("{p}: not in the list")),
        }
    }
    if !problems.is_empty() {
        bail!(
            "not the vanilla game of {}:\n  {}",
            list.display(),
            problems.join("\n  ")
        );
    }
    Ok(files.len())
}

/// The manifest without machine details: OS and CPU architecture stay
/// (they decide float results); CPU, GPU and driver names go.
pub fn redact_manifest(m: &Value, list: &str, checked: usize) -> Value {
    let mut m = m.clone();
    m["hardware"] = json!({
        "os": m["hardware"]["os"],
        "arch": m["hardware"]["arch"],
        "machine": "reference machine (not identified)",
    });
    if let Some(r) = m.get_mut("renderer").filter(|r| r.is_object()) {
        *r = json!({ "backend": r["backend"], "resolution": r["resolution"] });
    }
    m["published"] = json!({
        "vanilla_hash_list": list,
        "files_verified": checked,
        "note": "Scores only: no screenshots, logs or machine details. \
                 Performance figures are from one reference machine.",
    });
    m
}

pub fn redact_metrics(m: &Value) -> Value {
    let mut m = m.clone();
    if let Some(r) = m.get_mut("render").and_then(Value::as_object_mut) {
        r.remove("adapter");
    }
    m
}

/// Why a run cannot be a baseline, if it cannot. `scenario_hash` is the
/// SHA-256 of the scenario file in this checkout.
pub fn refuse(manifest: &Value, metrics: &Value, scenario_hash: &str) -> Option<String> {
    if metrics["result"] != "passed" {
        return Some("only a passed run can be a baseline".into());
    }
    if manifest["settings"]["inject"].as_str().is_some() {
        return Some("a run with a deliberate fault (--inject) cannot be a baseline".into());
    }
    if metrics.pointer("/determinism/identical") != Some(&json!(true)) {
        return Some(
            "a baseline needs repeats that agree: run with --repeat 2 or more \
             (determinism.identical must be true)"
                .into(),
        );
    }
    if manifest.pointer("/scenario/file_sha256") != Some(&json!(scenario_hash)) {
        return Some(
            "the run used another text of the scenario than this checkout's: \
             re-run it (and bump the scenario version if the file changed)"
                .into(),
        );
    }
    None
}

/// The scenario file a published baseline belongs to:
/// `pc-ver/crates/c2-regress/scenarios/<id>.toml`.
pub fn scenario_path(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenarios")
        .join(format!("{id}.toml"))
}

/// What changes between a published baseline and its replacement:
/// scenario, game content, build, results, determinism, checkpoint state.
pub fn baseline_changes(old: (&Value, &Value), new: (&Value, &Value)) -> Vec<String> {
    let ((om, ox), (nm, nx)) = (old, new);
    let mut out = Vec::new();
    let mut cmp = |what: &str, a: &Value, b: &Value| {
        if a != b {
            out.push(format!("{what}: {a} -> {b}"));
        }
    };
    for k in [
        "/scenario/version",
        "/scenario/file_sha256",
        "/game/area",
        "/content/sha256",
        "/build/version",
        "/build/commit",
        "/build/dirty",
        "/settings/repeats",
    ] {
        cmp(
            k,
            om.pointer(k).unwrap_or(&Value::Null),
            nm.pointer(k).unwrap_or(&Value::Null),
        );
    }
    for k in ["/result", "/determinism/identical", "/ui"] {
        cmp(
            k,
            ox.pointer(k).unwrap_or(&Value::Null),
            nx.pointer(k).unwrap_or(&Value::Null),
        );
    }
    let empty = serde_json::Map::new();
    let (of, nf) = (
        ox["functional"].as_object().unwrap_or(&empty),
        nx["functional"].as_object().unwrap_or(&empty),
    );
    let mut keys: Vec<&String> = of.keys().chain(nf.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        cmp(
            &format!("/functional/{k}"),
            of.get(k).unwrap_or(&Value::Null),
            nf.get(k).unwrap_or(&Value::Null),
        );
    }
    out.extend(
        crate::compare::state_differences(ox, nx)
            .into_iter()
            .map(|d| format!("checkpoint state: {d}")),
    );
    out
}

/// The baseline already in `to`, if any: `(manifest, metrics)`.
fn existing(to: &Path) -> Result<Option<(Value, Value)>> {
    let man = to.join("manifest.json");
    if !man.exists() {
        return Ok(None);
    }
    Ok(Some((
        read_json(&man)?,
        read_json(&to.join("metrics.json"))?,
    )))
}

/// `None` when `to` holds no baseline; else its changes, which are only
/// allowed with `replace`.
fn check_replace(to: &Path, new: (&Value, &Value), replace: bool) -> Result<Option<Vec<String>>> {
    let Some((om, ox)) = existing(to)? else {
        return Ok(None);
    };
    let mut changes = baseline_changes((&om, &ox), new);
    if changes.is_empty() {
        changes.push("none in scenario, content, build, results or checkpoint state".into());
    }
    if !replace {
        bail!(
            "{} already holds a baseline; pass --replace to overwrite it. Changes:\n  {}",
            to.display(),
            changes.join("\n  ")
        );
    }
    Ok(Some(changes))
}

/// Copies `run` to `to` after the checks. An existing baseline there is
/// replaced only with `replace`; the changes against it are returned.
pub fn publish(
    run: &Path,
    list: &Path,
    list_label: &str,
    to: &Path,
    replace: bool,
) -> Result<Option<Vec<String>>> {
    let manifest = read_json(&run.join("manifest.json"))?;
    let metrics = read_json(&run.join("metrics.json"))?;
    let id = manifest
        .pointer("/scenario/id")
        .and_then(Value::as_str)
        .context("manifest without scenario id")?;
    let scenario = ScenarioFile::load(&scenario_path(id))
        .context("a published baseline must be of a scenario in this repository")?;
    if let Some(why) = refuse(&manifest, &metrics, &scenario.hash) {
        bail!("{why}");
    }
    let checked = verify_vanilla(&manifest, list)?;
    let events = std::fs::read_to_string(run.join("events.jsonl"))
        .with_context(|| format!("reading {}/events.jsonl", run.display()))?;
    let (manifest, metrics) = (
        redact_manifest(&manifest, list_label, checked),
        redact_metrics(&metrics),
    );
    let changes = check_replace(to, (&manifest, &metrics), replace)?;
    std::fs::create_dir_all(to)?;
    write_json(&to.join("manifest.json"), &manifest)?;
    write_json(&to.join("metrics.json"), &metrics)?;
    std::fs::write(to.join("events.jsonl"), events)?;
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Value {
        json!({
            "content": { "files": [
                { "path": "HUNTDAT/_RES.TXT", "sha256": "aa" },
                { "path": "HUNTDAT/AREAS/AREA2.MAP", "sha256": "bb" },
            ]},
            "hardware": { "os": "linux", "arch": "x86_64", "cpu": "Some CPU 9000",
                          "logical_cpus": 32, "emulator": null },
            "renderer": { "adapter": "Some GPU", "backend": "Vulkan", "resolution": [960, 540] },
            "settings": { "inject": null },
        })
    }

    #[test]
    fn only_reproducible_runs_of_this_scenario_text_are_published() {
        let man = json!({ "scenario": { "file_sha256": "abc" }, "settings": { "inject": null } });
        let ok = json!({ "result": "passed", "determinism": { "identical": true } });
        assert_eq!(refuse(&man, &ok, "abc"), None);
        assert!(refuse(&man, &ok, "def").unwrap().contains("scenario"));
        let one = json!({ "result": "passed",
                          "determinism": { "identical": { "unavailable": "one run" } } });
        assert!(refuse(&man, &one, "abc").unwrap().contains("--repeat"));
        let flaky = json!({ "result": "passed", "determinism": { "identical": false } });
        assert!(refuse(&man, &flaky, "abc").unwrap().contains("--repeat"));
        let failed = json!({ "result": "failed", "determinism": { "identical": true } });
        assert!(refuse(&man, &failed, "abc").unwrap().contains("passed"));
    }

    #[test]
    fn an_existing_baseline_is_replaced_only_on_request_with_its_changes() {
        let dir = std::env::temp_dir().join(format!("c2-regress-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let man = json!({ "scenario": { "version": 2 }, "build": { "commit": "aaa" } });
        let met = json!({ "result": "passed", "functional": { "animals_end": 4 },
                          "checkpoints": [{ "name": "warmup", "tick": 500, "state_hash": "aa" }] });
        assert_eq!(check_replace(&dir, (&man, &met), false).unwrap(), None);
        write_json(&dir.join("manifest.json"), &man).unwrap();
        write_json(&dir.join("metrics.json"), &met).unwrap();

        let mut man2 = man.clone();
        man2["build"]["commit"] = json!("bbb");
        let mut met2 = met.clone();
        met2["functional"]["animals_end"] = json!(3);
        met2["checkpoints"][0]["state_hash"] = json!("bb");
        let e = check_replace(&dir, (&man2, &met2), false)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("--replace") && e.contains("/build/commit"),
            "{e}"
        );
        let c = check_replace(&dir, (&man2, &met2), true).unwrap().unwrap();
        assert_eq!(c.len(), 3, "{c:?}");
        assert!(c
            .iter()
            .any(|c| c.contains("/functional/animals_end: 4 -> 3")));
        assert!(c
            .iter()
            .any(|c| c.starts_with("checkpoint state") && c.contains("warmup")));
        let same = check_replace(&dir, (&man, &met), true).unwrap().unwrap();
        assert!(same[0].starts_with("none"), "{same:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn machine_details_are_removed() {
        let m = redact_manifest(&manifest(), "recipes/bases/x.toml", 2);
        let text = m.to_string();
        assert!(!text.contains("Some CPU") && !text.contains("Some GPU"));
        assert!(!text.contains("logical_cpus"));
        assert_eq!(m["hardware"]["arch"], "x86_64");
        assert_eq!(m["renderer"]["backend"], "Vulkan");
        let metrics = json!({ "render": { "adapter": "Some GPU", "frames": 1 } });
        assert!(!redact_metrics(&metrics).to_string().contains("Some GPU"));
    }

    #[test]
    fn only_listed_vanilla_files_pass() {
        let dir = std::env::temp_dir().join(format!("c2-regress-pub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("base.toml");
        std::fs::write(
            &list,
            "[files]\n\"HUNTDAT/_RES.TXT\" = \"sha256:aa\"\n\"HUNTDAT/AREAS/AREA2.MAP\" = \"sha256:bb\"\n",
        )
        .unwrap();
        assert_eq!(verify_vanilla(&manifest(), &list).unwrap(), 2);
        std::fs::write(&list, "[files]\n\"HUNTDAT/_RES.TXT\" = \"sha256:zz\"\n").unwrap();
        let e = verify_vanilla(&manifest(), &list).unwrap_err().to_string();
        assert!(
            e.contains("differs") && e.contains("not in the list"),
            "{e}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
