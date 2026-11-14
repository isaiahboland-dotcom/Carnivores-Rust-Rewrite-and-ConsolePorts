use alloc::vec;

use super::terminal::*;
use super::*;
use c2_formats::dhc::Island;

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
    assert_eq!(t.main_row, MainItem::ALL.len() - 1, "stays on the last");
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
