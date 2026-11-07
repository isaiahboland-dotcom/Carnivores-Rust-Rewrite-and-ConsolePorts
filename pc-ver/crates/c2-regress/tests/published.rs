//! The published reference baselines (`baselines/<scenario>/<game>/`):
//! complete, passed, of the current scenario version, verified against
//! the repository's vanilla hash lists, and free of machine details.
//! Needs no game files, so it runs in CI.

use std::path::Path;

use c2_regress::output::read_json;
use c2_regress::publish::verify_vanilla;
use c2_regress::scenario_file::ScenarioFile;

#[test]
fn published_baselines_are_complete_and_vanilla() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = crate_dir.join("../../..");
    let mut seen = 0;
    for scenario in std::fs::read_dir(crate_dir.join("baselines"))
        .unwrap()
        .flatten()
    {
        if !scenario.path().is_dir() {
            continue;
        }
        let file = crate_dir
            .join("scenarios")
            .join(format!("{}.toml", scenario.file_name().to_string_lossy()));
        let sf = ScenarioFile::load(&file).unwrap();
        for game in std::fs::read_dir(scenario.path()).unwrap().flatten() {
            let d = game.path();
            let id = game.file_name().to_string_lossy().into_owned();
            let def = sf.game(&id).unwrap();
            let man = read_json(&d.join("manifest.json")).unwrap();
            let m = read_json(&d.join("metrics.json")).unwrap();
            assert!(d.join("events.jsonl").exists(), "{id}");
            assert_eq!(man["scenario"]["version"], sf.version(), "{id}: re-publish");
            assert_eq!(m["result"], "passed", "{id}");
            let f = &m["functional"];
            assert_eq!(f["animals_end"], f["animals_expected"], "{id}");
            assert_eq!(f["weapons_fired_once"], f["weapon_registry"], "{id}");
            assert_eq!(f["weapon_registry"].as_u64(), def.weapons.map(|n| n as u64));
            assert_eq!(m["determinism"]["identical"], true, "{id}");
            let list = man["published"]["vanilla_hash_list"].as_str().unwrap();
            verify_vanilla(&man, &repo.join(list)).unwrap();
            let hw = man["hardware"].as_object().unwrap();
            let mut keys: Vec<&str> = hw.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["arch", "machine", "os"], "{id}");
            assert!(m["render"].get("adapter").is_none(), "{id}");
            for name in ["screenshots", "engine.log", "comparison.json"] {
                assert!(!d.join(name).exists(), "{id}: {name} must not be published");
            }
            seen += 1;
        }
    }
    assert!(seen >= 4, "Carnivores 2, Ice Age and their APK-built bases");
}
