use alloc::vec;

use super::terminal::*;
use super::*;

fn area(name: &str, stem: &str, price: i32) -> Item {
    Item {
        name: name.into(),
        price,
        stem: stem.into(),
    }
}

fn catalog() -> Catalog {
    Catalog {
        areas: vec![area("Fern Valley", "AREA1", 0), area("Ravan", "AREA2", 50)],
        animals: vec![
            Item::new("Parasaurolophus", 20),
            Item::new("Velociraptor", 80),
            Item::new("Tyrannosaurus", 300),
        ],
        weapons: vec![Item::new("Pistol", 10), Item::new("Shotgun", 40)],
        equipment: vec![
            Item::new("Camouflage", 10),
            Item::new("Radar", 30),
            Item::new("Cover Scent", 20),
            Item::new("Double Ammo", 15),
        ],
    }
}

fn rules() -> Rules {
    Rules {
        dhc: Some(Dhc {
            islands: vec![Island {
                area: "AREA2".into(),
                animals: vec!["Velociraptor".into()],
                weapons: vec!["Shotgun".into()],
                equipment: vec!["Camouflage".into()],
                times: 0b100,
            }],
            ..Dhc::default()
        }),
        limits: Limits::default(),
    }
}

fn setup(score: i32, rules: Rules) -> Terminal {
    let mut t = Terminal::new(catalog(), rules, score, false);
    t.act(Action::Confirm); // HUNT SETUP
    assert_eq!(t.screen, Screen::Setup);
    t
}

#[test]
fn boot_runs_to_the_main_menu_or_is_skipped() {
    let mut t = Terminal::new(catalog(), Rules::default(), 100, true);
    assert_eq!(t.screen, Screen::Boot);
    assert_eq!(t.boot_shown(), 1);
    t.step(BOOT_LINE_S * 3.0 + 0.01);
    assert_eq!(t.boot_shown(), 4);
    t.step(10.0);
    assert_eq!(t.screen, Screen::Main);
    let mut t = Terminal::new(catalog(), Rules::default(), 100, true);
    t.act(Action::Skip);
    assert_eq!(t.screen, Screen::Main);
    assert!(t.boot_lines.iter().any(|l| l.contains("STANDARD")));
    assert!(Terminal::new(catalog(), rules(), 0, true)
        .boot_lines
        .iter()
        .any(|l| l.contains("200 PTS")));
}

#[test]
fn main_menu_moves_opens_and_hands_back_platform_screens() {
    let mut t = Terminal::new(catalog(), Rules::default(), 100, false);
    t.act(Action::Up);
    assert_eq!(t.main_row, 0);
    t.act(Action::Down);
    t.act(Action::Confirm);
    assert_eq!(t.screen, Screen::Manual);
    t.act(Action::Back);
    assert_eq!(t.screen, Screen::Main);
    t.act(Action::Back);
    assert_eq!(t.take_outcome(), Some(Outcome::Quit));
    for (i, want) in [
        (2, Outcome::TrophyRoom),
        (3, Outcome::Options),
        (4, Outcome::Hunters),
        (5, Outcome::ClassicMenu),
        (6, Outcome::Quit),
    ] {
        t.act(Action::Pick(i));
        assert_eq!(t.take_outcome(), Some(want));
        assert_eq!(t.take_outcome(), None);
    }
    t.act(Action::Focus(9));
    t.act(Action::Down);
    t.act(Action::Down);
    assert_eq!(t.main_row, t.main_items().len() - 1, "stays on the last");
    t.act(Action::Pick(0));
    assert_eq!(t.screen, Screen::Setup);
}

#[test]
fn tabs_cycle_and_rows_stay_in_their_list() {
    let mut t = setup(1000, Rules::default());
    t.act(Action::Left);
    assert_eq!(t.tab, Tab::Gear);
    t.act(Action::Right);
    t.act(Action::Right);
    assert_eq!(t.tab, Tab::Targets);
    for _ in 0..10 {
        t.act(Action::Down);
    }
    assert_eq!(t.rows[Tab::Targets.index()], 2);
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Focus(1));
    assert_eq!(t.rows[Tab::Arms.index()], 1);
    t.act(Action::Focus(7));
    assert_eq!(t.rows[Tab::Arms.index()], 1, "out of range is ignored");
    t.act(Action::Back);
    assert_eq!(t.screen, Screen::Main);
}

