//! The field manual's area surveys and armoury files: the index's lists,
//! what the files say, and links between files (an area's animal opens its
//! file; Back returns to the area).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::super::armoury::{self, Arm};
use super::super::beep::Beep;
use super::super::specimen::RETRIEVE_S;
use super::super::survey;
use super::super::terminal::*;
use super::super::*;
use super::specimens::{db, open_manual, text};
use super::{catalog, rules};
use crate::record::{HuntDone, Kill, Record};

/// Ravan (area 1, `AREA2`) issues the Velociraptor and the shotgun, at
/// night; Fern Valley is unrestricted.
const RAVAN: usize = 1;

#[test]
fn the_index_lists_areas_animals_and_weapons() {
    let mut t = db();
    open_manual(&mut t);
    assert_eq!(t.index_tab(), Tab::Targets, "animals first, as before");
    assert_eq!(t.act_beep(Action::Left), Some(Beep::Tick));
    assert_eq!(t.index_tab(), Tab::Area);
    t.act(Action::Down);
    t.act(Action::Down);
    assert_eq!(t.index_row(), 1, "two areas");
    t.act(Action::Left);
    assert_eq!(t.index_tab(), Tab::Arms, "wraps round");
    t.act(Action::Tab(Tab::Gear));
    assert_eq!(t.index_tab(), Tab::Arms, "gear has no files");
    t.act(Action::Tab(Tab::Area));
    assert_eq!(t.index_row(), 1, "each list keeps its row");
    t.act(Action::Confirm);
    assert_eq!((t.file_kind(), t.file()), (Tab::Area, RAVAN));
    assert!(t.retrieve_log()[0].contains("SURVEY ARCHIVE"));
    assert!(t.retrieve_log()[1].ends_with("RAVAN"));
    assert!(t.retrieve_log().iter().any(|l| l.contains("AERIAL")));
    t.specimen_models = true;
    assert!(t.retrieve_log().iter().any(|l| l.contains("TERRAIN MODEL")));
    t.step(RETRIEVE_S);
    t.act(Action::Back);
    assert_eq!((t.screen, t.index_tab()), (Screen::Specimens, Tab::Area));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(0));
    assert!(t.retrieve_log()[0].contains("ARMOURY"));
    assert!(t.retrieve_log().iter().any(|l| l.contains("SCHEMATIC")));
}

#[test]
fn the_setup_opens_area_and_weapon_files_too() {
    let mut t = db();
    t.act(Action::Confirm); // HUNT SETUP, on the area tab
    t.act(Action::Down);
    t.act(Action::Inspect);
    assert_eq!(
        (t.screen, t.file_kind(), t.file(), t.file_from()),
        (Screen::Specimen, Tab::Area, 1, Screen::Setup)
    );
    t.act(Action::Back);
    assert_eq!((t.screen, t.tab), (Screen::Setup, Tab::Area));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Inspect);
    assert_eq!((t.file_kind(), t.file()), (Tab::Arms, 0));
}

#[test]
fn an_areas_animal_opens_its_file_and_back_returns_to_the_area() {
    // Fern Valley issues only the pistol, so the shotgun is Ravan's alone.
    let mut r = rules();
    if let Some(d) = &mut r.dhc {
        d.islands.push(Island {
            area: "AREA1".into(),
            weapons: vec!["Pistol".into()],
            ..Island::default()
        });
    }
    let mut t = Terminal::new(catalog(), r, 100, false);
    t.specimen_db = true;
    open_manual(&mut t);
    t.act(Action::Tab(Tab::Area));
    t.act(Action::Pick(RAVAN));
    assert!(t.links().is_empty() || t.retrieving());
    t.act(Action::Skip);
    assert_eq!(t.links(), vec![(Tab::Targets, 1), (Tab::Arms, 1)]);
    // The arrows move between the links; Enter opens the focused one.
    assert_eq!(t.act_beep(Action::Down), Some(Beep::Tick));
    assert_eq!(t.file_link(), Some(0));
    assert_eq!(t.act_beep(Action::Confirm), Some(Beep::Select));
    assert_eq!((t.file_kind(), t.file()), (Tab::Targets, 1));
    assert!(t.retrieving(), "a new file is retrieved");
    t.act(Action::Skip);
    assert_eq!(t.act_beep(Action::Back), Some(Beep::Back));
    assert_eq!(
        (t.screen, t.file_kind(), t.file()),
        (Screen::Specimen, Tab::Area, RAVAN)
    );
    assert!(!t.retrieving(), "the area shows again at once");
    assert_eq!(t.file_link(), Some(0), "with its link still focused");
    // A click opens a link; its file links back to the area, which is
    // shown again rather than stacked twice.
    t.act(Action::Pick(1));
    t.act(Action::Skip);
    assert_eq!((t.file_kind(), t.file()), (Tab::Arms, 1));
    assert_eq!(t.links(), vec![(Tab::Area, RAVAN)]);
    t.act(Action::Pick(0));
    assert_eq!(t.files().len(), 1);
    assert_eq!((t.file_kind(), t.file()), (Tab::Area, RAVAN));
    t.act(Action::Back);
    assert_eq!(t.screen, Screen::Specimens, "the first file goes back home");
}

