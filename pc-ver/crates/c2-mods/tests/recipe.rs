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
    let found = locate(&r, std::slice::from_ref(&downloads));
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
    let found = locate(&r, std::slice::from_ref(&downloads));
    assert_eq!(found[1].mismatched.len(), 1);
    assert!(prepare(&rpath, &r, &found, &v).is_err());
    for d in [base, authoring, rdir, downloads, out] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// Each entry's own folder (the launcher's recipe page): checked alone, and
/// given as `ID=FOLDER` it is that entry's whatever its name.
#[test]
fn entries_from_their_own_folders() {
    use c2_mods::recipe::{check_folder, RecipeMod};
    let mine = tmp("own-folders");
    write(
        &mine,
        &[
            ("whatever name/HUNTDAT/MENU/MENUM.TGA", b"red"),
            ("other/HUNTDAT/MENU/MENUM.TGA", b"blue"),
        ],
    );
    let entry = RecipeMod {
        id: "red-menu".into(),
        files: [(
            "HUNTDAT/MENU/MENUM.TGA".to_string(),
            c2_mods::recipe::sha256(b"red"),
        )]
        .into(),
        ..Default::default()
    };
    assert!(check_folder(&entry, &mine.join("whatever name"), false).complete());
    let wrong = check_folder(&entry, &mine.join("other"), false);
    assert_eq!(wrong.mismatched.len(), 1, "{wrong:?}");
    assert!(check_folder(&entry, &mine.join("none"), false)
        .root
        .is_none());
    let r = Recipe {
        mods: vec![entry],
        ..Default::default()
    };
    let arg = PathBuf::from(format!("red-menu={}", mine.join("whatever name").display()));
    let found = locate(&r, &[arg]);
    assert!(found[0].complete(), "{found:?}");
    let _ = std::fs::remove_dir_all(mine);
}

#[test]
fn settings_round_trip_and_stay_out_when_unset() {
    use c2_mods::recipe::{Settings, Source};
    let plain = Recipe::parse("[recipe]\nid = \"a\"\n").expect("parse");
    assert!(!plain.to_toml().contains("settings"));
    let text =
        "[recipe]\nid = \"a\"\n\n[settings]\nmenu = \"mobile\"\nsurvival = [\"classic\"]\n\n\
                [settings.pictures]\nscope = \"mobile\"\n\n[settings.tuning]\nshotgun = \"pc\"\n";
    let r = Recipe::parse(text).expect("parse");
    assert_eq!(r.settings.menu, "mobile");
    assert_eq!(r.settings.pictures.scope, Source::Mobile);
    assert_eq!(r.settings.pictures.weapons, Source::Auto);
    assert_eq!(r.settings.tuning.shotgun, Source::Pc);
    let back = Recipe::parse(&r.to_toml()).expect("reparse");
    assert_eq!(back, r);
    assert_ne!(back.settings, Settings::default());
}