#[test]
fn picks_toggle_and_the_budget_follows() {
    let mut t = setup(200, Rules::default());
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(1));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(0));
    assert_eq!(t.spent(), 90);
    assert_eq!(t.remaining(), 110);
    t.act(Action::Pick(0));
    assert_eq!(t.spent(), 80, "a second pick drops it");
    // The Rex is over the account: refused, nothing changes.
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(2));
    assert_eq!(t.notice, Some(Notice::Funds));
    assert!(!t.picked(Tab::Targets, 2));
    assert!(!t.affordable(Tab::Targets, 2));
    // Dropping is always allowed.
    t.act(Action::Pick(1));
    assert_eq!(t.spent(), 0);
    assert_eq!(t.notice, None);
}

#[test]
fn an_area_swap_counts_the_old_areas_price_back() {
    let mut t = setup(60, Rules::default());
    t.act(Action::Pick(1)); // Ravan, 50
    assert_eq!((t.area, t.remaining()), (1, 10));
    assert!(t.affordable(Tab::Area, 0));
    t.act(Action::Pick(0));
    assert_eq!((t.area, t.remaining()), (0, 60));
}

#[test]
fn island_rules_restrict_picks_and_prune_on_arrival() {
    let mut t = setup(1000, rules());
    // Fern Valley has no island: everything is allowed, any time.
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(0));
    t.act(Action::Pick(1));
    t.act(Action::Tab(Tab::Gear));
    t.act(Action::Pick(1));
    assert_eq!(t.time, 1);
    // Ravan allows only the raptor, the shotgun, camouflage, at night.
    t.act(Action::Tab(Tab::Area));
    t.act(Action::Pick(1));
    assert_eq!(t.loadout().animals, [1], "the parasaur is dropped");
    assert!(t.loadout().equipment.is_empty(), "the radar is dropped");
    assert_eq!(t.time, 2, "moved to the only time offered");
    t.act(Action::CycleTime);
    assert_eq!(t.time, 2);
    t.act(Action::SetTime(0));
    assert_eq!(t.time, 2);
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(0));
    assert_eq!(t.notice, Some(Notice::NotAuthorised));
    assert!(!t.allowed(Tab::Arms, 0) && t.allowed(Tab::Arms, 1));
    t.act(Action::Pick(1));
    assert_eq!(t.deploy_problem(), None);
}

#[test]
fn an_empty_rule_set_offers_the_original_menu() {
    let open = Rules {
        dhc: Some(Dhc::default()),
        limits: Limits::default(),
    };
    let cat = catalog();
    for r in [Rules::default(), open] {
        for a in 0..cat.areas.len() {
            for tab in Tab::ALL {
                for i in 0..cat.list(tab).len() {
                    assert!(r.allows(&cat, a, tab, i), "{tab:?} {i} on {a}");
                }
            }
            assert!((0..3).all(|t| r.allows_time(&cat, a, t)));
        }
    }
}

#[test]
fn islands_match_by_map_or_area_name() {
    let mut r = rules();
    if let Some(d) = &mut r.dhc {
        d.islands[0].area = "ravan".into();
    }
    assert!(!r.allows(&catalog(), 1, Tab::Arms, 0));
}

#[test]
fn platform_limits_cap_picks_and_bar_gear_and_times() {
    let r = Rules {
        dhc: None,
        limits: Limits {
            times: 0b010,
            max_animals: Some(1),
            max_weapons: Some(1),
            barred_equipment: vec![1],
        },
    };
    let mut t = setup(1000, r);
    t.act(Action::CycleTime);
    assert_eq!(t.time, 1, "only day is offered");
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(0));
    t.act(Action::Pick(1));
    assert_eq!(t.notice, Some(Notice::LimitAnimals(1)));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(0));
    t.act(Action::Pick(1));
    assert_eq!(t.notice, Some(Notice::LimitWeapons(1)));
    t.act(Action::Tab(Tab::Gear));
    t.act(Action::Pick(1));
    assert_eq!(t.notice, Some(Notice::NotAuthorised));
    assert_eq!(t.loadout().animals, [0]);
}

