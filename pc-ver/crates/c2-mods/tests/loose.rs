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
