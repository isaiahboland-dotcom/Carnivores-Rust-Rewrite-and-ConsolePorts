//! The Advanced DHC OS UI's preparation over the terminal's setup.

use super::*;
use crate::dhc::beep::Beep;
use crate::dhc::prep::{points_left, ring_delta, Prep, PrepAction as P, Stage};

fn prep(score: i32, rules: Rules) -> (Prep, Terminal) {
    let mut t = setup(score, rules);
    let mut p = Prep::default();
    p.enter(&mut t);
    (p, t)
}

#[test]
fn rings_go_the_short_way_round() {
    assert_eq!(ring_delta(0.0, 1.0, 5), 1.0);
    assert_eq!(ring_delta(0.0, 4.0, 5), -1.0);
    assert_eq!(ring_delta(4.0, 0.0, 5), 1.0);
    assert_eq!(ring_delta(0.0, 2.0, 4), 2.0, "a tie goes forwards");
    assert_eq!(ring_delta(3.0, 3.0, 0), 0.0);
}

#[test]
fn browsing_maps_highlights_without_committing() {
    let (mut p, mut t) = prep(1000, Rules::default());
    assert_eq!(
        (p.stage, t.area, p.highlight(&t, Stage::Map)),
        (Stage::Map, 0, 0)
    );
    assert_eq!(p.act(&mut t, P::Next), Some(Beep::Tick));
    assert_eq!(
        (p.highlight(&t, Stage::Map), t.area),
        (1, 0),
        "not committed"
    );
    // The carousel wraps round both ways.
    p.act(&mut t, P::Next);
    assert_eq!(p.highlight(&t, Stage::Map), 0);
    p.act(&mut t, P::Prev);
    assert_eq!(p.highlight(&t, Stage::Map), 1);
    assert_eq!(t.spent(), 0, "browsing costs nothing");
    // Using the map commits it and moves on to the dinosaurs.
    assert_eq!(p.act(&mut t, P::Commit), Some(Beep::Select));
    assert_eq!((t.area, p.stage, t.tab), (1, Stage::Dinos, Tab::Targets));
    assert_eq!(t.spent(), 50);
}

#[test]
fn a_map_the_account_cannot_pay_is_refused_and_stays() {
    let (mut p, mut t) = prep(10, Rules::default());
    p.act(&mut t, P::Highlight(1));
    assert_eq!(p.act(&mut t, P::Commit), Some(Beep::Refuse));
    assert_eq!((t.area, p.stage), (0, Stage::Map));
    assert_eq!(t.notice, Some(Notice::Funds));
}

#[test]
fn back_restores_each_stages_highlight_and_scroll() {
    let (mut p, mut t) = prep(1000, Rules::default());
    p.act(&mut t, P::Next);
    p.step(&t, 5.0);
    assert!(p.settled(&t, Stage::Map));
    p.act(&mut t, P::Commit);
    p.act(&mut t, P::Next);
    p.act(&mut t, P::Next);
    p.step(&t, 5.0);
    p.act(&mut t, P::Commit);
    assert_eq!(p.stage, Stage::Guns);
    assert_eq!(p.act(&mut t, P::Back), Some(Beep::Back));
    assert_eq!(p.stage, Stage::Dinos);
    assert_eq!(p.highlight(&t, Stage::Dinos), 2);
    assert_eq!(p.scroll(Stage::Dinos), 2.0);
    p.act(&mut t, P::Back);
    assert_eq!(
        (p.stage, p.highlight(&t, Stage::Map), p.scroll(Stage::Map)),
        (Stage::Map, 1, 1.0)
    );
    // Back from the first stage is the terminal's main menu.
    assert_eq!(p.act(&mut t, P::Back), Some(Beep::Back));
    assert_eq!(t.screen, Screen::Main);
}

#[test]
fn a_carousel_eases_onto_its_highlight_the_short_way() {
    let (mut p, mut t) = prep(1000, Rules::default());
    p.act(&mut t, P::Commit);
    p.act(&mut t, P::Prev); // 0 -> 2 of 3: one step back, not two on
    p.step(&t, 0.02);
    let s = p.scroll(Stage::Dinos);
    assert!(s > 2.0 && s < 3.0, "went backwards round the ring: {s}");
    assert!(!p.settled(&t, Stage::Dinos));
    p.step(&t, 2.0);
    assert!(p.settled(&t, Stage::Dinos));
    assert_eq!(p.offset(Stage::Dinos, 3, 0), 1.0);
    assert_eq!(p.offset(Stage::Dinos, 3, 1), -1.0);
}