#[test]
fn deploy_needs_targets_and_arms_then_runs_its_sequence() {
    let mut t = setup(500, Rules::default());
    t.act(Action::Deploy);
    assert_eq!(t.notice, Some(Notice::NoTargets));
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(0));
    t.act(Action::Deploy);
    assert_eq!(t.notice, Some(Notice::NoArms));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(1));
    t.act(Action::Deploy);
    assert_eq!(t.screen, Screen::Deploy);
    assert!(t.deploy_log()[0].contains("Fern Valley / DAY"));
    t.step(DEPLOY_S / 2.0);
    assert!(t.take_outcome().is_none());
    assert!((t.deploy_progress() - 0.5).abs() < 1e-4);
    t.step(DEPLOY_S);
    let want = Loadout {
        area: 0,
        animals: vec![0],
        weapons: vec![1],
        equipment: vec![],
        time: 1,
        observer: false,
        hardcore: false,
    };
    assert_eq!(t.take_outcome(), Some(Outcome::Deploy(want.clone())));
    assert_eq!(t.screen, Screen::Setup);
    // Skipping gives the same loadout; Back cancels.
    t.act(Action::Deploy);
    t.act(Action::Back);
    assert_eq!((t.screen, t.take_outcome()), (Screen::Setup, None));
    t.act(Action::Deploy);
    t.act(Action::Skip);
    assert_eq!(t.take_outcome(), Some(Outcome::Deploy(want)));
}

#[test]
fn earnings_follow_the_originals_multipliers() {
    let mut t = setup(500, Rules::default());
    assert_eq!(t.earnings(), 1.0);
    t.act(Action::Tab(Tab::Gear));
    t.act(Action::Pick(1));
    assert!((t.earnings() - 0.70).abs() < 1e-6);
    t.act(Action::Pick(0));
    t.act(Action::Pick(2));
    t.act(Action::Pick(3));
    assert!((t.earnings() - 0.70 * 0.85 * 0.80).abs() < 1e-6);
    t.tranq = true;
    assert!((t.earnings() - 1.25 * 0.70 * 0.85 * 0.80).abs() < 1e-6);
}

#[test]
fn a_restored_loadout_keeps_only_what_is_allowed_and_affordable() {
    let mut t = setup(150, rules());
    t.restore(&Loadout {
        area: 1,
        animals: vec![0, 1, 2],
        weapons: vec![0, 1],
        equipment: vec![0, 1],
        time: 0,
        observer: false,
        hardcore: false,
    });
    let l = t.loadout();
    assert_eq!((l.area, l.animals, l.weapons), (1, vec![1], vec![]));
    assert_eq!(l.equipment, [0]);
    assert_eq!(l.time, 2);
    assert!(t.remaining() >= 0);
    assert_eq!(t.notice, None);
}

#[test]
fn the_same_actions_give_the_same_state() {
    let run = || {
        let mut t = Terminal::new(catalog(), rules(), 400, true);
        for a in [
            Action::Skip,
            Action::Confirm,
            Action::Right,
            Action::Pick(1),
            Action::Right,
            Action::Pick(1),
            Action::CycleTime,
            Action::Deploy,
        ] {
            t.act(a);
            t.step(0.05);
        }
        t.step(DEPLOY_S);
        (t.take_outcome(), t.remaining(), t.screen)
    };
    assert_eq!(run(), run());
    assert!(matches!(run().0, Some(Outcome::Deploy(_))));
}

#[test]
fn an_observer_needs_no_arms_or_funds() {
    let mut t = setup(0, rules());
    assert!(t.deploy_problem().is_some());
    t.act(Action::ToggleObserver);
    assert!(t.observer);
    assert_eq!(t.deploy_problem(), None);
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(0));
    assert_eq!(t.spent(), 0);
    assert!(t.loadout().observer);
    t.act(Action::ToggleObserver);
    assert!(!t.observer);
}

#[test]
fn the_tranquilizer_is_the_platforms_to_flip() {
    let mut t = setup(100, Rules::default());
    t.act(Action::ToggleTranq);
    assert_eq!(t.take_outcome(), Some(Outcome::ToggleTranq));
}

