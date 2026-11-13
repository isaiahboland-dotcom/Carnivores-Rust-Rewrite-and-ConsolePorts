#![allow(clippy::expect_used)]

//! The variant editor's catalog: huntable characters by source, and maps by
//! the source that provides them, from a synthetic base game and add-on.

use c2_mods::variants::{catalog, BASE};

fn write(root: &std::path::Path, file: &str, data: &[u8]) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, data).expect("write");
}

#[test]
fn catalog_lists_huntable_characters_and_maps_by_source() {
    let root = std::env::temp_dir().join(format!("c2-variant-catalog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let base =
        "characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Para'\n ai = 10\n}\n}\n.\n";
    write(&root, "base/HUNTDAT/_RES.TXT", base.as_bytes());
    write(&root, "base/HUNTDAT/AREAS/AREA1.MAP", b"map");
    write(&root, "base/HUNTDAT/MENU/TXT/AREA1.TXT", b"Fern Valley\r\n");
    let addon = base.replace(
        "}\n.\n",
        "{\n name = 'Stegosaurus'\n ai = 18\n file = 'steg.car'\n}\n}\n.\n",
    );
    write(&root, "skins/HUNTDAT/_RES.TXT", addon.as_bytes());
    write(&root, "skins/HUNTDAT/steg.car", b"model");
    write(&root, "skins/HUNTDAT/AREAS/AREA7.MAP", b"map");

    let bases = vec![root.join("base")];
    let addons = vec![("skins".to_string(), root.join("skins"))];
    let c = catalog(&bases, &[], &addons).expect("catalog");

    let names: Vec<(&str, &str)> = c
        .dinos
        .iter()
        .map(|d| (d.source.as_str(), d.name.as_str()))
        .collect();
    assert!(names.contains(&(BASE, "Para")), "{names:?}");
    assert!(
        !names.iter().any(|(_, n)| *n == "Hunter"),
        "AI 0 is not huntable"
    );
    assert!(names.contains(&("skins", "Stegosaurus")), "{names:?}");
    assert!(
        !names.contains(&(BASE, "Stegosaurus")),
        "the add-on's character is its own source"
    );

    let maps: Vec<(&str, &str)> = c
        .maps
        .iter()
        .map(|m| (m.map.source.as_str(), m.map.area.as_str()))
        .collect();
    assert!(maps.contains(&(BASE, "area1")), "{maps:?}");
    assert!(maps.contains(&("skins", "area7")), "{maps:?}");
    let fern = c
        .maps
        .iter()
        .find(|m| m.map.area == "area1")
        .expect("area1");
    assert_eq!(fern.title, "Fern Valley");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn catalog_lists_a_standalones_kept_animals_and_maps_by_its_own_names() {
    use c2_mods::standalone::Skip;
    let root = std::env::temp_dir().join(format!("c2-variant-catalog-sa-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let base =
        "characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Para'\n ai = 10\n}\n}\n.\n";
    write(&root, "base/HUNTDAT/_RES.TXT", base.as_bytes());
    write(&root, "base/HUNTDAT/AREAS/AREA1.MAP", b"map");
    let game = "characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Para'\n ai = 10\n}\n\
{\n name = 'Spotted'\n ai = 15\n}\n}\nprices {\n area = 1\n area = 2\n dino = 1\n dino = 2\n}\n.\n";
    write(&root, "Game A/HUNTDAT/_RES.TXT", game.as_bytes());
    write(&root, "Game A/HUNTDAT/AREAS/AREA1.MAP", b"map");
    write(&root, "Game A/HUNTDAT/AREAS/AREA2.MAP", b"map");
    write(&root, "Game A/HUNTDAT/MENU/TXT/AREA2.TXT", b"Swamp\r\n");

    let skip = Skip {
        huntables: vec![1],
        areas: vec![1],
        ..Default::default()
    };
    let standalones = vec![("game-a".to_string(), root.join("Game A"), skip)];
    let c = catalog(&[root.join("base")], &standalones, &[]).expect("catalog");
    let names: Vec<(&str, &str)> = c
        .dinos
        .iter()
        .map(|d| (d.source.as_str(), d.name.as_str()))
        .collect();
    // Its own names (not the combined game's renames), without left-out ones.
    assert_eq!(names, [(BASE, "Para"), ("game-a", "Spotted")]);
    let maps: Vec<(&str, &str, &str)> = c
        .maps
        .iter()
        .map(|m| (m.map.source.as_str(), m.map.area.as_str(), m.title.as_str()))
        .collect();
    assert_eq!(
        maps,
        [(BASE, "area1", "area1"), ("game-a", "area2", "Swamp")]
    );
    let _ = std::fs::remove_dir_all(&root);
}
