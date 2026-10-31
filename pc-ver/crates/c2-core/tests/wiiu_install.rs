#![cfg(feature = "portable-load")]
//! The Wii U's installs (`c2-cook --target wiiu`), opened and hunted on the
//! host as the console does: with C2_WIIU_INSTALLS set to install folders
//! (separated by `:`), each one's menu lists, a hunt with every animal and
//! weapon on each area, a restart, a survival and the trophy room.
extern crate alloc;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/install.rs"]
mod install;
#[allow(dead_code)]
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/hunt_setup.rs"]
mod hunt_setup;

mod wiiu_common;
use c2_vfs::{Backend, Vfs, VfsError};
use std::path::PathBuf;

/// The install folder by its index (`FILES.TXT`) and upper-case names, as
/// the console reads it (`SdBackend` in lib.rs).
struct Indexed {
    root: PathBuf,
    files: Vec<String>,
}

impl Backend for Indexed {
    fn files(&self) -> Vec<String> {
        self.files.clone()
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        std::fs::read(self.root.join(path.to_ascii_uppercase()))
            .map_err(|_| VfsError::NotFound { path: path.into() })
    }
}

fn open(dir: &str) -> install::Install {
    let root = PathBuf::from(dir);
    let index = std::fs::read_to_string(root.join("FILES.TXT")).expect("FILES.TXT");
    let files = index.lines().map(String::from).collect();
    let mut vfs = Vfs::new();
    vfs.push(
        "install",
        Box::new(Indexed {
            root: root.clone(),
            files,
        }),
    );
    let info =
        install::Info::parse(&std::fs::read_to_string(root.join("INFO.TXT")).expect("INFO.TXT"));
    install::Install::open(vfs, info).expect("rules")
}

#[test]
fn installs_open_and_hunt() {
    let Ok(dirs) = std::env::var("C2_WIIU_INSTALLS") else {
        eprintln!("C2_WIIU_INSTALLS not set: skipped");
        return;
    };
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        let inst = open(dir);
        let names: Vec<&str> = inst.animals.iter().map(|a| a.name.as_str()).collect();
        println!(
            "{dir}: areas {:?}, animals {names:?}, weapons {:?}, trophy room {}",
            inst.areas
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            inst.weapons,
            inst.trophy_room
        );
        assert!(!inst.areas.is_empty() && !inst.animals.is_empty() && !inst.weapons.is_empty());
        let all = if inst.animals.len() >= 64 {
            u64::MAX
        } else {
            (1u64 << inst.animals.len()) - 1
        };
        for area in 0..inst.areas.len() as u32 {
            let choice = hunt_setup::HuntChoice {
                area,
                animals: all,
                weapons: (1 << inst.weapons.len()) - 1,
                ..Default::default()
            };
            let l = hunt_setup::load(Some(&inst), &choice, &hunt_setup::new_save(), 7)
                .unwrap_or_else(|e| {
                    panic!("{dir} {}: load error {e}", inst.areas[area as usize].file)
                });
            let models = l.cars.iter().filter(|c| c.is_some()).count() + l.weapon_cars.len();
            let placed = l.hunt.herd.chars.len();
            let targets = l
                .hunt
                .herd
                .chars
                .iter()
                .filter(|c| l.hunt.is_target(c))
                .count();
            println!(
                "  {}: {placed} placed ({targets} targets), {models} models, {} weapons, {} calls, fx {}",
                inst.areas[area as usize].file,
                l.weapon_cars.len(),
                l.calls.len(),
                l.fx.iter().filter(|f| !f.is_empty()).count()
            );
            assert!(models <= hunt_setup::MODEL_SLOTS);
            assert!(targets > 0, "no targets placed");
            assert_eq!(l.weapon_cars.len(), inst.weapons.len());
            assert!(!l.calls.is_empty());
            // Every placed animal has its model.
            for c in &l.hunt.herd.chars {
                assert!(
                    l.cars.get(c.ctype).is_some_and(Option::is_some),
                    "ctype {} has no model",
                    c.ctype
                );
            }
            if area == 0 {
                let again = hunt_setup::restart(
                    &l.area,
                    &l.hunt,
                    &choice,
                    l.tables.as_ref(),
                    l.targets,
                    &l.roamers,
                    &hunt_setup::new_save(),
                    8,
                )
                .expect("restart");
                assert!(again.herd.chars.iter().any(|c| again.is_target(c)));
                let survival = hunt_setup::HuntChoice {
                    mode: hunt_setup::MODE_SURVIVAL,
                    ..choice
                };
                let s = hunt_setup::load(Some(&inst), &survival, &hunt_setup::new_save(), 9)
                    .expect("survival");
                assert!(s.hunt.survival.is_some());
            }
        }
        if inst.trophy_room {
            let choice = hunt_setup::HuntChoice {
                mode: hunt_setup::MODE_TROPHY,
                ..Default::default()
            };
            let l = hunt_setup::load(Some(&inst), &choice, &hunt_setup::new_save(), 1)
                .expect("trophy room");
            assert!(l.hunt.opts.trophy);
        }
    }
}
