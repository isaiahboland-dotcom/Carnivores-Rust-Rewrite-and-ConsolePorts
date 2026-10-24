//! Standalone games combined beside a synthetic dialect base (README M16).
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use c2_formats::car::{self, Animation, Car};
use c2_formats::model::{Mesh, Vertex};
use c2_formats::res_txt::ResTxt;
use c2_mods::standalone::{build, Ambients, Options};

fn write(root: &Path, files: &[(&str, &[u8])]) {
    for (p, d) in files {
        let p = root.join(p);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(p, d).expect("write");
    }
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("c2-standalone-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A one-vertex model with animations of these names.
fn model(names: &[&str]) -> Vec<u8> {
    let v = Vertex {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        owner: 0,
        hide: 0,
    };
    car::write(&Car {
        name_raw: [0; 32],
        mesh: Mesh {
            vertices: vec![v],
            faces: vec![],
            texture: vec![],
        },
        animations: names
            .iter()
            .map(|n| {
                let mut name_raw = [0u8; 32];
                name_raw[..n.len()].copy_from_slice(n.as_bytes());
                Animation {
                    name_raw,
                    kps: 2,
                    frames: 1,
                    data: vec![0; 3],
                }
            })
            .collect(),
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    })
}

const BASE_RES: &str = "weapons {\n{\n name = 'Pistol'\n file = 'pistol.car'\n power = 1\n}\n}\n\
characters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n\
{\n name = 'Raptor'\n file = 'velo.car'\n ai = 15\n callNo = 0\n runAnim = 0\n walkAnim = 1\n\
 deathtype {\n  dieAnim = 2\n }\n overwrite char0 {\n  basescore = 12\n  spawninfo {\n   spawngroup = 1\n  }\n }\n\
 spawninfo {\n  spawngroup = 0\n }\n}\n}\n\
spawntable {\n spawngroup {\n  spawnmax = 4\n }\n spawngroup {\n  spawnmax = 8\n }\n}\n\
prices {\n start = 100\n area = 10\n dino = 20\n weapon = 5\n}\n.\n";

const BASE_MENU: &str = "weapons {\n{\n name = 'Pistol'\n power = 1\n}\n}\n\
characters {\n{\n name = 'Raptor'\n ai = 15\n}\n}\nprices {\n start = 100\n area = 10\n dino = 20\n weapon = 5\n}\n.\n";

/// A dialect standalone with an animal named like the base's.
const A_RES: &str = "weapons {\n{\n name = 'Bow'\n file = 'bow.car'\n power = 3\n}\n}\n\
characters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n\
{\n name = 'Raptor'\n file = 'raptor_a.car'\n ai = 15\n callNo = 0\n runAnim = 0\n\
 overwrite area1 {\n  file = 'raptor_snow.car'\n }\n spawninfo {\n  spawngroup = 0\n }\n}\n}\n\
spawntable {\n spawngroup {\n  spawnmax = 2\n }\n}\n\
prices {\n start = 100\n area = 30\n dino = 40\n weapon = 7\n}\n.\n";

/// An original-engine standalone: no spawn tables, an AI the base has and
/// one it hasn't (a patched engine's extra slot); its prices list two
/// areas, it ships one map.
const B_RES: &str = "weapons {\n{\n name = 'Rifle'\n file = 'rifle.car'\n power = 4\n}\n}\n\
characters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n\
{\n name = 'Utahraptor'\n file = 'utah.car'\n ai = 15\n health = 9\n}\n\
{\n name = 'Big'\n file = 'big.car'\n ai = 19\n health = 50\n danger = TRUE\n}\n}\n\
prices {\n start = 100\n area = 50\n area = 55\n dino = 60\n dino = 65\n weapon = 9\n}\n.\n";

#[test]
fn standalones_are_renumbered_tied_to_their_maps_and_original_ones_converted() {
    let base = tmp("base");
    write(
        &base,
        &[
            ("HUNTDAT/_RES.TXT", BASE_RES.as_bytes()),
            ("HUNTDAT/_MENU.TXT", BASE_MENU.as_bytes()),
            ("HUNTDAT/hunter.car", &model(&["stand"])),
            ("HUNTDAT/velo.car", &model(&["run", "walk", "die"])),
            ("HUNTDAT/WEAPONS/pistol.car", b"pistol"),
            ("HUNTDAT/AREAS/AREA1.MAP", b"base map"),
            ("HUNTDAT/AREAS/AREA1.RSC", b"base rsc"),
        ],
    );
    let games = tmp("games");
    let a = games.join("Game A");
    write(
        &a,
        &[
            ("HUNTDAT/_RES.TXT", A_RES.as_bytes()),
            ("HUNTDAT/raptor_a.car", &model(&["run"])),
            ("HUNTDAT/raptor_snow.car", &model(&["run"])),
            ("HUNTDAT/WEAPONS/bow.car", b"bow"),
            ("HUNTDAT/AREAS/AREA1.MAP", b"a map"),
            ("HUNTDAT/AREAS/AREA1.RSC", b"a rsc"),
            ("HUNTDAT/MENU/DINO1.TGA", b"a pic"),
        ],
    );
    let b = games.join("Game B");
    write(
        &b,
        &[
            ("HUNTDAT/_RES.TXT", B_RES.as_bytes()),
            ("HUNTDAT/utah.car", &model(&["Ut_run", "Ut_wlk", "Ut_die"])),
            (
                "HUNTDAT/big.car",
                &model(&["Bg_walk", "Bg_run", "Bg_idle1", "Bg_die", "Bg_slp"]),
            ),
            ("HUNTDAT/WEAPONS/rifle.car", b"rifle"),
            ("HUNTDAT/AREAS/AREA1.MAP", b"b map"),
            ("HUNTDAT/AREAS/AREA1.RSC", b"b rsc"),
            // A spare copy in a folder: not the map.
            ("HUNTDAT/AREAS/New Folder/area1.map", b"b old map"),
        ],
    );
    let out = tmp("out");
    let built = build(
        std::slice::from_ref(&base),
        &[a.clone(), b.clone()],
        &out,
        &Options {
            ambients: Ambients::Tied,
        },
    )
    .expect("combine");
    let report = &built.report;
    assert!(report.contains("original engine"), "{report}");
    assert!(
        report.contains("lists 2 areas, it has maps for 1"),
        "{report}"
    );

    // Areas: base 1, A 2, B 3 (B's second, without a map, left out).
    let h = out.join("HUNTDAT");
    assert_eq!(
        std::fs::read(h.join("AREAS/AREA2.MAP")).ok(),
        Some(b"a map".to_vec())
    );
    assert_eq!(
        std::fs::read(h.join("AREAS/AREA3.MAP")).ok(),
        Some(b"b map".to_vec())
    );
    assert!(!h.join("AREAS/AREA4.MAP").exists());
    // Menu files follow the slots: A's first huntable is slot 2.
    assert!(h.join("MENU/DINO2.TGA").exists());

    let res =
        ResTxt::parse(&std::fs::read_to_string(h.join("_RES.TXT")).expect("res")).expect("parse");
    let chars = &res.section("characters").expect("characters").blocks;
    let by = |n: &str| {
        chars
            .iter()
            .find(|c| c.get(&["name"]).is_some_and(|v| v.contains(n)))
            .cloned()
            .unwrap_or_else(|| panic!("no {n}"))
    };
    // A's Raptor: renamed, its models (its area's too) under MODS, kept to
    // its area.
    let ra = by("Raptor (Game A)");
    assert_eq!(ra.get(&["file"]), Some("'MODS/game-a/raptor_a.car'"));
    let snow = ra
        .subs
        .iter()
        .find(|s| s.name.starts_with("overwrite area2"))
        .expect("A's area 1 is area 2");
    assert_eq!(
        snow.block.get(&["file"]),
        Some("'MODS/game-a/raptor_snow.car'")
    );
    assert!(ra.subs.iter().any(|s| s.name == "addition area2"));
    // B's Utahraptor: the base raptor's animations and spawns, its picked
    // spawn at its own slot (3: after the base's and A's), on area 3 only.
    let u = by("Utahraptor");
    assert_eq!(u.get(&["runAnim"]), Some("0"));
    assert_eq!(u.get(&["health"]), Some("9"), "its own numbers stay");
    assert_eq!(u.get(&["callNo"]), Some("2"));
    let picked = u
        .subs
        .iter()
        .find(|s| s.name.starts_with("overwrite char2"))
        .expect("picked condition at its slot");
    assert!(picked.name.contains("area3"));
    assert!(
        picked.block.get(&["basescore"]).is_none(),
        "not the donor's score"
    );
    assert!(u.subs.iter().any(|s| s.name == "addition area3"));
    // B's Big (AI 19): a hunter's kind, animations by their names.
    let big = by("Big");
    assert_eq!(big.get(&["ai"]), Some("15"));
    assert_eq!(big.get(&["walkAnim"]), Some("0"));
    assert_eq!(big.get(&["runAnim"]), Some("1"));
    assert_eq!(big.get(&["health"]), Some("50"));
    // The base's own animal stays on the base's map.
    let base_raptor = chars
        .iter()
        .find(|c| c.get(&["name"]) == Some("'Raptor'"))
        .expect("base raptor");
    assert!(base_raptor.subs.iter().any(|s| s.name == "addition area1"));

    // Menu: 4 huntables, 3 weapons, 3 areas.
    let menu = c2_formats::menu_txt::MenuData::from_res(
        &ResTxt::parse(&std::fs::read_to_string(h.join("_MENU.TXT")).expect("menu"))
            .expect("parse"),
    );
    assert_eq!(menu.huntables.len(), 4);
    assert_eq!(menu.weapons.len(), 3);
    assert_eq!(menu.areas.len(), 3);
    for d in [base, games, out] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// A dialect game without spawn tables, played on its own, gets the
/// default ones (else none of its animals would come).
#[test]
fn spawnless_dialect_games_get_default_spawns() {
    let game = tmp("spawnless");
    let res = "characters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n\
{\n name = 'Moth'\n file = 'moth.car'\n ai = 3\n runAnim = 0\n}\n\
{\n name = 'Raptor'\n file = 'velo.car'\n ai = 15\n runAnim = 0\n killdist = 300\n\
 deathtype {\n  dieAnim = 2\n }\n}\n}\n\
prices {\n start = 100\n area = 10\n dino = 20\n}\n.\n";
    write(&game, &[("HUNTDAT/_RES.TXT", res.as_bytes())]);
    let v = c2_mods::vanilla(std::slice::from_ref(&game)).expect("vanilla");
    let out = ResTxt::parse(&String::from_utf8_lossy(
        &v.read("HUNTDAT/_RES.TXT").expect("res"),
    ))
    .expect("parse");
    assert!(out.section("spawntable").is_some());
    let chars = &out.section("characters").expect("characters").blocks;
    let groups = |b: &c2_formats::res_txt::Block| -> Vec<String> {
        b.subs
            .iter()
            .flat_map(|s| {
                std::iter::once(s)
                    .chain(s.block.subs.iter())
                    .filter(|x| x.name == "spawninfo")
                    .filter_map(|x| x.block.get(&["spawngroup"]).map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    assert_eq!(groups(&chars[1]), ["0"], "ambient");
    assert_eq!(groups(&chars[2]), ["1", "2"], "random roster, and picked");
    assert_eq!(chars[2].get(&["callNo"]), Some("0"));
    assert!(chars[2].subs.iter().any(|s| s.name == "overwrite char0"));
    let _ = std::fs::remove_dir_all(game);
}
