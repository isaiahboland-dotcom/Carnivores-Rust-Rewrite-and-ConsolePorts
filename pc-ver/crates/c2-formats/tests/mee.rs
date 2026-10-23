//! The Modder's Edition `overwrite` / `addition` rules, on synthetic text.

use c2_formats::mee::{applies, is_dialect, resolve, Hunt, Tables};
use c2_formats::res_txt::ResTxt;

fn hunt(area: u32, time: u8, picked: u128) -> Hunt {
    let picked = c2_formats::bits::Bits::from(picked);
    Hunt {
        area: Some(area),
        time,
        picked,
    }
}

#[test]
fn conditions_match_like_the_engine() {
    assert!(applies("overwrite area2 area3", &hunt(3, 1, 0)));
    assert!(!applies("overwrite area2 area3", &hunt(4, 1, 0)));
    // Area 10 is "area0" (the engine's last-digit rule); numbers count
    // whole, so combined games' areas 11 and up don't collide.
    assert!(applies("overwrite area0", &hunt(10, 1, 0)));
    assert!(applies("overwrite area10", &hunt(10, 1, 0)));
    assert!(!applies("overwrite area10", &hunt(1, 1, 0)));
    assert!(applies("overwrite area23 area2", &hunt(23, 1, 0)));
    assert!(!applies("overwrite area23", &hunt(3, 1, 0)));
    // All categories must match.
    assert!(!applies("overwrite area1 time2", &hunt(1, 1, 0)));
    assert!(applies("overwrite area1 time2", &hunt(1, 2, 0)));
    // char<i> = menu slot i picked.
    assert!(applies("overwrite char0 char3", &hunt(1, 1, 0b1000)));
    assert!(!applies("overwrite char0", &hunt(1, 1, 0b10)));
    // Slots past 9 (add-ons given their own): `char1` doesn't match `char10`.
    assert!(applies("overwrite char10", &hunt(1, 1, 1 << 10)));
    assert!(!applies("overwrite char10", &hunt(1, 1, 0b10)));
    assert!(!applies("overwrite char1", &hunt(1, 1, 1 << 10)));
    // The trophy room takes every block.
    let trophy = Hunt {
        area: None,
        time: 1,
        picked: c2_formats::bits::Bits::NONE,
    };
    assert!(applies("overwrite area5 time2 char9", &trophy));
}

const RES: &str =
    "characters {\n{\n name = 'Para'\n file = 'para.car'\n lookAnim = 1\n lookAnim = 2\n\
spawninfo{\n spawngroup = 1\n}\n\
 overwrite time2 {\n  file = 'para_night.car'\n  lookAnim = 5\n }\n\
 overwrite char0 {\n  spawninfo{\n   spawngroup = 2\n  }\n }\n\
 addition area3 {\n  spawninfo{\n   spawngroup = 7\n  }\n }\n\
}\n}\n.\n";

fn para(h: &Hunt) -> c2_formats::res_txt::Block {
    let r = ResTxt::parse(RES).unwrap_or_default();
    r.section("characters")
        .and_then(|s| s.blocks.first())
        .map(|b| resolve(b, h))
        .unwrap_or_default()
}

fn groups(b: &c2_formats::res_txt::Block) -> Vec<&str> {
    b.subs_named("spawninfo")
        .filter_map(|s| s.get(&["spawngroup"]))
        .collect()
}

#[test]
fn overwrite_replaces_and_addition_adds() {
    // Day, nothing picked, area 1: no conditional block applies.
    let b = para(&hunt(1, 1, 0));
    assert_eq!(b.get(&["file"]), Some("'para.car'"));
    assert_eq!(b.all("lookAnim").collect::<Vec<_>>(), ["1", "2"]);
    assert_eq!(groups(&b), ["1"]);
    assert!(b.subs.iter().all(|s| !s.name.contains("overwrite")));
    // Night: the file is set again, the lookAnim list replaced.
    let b = para(&hunt(1, 2, 0));
    assert_eq!(b.get(&["file"]), Some("'para_night.car'"));
    assert_eq!(b.all("lookAnim").collect::<Vec<_>>(), ["5"]);
    // Slot 0 picked: its spawn info replaces the old one.
    assert_eq!(groups(&para(&hunt(1, 1, 1))), ["2"]);
    // Area 3: one more, kept alongside.
    assert_eq!(groups(&para(&hunt(3, 1, 1))), ["2", "7"]);
}

const TABLES: &str = "spawntable {\n spawngroup{\n  spawnmax = 6\n  spawnmin = 2\n  spawnrate = 0.5\n  moveForward = TRUE\n }\n\
 spawngroup{\n  spawnmax = 3\n  region{\n   xmin = 100\n   xmax = 200\n   ymin = 300\n   ymax = 400\n  }\n  overwrite area2 {\n   region{\n    xmin = 1\n   }\n  }\n }\n}\n\
packtable {\n packgroup{\n  packMax = 4\n  packMin = 2\n  spawninfo{\n   spawngroup = 1\n   spawnratio = 2\n  }\n }\n}\n\
characters {\n{\n name = 'Hunter'\n}\n{\n name = 'Raptor'\n packinfo{\n  group = 0\n  ratio = 1\n }\n}\n\
{\n name = 'Fish'\n spawngroup{\n  spawnmax = 9\n }\n}\n{\n name = 'Moschops'\n spawninfo{\n  spawngroup = 0\n  spawnratio = 1\n }\n}\n}\n.\n";

#[test]
fn tables_link_groups_packs_and_characters() {
    let r = ResTxt::parse(TABLES).expect("parse");
    let t = Tables::read(&r, &hunt(1, 1, 0));
    // Two table groups, then the fish's inline one.
    assert_eq!(t.spawn_groups.len(), 3);
    let g = &t.spawn_groups[0];
    assert_eq!((g.min, g.max, g.move_forward), (2, 6, true));
    // No region given: the whole map.
    assert_eq!(g.regions[0].xmax, 988);
    assert_eq!(t.spawn_groups[1].regions[0].xmin, 100);
    assert_eq!(t.pack_groups[0].spawn[0].group, 1);
    assert_eq!(t.pack_groups[0].spawn[0].ratio, 2.0);
    assert_eq!(t.characters.len(), 4);
    assert_eq!(t.characters[1].packs[0].group, 0);
    assert_eq!(t.characters[2].spawn[0].group, 2);
    assert_eq!(t.characters[2].spawn[0].ratio, 1.0);
    assert_eq!(t.characters[3].spawn[0].group, 0);
    // On area 2 the region is overwritten.
    let t2 = Tables::read(&r, &hunt(2, 1, 0));
    assert_eq!(t2.spawn_groups[1].regions[0].xmin, 1);
    assert!(is_dialect(&r));
}

#[test]
fn area_numbers_come_from_the_map_name() {
    use c2_formats::mee::area_of;
    assert_eq!(area_of("AREA3"), Some(3));
    assert_eq!(area_of("area10"), Some(10));
    assert_eq!(area_of("TROPHY"), None);
    assert_eq!(area_of("external"), Some(6));
    assert_eq!(area_of("valley"), Some(0));
    assert!(!applies("overwrite area1", &hunt(0, 1, 0)));
}
