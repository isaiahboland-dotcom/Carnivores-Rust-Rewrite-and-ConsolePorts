//! Loose add-ons (a model, a pasted `_RES.TXT` entry, menu files, a hunter
//! model) over a synthetic install.
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use c2_formats::car::{self, Animation, Car};
use c2_formats::model::{Mesh, Vertex};
use c2_mods::{build, load_mod, vanilla};

const RES: &str = "weapons {\n{\n name = 'Pistol'\n power = 1\n}\n}\ncharacters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n{\n name = 'Parasaurolophus'\n ai = 10\n}\n{\n name = 'Spinosaurus'\n file = 'spino.car'\n ai = 16\n}\n}\n.\n";

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
    let d = std::env::temp_dir().join(format!("c2-mods-loose-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A one-vertex model with one animation per tag.
fn model(tags: &[i16]) -> Vec<u8> {
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
        animations: tags
            .iter()
            .map(|&t| Animation {
                name_raw: [0; 32],
                kps: 2,
                frames: 1,
                data: vec![t; 3],
            })
            .collect(),
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    })
}

#[test]
fn loose_add_ons_are_placed_merged_and_their_hunters_combined() {
    let base = tmp("base");
    write(
        &base,
        &[
            ("HUNTDAT/_RES.TXT", RES.as_bytes()),
            ("HUNTDAT/hunter.car", &model(&[0, 1])),
            ("HUNTDAT/MENU/PICS/DINO7.TGA", b"spino pic"),
            ("HUNTDAT/SOUNDFX/CALLS/CALL7_A.WAV", b"spino call"),
        ],
    );
    // Takes over the Spinosaurus slot: menu art, a call, an entry in the
    // readme, and a hunter with one new death animation.
    let a = tmp("a");
    write(
        &a,
        &[
            ("Raptor X/rx.car", b"rx"),
            ("Raptor X/DINO7.TGA", b"rx pic"),
            ("Raptor X/CALL7_A.wav", b"rx call"),
            ("Raptor X/hunter_rx.car", &model(&[0, 1, 7])),
            ("Raptor X/promo.png", b"png"),
            (
                "Raptor X/README.txt",
                b"Replace the Spinosaurus entry with:\r\n\r\n{\r\n name = 'Raptor X'\r\n file = 'rx.car'\r\n ai = 16\r\n killtype{\r\n  hunterAnim = 2\r\n }\r\n}\r\n",
            ),
        ],
    );
    // A new animal for the maps, with its own new death animation.
    let b = tmp("b");
    write(
        &b,
        &[
            ("big.car", b"big"),
            ("hunter_big.car", &model(&[0, 1, 9])),
            (
                "Big _RES.txt",
                b"{\n name = 'Big'\n file = 'big.car'\n ai = 12\n killtype{\n  hunterAnim = 2\n }\n}\n",
            ),
        ],
    );
    let bases = [base.clone()];
    let v = vanilla(&bases).expect("vanilla");
    let ma = load_mod(&v, &a).expect("a");
    let mb = load_mod(&v, &b).expect("b");
    assert!(ma.files.iter().any(|f| f == "HUNTDAT/MENU/PICS/DINO7.TGA"));
    assert!(ma
        .files
        .iter()
        .any(|f| f == "HUNTDAT/SOUNDFX/CALLS/CALL7_A.WAV"));
    assert!(ma.files.iter().any(|f| f == "HUNTDAT/rx.car"));
    assert!(!ma
        .files
        .iter()
        .any(|f| f.contains("hunter") || f.contains("png")));
    let pack = build(&bases, vec![ma, mb]).expect("build");

    assert_eq!(
        pack.vfs.read("HUNTDAT/MENU/PICS/DINO7.TGA").expect("pic"),
        b"rx pic"
    );
    let res = c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(
        &pack.vfs.read("HUNTDAT/_RES.TXT").expect("res"),
    ))
    .expect("parse");
    let chars = &res.section("characters").expect("characters").blocks;
    let names: Vec<_> = chars.iter().filter_map(|b| b.get(&["name"])).collect();
    assert_eq!(
        names,
        ["'Hunter'", "'Parasaurolophus'", "'Raptor X'", "'Big'"]
    );
    let hunter_anim = |i: usize| {
        chars[i].subs[0]
            .block
            .get(&["hunterAnim"])
            .map(str::to_owned)
    };
    // Raptor X's animation 2 stays 2; Big's moves to 3, after it.
    assert_eq!(hunter_anim(2).as_deref(), Some("2"));
    assert_eq!(hunter_anim(3).as_deref(), Some("3"));
    let hunter = car::parse(&pack.vfs.read("HUNTDAT/hunter.car").expect("hunter")).expect("car");
    let tags: Vec<i16> = hunter.animations.iter().map(|a| a.data[0]).collect();
    assert_eq!(tags, [0, 1, 7, 9]);
    let report = pack.report();
    assert!(
        report.contains("takes over characters slot 2 ('spinosaurus')"),
        "{report}"
    );
    assert!(report.contains("merged into"), "{report}");
}

