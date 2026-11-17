//! The specimen database: the field manual's index, a file's retrieval
//! and what the file says.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::super::beep::Beep;
use super::super::specimen::*;
use super::super::terminal::*;
use super::super::*;
use super::{catalog, rules};
use crate::record::{Kill, Mode, Record};

fn db() -> Terminal {
    let mut t = Terminal::new(catalog(), rules(), 100, false);
    t.specimen_db = true;
    t
}

fn open_manual(t: &mut Terminal) {
    let i = t
        .main_items()
        .iter()
        .position(|&m| m == MainItem::Manual)
        .unwrap_or(0);
    t.act(Action::Pick(i));
}

#[test]
fn the_field_manual_is_the_index_where_the_platform_offers_it() {
    let mut t = Terminal::new(catalog(), rules(), 100, false);
    open_manual(&mut t);
    assert_eq!(t.screen, Screen::Manual, "the 3DS keeps its text");
    let mut t = db();
    open_manual(&mut t);
    assert_eq!(t.screen, Screen::Specimens);
    t.act(Action::Down);
    t.act(Action::Down);
    t.act(Action::Down);
    assert_eq!(t.specimen_row(), 2, "stays on the last");
    t.act(Action::Up);
    t.act(Action::Back);
    assert_eq!(t.screen, Screen::Main);
}

#[test]
fn a_file_is_retrieved_then_shown_and_goes_back_to_the_index() {
    let mut t = db();
    open_manual(&mut t);
    t.act(Action::Down);
    t.act(Action::Confirm);
    assert_eq!(
        (t.screen, t.file(), t.file_from()),
        (Screen::Specimen, 1, Screen::Specimens)
    );
    assert!(t.retrieving());
    assert_eq!(t.retrieve_shown(), 1);
    assert!(t.retrieve_log()[1].contains("VELOCIRAPTOR"));
    assert!(t.retrieve_log().iter().any(|l| l.contains("PHOTOGRAPH")));
    t.step(RETRIEVE_S / 2.0);
    let half = t.retrieve_shown();
    assert!(half > 1 && half < t.retrieve_log().len());
    t.step(RETRIEVE_S);
    assert!(!t.retrieving());
    t.act(Action::Back);
    assert_eq!((t.screen, t.specimen_row()), (Screen::Specimens, 1));
    // A click skips the retrieval; the next closes the file.
    t.specimen_models = true;
    t.act(Action::Pick(2));
    assert!(t.retrieve_log().iter().any(|l| l.contains("3D SPECIMEN")));
    t.act(Action::Pick(0));
    assert!(!t.retrieving() && t.screen == Screen::Specimen);
    t.act(Action::Confirm);
    assert_eq!(t.screen, Screen::Specimens);
    t.act(Action::Pick(9));
    assert_eq!(t.screen, Screen::Specimens, "no such animal");
}

#[test]
fn the_setup_opens_the_focused_targets_file_and_keeps_its_picks() {
    let mut t = db();
    t.act(Action::Confirm); // HUNT SETUP
    t.act(Action::Inspect);
    assert_eq!(t.screen, Screen::Setup, "only on the targets tab");
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(0));
    t.act(Action::Down);
    t.act(Action::Inspect);
    assert_eq!(
        (t.screen, t.file(), t.file_from()),
        (Screen::Specimen, 1, Screen::Setup)
    );
    assert_eq!(t.act_beep(Action::Back), Some(Beep::Back));
    assert_eq!((t.screen, t.tab), (Screen::Setup, Tab::Targets));
    assert!(t.picked(Tab::Targets, 0));
    // Without the database (the 3DS), the setup has no files.
    let mut t = Terminal::new(catalog(), rules(), 100, false);
    t.act(Action::Confirm);
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Inspect);
    assert_eq!(t.screen, Screen::Setup);
}

#[test]
fn moving_in_the_index_ticks() {
    let mut t = db();
    open_manual(&mut t);
    assert_eq!(t.act_beep(Action::Down), Some(Beep::Tick));
    assert_eq!(t.act_beep(Action::Confirm), Some(Beep::Select));
}

#[test]
fn native_areas_follow_the_island_rules_and_the_games_homes() {
    let (cat, r) = (catalog(), rules());
    // Ravan (AREA2) permits only the Velociraptor.
    assert_eq!(native_areas(&cat, &r, 0, None), Some(vec![0]));
    assert_eq!(native_areas(&cat, &r, 1, None), None, "unrestricted");
    assert_eq!(native_areas(&cat, &r, 1, Some(&[1])), Some(vec![1]));
    assert_eq!(native_areas(&cat, &r, 0, Some(&[1])), Some(vec![]));
    assert_eq!(
        native_areas(&cat, &Rules::default(), 2, Some(&[0, 1])),
        None
    );
}

#[test]
fn the_original_places_seven_species_from_ai_ten_without_a_permit() {
    let vanilla = [0, 1, 2, 3, 4, 5, 6, 10, 11, 12, 13, 14, 15, 16, 17, 18];
    assert_eq!(unpermitted(&vanilla), (7..14).collect::<Vec<_>>());
    assert_eq!(unpermitted(&[0, 10, 11]), vec![1, 2]);
    assert!(unpermitted(&[0, 1, 2]).is_empty());
}