#[test]
fn adding_and_removing_go_through_the_terminal() {
    let (mut p, mut t) = prep(1000, rules());
    p.act(&mut t, P::Highlight(1));
    p.act(&mut t, P::Commit); // Ravan: only the Velociraptor, the Shotgun
    p.act(&mut t, P::Highlight(0));
    assert_eq!(
        p.act(&mut t, P::Add),
        Some(Beep::Refuse),
        "not on this island"
    );
    p.act(&mut t, P::Highlight(1));
    assert_eq!(p.act(&mut t, P::Add), Some(Beep::Select));
    assert!(t.picked(Tab::Targets, 1));
    assert_eq!(p.highlight(&t, Stage::Dinos), 1, "the highlight stays");
    p.act(&mut t, P::Commit);
    p.act(&mut t, P::Highlight(1));
    p.act(&mut t, P::Add);
    assert_eq!(t.loadout().weapons, [1]);
    // The record's removal, from another stage, leaves this one alone.
    assert_eq!(p.act(&mut t, P::Remove(Tab::Targets, 1)), Some(Beep::Back));
    assert!(!t.picked(Tab::Targets, 1));
    assert_eq!((p.stage, t.tab), (Stage::Guns, Tab::Arms));
    assert_eq!(
        p.act(&mut t, P::Remove(Tab::Targets, 1)),
        None,
        "not picked"
    );
    assert_eq!(
        p.act(&mut t, P::Remove(Tab::Area, 1)),
        None,
        "the map stays"
    );
}

#[test]
fn the_preparation_starts_what_the_terminal_starts() {
    let (mut p, mut t) = prep(1000, Rules::default());
    for a in [
        P::Next,
        P::Commit,
        P::Add,
        P::Commit,
        P::Next,
        P::Add,
        P::Commit,
    ] {
        p.act(&mut t, a);
    }
    assert_eq!(p.stage, Stage::Gear);
    let mut plain = setup(1000, Rules::default());
    for a in [
        Action::Pick(1),
        Action::Tab(Tab::Targets),
        Action::Pick(0),
        Action::Tab(Tab::Arms),
        Action::Pick(1),
    ] {
        plain.act(a);
    }
    assert_eq!(t.loadout(), plain.loadout());
    assert_eq!(
        p.act(&mut t, P::Commit),
        None,
        "gear is the last stage for now"
    );
    t.act(Action::Deploy);
    t.step(10.0);
    assert_eq!(t.take_outcome(), Some(Outcome::Deploy(plain.loadout())));
}

#[test]
fn entering_highlights_the_committed_map_and_follows_the_terminal() {
    let mut t = setup(1000, Rules::default());
    t.act(Action::Pick(1));
    t.act(Action::Tab(Tab::Gear));
    let mut p = Prep::default();
    p.enter(&mut t);
    assert_eq!(
        (p.stage, t.tab, p.highlight(&t, Stage::Map)),
        (Stage::Map, Tab::Area, 1)
    );
    assert_eq!(p.scroll(Stage::Map), 1.0);
    t.act(Action::Tab(Tab::Arms));
    p.follow(&t);
    assert_eq!(p.stage, Stage::Guns);
    // Off the setup, nothing happens.
    t.act(Action::Back);
    assert_eq!(p.act(&mut t, P::Next), None);
}

#[test]
fn points_left_is_the_share_of_the_starting_balance() {
    assert_eq!(points_left(1000, 1000), 1.0);
    assert_eq!(points_left(1000, 250), 0.25);
    assert_eq!(points_left(1000, -10), 0.0);
    assert_eq!((points_left(0, 0), points_left(0, -1)), (1.0, 0.0));
    // 20,000 down to 2,000 warns harder than 100 down to 80 (spec §8).
    assert!(points_left(20_000, 2_000) < points_left(100, 80));
}
