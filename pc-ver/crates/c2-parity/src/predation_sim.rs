//! `c2-parity predation <AREA.MAP> [seconds] [prey]`: the area's animals,
//! placed as for a hunt with predation on (carnivore fights too; `prey`:
//! prey first), stepped headless with the hunter away. Reports kills,
//! goals and the cost of a step, to tune predation on real data.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::predation::{run::Predators, Goal, Rules};
use c2_core::dino::{Ctx, DinoInfo, Herd, HunterView, PlaceOptions, Species};
use c2_core::rng::MsRand;
use c2_formats::predation::{Predation, Priority};
use c2_formats::res_txt::ResTxt;
use c2_formats::{car, map, rsc};

fn find(d: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(d)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

pub fn run(map_path: &Path, seconds: u32, prey_first: bool) -> Result<()> {
    let dir = map_path.parent().unwrap_or(Path::new("."));
    let huntdat = dir.parent().unwrap_or(Path::new("."));
    let stem = map_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let rsc_path = find(dir, &format!("{stem}.rsc")).context("no .RSC")?;
    let area = Area::new(
        map::parse(&std::fs::read(map_path)?)?,
        rsc::parse(&std::fs::read(rsc_path)?)?,
        TimeOfDay::Day,
    );
    let res_txt = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(
        find(huntdat, "_RES.TXT").context("no _RES.TXT")?,
    )?))?;
    let res = res_txt.to_data();
    let mut species = Vec::new();
    for d in &res.characters {
        let info = DinoInfo::from_def(d);
        let c = car::parse(&std::fs::read(
            find(huntdat, &info.file).with_context(|| info.file.clone())?,
        )?)?;
        species.push(Species::new(info, &c));
    }
    let menu = c2_formats::menu_txt::MenuData::from_res(&res_txt);
    c2_core::game::price_species(&mut species, &menu);
    let settings = Predation {
        carnivore_fights: true,
        priority: if prey_first {
            Priority::Prey
        } else {
            Priority::Hunter
        },
        ..Predation::default()
    };
    let rules = Rules::new(&settings, &species);
    for (i, sp) in species.iter().enumerate() {
        let k = rules.kind(i);
        println!(
            "  {:<16} price {:>3?}  {:?} {:?}",
            sp.info.name, sp.hunt_price, k.role, k.drive
        );
    }
    let mut rng = MsRand::new(777);
    let land = area.landings.first().copied().unwrap_or((512, 512));
    let (hx, hz) = (f32::from(land.0) * 256.0, f32::from(land.1) * 256.0);
    let mut herd = Herd {
        species,
        predation: Some(Box::new(Predators::new(rules))),
        ..Default::default()
    };
    let all = (10..=18).fold(c2_formats::bits::Bits::NONE, |b, a| {
        b | c2_formats::bits::Bits::bit(a)
    });
    herd.place(
        &area,
        (hx, hz),
        &mut rng,
        PlaceOptions {
            density: 255,
            opt_agres: 128,
            targets: all,
            max_ambient: Some(0),
        },
    );
    println!("placed {} animals", herd.chars.len());
    let hunter = HunterView {
        x: hx,
        z: hz,
        y: area.land_height(hx, hz) + 220.0,
        head_y: 220.0,
        alive: false,
        ..Default::default()
    };
    let mut events = Vec::new();
    let (mut worst, mut total) = (0f64, 0f64);
    let steps = seconds * 50;
    let mut killed = vec![false; herd.chars.len()];
    for t in 0..steps {
        let t0 = Instant::now();
        let mut ctx = Ctx {
            area: &area,
            rng: &mut rng,
            dt: 20,
            real_time: t as i32 * 20,
            hunter,
            target: Default::default(),
            struck: None,
            opt_agres: 128,
            tranq: false,
            view_r: 42,
            render_r: 42,
            run_k: 1.0,
            events: &mut events,
        };
        herd.step(&mut ctx);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        worst = worst.max(ms);
        total += ms;
        for (i, c) in herd.chars.iter().enumerate() {
            if c.predation.killed_by_animal && !killed[i] {
                killed[i] = true;
                let by = herd
                    .chars
                    .iter()
                    .filter(|k| k.health != 0 && k.predation.eat_ms > 0)
                    .min_by(|a, b| {
                        let d = |k: &c2_core::dino::Character| {
                            (k.pos[0] - c.pos[0]).powi(2) + (k.pos[2] - c.pos[2]).powi(2)
                        };
                        d(a).total_cmp(&d(b))
                    })
                    .map_or("?", |k| herd.species[k.ctype].info.name.as_str());
                println!(
                    "{:>6.1}s  {} killed by {by}",
                    t as f32 / 50.0,
                    herd.species[c.ctype].info.name
                );
            }
        }
    }
    let count = |f: fn(Goal) -> bool| herd.chars.iter().filter(|c| f(c.predation.goal)).count();
    println!(
        "after {seconds}s: {} killed, hunting {}, fleeing {}, kill events {}",
        killed.iter().filter(|k| **k).count(),
        count(|g| matches!(g, Goal::Hunt(_))),
        count(|g| matches!(g, Goal::Flee(_))),
        events
            .iter()
            .filter(|e| matches!(e, c2_core::dino::DinoEvent::KillHunter { .. }))
            .count()
    );
    println!(
        "step: {:.3} ms average, {:.3} ms worst",
        total / f64::from(steps.max(1)),
        worst
    );
    Ok(())
}