fn rex() -> Specimen {
    Specimen {
        name: "Tyrannosaurus".into(),
        permit: 300,
        score: Some(25),
        length: 10.0,
        mass: 6.0,
        scales: (1.0, 1.2),
        height_ratio: Some(0.4),
        diet: Some(Diet::Carnivore),
        homes: None,
        appears: Vec::new(),
    }
}

fn kill(score: i32, scale: f32) -> Kill {
    Kill {
        mode: Mode::Hunt,
        animal: "tyrannosaurus".into(),
        score,
        range: 40.0,
        weapon: "Rifle".into(),
        area: "Fern Valley".into(),
        date: (2026 << 20) + (11 << 10) + 17,
        scale,
    }
}

fn text(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|l| match l {
            Line::Head(h) => h.to_string(),
            Line::Fact(k, v) => alloc::format!("{k} {v}"),
            Line::Item(s) => s.clone(),
            Line::Blank => String::new(),
        })
        .collect()
}

#[test]
fn a_file_lists_its_facts_in_either_units() {
    let areas = vec!["Fern Valley".to_string(), "Ravan".to_string()];
    let s = rex();
    let t = text(&s.lines(&areas, None, None, false));
    for want in [
        "DIET CARNIVORE",
        "LENGTH 10.0 m - 12.0 m",
        "HEIGHT 4.0 m - 4.8 m (EST.)",
        "WEIGHT 6.00 T - 8.64 T",
        "PERMIT 300 PTS",
        "BOUNTY 25 PTS (TRANQ 31)",
        "PERMIT HUNTS ONLY",
        "NO HUNTER SIGNED IN",
    ] {
        assert!(t.iter().any(|l| l == want), "{want} missing from {t:?}");
    }
    assert!(!t.iter().any(|l| l == "NATIVE TO"), "unrestricted");
    let us = text(&s.lines(&areas, Some(&[1]), None, true));
    assert!(us.iter().any(|l| l == "LENGTH 32.8 ft - 39.4 ft"));
    assert!(us.iter().any(|l| l == "WEIGHT 6.62 t - 9.53 t"));
    let i = us.iter().position(|l| l == "NATIVE TO").unwrap_or(0);
    assert_eq!(us[i + 1], "Ravan");
    // Unknown sizes and AIs say nothing they don't know.
    let bare = Specimen {
        name: "Mystery".into(),
        scales: (1.0, 1.0),
        ..Specimen::default()
    };
    let t = text(&bare.lines(&areas, None, None, false));
    assert!(t.iter().any(|l| l == "DIET UNCLASSIFIED"));
    assert!(!t
        .iter()
        .any(|l| l.starts_with("LENGTH") || l.starts_with("WEIGHT") || l.starts_with("BOUNTY")));
    let everywhere = Specimen {
        appears: vec![0, 1],
        ..rex()
    };
    assert!(text(&everywhere.lines(&areas, None, None, false))
        .iter()
        .any(|l| l == "EVERY AREA"));
}

#[test]
fn a_file_remembers_the_biggest_and_best_kills() {
    let areas = vec!["Fern Valley".to_string()];
    let s = rex();
    let empty = Record::default();
    assert!(text(&s.lines(&areas, None, Some(&empty), false))
        .iter()
        .any(|l| l == "NO KILLS ON FILE"));
    let rec = Record {
        kills: vec![kill(25, 1.1), kill(40, 1.0), kill(30, 0.0)],
        best_wave: 0,
    };
    let t = text(&s.lines(&areas, None, Some(&rec), false));
    assert!(t.iter().any(|l| l == "KILLS 3"), "{t:?}");
    assert!(t.iter().any(|l| l == "LARGEST 7.26 T / 11.0 m"), "{t:?}");
    assert!(t.iter().any(|l| l == "TOP SCORE 40 PTS"));
    assert!(t.iter().any(|l| l == "  RIFLE, FERN VALLEY, 17.11.2026"));
    assert!(t.iter().any(|l| l.starts_with("ARCHIVED")));
    // Kills from before sizes were kept still count, without a size.
    let old = Record {
        kills: vec![kill(25, 0.0)],
        best_wave: 0,
    };
    let t = text(&s.lines(&areas, None, Some(&old), false));
    assert!(t.iter().any(|l| l == "LARGEST SIZE NOT RECORDED"), "{t:?}");
    assert!(t.iter().any(|l| l == "TOP SCORE 25 PTS"));
}

#[test]
fn height_is_measured_against_the_longer_side() {
    let pts = [[0.0, 0.0, 0.0], [10.0, 4.0, 2.0], [-2.0, 1.0, 20.0]];
    assert_eq!(height_ratio(&pts), Some(0.2));
    assert_eq!(height_ratio(&[]), None);
    // A stray vertex no face uses is left out.
    let face = c2_formats::model::Face {
        v: [0, 1, 2],
        tu: [0; 3],
        tv: [0; 3],
        flags: 0,
        dmask: 0,
        distant: 0,
        next: 0,
        group: 0,
        reserved: [0; 12],
    };
    let mut stray = pts.to_vec();
    stray.push([500.0, 0.0, 0.0]);
    assert_eq!(height_ratio(&used_points(&stray, &[face])), Some(0.2));
    assert_eq!(used_points(&stray, &[]).len(), 4, "no faces: all of them");
    assert_eq!(height_ratio(&[[0.0, 0.0, 0.0], [5.0, 0.0, 5.0]]), None);
}
