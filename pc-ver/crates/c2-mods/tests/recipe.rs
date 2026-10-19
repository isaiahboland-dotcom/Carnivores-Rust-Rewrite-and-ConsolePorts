//! A recipe made from two synthetic mods, verified and built elsewhere.
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use c2_mods::recipe::{locate, new_recipe, prepare, PatchRef, Recipe, Resolve};
use c2_mods::{build_with, load_mod, materialize, vanilla};

const RES: &str = "weapons {\r\n{\r\n name = 'Pistol'\r\n power = 1\r\n}\r\n}\r\nprices {\r\n start = 100\r\n}\r\n.\r\n";

fn write(root: &Path, files: &[(&str, &[u8])]) {
    for (p, d) in files {
        let p = root.join(p);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(p, d).expect("write");
    }
}

/// A zip with the mod one folder down, as mods are often shipped.
fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
    use std::io::Write as _;
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).expect("create"));
    for (p, d) in files {
        w.start_file(*p, zip::write::SimpleFileOptions::default())
            .expect("start");
        w.write_all(d).expect("write");
    }
    w.finish().expect("finish");
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("c2-recipe-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn recipe_round_trip() {
    let base = tmp("base");
    write(
        &base,
        &[
            ("HUNTDAT/_RES.TXT", RES.as_bytes()),
            ("HUNTDAT/MENU/MENUM.TGA", b"menu"),
        ],
    );
    let authoring = tmp("authoring");
    write(
        &authoring,
        &[
            ("Red Menu/HUNTDAT/MENU/MENUM.TGA", b"red"),
            (
                "Red Menu/HUNTDAT/_RES.TXT",
                RES.replace("power = 1", "power = 2").as_bytes(),
            ),
            ("Blue Menu/HUNTDAT/MENU/MENUM.TGA", b"blue"),
        ],
    );
    let v = vanilla(std::slice::from_ref(&base)).expect("vanilla");
    let mods = ["Red Menu", "Blue Menu"]
        .map(|m| load_mod(&v, &authoring.join(m)).expect("mod"))
        .into_iter()
        .collect::<Vec<_>>();
    let mut r = new_recipe("menus", "Menus", "", &mods).expect("recipe");
    assert_eq!(r.mods[0].files.len(), 2, "menu + _RES.TXT");
    // Red's menu wins although Blue loads later; a patch tweaks the result.
    r.resolve.push(Resolve {
        path: "HUNTDAT/MENU/MENUM.TGA".into(),
        winner: "red-menu".into(),
        ..Default::default()
    });
    r.patches.push(PatchRef {
        target: "_RES.TXT".into(),
        patch: "patches/power.patch".into(),
    });
    let rdir = tmp("recipes");
    write(
        &rdir,
        &[
            ("menus.toml", r.to_toml().as_bytes()),
            (
                "patches/power.patch",
                b"@@ -3,2 +3,2 @@\n  name = 'Pistol'\n- power = 2\n+ power = 7\n",
            ),
        ],
    );
    let rpath = rdir.join("menus.toml");
    let r = Recipe::load(&rpath).expect("load");

    // Someone else's download folder, with different names; Blue is zipped.
    let downloads = tmp("downloads");
    write(
        &downloads,
        &[
            ("red_menu_v1/HUNTDAT/MENU/MENUM.TGA", b"red"),
            (
                "red_menu_v1/HUNTDAT/_RES.TXT",
                RES.replace("power = 1", "power = 2").as_bytes(),
            ),
        ],
    );
    write_zip(
        &downloads.join("Blue Menu.zip"),
        &[("Blue Menu/HUNTDAT/MENU/MENUM.TGA", b"blue")],
    );
    let found = locate(&r, &downloads);
    assert!(found.iter().all(|l| l.complete()), "{found:?}");
    let (sources, opts) = prepare(&rpath, &r, &found, &v).expect("prepare");
    let pack = build_with(std::slice::from_ref(&base), sources, &opts).expect("build");
    let out = tmp("out");
    materialize(&pack, &out).expect("materialize");
    assert_eq!(
        std::fs::read(out.join("HUNTDAT/MENU/MENUM.TGA")).ok(),
        Some(b"red".to_vec())
    );
    let res = std::fs::read_to_string(out.join("HUNTDAT/_RES.TXT")).unwrap_or_default();
    assert!(res.contains("power = 7") && res.contains("\r\n"), "{res}");
    let report = pack.report();
    assert!(report.contains("chosen by the recipe"), "{report}");
    assert!(report.contains("patch `patches/power.patch`"), "{report}");

    // A changed file is reported, with where to get the right one.
    write_zip(
        &downloads.join("Blue Menu.zip"),
        &[("Blue Menu/HUNTDAT/MENU/MENUM.TGA", b"BLUE v2")],
    );
    let found = locate(&r, &downloads);
    assert_eq!(found[1].mismatched.len(), 1);
    assert!(prepare(&rpath, &r, &found, &v).is_err());
    for d in [base, authoring, rdir, downloads, out] {
        let _ = std::fs::remove_dir_all(d);
    }
}
