//! `c2-cook --plan PACK`: the memory plan the 3DS makes for each area
//! (`c2-3ds`'s `budget.rs`, built here too), for one pick and for the three
//! costliest picks and weapons, without a console.

use std::path::Path;

use anyhow::{Context, Result};
use c2_core::area::TimeOfDay;
use c2_formats::bits::Bits;
use c2_vfs::{PackBackend, Vfs};

use crate::budget::{plan, Plan, Request};
use crate::pack_info::PackInfo;
use crate::rules::{HuntRules, Role, Rules};

/// The 3DS's settings as it ships: `AMBIENT_CAP`, 10 secret animals, the
/// default density (`c2-3ds`'s `settings.rs`).
const AMBIENT_MAX: usize = 5;
const SECRET_MAX: usize = 10;
const DENSITY: usize = 128;

fn mib(kb: usize) -> f32 {
    kb as f32 / 1024.0
}

/// One line: what the plan loads and what it leaves.
fn line(label: &str, p: &Plan) -> String {
    if !p.planned {
        return format!(
            "    {label:<10} not planned (no figures in the pack){}",
            if p.fits { "" } else { ": DOES NOT FIT" }
        );
    }
    if !p.fits {
        return format!(
            "    {label:<10} DOES NOT FIT (needs linear {:.1} / heap {:.1} MiB)",
            mib(p.used.linear),
            mib(p.used.heap)
        );
    }
    let left = p.spare;
    format!(
        "    {label:<10} dinos up to {:>3} ({}), secrets {}, ambient {} of {} species, aquatic {}; \
         spare linear {:.1} / heap {:.1} MiB",
        p.huntables_max(),
        p.huntables
            .iter()
            .map(|h| h.1.to_string())
            .collect::<Vec<_>>()
            .join("/"),
        p.secrets(),
        p.ambients_max,
        p.ambient_models.len(),
        p.aquatic.iter().map(|a| a.1).sum::<usize>(),
        mib(left.linear),
        mib(left.heap),
    )
}

/// The slots of `hr`'s menu animals, costliest model first.
fn slots_by_cost(info: &PackInfo, hr: &HuntRules) -> Vec<usize> {
    let mut slots: Vec<(usize, usize)> = hr
        .roles
        .iter()
        .enumerate()
        .filter_map(|(c, r)| match r {
            Role::Slot(k) => {
                let m = format!("m:{}", hr.keys[c]);
                Some((*k, info.cost(&m) + info.heap(&m)))
            }
            _ => None,
        })
        .collect();
    slots.sort_by_key(|s| (std::cmp::Reverse(s.1), s.0));
    slots.dedup_by_key(|s| s.0);
    slots.into_iter().map(|s| s.0).collect()
}

pub fn report(pack: &Path) -> Result<()> {
    let mut vfs = Vfs::new();
    vfs.push(
        "pack",
        Box::new(PackBackend::open(pack).with_context(|| format!("opening {}", pack.display()))?),
    );
    let info = PackInfo::parse(&String::from_utf8_lossy(&vfs.read("C2RS/INFO.TXT")?));
    let rules = Rules::load(&vfs)?;
    let inst = c2_core::load::rules(&vfs);
    let mut guns: Vec<usize> = (0..rules.menu.weapons.len()).collect();
    guns.sort_by_key(|&w| std::cmp::Reverse(info.cost(&format!("wpn{w}"))));
    println!(
        "{} ({}, pack {}): budget linear {:.0} / heap {:.0} MiB",
        pack.display(),
        info.target,
        info.pack,
        mib(info.linear_kb),
        mib(info.heap_kb)
    );
    for area in info.areas.iter().filter(|a| a.starts_with("AREA")) {
        let hr = rules.hunt(area, TimeOfDay::Day, Bits::ALL);
        let slots = slots_by_cost(&info, &hr);
        let names: Vec<String> = hr
            .res
            .characters
            .iter()
            .map(|d| c2_core::dino::DinoInfo::from_def(d).name)
            .collect();
        let rule_secrets: Vec<(usize, usize)> = inst
            .secrets_on(area)
            .filter_map(|s| {
                let c = c2_core::game::index_named(names.iter().map(String::as_str), &s.name)?;
                Some((c, s.count as usize))
            })
            .collect();
        println!("  {area}:");
        for (label, picks, weapons) in [("1 pick", 1, 1), ("3 costliest", 3, 3)] {
            let targets = slots
                .iter()
                .take(picks)
                .fold(Bits::NONE, |b, &k| b | Bits::bit(k + 10));
            // The rules as hunted: the picks' own models on this area.
            let hr = rules.hunt(area, TimeOfDay::Day, targets);
            let req = Request {
                area: area.clone(),
                hunting: true,
                targets,
                weapons: guns.iter().copied().take(weapons).collect(),
                ambient_max: AMBIENT_MAX,
                secret_max: SECRET_MAX,
                rule_secrets: rule_secrets.clone(),
                huntable_want: 8 + DENSITY / 30 + 5,
                water: info.has_water(area),
                ..Default::default()
            };
            println!("{}", line(label, &plan(&info, &hr, &req)));
        }
    }
    Ok(())
}
