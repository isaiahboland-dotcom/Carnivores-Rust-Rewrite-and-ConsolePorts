//! Play Vanilla builds (`play --recipe R --classic`): what the classic pack
//! holds, and that building it leaves the enhanced pack and the recipe as
//! they were. Synthetic data only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::mods_cli::{build_recipe, recipe_pack_dir, CLASSIC_SUFFIX};

const RES: &str = "weapons {\r\n{\r\n name = 'Pistol'\r\n power = 1\r\n}\r\n}\r\n\
                   prices {\r\n start = 100\r\n}\r\n.\r\n";
const MENU: &str = "menu {\r\n}\r\n.\r\n";

/// The game folder, a recipe for it with every enhanced section, and the
/// same recipe with them taken out by hand.
fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("c2rs-classic-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let h = dir.join("game/HUNTDAT");
    std::fs::create_dir_all(&h).expect("game");
    std::fs::write(h.join("_RES.TXT"), RES).expect("res");
    std::fs::write(h.join("_MENU.TXT"), MENU).expect("menu");
    std::fs::write(h.join("X.CAR"), b"model").expect("car");
    let plain = "[recipe]\nid = \"sa\"\n\n\
                 [[settings.weapon_restrictions]]\nname = \"Pistol\"\nallow_trophy = false\n\n\
                 [settings.rules]\ntimes = [\"day\", \"night\"]\nhidden = [\"Velociraptor\"]\n";
    let advanced = format!(
        "{plain}\n[settings.rules.locks]\nteleport = true\nsecret_dinos = 3\n\n\
         [settings.predation]\nenabled = true\n\n[settings.flee]\nenabled = true\n\n\
         [settings.behavior.awareness]\nenabled = true\n"
    );
    let recipe = dir.join("sa.toml");
    std::fs::write(&recipe, advanced).expect("recipe");
    let plain_recipe = dir.join("plain/sa.toml");
    std::fs::create_dir_all(dir.join("plain")).expect("plain");
    std::fs::write(&plain_recipe, plain).expect("plain recipe");
    (dir.clone(), recipe, plain_recipe, dir.join("game"))
}

fn build(recipe: &Path, game: &Path, out: &Path, classic: bool) {
    build_recipe(recipe, &[game.to_path_buf()], &[], None, out, true, classic)
        .unwrap_or_else(|e| panic!("building {}: {e:#}", out.display()));
}

/// Every file under `dir` and its bytes.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).expect("read dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).expect("under root");
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.insert(rel, std::fs::read(&p).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn the_classic_pack_leaves_out_the_enhanced_rules_only() {
    let (dir, recipe, _, game) = fixture("contents");
    let (enhanced, classic) = (
        dir.join("packs/recipe-sa"),
        dir.join("packs/recipe-sa.classic"),
    );
    build(&recipe, &game, &enhanced, false);
    build(&recipe, &game, &classic, true);
    let (e, c) = (tree(&enhanced), tree(&classic));
    let _ = std::fs::remove_dir_all(&dir);

    let file = |t: &BTreeMap<String, Vec<u8>>, p: &str| t.get(p).cloned();
    for p in [
        c2_formats::predation::PATH,
        c2_formats::flee::PATH,
        c2_formats::behavior::PATH,
    ] {
        assert!(e.contains_key(p), "the enhanced pack has {p}");
        assert!(!c.contains_key(p), "the classic pack has {p}");
    }
    // The rules file: the times of day and hidden animals, no locks.
    let rules =
        |t| c2_formats::rules::Rules::parse(&file(t, c2_formats::rules::PATH).unwrap_or_default());
    let (er, cr) = (rules(&e), rules(&c));
    assert!(!er.locks.is_empty());
    assert!(cr.locks.is_empty(), "{cr:?}");
    assert!(cr.huntable.is_empty() && cr.secrets.is_empty());
    assert_eq!((cr.times, &cr.hidden), (er.times, &er.hidden));
    assert_eq!(cr.hidden, ["Velociraptor"]);
    // The game itself is the same, weapon restrictions included.
    for p in ["HUNTDAT/_RES.TXT", "HUNTDAT/_MENU.TXT", "HUNTDAT/X.CAR"] {
        assert!(file(&c, p).is_some(), "{p}");
        assert_eq!(file(&c, p), file(&e, p), "{p}");
    }
    let res = String::from_utf8_lossy(&c["HUNTDAT/_RES.TXT"]).into_owned();
    assert!(res.contains("c2rs_allow_trophy"), "{res}");
}

#[test]
fn a_classic_build_changes_neither_the_enhanced_pack_nor_the_recipe() {
    let (dir, recipe, _, game) = fixture("untouched");
    let (enhanced, classic) = (
        dir.join("packs/recipe-sa"),
        dir.join("packs/recipe-sa.classic"),
    );
    build(&recipe, &game, &enhanced, false);
    let (pack_before, recipe_before) = (tree(&enhanced), std::fs::read(&recipe).expect("read"));
    build(&recipe, &game, &classic, true);
    let (pack_after, recipe_after) = (tree(&enhanced), std::fs::read(&recipe).expect("read"));
    // Back to the enhanced game: its pack is still current, and kept.
    build(&recipe, &game, &enhanced, false);
    let pack_again = tree(&enhanced);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(recipe_before, recipe_after, "the recipe file was written");
    assert_eq!(pack_before, pack_after, "the enhanced pack changed");
    assert_eq!(pack_before, pack_again);
}

/// A classic build is the build of the recipe without its enhanced sections:
/// the same files, byte for byte, so a scenario played on either gives the
/// same state hashes.
#[test]
fn a_classic_build_matches_the_recipe_without_advanced_sections() {
    let (dir, recipe, plain, game) = fixture("plain");
    build(&recipe, &game, &dir.join("classic"), true);
    build(&plain, &game, &dir.join("plain-out"), false);
    let (c, p) = (tree(&dir.join("classic")), tree(&dir.join("plain-out")));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(c.contains_key(c2_mods::pack::KEY_FILE));
    assert_eq!(c, p);
}

#[test]
fn the_classic_pack_has_its_own_folder() {
    let enhanced = recipe_pack_dir(Path::new("no-such-recipe.toml"), false);
    let classic = recipe_pack_dir(Path::new("no-such-recipe.toml"), true);
    assert_ne!(enhanced, classic);
    assert_eq!(enhanced.parent(), classic.parent());
    let name = classic
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    assert_eq!(
        name.as_deref(),
        Some(&*format!("recipe-recipe{CLASSIC_SUFFIX}"))
    );
    // No slug ends in the suffix, so no recipe's own folder can be it.
    assert!(!c2_mods::manifest::slug("x.classic").ends_with(CLASSIC_SUFFIX));
}
