//! `c2-parity spawns`: how often each character spawns in a Modder's Edition
//! game, per area and time of day, over many placements. Skins and rarities
//! are spawn ratios, area/time overwrites and spawn groups; this shows what
//! they come to without playing.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::{DinoInfo, Herd, Species};
use c2_core::rng::MsRand;
use c2_formats::bits::Bits;
use c2_formats::mee::Hunt;
use c2_formats::{car, map, res_txt::ResTxt, rsc};

use crate::{find_ci, find_path};

pub const USAGE: &str = "usage: c2-parity spawns <GAME_DIR> [--areas 1,2|all] \
[--times 0,1,2|all] [--picked slots|all|none] [--hunts N]";

/// The game's characters as `m`'s tables resolve them, models read once
/// per file (`cache`). Characters without a readable model get none.
pub fn species(
    huntdat: &Path,
    res: &c2_formats::res_txt::ResData,
    cache: &mut HashMap<String, Option<car::Car>>,
) -> Vec<Species> {
    let empty = car::Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: vec![],
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    };
    res.characters
        .iter()
        .map(|d| {
            let info = DinoInfo::from_def(d);
            let car = cache
                .entry(info.file.to_ascii_lowercase())
                .or_insert_with(|| {
                    find_path(huntdat, &info.file)
                        .and_then(|p| std::fs::read(p).ok())
                        .and_then(|d| car::parse(&d).ok())
                });
            Species::new(info, car.as_ref().unwrap_or(&empty))
        })
        .collect()
}

/// Menu slots as hunt bits (slot + 10), as `mee-sim` takes them.
pub fn picked_bits(picked: &str) -> Result<Bits> {
    let mut b = Bits::NONE;
    for p in picked.split(',').filter(|p| !p.is_empty() && *p != "none") {
        if p == "all" {
            for i in 10..Bits::LEN {
                b.set(i);
            }
        } else {
            b.set(10 + p.parse::<usize>().context("picked: menu slots")?);
        }
    }
    Ok(b)
}

fn list(arg: Option<&str>, all: &[u32]) -> Result<Vec<u32>> {
    match arg {
        None | Some("all") => Ok(all.to_vec()),
        Some(s) => s
            .split(',')
            .map(|v| v.parse().context("a number list"))
            .collect(),
    }
}

/// Areas the game has a `.MAP` and `.RSC` for (`AREAn`).
fn areas_present(areas_dir: &Path) -> Vec<u32> {
    (1..=99)
        .filter(|n| {
            find_ci(areas_dir, &format!("AREA{n}.MAP")).is_some()
                && find_ci(areas_dir, &format!("AREA{n}.RSC")).is_some()
        })
        .collect()
}

/// One column: in how many hunts each character appeared, and how many in all.
struct Column {
    label: String,
    present: Vec<u32>,
    total: Vec<u32>,
}

pub fn run(args: &[String]) -> Result<()> {
    let dir = Path::new(args.first().context(USAGE)?);
    let flag = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let hunts: u32 = flag("--hunts").map_or(Ok(200), str::parse)?;
    let picked = picked_bits(flag("--picked").unwrap_or("all"))?;
    let huntdat = find_ci(dir, "HUNTDAT").context("no HUNTDAT")?;
    let areas_dir = find_ci(&huntdat, "AREAS").context("no AREAS")?;
    let res_path = find_ci(&huntdat, "_RES.TXT").context("no _RES.TXT")?;
    let res = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(res_path)?))?;
    let areas = list(flag("--areas"), &areas_present(&areas_dir))?;
    let times = list(flag("--times"), &[0, 1, 2])?;

    let mut cache = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    let mut cols = Vec::new();
    for &a in &areas {
        let map_path = find_ci(&areas_dir, &format!("AREA{a}.MAP")).context("no map")?;
        let rsc_path = find_ci(&areas_dir, &format!("AREA{a}.RSC")).context("no rsc")?;
        let (map_data, rsc_data) = (std::fs::read(map_path)?, std::fs::read(rsc_path)?);
        for &t in &times {
            let time = match t {
                0 => TimeOfDay::Dusk,
                2 => TimeOfDay::Night,
                _ => TimeOfDay::Day,
            };
            let area = Area::new(map::parse(&map_data)?, rsc::parse(&rsc_data)?, time);
            let h = Hunt {
                area: Some(a),
                time: t as u8,
                picked,
            };
            let m = c2_core::mee::read_picked_anywhere(&res, &h, &[]);
            let mut sp = species(&huntdat, &m.res, &mut cache);
            c2_core::mee::tune(&mut sp, &m.tables);
            if names.len() < sp.len() {
                names = sp.iter().map(|s| s.info.name.clone()).collect();
            }
            let hunter = area
                .landings
                .first()
                .map_or((512.0 * 256.0, 512.0 * 256.0), |l| {
                    (
                        f32::from(l.0) * 256.0 + 128.0,
                        f32::from(l.1) * 256.0 + 128.0,
                    )
                });
            let mut herd = Herd {
                species: sp,
                ..Default::default()
            };
            let n = herd.species.len();
            let mut col = Column {
                label: format!("a{a}t{t}"),
                present: vec![0; n],
                total: vec![0; n],
            };
            for seed in 0..hunts {
                herd.place_mee(&area, hunter, &mut MsRand::new(seed + 1), &m.tables, 128);
                let mut seen = vec![false; n];
                for c in &herd.chars {
                    col.total[c.ctype] += 1;
                    seen[c.ctype] = true;
                }
                for (p, s) in col.present.iter_mut().zip(seen) {
                    *p += u32::from(s);
                }
            }
            cols.push(col);
        }
    }
    print(&names, &cols, hunts);
    Ok(())
}

/// Rows: characters that spawned anywhere. Cells: percent of hunts it
/// appeared in / mean count per hunt; `-` never.
fn print(names: &[String], cols: &[Column], hunts: u32) {
    println!("{hunts} placements per column; cell = % of hunts present / mean count");
    print!("{:>5} {:<26}", "index", "name");
    for c in cols {
        print!(" {:>12}", c.label);
    }
    println!();
    for (i, name) in names.iter().enumerate() {
        if cols
            .iter()
            .all(|c| c.total.get(i).copied().unwrap_or(0) == 0)
        {
            continue;
        }
        print!("{i:>5} {:<26}", name.chars().take(26).collect::<String>());
        for c in cols {
            let (p, t) = (
                c.present.get(i).copied().unwrap_or(0),
                c.total.get(i).copied().unwrap_or(0),
            );
            if t == 0 {
                print!(" {:>12}", "-");
            } else {
                let pct = f64::from(p) * 100.0 / f64::from(hunts);
                let mean = f64::from(t) / f64::from(hunts);
                print!(" {:>12}", format!("{pct:.0}%/{mean:.1}"));
            }
        }
        println!();
    }
}
