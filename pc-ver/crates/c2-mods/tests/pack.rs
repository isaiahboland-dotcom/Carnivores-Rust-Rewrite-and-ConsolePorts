//! Two synthetic add-on mods over a synthetic vanilla install.
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use c2_mods::{build, load_mod, materialize, vanilla};

const RES: &str = "weapons {\r\n{\r\n name = 'Pistol'\r\n power = 1\r\n}\r\n}\r\ncharacters {\r\n{\r\n name = 'Hunter'\r\n ai = 0\r\n}\r\n{\r\n name = 'T-Rex'\r\n ai = 18\r\n}\r\n}\r\nprices {\r\n start = 100\r\n}\r\n.\r\n";

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
    let d = std::env::temp_dir().join(format!("c2-mods-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn full_copy_and_partial_mods_merge_with_a_report() {
    let base = tmp("base");
    write(
        &base,
        &[
            ("carn2.exe", b"exe"),
            ("HUNTDAT/_RES.TXT", RES.as_bytes()),
            ("HUNTDAT/TIREX.CAR", b"rex"),
            ("HUNTDAT/MENU/MENUM.TGA", b"menu"),
        ],
    );
    // A whole copy of the game with a new T-Rex and a stronger pistol.
    let a = tmp("a");
    write(
        &a,
        &[
            ("Rex Pack/mymod.exe", b"exe2"),
            (
                "Rex Pack/HUNTDAT/_RES.TXT",
                RES.replace("power = 1", "power = 3").as_bytes(),
            ),
            ("Rex Pack/HUNTDAT/TIREX.CAR", b"new rex"),
            ("Rex Pack/HUNTDAT/MENU/MENUM.TGA", b"menu"),
            ("Rex Pack/HUNTDAT/carnivor.log", b"log"),
        ],
    );
    // A partial mod with a manifest: a new menu picture and a new file.
    let b = tmp("b");
    write(
        &b,
        &[
            ("HUNTDAT/menu/menum.tga", b"other menu"),
            ("HUNTDAT/TIREX.CAR", b"third rex"),
            ("HUNTDAT/EXTRA.TGA", b"x"),
            (
                "mod.toml",
                b"[mod]\nid = \"menus\"\nname = \"Menus\"\nversion = \"2\"\n",
            ),
        ],
    );
    let v = vanilla(std::slice::from_ref(&base)).expect("vanilla");
    let ma = load_mod(&v, &a).expect("mod a");
    // Named after its folder; the data is found in the "Rex Pack" subfolder.
    assert!(
        ma.manifest.info.id.ends_with("-a"),
        "{}",
        ma.manifest.info.id
    );
    assert_eq!(ma.files, vec!["HUNTDAT/TIREX.CAR".to_string()]);
    assert_eq!(ma.identical, 1);
    assert_eq!(ma.manifest.res.modifies_weapons, vec!["pistol".to_string()]);
    let mb = load_mod(&v, &b).expect("mod b");
    assert_eq!(mb.manifest.info.id, "menus");
    assert_eq!(mb.files.len(), 3);

    let pack = build(std::slice::from_ref(&base), vec![ma, mb]).expect("build");
    let out = tmp("out");
    materialize(&pack, &out).expect("materialize");
    let read = |p: &str| std::fs::read(out.join(p)).unwrap_or_default();
    assert_eq!(read("HUNTDAT/TIREX.CAR"), b"third rex");
    assert_eq!(read("HUNTDAT/menu/menum.tga"), b"other menu");
    assert_eq!(read("HUNTDAT/EXTRA.TGA"), b"x");
    let res = String::from_utf8(read("HUNTDAT/_RES.TXT")).unwrap_or_default();
    assert!(res.contains("power = 3"), "{res}");
    let report = String::from_utf8(read("MERGE_REPORT.md")).unwrap_or_default();
    assert!(report.contains("file `HUNTDAT/TIREX.CAR`"), "{report}");
    assert!(report.contains("`menus` wins"), "{report}");
    assert!(report.contains("modifies 'pistol' (power)"), "{report}");
    assert!(
        report.contains("1 identical to vanilla skipped"),
        "{report}"
    );
    // Rebuilding over an earlier pack is allowed; over anything else not.
    materialize(&pack, &out).expect("rebuild");
    assert!(materialize(&pack, &base).is_err());
    for d in [base, a, b, out] {
        let _ = std::fs::remove_dir_all(d);
    }
}