/// A Modder's Edition-style base: entries with spawn tables, and the menu's
/// own `_MENU.TXT`.
const MEE_RES: &str = "weapons {\n{\n name = 'Pistol'\n power = 1\n}\n}\ncharacters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n{\n name = 'Spinosaurus'\n file = 'spino.car'\n ai = 16\n runAnim = 0\n walkAnim = 1\n deathtype {\n  dieAnim = 6\n }\n overwrite char0 {\n  spawninfo{\n   spawngroup = 2\n  }\n }\n spawninfo{\n  spawngroup = 1\n }\n}\n}\n.\n";
const MEE_MENU: &str = "weapons {\n{\n name = 'Pistol'\n power = 1\n}\n}\ncharacters {\n{\n name = 'Spinosaurus'\n ai = 16\n}\n}\nprices {\n start = 100\n dino = 250\n weapon = 20\n}\n.\n";

#[test]
fn on_a_modders_edition_base_slot_add_ons_get_their_own_slot() {
    let base = tmp("mee-base");
    write(
        &base,
        &[
            ("HUNTDAT/_RES.TXT", MEE_RES.as_bytes()),
            ("HUNTDAT/_MENU.TXT", MEE_MENU.as_bytes()),
            ("HUNTDAT/hunter.car", &model(&[0, 1])),
            ("HUNTDAT/MENU/PICS/DINO1.TGA", b"spino pic"),
            ("HUNTDAT/MENU/PICS/WEAPON1.TGA", b"pistol pic"),
            ("HUNTDAT/SOUNDFX/CALLS/CALL1_A.WAV", b"spino call"),
            ("HUNTDAT/WEAPONS/BULLET1.TGA", b"pistol bullet"),
        ],
    );
    // Made for the Spinosaurus slot, as an original-game entry (no spawn
    // tables), plus a weapon made for the pistol slot.
    let a = tmp("mee-a");
    write(
        &a,
        &[
            ("rx.car", b"rx"),
            ("DINO1.TGA", b"rx pic"),
            ("CALL1_A.wav", b"rx call"),
            (
                "README.txt",
                b"{\n name = 'Raptor X'\n file = 'rx.car'\n ai = 16\n}\n",
            ),
            (
                "gun/_RES.TXT",
                b"{\n name = 'Gun'\n pic = 'bullet1.tga'\n power = 2\n}\n",
            ),
            ("gun/MENU/PICS/WEAPON1.TGA", b"gun pic"),
            ("gun/WEAPONS/BULLET1.TGA", b"gun bullet"),
        ],
    );
    let bases = [base.clone()];
    let v = vanilla(&bases).expect("vanilla");
    let pack = build(&bases, vec![load_mod(&v, &a).expect("a")]).expect("build");
    let read = |p: &str| pack.vfs.read(p).expect(p);
    // The originals stay; the add-on's files moved to the new slots.
    assert_eq!(read("HUNTDAT/MENU/PICS/DINO1.TGA"), b"spino pic");
    assert_eq!(read("HUNTDAT/MENU/PICS/DINO2.TGA"), b"rx pic");
    assert_eq!(read("HUNTDAT/SOUNDFX/CALLS/CALL2_A.wav"), b"rx call");
    assert_eq!(read("HUNTDAT/MENU/PICS/WEAPON2.TGA"), b"gun pic");
    assert_eq!(read("HUNTDAT/WEAPONS/BULLET2.TGA"), b"gun bullet");
    assert_eq!(read("HUNTDAT/WEAPONS/BULLET1.TGA"), b"pistol bullet");
    let parse = |p: &str| {
        c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(&read(p))).expect("parse")
    };
    let menu = c2_formats::menu_txt::MenuData::from_res(&parse("HUNTDAT/_MENU.TXT"));
    let dinos: Vec<_> = menu
        .huntables
        .iter()
        .map(|d| (d.name.as_str(), d.price))
        .collect();
    assert_eq!(dinos, [("Spinosaurus", 250), ("Raptor X", 250)]);
    let guns: Vec<_> = menu
        .weapons
        .iter()
        .map(|w| (w.name.as_str(), w.price))
        .collect();
    assert_eq!(guns, [("Pistol", 20), ("Gun", 20)]);
    let res = parse("HUNTDAT/_RES.TXT");
    let rx = &res.section("characters").expect("characters").blocks[2];
    // The Spinosaurus slot's spawn tables, its slot condition moved along.
    let subs: Vec<_> = rx.subs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(subs, ["deathtype", "overwrite char1", "spawninfo"]);
    // And its animation keys: an entry for the original game has none.
    assert_eq!(rx.get(&["runAnim"]), Some("0"));
    assert_eq!(rx.get(&["walkAnim"]), Some("1"));
    let gun = &res.section("weapons").expect("weapons").blocks[1];
    assert_eq!(gun.get(&["pic"]), Some("'bullet2.tga'"));
}
