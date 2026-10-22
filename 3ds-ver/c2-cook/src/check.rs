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

static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn clock_ms() -> f64 {
    T0.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
}

pub fn check(pack: &Path, bench: Option<&str>) -> Result<()> {
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
    if let Some(name) = bench {
        picks.retain(|p| {
            p.0.to_ascii_lowercase()
                .starts_with(&name.to_ascii_lowercase())
        });
    }
    let mut problems = 0;
    for area_name in &areas {
        let area = c2_core::load::area_with(&vfs, area_name, TimeOfDay::Day, true)?;
        println!("{area_name}:");
        for (name, targets, roamers) in &picks {
            let hr = rules.hunt_with(area_name, TimeOfDay::Day, *targets, roamers);
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
            for i in (0..species.len()).filter(|&i| keep(i)) {
                let sp = &species[i];
                if sp.mee.as_ref().is_some_and(|m| m.anim_index.is_empty())
                    && !sp.ani_time.is_empty()
                {
                    missing.push(format!("animation keys of #{i} ({})", sp.info.name));
                }
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
            hunt.prof.clock = Some(clock_ms);
            if bench.is_some() {
                close_range(&area, &mut hunt, &car_refs, &wanted);
                continue;
            }
            // Ticks' parts summed over the run (ms).
            let mut parts = [0f32; c2_core::prof::SECTIONS.len()];
            let mut worst = 0f64;
            let t_all = clock_ms();
            for _ in 0..3000 {
                let t = clock_ms();
                hunt.tick(&area, &car_refs, &HuntInput::default(), 20, &mut out);
                worst = worst.max(clock_ms() - t);
                out.clear();
                for (p, m) in parts.iter_mut().zip(hunt.prof.mean) {
                    *p += m / 3000.0;
                }
            }
            let mean = (clock_ms() - t_all) / 3000.0;
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
            let slow: Vec<String> = c2_core::prof::SECTIONS
                .iter()
                .zip(parts)
                .filter(|(_, p)| *p > 0.05)
                .map(|(n, p)| format!("{n} {p:.2}"))
                .collect();
            println!(
                "  {name:<32} {placed:>2} placed ({total} animals in all), tick {mean:.3} ms (worst {worst:.2}) {}{}",
                slow.join(" "),
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

/// The hunter 1200 units from the first animal of `wanted`, standing for
/// 20 s; it is killed and watched for 20 s more. Timings every 5 s, and
/// what the body plays.
fn close_range(
    area: &c2_core::area::Area,
    hunt: &mut Hunt,
    cars: &[Option<&c2_formats::car::Car>],
    wanted: &[usize],
) {
    let Some(target) = hunt
        .herd
        .chars
        .iter()
        .position(|c| wanted.contains(&c.ctype))
    else {
        println!("    none placed");
        return;
    };
    let near = |hunt: &mut Hunt| {
        let c = &hunt.herd.chars[target];
        let (x, z) = (c.pos[0] + 1200.0, c.pos[2]);
        let p = &mut hunt.player;
        p.x = x;
        p.z = z;
        p.y = area.land_height(x, z);
    };
    near(hunt);
    {
        let c = &hunt.herd.chars[target];
        let sp = &hunt.herd.species[c.ctype];
        println!(
            "    #{target}: c.ai {} info.ai {} mee {:?} anims {} ani_time {:?}",
            c.ai,
            sp.info.ai,
            sp.mee.as_ref().map(|m| (m.ai, m.anim_index.clone())),
            sp.anims().len(),
            sp.ani_time
        );
    }
    let mut out = Vec::new();
    for t in 0..2000u32 {
        if t == 1000 {
            hunt.debug_bag(area, target, &mut out);
            println!("    -- killed #{target}");
        }
        hunt.tick(area, cars, &HuntInput::default(), 20, &mut out);
        out.clear();
        if (t + 1) % 250 == 0 {
            let c = &hunt.herd.chars[target];
            let sp = &hunt.herd.species[c.ctype];
            let anim = sp.anims().get(c.phase).copied();
            let nearby = hunt
                .herd
                .chars
                .iter()
                .filter(|o| {
                    o.health > 0
                        && (o.pos[0] - hunt.player.x).hypot(o.pos[2] - hunt.player.z) < 256.0 * 34.0
                })
                .count();
            let parts: Vec<String> = c2_core::prof::SECTIONS
                .iter()
                .zip(hunt.prof.mean)
                .map(|(n, m)| format!("{}{m:.3}", &n[..1]))
                .collect();
            println!(
                "    t={:>2}s {} | #{target} health {} state {} phase {} anim {:?} ftime {} | {nearby} alive in view",
                (t + 1) / 50,
                parts.join(" "),
                c.health,
                c.state,
                c.phase,
                anim,
                c.ftime
            );
        }
    }
}