#[test]
fn an_unrestricted_area_has_no_links_and_enter_closes_it() {
    let mut t = db();
    open_manual(&mut t);
    t.act(Action::Tab(Tab::Area));
    t.act(Action::Pick(0));
    t.act(Action::Skip);
    assert!(t.links().is_empty());
    t.act(Action::Down);
    assert_eq!(t.file_link(), None);
    t.act(Action::Pick(0));
    assert_eq!(t.screen, Screen::Specimen, "no link to pick");
    t.act(Action::Confirm);
    assert_eq!(t.screen, Screen::Specimens);
}

#[test]
fn a_survey_lists_what_the_island_issues_or_all() {
    let (cat, r) = (catalog(), rules());
    let rec = Record {
        hunts: vec![
            HuntDone {
                map: "area2".into(),
                date: 1,
            },
            HuntDone {
                map: "AREA2".into(),
                date: 2,
            },
        ],
        ..Record::default()
    };
    let t = text(&survey::lines(&cat, &r, RAVAN, 2, Some(&rec)));
    for want in [
        "PERMIT 50 PTS",
        "TIMES NIGHT",
        "LICENSED SPECIMENS",
        "0> VELOCIRAPTOR",
        "TOTAL DINOSAURS UNKNOWN: 2",
        "AUTHORISED ARMS",
        "1> SHOTGUN",
        "HUNTS 2 COMPLETED",
    ] {
        assert!(t.iter().any(|l| l == want), "{want} missing from {t:?}");
    }
    let open = text(&survey::lines(&cat, &r, 0, 0, None));
    assert_eq!(open.iter().filter(|l| *l == "ALL").count(), 2, "{open:?}");
    assert!(!open.iter().any(|l| l.starts_with("TIMES")), "every time");
    assert!(open.iter().any(|l| l == "NO HUNTER SIGNED IN"));
    assert!(open.iter().any(|l| l == "TOTAL DINOSAURS UNKNOWN: 0"));
    // An island that issues no weapon says so.
    let mut none = rules();
    if let Some(d) = &mut none.dhc {
        d.islands[0].weapons = vec!["Harpoon".into()];
    }
    let t = text(&survey::lines(&cat, &none, RAVAN, 0, None));
    assert!(t.iter().any(|l| l == "NONE AUTHORISED"), "{t:?}");
    assert!(survey::lines(&cat, &r, 9, 0, None).is_empty());
}

#[test]
fn unknown_species_are_the_unlisted_ones_counted_once() {
    let cat = catalog();
    let appear: Vec<String> = [
        "Velociraptor",
        "Dimorphodon",
        "dimorphodon ",
        "Spinosaurus",
        "",
    ]
    .iter()
    .map(|s| String::from(*s))
    .collect();
    assert_eq!(survey::unknown(&cat, &appear), 2);
    assert_eq!(survey::unknown(&cat, &[]), 0);
}

fn kill(weapon: &str, range: f32) -> Kill {
    Kill {
        animal: "Velociraptor".into(),
        weapon: weapon.into(),
        range,
        ..Kill::default()
    }
}

#[test]
fn an_armoury_file_names_its_restrictions_and_areas() {
    let (cat, r) = (catalog(), rules());
    let harpoon = Arm {
        rounds: 8,
        magazine: 2,
        pellets: 1,
        scope: true,
        harpoon: true,
        tranq: false,
        trophy: false,
    };
    let rec = Record {
        kills: vec![
            kill("shotgun", 12.0),
            kill("Shotgun", 40.5),
            kill("Pistol", 99.0),
        ],
        ..Record::default()
    };
    let t = text(&harpoon.lines(&cat, &r, 0, Some(&rec), false));
    for want in [
        "ROUNDS 8",
        "MAGAZINE 2",
        "OPTICS TELESCOPIC SIGHT",
        "WATER FIRES BELOW THE SURFACE",
        "PRICE 10 PTS",
        "TRANQ DARTS NOT PERMITTED",
        "TROPHIES KILLS ARE NOT KEPT",
        "AUTHORISED AREAS",
        "0> FERN VALLEY",
        "KILLS 1",
        "LONG SHOT 99.0 m (VELOCIRAPTOR)",
    ] {
        assert!(t.iter().any(|l| l == want), "{want} missing from {t:?}");
    }
    assert_eq!(armoury::links(&cat, &r, 0), vec![(Tab::Area, 0)]);
    // The shotgun is issued everywhere and restricts nothing.
    let plain = Arm::default();
    let t = text(&plain.lines(&cat, &r, 1, Some(&rec), true));
    for want in [
        "NONE",
        "ALL AREAS",
        "KILLS 2",
        "LONG SHOT 132.9 ft (VELOCIRAPTOR)",
    ] {
        assert!(t.iter().any(|l| l == want), "{want} missing from {t:?}");
    }
    assert!(!t
        .iter()
        .any(|l| l.starts_with("PELLETS") || l.starts_with("OPTICS")));
    assert!(armoury::links(&cat, &r, 1).is_empty());
    let t = text(&plain.lines(&cat, &r, 1, Some(&Record::default()), false));
    assert!(t.iter().any(|l| l == "NO KILLS ON FILE"));
}
