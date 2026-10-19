//! The Modder's Edition `overwrite` / `addition` rules, on synthetic text.

use c2_formats::mee::{applies, resolve, Hunt};
use c2_formats::res_txt::ResTxt;

fn hunt(area: u32, time: u8, picked: u32) -> Hunt {
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
    // Substring rule: "area10" contains "area1"; area 10 is "area0".
    assert!(applies("overwrite area10", &hunt(1, 1, 0)));
    assert!(applies("overwrite area0", &hunt(10, 1, 0)));
    // All categories must match.
    assert!(!applies("overwrite area1 time2", &hunt(1, 1, 0)));
    assert!(applies("overwrite area1 time2", &hunt(1, 2, 0)));
    // char<i> = menu slot i picked.
    assert!(applies("overwrite char0 char3", &hunt(1, 1, 0b1000)));
    assert!(!applies("overwrite char0", &hunt(1, 1, 0b10)));
    // The trophy room takes every block.
    let trophy = Hunt {
        area: None,
        time: 1,
        picked: 0,
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
