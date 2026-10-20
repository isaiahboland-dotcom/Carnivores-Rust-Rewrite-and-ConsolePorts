//! `c2-cook --check PACK`: loads a cooked pack the way `c2-3ds` does (its
//! own `rules.rs`) and plays a short hunt for each animal on the PC, to find
//! pack problems without a console: missing models, textures or sounds,
//! animals that never spawn.

use std::path::Path;

use anyhow::{Context, Result};
use c2_core::area::TimeOfDay;
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{Hunt, HuntInput, HuntOptions, Weapons};
use c2_core::player::Player;
use c2_core::rng::MsRand;
use c2_vfs::PackBackend;
use c2_vfs::Vfs;

#[path = "../../c2-3ds/src/rules.rs"]
#[allow(dead_code)]
mod rules;

use rules::{Role, Rules};

pub fn check(pack: &Path) -> Result<()> {
    let mut vfs = Vfs::new();
    vfs.push(
        "pack",
        Box::new(PackBackend::open(pack).with_context(|| format!("opening {}", pack.display()))?),
    );
    let info = String::from_utf8_lossy(&vfs.read("C2RS/INFO.TXT")?).into_owned();
    let areas: Vec<String> = info
        .lines()
        .find_map(|l| l.strip_prefix("areas = "))
        .unwrap_or_default()
        .split(',')
        .map(str::to_string)
        .filter(|a| a.starts_with("AREA"))
        .collect();
    let rules = Rules::load(&vfs)?;
    let first = areas.first().context("no areas")?;
    let neutral = rules.hunt(first, TimeOfDay::Day, u128::MAX);
    println!(
        "{}: {} areas, {} menu animals, {} weapons",
        pack.display(),
        areas.len(),
        rules.menu.huntables.len(),
        rules.menu.weapons.len()
    );
    // Each menu animal, then each roamer, alone on the first area.
    let mut picks: Vec<(String, u128, Vec<usize>)> = rules
        .menu
        .huntables
        .iter()
        .enumerate()
        .map(|(k, d)| (d.name.clone(), 1u128 << (k + 10), Vec::new()))
        .collect();
    for (i, r) in neutral.roles.iter().enumerate() {
        if *r == Role::Roamer {
            let name = neutral.res.characters[i].name.clone().unwrap_or_default();
            picks.push((format!("{name} (roams)"), 0, vec![i]));
        }
    }
    let mut problems = 0;
    for area_name in &areas {
        let area = c2_core::load::area_with(&vfs, area_name, TimeOfDay::Day, true)?;
        println!("{area_name}:");
        for (name, targets, roamers) in &picks {
            let hr = rules.hunt(area_name, TimeOfDay::Day, *targets);
            let keep = |i: usize| match hr.roles.get(i) {
                Some(Role::Hunter | Role::Ambient) => true,
                Some(Role::Slot(k)) => *k < 118 && targets & (1 << (k + 10)) != 0,
                Some(Role::Roamer) => roamers.contains(&i),
                None => false,
            };
            let (mut species, cars) = c2_core::load::species_with(&vfs, &hr.res, &keep);
            let mut missing = Vec::new();
            for (i, car) in cars.iter().enumerate() {
                if !keep(i) {
                    continue;
                }
                let key = &hr.keys[i];
                match car {
                    None => missing.push(format!("model of #{i} ({key})")),
                    Some(c) => {
                        if vfs.resolve(&format!("C2RS/M/{key}.TX")).is_none() {
                            missing.push(format!("texture {key}"));
                        }
                        // Empty sounds aren't packed; none at all is a problem.
                        let found = (0..c.sounds.len())
                            .filter(|k| vfs.resolve(&format!("C2RS/SND/M/{key}/{k}.PCM")).is_some())
                            .count();
                        if found == 0 && !c.sounds.is_empty() {
                            missing.push(format!("sounds of {key}"));
                        }
                    }
                }
            }
            if let Some(t) = &hr.tables {
                c2_core::mee::tune(&mut species, t);
            }
            let wanted: Vec<usize> = (0..hr.roles.len())
                .filter(|&i| keep(i) && matches!(hr.roles[i], Role::Slot(_) | Role::Roamer))
                .collect();
            let mut rng = MsRand::new(7);
            let cell = area.landings.first().copied().unwrap_or((512, 512));
            let player = Player::new(&area, cell);
            let mut herd = Herd {
                species,
                ..Default::default()
            };
            match &hr.tables {
                Some(t) => herd.place_mee(&area, (player.x, player.z), &mut rng, t, 128),
                None => herd.place(
                    &area,
                    (player.x, player.z),
                    &mut rng,
                    PlaceOptions {
                        density: 128,
                        opt_agres: 128,
                        targets: *targets,
                        max_ambient: Some(4),
                    },
                ),
            }
            let opts = HuntOptions {
                targets: *targets,
                view_r: 34,
                render_r: 34,
                ..Default::default()
            };
            let room = c2_core::profile::new_player("check", 0, 0);
            let mut hunt = Hunt::new(
                &area,
                player,
                herd,
                Weapons::new(vec![], vec![], 0, false),
                rng,
                opts,
                room,
            );
            let car_refs: Vec<_> = cars.iter().map(Option::as_ref).collect();
            let mut out = Vec::new();
            for _ in 0..3000 {
                hunt.tick(&area, &car_refs, &HuntInput::default(), 20, &mut out);
                out.clear();
            }
            let placed = hunt
                .herd
                .chars
                .iter()
                .filter(|c| wanted.contains(&c.ctype))
                .count();
            let total = hunt.herd.chars.len();
            if !missing.is_empty() {
                problems += 1;
            }
            println!(
                "  {name:<32} {placed:>2} placed ({total} animals in all){}",
                if missing.is_empty() {
                    String::new()
                } else {
                    format!("  MISSING: {}", missing.join(", "))
                }
            );
        }
    }
    if problems > 0 {
        anyhow::bail!("{problems} hunts with missing files");
    }
    println!("ok");
    Ok(())
}