/// A terminal with both extra entries: the hardcore hunt (no radar, one
/// weapon) and two survival modes.
fn full_menu() -> Terminal {
    let mut t = Terminal::new(catalog(), Rules::default(), 1000, false);
    t.offer_hardcore(Limits {
        max_weapons: Some(1),
        barred_equipment: vec![1],
        ..Limits::default()
    });
    t.survival = vec!["MODDED".into(), "HARDCORE".into()];
    t
}

#[test]
fn the_extra_entries_are_absent_unless_offered() {
    let t = Terminal::new(catalog(), Rules::default(), 0, false);
    let items = t.main_items();
    assert_eq!(items[0], MainItem::Hunt);
    assert!(!items.contains(&MainItem::HardcoreHunt));
    assert!(!items.contains(&MainItem::Survival));
    let items = full_menu().main_items();
    assert_eq!(
        &items[..4],
        [
            MainItem::Hunt,
            MainItem::HardcoreHunt,
            MainItem::Survival,
            MainItem::Manual
        ],
        "the hardcore hunt sits below the regular one"
    );
}

#[test]
fn the_hardcore_hunt_has_its_own_limits_and_the_regular_one_none() {
    let mut t = full_menu();
    t.act(Action::Pick(1));
    assert!(t.hardcore && t.screen == Screen::Setup);
    t.act(Action::Tab(Tab::Gear));
    t.act(Action::Pick(1));
    assert!(!t.picked(Tab::Gear, 1), "radar is barred");
    assert_eq!(t.notice, Some(Notice::NotAuthorised));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(0));
    t.act(Action::Pick(1));
    assert!(
        t.picked(Tab::Arms, 0) && !t.picked(Tab::Arms, 1),
        "one weapon"
    );
    assert_eq!(t.notice, Some(Notice::LimitWeapons(1)));
    t.act(Action::Tab(Tab::Targets));
    t.act(Action::Pick(0));
    t.act(Action::Deploy);
    t.act(Action::Skip);
    let Some(Outcome::Deploy(l)) = t.take_outcome() else {
        panic!("no deploy")
    };
    assert!(l.hardcore);
    // Back on the regular setup, nothing is barred and the flag is off.
    t.act(Action::Back);
    t.act(Action::Pick(0));
    assert!(!t.hardcore);
    t.act(Action::Tab(Tab::Gear));
    t.act(Action::Pick(1));
    assert!(t.picked(Tab::Gear, 1));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(1));
    assert!(t.picked(Tab::Arms, 1), "two weapons on a regular hunt");
    assert!(!t.loadout().hardcore);
}

#[test]
fn entering_the_hardcore_hunt_drops_picks_it_bars_and_trims_the_arms() {
    let mut t = full_menu();
    t.act(Action::Pick(0));
    for (tab, i) in [(Tab::Gear, 1), (Tab::Arms, 0), (Tab::Arms, 1)] {
        t.act(Action::Tab(tab));
        t.act(Action::Pick(i));
    }
    t.act(Action::Back);
    t.act(Action::Pick(1));
    assert!(!t.picked(Tab::Gear, 1));
    assert_eq!(t.loadout().weapons, vec![0], "the first weapon stays");
}

#[test]
fn survival_lists_the_platforms_modes_and_hands_one_back() {
    let mut t = full_menu();
    t.act(Action::Pick(2));
    assert_eq!(t.screen, Screen::Survival);
    t.act(Action::Down);
    t.act(Action::Down);
    assert_eq!(t.survival_row(), 1, "stays on the last");
    t.act(Action::Confirm);
    assert_eq!(t.take_outcome(), Some(Outcome::Survival(1)));
    t.act(Action::Pick(0));
    assert_eq!(t.take_outcome(), Some(Outcome::Survival(0)));
    t.act(Action::Pick(5));
    assert_eq!(t.take_outcome(), None, "out of range is ignored");
    t.act(Action::Back);
    assert_eq!(t.screen, Screen::Main);
}

