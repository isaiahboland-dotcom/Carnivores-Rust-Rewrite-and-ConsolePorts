use c2_formats::res_txt::{c_atof, c_atoi, ResTxt};

const SAMPLE: &str = "\r\nweapons {\r\n{\r\n name  = 'Pistol'\r\n file  = 'pistol.car'\r\n power = 2\r\n prec  = 0.8\r\n trace = 6\r\n hipx = 40\r\n}\r\n\r\n}  //==== end of weapons =========//\r\n\r\ncharacters {\r\n{\r\n name = 'Maiasaura'\r\n ai = 12\r\n hearK = 0.7\r\n danger = FALSE\r\n scaleA = 400\r\n}\r\n{\r\n name = 'Hunter'\r\n}\r\n}\r\n\r\nprices {\r\n start = 100\r\n area = 20\r\n area = 50\r\n acces = 0\r\n}\r\n.\r\ngarbage after the end {\r\n";

#[test]
fn parses_sections_blocks_and_prices() {
    let r = ResTxt::parse(SAMPLE).expect("parse");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let d = r.to_data();

    assert_eq!(d.weapons.len(), 1);
    let w = &d.weapons[0];
    assert_eq!(w.name.as_deref(), Some("Pistol"));
    assert_eq!(w.file.as_deref(), Some("pistol.car"));
    assert_eq!(w.power, Some(2.0));
    assert_eq!(w.trace, Some(6));
    assert_eq!(w.extra.len(), 1, "unknown mod key kept");

    assert_eq!(d.characters.len(), 2);
    let c = &d.characters[0];
    // A name containing "ai" must not clobber the ai field (original strstr bug).
    assert_eq!(c.ai, Some(12));
    assert_eq!(c.hear, Some(0.7));
    assert!(c.danger, "original treats any danger line as true");
    assert_eq!(c.scale_a, Some(400));
    assert_eq!(d.characters[1].ai, None);

    let p = d.prices.expect("prices");
    assert_eq!(p.start, Some(100));
    assert_eq!(p.area, vec![20, 50]);
    assert_eq!(p.accessory, vec![0]);
}

#[test]
fn write_then_parse_is_stable() {
    let r = ResTxt::parse(SAMPLE).expect("parse");
    let again = ResTxt::parse(&r.write()).expect("reparse");
    assert_eq!(again.sections, r.sections);
}

#[test]
fn c_number_semantics() {
    assert_eq!(c_atoi(" 12abc"), 12);
    assert_eq!(c_atoi("-7"), -7);
    assert_eq!(c_atoi("TRUE"), 0);
    assert_eq!(c_atoi("99999999999"), i32::MAX);
    assert_eq!(c_atof(" 0.85  "), 0.85);
    assert_eq!(c_atof("1.5e2x"), 150.0);
    assert_eq!(c_atof("1e"), 1.0);
    assert_eq!(c_atof("."), 0.0);
    assert_eq!(c_atof("255.0.0"), 255.0);
}

#[test]
fn garbage_does_not_panic() {
    for s in [
        "{",
        "}",
        "x {\n{\n=\n",
        "a {\n{\n{\n}\n",
        "'\n'=\n",
        "\u{0}\u{ff}",
    ] {
        let _ = ResTxt::parse(s);
    }
}

#[test]
fn blocks_outside_a_section_are_kept_and_ignored() {
    // Some modded engines add their own blocks between the sections.
    let text = "weapons {\n{\n name = 'Gun'\n}\n}\n\n{\n tree = 9\n}\n\ncharacters {\n{\n name = 'Rex'\n}\n}\n.\n";
    let r = ResTxt::parse(text).expect("parse");
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    let d = r.to_data();
    assert_eq!(d.weapons.len(), 1);
    assert_eq!(d.characters.len(), 1);
    let again = ResTxt::parse(&r.write()).expect("reparse");
    assert_eq!(again.sections, r.sections);
}

#[test]
fn nested_blocks_keep_their_place() {
    // The Modder's Edition dialect nests named blocks inside entries.
    let text = "characters {\n{\n name = 'Moschops'\n ai = 1\nspawninfo{\n spawngroup = 0\n}\n health = 2\n overwrite area2 time0 {\n  health = 3\n  idlegroup{\n   anim = 4\n  }\n }\n}\n}\nspawntable {\n spawngroup{\n  spawnmax = 6\n  region{\n   xmin = 1\n  }\n }\n spawngroup{\n  spawnmax = 4\n }\n}\n.\n";
    let r = ResTxt::parse(text).expect("parse");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let c = &r.section("characters").expect("characters").blocks[0];
    assert_eq!(c.get(&["health"]), Some("2"));
    assert_eq!(c.subs.len(), 2);
    assert_eq!((c.subs[0].name.as_str(), c.subs[0].at), ("spawninfo", 2));
    assert_eq!(c.subs[1].name, "overwrite area2 time0");
    assert_eq!(c.subs[1].block.subs_named("idlegroup").count(), 1);
    let t = r.section("spawntable").expect("spawntable");
    assert_eq!(t.subs.len(), 2);
    assert_eq!(t.subs[0].block.subs[0].name, "region");
    let again = ResTxt::parse(&r.write()).expect("reparse");
    assert_eq!(again.sections, r.sections);
}
