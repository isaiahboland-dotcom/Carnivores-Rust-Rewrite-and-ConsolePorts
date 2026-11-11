#![allow(clippy::expect_used)]

use c2_mods::recipe::Recipe;
use c2_mods::variants::{validate, AreaId};

const RECIPE: &str = r#"
[recipe]
id = "variant-test"

[[variants]]
id = "rex"
menu_owner = "classic"

[[variants.members]]
id = "classic"
source = "classic-rex"
character = "Tyrannosaurus"
weight = 75

[[variants.members]]
id = "striped"
source = "striped-rex"
character = "Tyrannosaurus"
weight = 25
maps = [{ source = "base", area = "area1" }]
map_weights = [{ map = { source = "base", area = "area1" }, weight = 75 }]
"#;

fn area(n: usize) -> AreaId {
    AreaId {
        source: "base".into(),
        area: format!("area{n}"),
    }
}

#[test]
fn recipe_retains_variant_rules_and_ordinary_recipes_omit_them() {
    let recipe = Recipe::parse(RECIPE).expect("parse");
    validate(&recipe.variants).expect("valid");
    assert_eq!(
        Recipe::parse(&recipe.to_toml()).expect("round trip"),
        recipe
    );
    assert!(!Recipe::default().to_toml().contains("variants"));
    assert!(Recipe::parse("[recipe]\nid = 'old'\n")
        .expect("old recipe")
        .variants
        .is_empty());
}

#[test]
fn eligibility_precedes_overrides_and_normalization() {
    let mut group = Recipe::parse(RECIPE).expect("parse").variants.remove(0);
    assert_eq!(
        group.probabilities(&area(1)).expect("weights"),
        vec![0.5, 0.5]
    );
    assert_eq!(
        group.probabilities(&area(2)).expect("weights"),
        vec![1.0, 0.0]
    );
    group.members[1].map_weights.clear();
    assert_eq!(
        group.probabilities(&area(1)).expect("weights"),
        vec![0.75, 0.25]
    );
    group.members[0].maps = Some(Vec::new());
    assert_eq!(
        group.probabilities(&area(2)).expect("weights"),
        vec![0.0, 0.0]
    );
    group.members[0].maps = None;
    group.members[0].weight = 0.0;
    assert_eq!(
        group.probabilities(&area(2)).expect("weights"),
        vec![0.0, 0.0]
    );
}

#[test]
fn weight_defaults_and_empty_map_lists_survive_serialization() {
    let recipe = Recipe::parse(&RECIPE.replace("weight = 75\n", "")).expect("parse");
    assert_eq!(recipe.variants[0].members[0].weight, 1.0);
    let mut recipe = recipe;
    recipe.variants[0].members[1].maps = Some(Vec::new());
    recipe.variants[0].members[1].map_weights.clear();
    let back = Recipe::parse(&recipe.to_toml()).expect("round trip");
    assert_eq!(back.variants[0].members[1].maps, Some(Vec::new()));
    assert_eq!(back.variants[0].members[0].maps, None);
}

#[test]
fn invalid_and_ambiguous_rules_fail_validation() {
    let group = Recipe::parse(RECIPE).expect("parse").variants.remove(0);
    for weight in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut invalid = group.clone();
        invalid.members[0].weight = weight;
        assert!(validate(&[invalid]).is_err());
        let mut invalid = group.clone();
        invalid.members[1].map_weights[0].weight = weight;
        assert!(validate(&[invalid]).is_err());
    }
    let mut invalid = group.clone();
    invalid.members[1].source = invalid.members[0].source.clone();
    invalid.members[1].character = " ' TYRANNOSAURUS ' ".into();
    assert!(
        validate(&[invalid]).is_err(),
        "canonical duplicate identity"
    );
    let mut invalid = group.clone();
    invalid.menu_owner = "missing".into();
    assert!(validate(&[invalid]).is_err());
    let mut invalid = group.clone();
    let duplicate = invalid.members[1].map_weights[0].clone();
    invalid.members[1].map_weights.push(duplicate);
    assert!(validate(&[invalid]).is_err());
    let mut invalid = group.clone();
    invalid.members[1].maps = Some(Vec::new());
    assert!(validate(&[invalid]).is_err(), "override on excluded map");
    assert!(
        validate(&[group.clone(), group]).is_err(),
        "duplicate group IDs"
    );
}

#[test]
fn huge_weights_normalize_without_overflow() {
    let mut group = Recipe::parse(RECIPE).expect("parse").variants.remove(0);
    for member in &mut group.members {
        member.weight = f64::MAX;
        member.map_weights.clear();
    }
    assert_eq!(
        group.probabilities(&area(1)).expect("weights"),
        vec![0.5, 0.5]
    );
}

#[test]
fn misspelled_configuration_is_rejected_instead_of_ignored() {
    assert!(Recipe::parse(&RECIPE.replace("weight = 25", "wieght = 25")).is_err());
}