#[test]
fn phone_rules_only_on_an_apk_base_alone() {
    use c2_mods::recipe::{load_base, Source};
    let root = tmp("rules");
    write(
        &root,
        &[
            (
                "bases/dh-apk.toml",
                b"[base]\nid = \"dh-apk\"\napk = \"sha256:00\"\n",
            ),
            ("bases/c2.toml", b"[base]\nid = \"c2\"\n"),
        ],
    );
    let deep = root.join("instances/testing/r.toml");
    let apk = Recipe::parse("[recipe]\nid = \"a\"\nbase = \"dh-apk\"\n").expect("parse");
    let b = load_base(&deep, &apk);
    assert!(b.is_some(), "bases/ found from a folder below");
    assert!(apk.mobile_rules(b.as_ref()));
    assert!(Source::Auto.mobile(apk.mobile_rules(b.as_ref())));
    assert!(!Source::Pc.mobile(true));
    let mut with_me = apk.clone();
    with_me.overlays.push(Default::default());
    assert!(!with_me.mobile_rules(b.as_ref()));
    let pc = Recipe::parse("[recipe]\nid = \"b\"\nbase = \"c2\"\n").expect("parse");
    assert!(!pc.mobile_rules(load_base(&deep, &pc).as_ref()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn recipes_found_in_subfolders_but_not_base_lists() {
    let root = tmp("find");
    write(
        &root,
        &[
            ("a.toml", b"[recipe]\nid = \"a\"\n"),
            ("instances/testing/b.toml", b"[recipe]\nid = \"b\"\n"),
            ("bases/c2.toml", b"[base]\nid = \"c2\"\n"),
            ("notes.txt", b"[recipe]\n"),
            ("broken.toml", b"[recipe\n"),
        ],
    );
    let found = c2_mods::recipe::find_recipes(&root);
    let names: Vec<_> = found
        .iter()
        .map(|p| p.strip_prefix(&root).expect("under root").to_path_buf())
        .collect();
    assert_eq!(
        names,
        [
            PathBuf::from("a.toml"),
            PathBuf::from("instances/testing/b.toml")
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The launcher's editor saves a recipe by writing it out again: every
/// recipe in the repository reads back the same.
#[test]
fn the_repository_recipes_write_out_unchanged() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../recipes");
    let found = c2_mods::recipe::find_recipes(&dir);
    assert!(!found.is_empty(), "no recipes under {}", dir.display());
    for p in found {
        let r = c2_mods::recipe::Recipe::load(&p).expect("loads");
        let again = c2_mods::recipe::Recipe::parse(&r.to_toml()).expect("reparses");
        assert_eq!(r, again, "{}", p.display());
    }
}

#[test]
fn pinned_collection_keeps_each_animals_donor_art_separate() {
    use c2_mods::recipe::{check_folder, sha256, RecipeMod};
    let root = tmp("collection-art");
    let base = root.join("base");
    let addons = root.join("imports");
    write(&base, &[
        ("HUNTDAT/_RES.TXT", b"characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Base animal'\n ai = 10\n}\n}\n"),
        ("HUNTDAT/MENU/PICS/DINO1.TGA", b"base-picture"),
    ]);
    let mut entry = RecipeMod {
        id: "imports".into(),
        loose: true,
        ..Default::default()
    };
    for name in ["Alpha", "Beta", "Gamma"] {
        let text = format!("{{\n name = '{name}'\n file = '{name}.car'\n ai = 15\n runAnim = 0\n walkAnim = 1\n}}\n");
        for (file, bytes) in [
            (format!("{name}/{name}.txt"), text.as_bytes()),
            (format!("{name}/DINO1.TGA"), name.as_bytes()),
            (format!("{name}/{name}.car"), b"synthetic-model".as_slice()),
        ] {
            write(&addons, &[(&file, bytes)]);
            entry.files.insert(file, sha256(bytes));
        }
    }
    // Unpinned neighbouring data must not enter the output.
    write(&addons, &[("Alpha/UNPINNED.CAR", b"not-selected")]);
    let mut recipe = Recipe::default();
    recipe.mods.push(entry);
    recipe.load_order.order.push("imports".into());
    let found = vec![check_folder(&recipe.mods[0], &addons, false)];
    assert!(found[0].complete());
    let v = vanilla(std::slice::from_ref(&base)).expect("base");
    let (sources, opts) = prepare(&root.join("recipe.toml"), &recipe, &found, &v).expect("prepare");
    assert_eq!(sources.len(), 3);
    let pack = build_with(&[base], sources, &opts).expect("build");
    let out = root.join("out");
    materialize(&pack, &out).expect("output");
    for (slot, expected) in ["base-picture", "Alpha", "Beta", "Gamma"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            std::fs::read(out.join(format!("HUNTDAT/MENU/PICS/DINO{}.TGA", slot + 1)))
                .expect("picture"),
            expected.as_bytes()
        );
    }
    assert!(!out.join("HUNTDAT/UNPINNED.CAR").exists());
    let _ = std::fs::remove_dir_all(root);
}
