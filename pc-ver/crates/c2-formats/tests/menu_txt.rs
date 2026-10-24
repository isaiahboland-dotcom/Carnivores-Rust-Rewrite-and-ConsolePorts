//! Hunt menu lists from synthetic `_MENU.TXT` / `.c2map` text.

use c2_formats::menu_txt::{parse_c2map, MenuData};
use c2_formats::res_txt::ResTxt;

const MENU: &str = "weapons {\n{\n name = 'Pistol'\n}\n{\n name = 'Bow'\n price = 7\n}\n}\n\
characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Small Herbivores'\n ai = 11\n}\n\
{\n name = 'Big Cats'\n ai = 10\n pic = 'huntdat\\\\menu\\\\pics\\\\cats.tga'\n}\n}\n\
prices {\n start = 40\n area = 20\n area = 90\n dino = 5\n dino = 30\n weapon = 0\n acces = 3\n}\n\
accessories {\n camo = 0.9\n}\n.\n";

#[test]
fn huntables_are_listed_by_slot_with_prices() {
    let m = MenuData::from_res(&ResTxt::parse(MENU).expect("parse"));
    assert_eq!(m.start, 40);
    // AI < 10 is not huntable; order is the file's, not the AI's.
    let names: Vec<&str> = m.huntables.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Small Herbivores", "Big Cats"]);
    assert_eq!((m.huntables[0].price, m.huntables[1].price), (5, 30));
    assert!(m.huntables[1].pic.is_some());
    // A prices line overrides the entry's own `price`; an unlisted one keeps it.
    assert_eq!((m.weapons[0].price, m.weapons[1].price), (0, 7));
    assert_eq!(m.areas.len(), 2);
    assert_eq!(
        (m.areas[1].project.as_str(), m.areas[1].number),
        ("area2", Some(2))
    );
    assert_eq!(m.accessory_prices, [3]);
    assert_eq!(m.accessory_scores, [("camo".to_string(), 0.9)]);
}

#[test]
fn c2map_descriptors_add_areas_once() {
    let src = "info\n{\n name = 'Lost Valley'\n mapfile = 'huntdat\\\\areas\\\\valley'\n price = 75\n pic = 'huntdat\\\\menu\\\\pics\\\\valley.tga'\n}\n.\n";
    let a = parse_c2map(src).expect("c2map");
    assert_eq!(a.project, "valley");
    assert_eq!(a.name.as_deref(), Some("Lost Valley"));
    assert_eq!(a.pic.as_deref(), Some("huntdat/menu/pics/valley.tga"));
    let mut m = MenuData::from_res(&ResTxt::parse(MENU).expect("parse"));
    m.add_c2map(a.clone());
    m.add_c2map(a);
    assert_eq!(m.areas.len(), 3);
    assert!(parse_c2map("info {\n name = 'x'\n}\n").is_none());
}

#[test]
fn a_start_too_small_for_any_hunt_is_the_originals_100() {
    let m = |start: i32| {
        let src = format!(
            "weapons {{\n{{\n name = 'Gun'\n}}\n}}\ncharacters {{\n{{\n name = 'Rex'\n ai = 18\n}}\n}}\n\
             prices {{\n start = {start}\n area = 2\n dino = 2\n weapon = 7\n}}\n.\n"
        );
        MenuData::from_res(&ResTxt::parse(&src).unwrap())
    };
    assert_eq!(m(5).cheapest_hunt(), 11);
    assert_eq!(m(5).start_points(), 100);
    assert_eq!(m(40).start_points(), 40);
}