#[test]
fn original_menus_filter_by_name_like_the_terminal() {
    let r = rules();
    let dhc = r.dhc.as_ref();
    // Found by stem, then by the menu's name; an unnamed map is open.
    let isl = island_for(dhc, "area2", "Ravan");
    assert!(isl.is_some());
    assert_eq!(island_for(dhc, "", "Ravan"), None);
    assert_eq!(island_for(dhc, "AREA1", "Fern Valley"), None);
    assert_eq!(island_for(None, "AREA2", "Ravan"), None);
    // The filter table, as the terminal's catalog sees it.
    let cat = catalog();
    for (tab, list) in [
        (Tab::Targets, &cat.animals),
        (Tab::Arms, &cat.weapons),
        (Tab::Gear, &cat.equipment),
    ] {
        for (i, it) in list.iter().enumerate() {
            assert_eq!(
                island_allows(isl, tab, &it.name),
                r.allows(&cat, 1, tab, i),
                "{} on AREA2",
                it.name
            );
            assert!(island_allows(None, tab, &it.name));
        }
    }
    assert!(island_allows(isl, Tab::Targets, " velociraptor "));
    assert!(!island_allows(isl, Tab::Targets, "Tyrannosaurus"));
    // An island with empty lists is the original menu.
    let open = Island {
        area: "AREA1".into(),
        ..Island::default()
    };
    assert!(island_allows(Some(&open), Tab::Gear, "Radar"));
    assert!(!island_forbids(
        Some(&open),
        &["Tyrannosaurus"],
        &["Pistol"],
        &["Radar"],
        0
    ));
}

#[test]
fn a_forbidden_hunt_start_is_refused() {
    let r = rules();
    let isl = island_for(r.dhc.as_ref(), "AREA2", "Ravan");
    let ok = |a: &[&str], w: &[&str], e: &[&str], t| !island_forbids(isl, a, w, e, t);
    assert!(ok(&["Velociraptor"], &["Shotgun"], &["Camouflage"], 2));
    assert!(ok(&[], &[], &[], 2), "an observer with no picks");
    assert!(
        !ok(&["Velociraptor"], &["Shotgun"], &[], 1),
        "day is not offered"
    );
    assert!(!ok(&["Tyrannosaurus"], &["Shotgun"], &[], 2));
    assert!(!ok(&["Velociraptor"], &["Pistol"], &[], 2));
    assert!(!ok(&["Velociraptor"], &["Shotgun"], &["Radar"], 2));
    // No rules, or an unrestricted map: nothing is refused.
    assert!(!island_forbids(
        None,
        &["Tyrannosaurus"],
        &["Pistol"],
        &["Radar"],
        1
    ));
}

#[test]
fn beeps_follow_what_an_action_did() {
    use super::beep::Beep;
    let mut t = Terminal::new(catalog(), rules(), 1000, false);
    assert_eq!(t.act_beep(Action::Down), Some(Beep::Tick));
    assert_eq!(t.act_beep(Action::Focus(0)), None, "hover is silent");
    assert_eq!(t.act_beep(Action::Up), None, "nothing moved");
    assert_eq!(
        t.act_beep(Action::Confirm),
        Some(Beep::Select),
        "setup opens"
    );
    assert_eq!(t.act_beep(Action::Right), Some(Beep::Tick), "next tab");
    assert_eq!(t.act_beep(Action::Confirm), Some(Beep::Select), "a target");
    // Ravan (AREA2) has no Parasaurolophus: refused.
    t.act(Action::Tab(Tab::Area));
    t.act(Action::Pick(1));
    t.act(Action::Tab(Tab::Targets));
    assert_eq!(t.act_beep(Action::Pick(0)), Some(Beep::Refuse));
    assert_eq!(
        t.act_beep(Action::Deploy),
        Some(Beep::Refuse),
        "no arms yet"
    );
    t.act(Action::Pick(1));
    t.act(Action::Tab(Tab::Arms));
    t.act(Action::Pick(1));
    assert_eq!(t.act_beep(Action::Deploy), Some(Beep::Deploy));
    assert_eq!(
        t.act_beep(Action::Back),
        Some(Beep::Back),
        "deploy called off"
    );
    assert_eq!(
        t.act_beep(Action::Back),
        Some(Beep::Back),
        "to the main menu"
    );
    assert_eq!(
        t.act_beep(Action::Back),
        Some(Beep::Select),
        "the quit prompt"
    );
    assert!(
        t.take_outcome().is_some(),
        "the outcome is still the platform's"
    );
}

mod files;
mod prep;
mod specimens;
