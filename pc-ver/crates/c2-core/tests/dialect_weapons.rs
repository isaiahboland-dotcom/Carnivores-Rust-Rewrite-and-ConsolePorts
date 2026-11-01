//! Every weapon of the game folders under `C2_GAMES_DIR` (the owner's
//! extracted mods; skipped when unset): loaded from its own `_RES.TXT` and
//! `.CAR`, drawn, fired until it has nothing left (reloading and racking
//! by hand), on PC's rules and on the consoles' (`auto_chamber`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use c2_core::area::Area;
use c2_core::dino::Herd;
use c2_core::game::{Hunt, HuntInput, HuntOptions, WeaponAnims, WeaponInfo, WeaponState, Weapons};
use c2_core::player::Player;
use c2_core::rng::MsRand;
use c2_formats::bits::Bits;
use c2_formats::res_txt::ResTxt;

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn huntdats(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        if e.file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("HUNTDAT")
        {
            out.push(p);
        } else {
            huntdats(&p, out);
        }
    }
}

/// Fires weapon `info` until it is empty: how many shots, and whether it got
/// stuck with rounds left.
fn empty_it(info: WeaponInfo, anims: WeaponAnims, auto: bool) -> (u32, i32) {
    let area = Area::flat(10);
    let player = Player::new(&area, (500, 500));
    let weapons = Weapons::new(vec![info], vec![anims], Bits::bit(0), false);
    let opts = HuntOptions {
        auto_chamber: auto,
        ..Default::default()
    };
    let mut h = Hunt::new(
        &area,
        player,
        Herd::default(),
        weapons,
        MsRand::new(3),
        opts,
        c2_core::profile::new_player("t", 0, 0),
    );
    let mut ev = Vec::new();
    let draw = HuntInput {
        toggle_weapon: true,
        ..Default::default()
    };
    h.tick(&area, &[], &draw, 10, &mut ev);
    let start = h.weapons.rounds(0);
    let mut shots = 0;
    for t in 0..60_000 {
        if h.weapons.rounds(0) == 0 {
            break;
        }
        let w = &h.weapons;
        let empty = w.chambered[0] == 0;
        let input = HuntInput {
            fire: t % 2 == 0,
            reload: !auto && empty && t % 7 == 1,
            rack: !auto && empty && t % 7 == 4,
            toggle_weapon: w.state == WeaponState::Hidden && t % 50 == 3,
            ..Default::default()
        };
        let before = h.shots_made;
        h.tick(&area, &[], &input, 10, &mut ev);
        shots += h.shots_made - before;
        ev.clear();
    }
    assert!(shots as i32 <= start, "never more shots than rounds");
    (shots, h.weapons.rounds(0))
}

#[test]
fn every_games_weapons_fire_until_empty() {
    let Ok(root) = std::env::var("C2_GAMES_DIR") else {
        return;
    };
    let mut dirs = Vec::new();
    huntdats(Path::new(&root), &mut dirs);
    let (mut n, mut stuck) = (0, Vec::new());
    for d in dirs {
        let Some(res) = find_ci(&d, "_RES.TXT") else {
            continue;
        };
        let Ok(res) = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(res).unwrap())) else {
            continue;
        };
        let res = res.to_data();
        let wdir = find_ci(&d, "WEAPONS");
        for def in &res.weapons {
            let info = WeaponInfo::from_def(def);
            let Some(car) = wdir
                .as_deref()
                .and_then(|w| find_ci(w, &info.file))
                .and_then(|p| c2_formats::car::parse(&std::fs::read(p).ok()?).ok())
            else {
                continue;
            };
            let anims = WeaponAnims::from_car(&info, &car);
            for auto in [false, true] {
                n += 1;
                let (shots, left) = empty_it(info.clone(), anims, auto);
                // A mod's "unlimited" (9999999 rounds) can't empty here.
                let endless = info.shots > 10_000 && shots > 0;
                if (left > 0 && !endless) || shots == 0 {
                    stuck.push(format!(
                        "{} {} (auto {auto}): {shots} shots, {left} left",
                        d.display(),
                        info.name
                    ));
                }
            }
        }
    }
    println!("{n} weapon runs");
    assert!(stuck.is_empty(), "stuck:\n{}", stuck.join("\n"));
}
