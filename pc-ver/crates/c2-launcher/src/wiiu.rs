//! The Wii U install (`wiiu-ver/`): for now the base game only. The Wii U
//! app reads the original files as they are (no cooked pack yet) from one
//! fixed SD folder, `wiiu/apps/carnivores-probe/`, each file by its bare
//! name (`wiiu-ver/perf-probe/main.c`, `stage_assets.py`). Add-ons, the
//! Modder's Edition, Ice Age and standalones show in the tab as "not on the
//! Wii U yet" until the port reads them.

use std::path::{Path, PathBuf};

use crate::config::find_ci;
use crate::jobs::Step;

/// The SD folder the Wii U app reads (fixed in its `main.c`).
pub const SD_FOLDER: &str = "carnivores-probe";

/// The Wii U app as the Wii U build makes it (`wiiu-ver/perf-probe`).
pub const APP: &str = "c2-wiiu-perf-probe.rpx";
pub const WUHB_TOOL: &str = "wuhbtool";

/// The files the Wii U app loads, under `HUNTDAT/` (the list in
/// `wiiu-ver/perf-probe/stage_assets.py`; keep the two the same).
pub fn assets() -> Vec<String> {
    let mut v: Vec<String> = [
        "HUNTER1.CAR",
        "_RES.TXT",
        "WEAPONS/PISTOL.CAR",
        "WEAPONS/SHOTGUN.CAR",
        "WEAPONS/DBSGUN.CAR",
        "WEAPONS/X_BOW.CAR",
        "WEAPONS/RIFLE.CAR",
        "WEAPONS/SNIPER.CAR",
        "MENU/MENUM.TGA",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for n in [
        "MOSH", "GALL", "DIMOR2", "PTERA", "DIMET", "BRACHI1", "PAR2", "ANKYLO1", "STEGO", "ALLO",
        "CHASMOSA", "VELO2", "SPINO", "CERATO1", "TIREX",
    ] {
        v.push(format!("{n}.CAR"));
    }
    for i in 1..=5 {
        v.push(format!("AREAS/AREA{i}.MAP"));
        v.push(format!("AREAS/AREA{i}.RSC"));
    }
    // The trophy room and the trophy plaques.
    for f in [
        "AREAS/TROPHY.MAP",
        "AREAS/TROPHY.RSC",
        "MENU/TROPHY_G.TGA",
        "MENU/TROPHY.TGA",
        "MENU/MAPFRAME.TGA",
    ] {
        v.push(f.to_string());
    }
    for i in 1..=5 {
        v.push(format!("MENU/PICS/AREA{i}.TGA"));
    }
    for p in ["DINO", "CALL"] {
        for i in 1..=9 {
            v.push(format!("MENU/PICS/{p}{i}.TGA"));
        }
    }
    for i in 1..=6 {
        v.push(format!("MENU/PICS/WEAPON{i}.TGA"));
    }
    for i in 1..=4 {
        v.push(format!("MENU/PICS/EQUIP{i}.TGA"));
    }
    // The menu's descriptions (src/menu.rs `FILES`).
    for i in 1..=5 {
        v.push(format!("MENU/TXT/AREA{i}.TXT"));
    }
    for ext in ["TXM", "TXU"] {
        for i in 1..=9 {
            v.push(format!("MENU/TXT/DINO{i}.{ext}"));
        }
    }
    for i in 1..=6 {
        v.push(format!("MENU/TXT/WEAPON{i}.TXT"));
    }
    for n in [
        "CAMOFLAG", "RADAR", "SCENT", "DOUBLE", "TRANQ", "OBSERVE", "DAY1", "DAY2", "DAY3",
    ] {
        v.push(format!("MENU/TXT/{n}.NFO"));
    }
    for i in 1..=9 {
        for c in ['A', 'B', 'C'] {
            v.push(format!("SOUNDFX/CALLS/CALL{i}_{c}.WAV"));
        }
    }
    for p in ["HWALK", "FOOTW"] {
        for i in 1..=3 {
            v.push(format!("SOUNDFX/STEPS/{p}{i}.WAV"));
        }
    }
    v.push("SOUNDFX/A_UNDERW.WAV".into());
    for i in 1..=4 {
        v.push(format!("SOUNDFX/HUM_DIE{i}.WAV"));
    }
    v
}

/// `rel` (with `/`) under `dir`, each part in any case.
fn find_path_ci(dir: &Path, rel: &str) -> Option<PathBuf> {
    rel.split('/')
        .try_fold(dir.to_path_buf(), |d, part| find_ci(&d, part))
}

/// The copies for an install on the SD card `sd` from the game folder
/// `game`, the app last; or the files the game folder lacks.
pub fn steps(
    game: &Path,
    app: &Path,
    sd: &Path,
    format: crate::config::WiiUFormat,
    wuhbtool: Option<&Path>,
) -> Result<Vec<Step>, Vec<String>> {
    let Some(huntdat) = find_ci(game, "HUNTDAT") else {
        return Err(vec!["HUNTDAT".into()]);
    };
    let dir = sd.join("wiiu").join("apps").join(SD_FOLDER);
    let mut steps = Vec::new();
    let mut missing = Vec::new();
    for rel in assets() {
        match find_path_ci(&huntdat, &rel) {
            Some(from) => {
                let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
                steps.push(Step::Copy {
                    from,
                    to: dir.join(name),
                });
            }
            None => missing.push(rel),
        }
    }
    if !missing.is_empty() {
        return Err(missing);
    }
    match format {
        crate::config::WiiUFormat::Rpx => steps.push(Step::Copy {
            from: app.to_path_buf(),
            to: dir.join(format!("{SD_FOLDER}.rpx")),
        }),
        crate::config::WiiUFormat::Wuhb => {
            let Some(tool) = wuhbtool else {
                return Err(vec![format!("{WUHB_TOOL} (from devkitPro wut-tools)")]);
            };
            steps.push(Step::Run {
                program: tool.to_path_buf(),
                args: vec![
                    app.display().to_string(),
                    dir.join(format!("{SD_FOLDER}.wuhb")).display().to_string(),
                    "--name=Carnivores 2".into(),
                    "--short-name=C2".into(),
                    "--author=Carnivores Rust Rewrite".into(),
                ],
            });
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_are_unique_once_flattened() {
        let a = assets();
        assert_eq!(a.len(), 148);
        let mut names: Vec<&str> = a.iter().filter_map(|r| r.rsplit('/').next()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), a.len());
    }

    #[test]
    fn steps_find_files_in_any_case_and_report_missing() {
        let root = std::env::temp_dir().join(format!("c2-wiiu-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("game");
        let huntdat = game.join("huntdat");
        for rel in assets() {
            let p = huntdat.join(rel.to_lowercase());
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, b"x");
        }
        let app = root.join(APP);
        let _ = std::fs::write(&app, b"rpx");
        let sd = root.join("sd");
        let n = steps(&game, &app, &sd, crate::config::WiiUFormat::Rpx, None)
            .map(|s| s.len())
            .unwrap_or(0);
        assert_eq!(n, assets().len() + 1);
        let _ = std::fs::remove_file(huntdat.join("tirex.car"));
        let missing = steps(&game, &app, &sd, crate::config::WiiUFormat::Rpx, None)
            .err()
            .unwrap_or_default();
        assert_eq!(missing, vec!["TIREX.CAR".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