#[test]
fn building_retains_same_name_characters_and_private_assets() {
    use c2_mods::variants::{Packed, PATH};
    use c2_mods::{build_with, load_mod, vanilla};
    let root = std::env::temp_dir().join(format!("c2-variant-build-{}", std::process::id()));
    let write = |file: &str, data: &[u8]| {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, data).expect("write");
    };
    let base =
        "characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Para'\n ai = 10\n}\n}\n.\n";
    write("base/HUNTDAT/_RES.TXT", base.as_bytes());
    write("base/HUNTDAT/AREAS/AREA1.MAP", b"map");
    let addon = base.replace(
        "}\n.\n",
        "{\n name = 'Tyrannosaurus'\n ai = 18\n file = 'rex.car'\n}\n}\n.\n",
    );
    for (id, model) in [
        ("classic-rex", b"classic".as_slice()),
        ("striped-rex", b"striped".as_slice()),
    ] {
        write(&format!("{id}/HUNTDAT/_RES.TXT"), addon.as_bytes());
        write(&format!("{id}/HUNTDAT/rex.car"), model);
        write(&format!("{id}/HUNTDAT/SOUNDFX/CALL9_A.WAV"), model);
        write(&format!("{id}/HUNTDAT/MENU/DINO9.TGA"), model);
    }
    let bases = vec![root.join("base")];
    let vfs = vanilla(&bases).expect("base");
    let sources: Vec<_> = ["classic-rex", "striped-rex"]
        .iter()
        .map(|id| load_mod(&vfs, &root.join(id)).expect("source"))
        .collect();
    let mut recipe = c2_mods::recipe::new_recipe("test", "test", "", &sources).expect("pin recipe");
    recipe.variants = Recipe::parse(RECIPE).expect("rules").variants;
    recipe.resolve.push(c2_mods::recipe::Resolve {
        path: "HUNTDAT/rex.car".into(),
        winner: "classic-rex".into(),
        ..Default::default()
    });
    let locations = c2_mods::recipe::locate(&recipe, std::slice::from_ref(&root));
    let (sources, opts) =
        c2_mods::recipe::prepare(&root, &recipe, &locations, &vfs).expect("prepare");
    assert_eq!(opts.variants, recipe.variants);
    let pack = build_with(&bases, sources, &opts).expect("build");
    let data: Packed =
        toml::from_str(&String::from_utf8(pack.vfs.read(PATH).expect("side file")).expect("utf8"))
            .expect("packed rules");
    let group = &data.groups[0];
    assert_eq!(group.members.len(), 2);
    assert_ne!(group.members[0].index, group.members[1].index);
    assert_ne!(group.members[0].token, group.members[1].token);
    for (member, expected) in group
        .members
        .iter()
        .zip([b"classic".as_slice(), b"striped".as_slice()])
    {
        assert_eq!(
            pack.vfs
                .read(&format!("HUNTDAT/{}", member.file))
                .expect("private model"),
            expected
        );
        let block = &pack
            .merged
            .res
            .section("characters")
            .expect("characters")
            .blocks[member.index];
        assert_eq!(
            block.get(&["menuSlot"]),
            Some(group.slot.to_string().as_str())
        );
        assert_eq!(
            block.get(&["callNo"]),
            Some(group.slot.to_string().as_str())
        );
    }
    let menu = c2_formats::res_txt::ResTxt::parse(
        &String::from_utf8(pack.vfs.read("HUNTDAT/_MENU.TXT").expect("menu")).expect("utf8"),
    )
    .expect("parse menu");
    let menu = c2_formats::menu_txt::MenuData::from_res(&menu);
    assert_eq!(
        menu.huntables
            .iter()
            .filter(|h| h.name == "Tyrannosaurus")
            .count(),
        1
    );
    assert_eq!(
        pack.vfs
            .read(&format!("HUNTDAT/SOUNDFX/CALL{}_A.WAV", group.slot + 1))
            .expect("owner call"),
        b"classic"
    );
    assert_eq!(
        pack.vfs
            .read(&format!("HUNTDAT/MENU/DINO{}.TGA", group.slot + 1))
            .expect("owner art"),
        b"classic"
    );
    assert!(c2_mods::standalone::leave_out::layer(
        &pack.vfs,
        &c2_mods::standalone::Skip {
            huntables: vec![1],
            ..Default::default()
        }
    )
    .is_err());
    // Also preserve pure model replacements, with no text diff to identify
    // the otherwise inherited base character.
    write("base/HUNTDAT/_RES.TXT", addon.as_bytes());
    write("base/HUNTDAT/rex.car", b"vanilla");
    let vfs = vanilla(&bases).expect("base with Rex");
    let sources: Vec<_> = ["classic-rex", "striped-rex"]
        .iter()
        .map(|id| load_mod(&vfs, &root.join(id)).expect("model replacement"))
        .collect();
    assert!(sources.iter().all(|s| s.res_changes.is_empty()));
    let pack = build_with(&bases, sources, &opts).expect("build model-only variants");
    let data: Packed =
        toml::from_str(&String::from_utf8(pack.vfs.read(PATH).expect("side file")).expect("utf8"))
            .expect("packed rules");
    assert_eq!(data.groups[0].members.len(), 2);
    assert_eq!(data.groups[0].owner, 2);
    assert_eq!(
        pack.vfs
            .read(&format!("HUNTDAT/{}", data.groups[0].members[1].file))
            .expect("replacement model"),
        b"striped"
    );
    std::fs::remove_dir_all(root).expect("cleanup");
}
