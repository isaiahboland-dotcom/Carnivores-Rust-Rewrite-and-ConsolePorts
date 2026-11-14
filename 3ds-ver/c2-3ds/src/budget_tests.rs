//! The memory plan on made-up packs: a hunter, five small ambient animals,
//! a brachiosaur, three big menu animals, a roaming add-on and two fish.

use super::*;
use c2_formats::mee::{Character, SpawnLink, Tables};
use c2_formats::res_txt::{CharacterDef, ResData};

const HUNTER: usize = 0;
const BRACH: usize = 6;
const REX: usize = 7;
const ALLO: usize = 8;
const VELO: usize = 9;
const ROAMER: usize = 10;
const SHARK: usize = 11;
const FISH: usize = 12;

/// (key, role, AI, linear, heap, per animal).
fn cast() -> Vec<(&'static str, Role, i32, usize, usize, usize)> {
    vec![
        ("HUNTER1", Role::Hunter, 0, 150, 400, 60),
        ("MOSH", Role::Ambient, 1, 100, 300, 40),
        ("GALL", Role::Ambient, 2, 100, 300, 40),
        ("DIMOR", Role::Ambient, 3, 100, 200, 40),
        ("PTERA", Role::Ambient, 4, 100, 200, 50),
        ("DIMET", Role::Ambient, 5, 100, 300, 40),
        ("BRACHI", Role::Ambient, 6, 200, 600, 70),
        ("TIREX", Role::Slot(0), 18, 1500, 800, 60),
        ("ALLO", Role::Slot(1), 13, 1200, 500, 50),
        ("VELO", Role::Slot(2), 15, 1000, 500, 50),
        ("ROAMER", Role::Roamer, 16, 900, 600, 60),
        ("SHARK", Role::Aquatic, 31, 700, 500, 60),
        ("FISH", Role::Aquatic, 30, 200, 100, 20),
    ]
}

/// The rules: the original's, or a dialect's whose tables place the
/// roaming add-on here.
fn rules(dialect: bool) -> HuntRules {
    let cast = cast();
    let tables = dialect.then(|| Tables {
        characters: (0..cast.len())
            .map(|c| Character {
                spawn: if c == ROAMER {
                    vec![SpawnLink {
                        group: 0,
                        ratio: 1.0,
                    }]
                } else {
                    Vec::new()
                },
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    HuntRules {
        res: ResData {
            characters: cast
                .iter()
                .map(|c| CharacterDef {
                    ai: Some(c.2),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        tables,
        roles: cast.iter().map(|c| c.1).collect(),
        keys: cast.iter().map(|c| c.0.to_string()).collect(),
    }
}

/// A pack with `linear` and `heap` KiB to spend; `planned`: with the
/// plan's figures.
fn pack(linear: usize, heap: usize, planned: bool) -> PackInfo {
    let mut t = format!(
        "pack = {}\nlinear_kb = {linear}\nheap_kb = {heap}\ncost.hunt = 1000\n\
         cost.AREA1 = 3000\nheap.AREA1 = 21500\ncost.wpn0 = 300\nheap.wpn0 = 100\n\
         cost.call10 = 50\ncost.call11 = 50\ncost.call12 = 50\n",
        if planned { 5 } else { 4 }
    );
    for (key, _, _, lin, heap, inst) in cast() {
        t += &format!("cost.m:{key} = {lin}\nheap.m:{key} = {heap}\n");
        if planned {
            t += &format!("inst.m:{key} = {inst}\n");
        }
    }
    if planned {
        t += "heapbase.AREA1 = 20000\nambients.AREA1 = MOSH,GALL,DIMOR,PTERA,DIMET,BRACHI\n";
    }
    PackInfo::parse(&t)
}

/// A hunt for the three big animals with one weapon, ambient animals on.
fn three_picks() -> Request {
    Request {
        area: "AREA1".into(),
        hunting: true,
        targets: Bits::bit(10) | Bits::bit(11) | Bits::bit(12),
        weapons: vec![0],
        ambient_max: 5,
        secret_max: 10,
        huntable_want: 12,
        water: true,
        ..Default::default()
    }
}

/// Linear memory the area, the hunter, the weapon and the calls take.
const FIXED: usize = 1000 + 3000 + 150 + 300 + 150;
/// The three picks' models and one animal of each.
const FLOOR: usize = 1500 + 1200 + 1000 + 60 + 50 + 50;

fn count(p: &Plan, c: usize) -> usize {
    p.huntables.iter().find(|h| h.0 == c).map_or(0, |h| h.1)
}

#[test]
fn one_of_each_pick_is_all_a_full_console_gets() {
    let p = plan(
        &pack(FIXED + FLOOR, 30000, true),
        &rules(false),
        &three_picks(),
    );
    assert!(p.fits && p.planned);
    assert_eq!(p.huntables, vec![(REX, 1), (ALLO, 1), (VELO, 1)]);
    assert_eq!((p.ambients_max, p.ambient_models.len()), (0, 0));
    assert!(!p.loads(1) && !p.loads(BRACH) && p.loads(HUNTER) && p.loads(REX));
    assert_eq!(p.used.linear, FIXED + FLOOR);
}

#[test]
fn picks_that_do_not_fit_are_refused_and_the_costliest_named() {
    let p = plan(
        &pack(FIXED + FLOOR - 1, 30000, true),
        &rules(false),
        &three_picks(),
    );
    assert!(!p.fits);
    assert_eq!(p.drop, Some(REX));
    // Main memory can refuse a hunt too.
    let p = plan(
        &pack(20000, 20000 + 100 + 1800 - 1, true),
        &rules(false),
        &three_picks(),
    );
    assert!(!p.fits);
}

#[test]
fn ambient_animals_go_before_any_pick_does() {
    let rules = rules(false);
    let req = three_picks();
    let mut ambients = Vec::new();
    // From plenty down to the bare floor.
    for linear in (FIXED + FLOOR..FIXED + FLOOR + 3000).rev().step_by(7) {
        let p = plan(&pack(linear, 30000, true), &rules, &req);
        assert!(p.fits, "{linear}");
        for c in [REX, ALLO, VELO] {
            assert!(count(&p, c) >= 1, "{linear}");
            // While there is an ambient animal, every pick has the four
            // (of twelve) the hunt aims for.
            assert!(p.ambients_max == 0 || count(&p, c) >= 4, "{linear}");
        }
        assert_eq!(p.ambients_max == 0, p.ambient_models.is_empty());
        assert!(p.used.within(p.budget));
        // An animal for every small species loaded (the brachiosaur's
        // are its own).
        let small = p.ambient_models.iter().filter(|&&c| c != BRACH).count();
        assert!(p.ambients_max >= small && p.ambients_max <= 5, "{linear}");
        ambients.push(p.ambient_models.len());
    }
    // The species go one by one as memory shrinks, to none.
    assert!(ambients.windows(2).all(|w| w[0] >= w[1]), "{ambients:?}");
    assert_eq!((ambients.first(), ambients.last()), (Some(&6), Some(&0)));
}

#[test]
fn ambient_animals_off_load_no_ambient_model() {
    let req = Request {
        ambient_max: 0,
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, true), &rules(false), &req);
    assert_eq!((p.ambients_max, p.ambient_models.len()), (0, 0));
    assert!((1..=BRACH).all(|c| !p.loads(c)));
}

#[test]
fn the_setting_is_the_most_ambient_animals_and_the_brachiosaurs_come_apart() {
    let p = plan(&pack(20000, 30000, true), &rules(false), &three_picks());
    assert_eq!(p.ambients_max, 5);
    assert_eq!(p.ambient_models, vec![1, 2, 3, 4, 5, BRACH]);
    assert_eq!(p.max[BRACH], Some(3));
    assert_eq!(p.max[1], Some(usize::MAX));
}

#[test]
fn random_ambients_load_only_their_species() {
    let req = Request {
        ambient_only: Some(vec![4, 2]),
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, true), &rules(false), &req);
    assert_eq!(p.ambient_models, vec![4, 2]);
    assert_eq!(p.ambients_max, 5);
    assert!(!p.loads(BRACH));
}

#[test]
fn aquatic_animals_take_only_what_is_left() {
    let rules = rules(false);
    let roomy = plan(&pack(20000, 30000, true), &rules, &three_picks());
    // The cheapest species first, two of a species, four in all.
    assert_eq!(roomy.aquatic, vec![(FISH, 2), (SHARK, 2)]);
    // No water, no aquatic animal.
    let dry = Request {
        water: false,
        ..three_picks()
    };
    assert!(plan(&pack(20000, 30000, true), &rules, &dry)
        .aquatic
        .is_empty());
    // Short of memory they go first, and cost the others nothing.
    for linear in (FIXED + FLOOR..FIXED + FLOOR + 3000).step_by(11) {
        let wet = plan(&pack(linear, 30000, true), &rules, &three_picks());
        let dry = plan(&pack(linear, 30000, true), &rules, &dry);
        assert_eq!(wet.ambients_max, dry.ambients_max, "{linear}");
        assert_eq!(wet.ambient_models, dry.ambient_models, "{linear}");
        for c in [REX, ALLO, VELO] {
            assert!(count(&wet, c) >= count(&dry, c).min(4), "{linear}");
        }
        if !wet.aquatic.is_empty() {
            assert_eq!(wet.ambients_max, 5, "{linear}");
        }
    }
}

#[test]
fn a_secret_animal_never_costs_a_pick_its_place() {
    let rules = rules(true);
    let secret = Request {
        rule_secrets: vec![(SHARK, 3)],
        ..three_picks()
    };
    // Room for the picks alone: the hunt goes on without secrets.
    let p = plan(&pack(FIXED + FLOOR, 30000, true), &rules, &secret);
    assert!(p.fits);
    assert_eq!(p.secrets(), 0);
    assert_eq!(p.huntables, vec![(REX, 1), (ALLO, 1), (VELO, 1)]);
    // Room for one more model: the first secret comes before the picks'
    // second animals, the instance's before the map's own.
    let p = plan(
        &pack(FIXED + FLOOR + 700 + 60, 30000, true),
        &rules,
        &secret,
    );
    assert_eq!(p.rule_secrets, vec![(SHARK, 1)]);
    assert!(p.native_secrets.is_empty());
    assert_eq!(count(&p, REX), 1);
    // Plenty: every secret asked for, and the map's own roamer.
    let p = plan(&pack(20000, 30000, true), &rules, &secret);
    assert_eq!(p.rule_secrets, vec![(SHARK, 3)]);
    assert_eq!(p.native_secrets.len(), 1);
    assert_eq!(p.native_secrets[0].0, ROAMER);
    assert!(p.native_secrets[0].1 >= 1 && p.native_secrets[0].1 <= 10);
}

#[test]
fn secrets_set_to_none_bring_no_roamer_of_the_map() {
    let req = Request {
        secret_max: 0,
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, true), &rules(true), &req);
    assert!(p.native_secrets.is_empty() && !p.loads(ROAMER));
    // Without tables (the original game) there is none either way.
    let p = plan(&pack(20000, 30000, true), &rules(false), &three_picks());
    assert!(p.native_secrets.is_empty());
}

#[test]
fn a_picked_roamer_is_a_pick_not_a_secret() {
    let req = Request {
        targets: Bits::bit(10),
        roamers: vec![ROAMER],
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, true), &rules(true), &req);
    assert!(p.native_secrets.is_empty());
    assert!(count(&p, ROAMER) >= 6 && count(&p, REX) >= 6);
}

#[test]
fn a_model_two_characters_share_is_charged_once() {
    let mut rules = rules(false);
    rules.keys[ALLO] = "TIREX".into();
    let req = Request {
        ambient_max: 0,
        huntable_want: 2,
        water: false,
        ..three_picks()
    };
    let p = plan(
        &pack(FIXED + 1500 + 1000 + 60 + 60 + 50, 30000, true),
        &rules,
        &req,
    );
    assert!(p.fits);
    assert_eq!(p.used.linear, FIXED + 1500 + 1000 + 60 + 60 + 50);
}

#[test]
fn the_same_hunt_gets_the_same_plan() {
    let req = Request {
        rule_secrets: vec![(SHARK, 2)],
        ..three_picks()
    };
    for linear in [FIXED + FLOOR, 9000, 12000, 20000] {
        let a = plan(&pack(linear, 30000, true), &rules(true), &req);
        let b = plan(&pack(linear, 30000, true), &rules(true), &req);
        assert_eq!(a, b);
    }
}

#[test]
fn a_pack_without_the_figures_loads_as_before() {
    let rules = rules(true);
    let req = Request {
        rule_secrets: vec![(SHARK, 3)],
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, false), &rules, &req);
    assert!(p.fits && !p.planned);
    // Every ambient species, no limit but the setting's.
    assert!((1..=BRACH).all(|c| p.max[c] == Some(usize::MAX)));
    assert_eq!(p.ambients_max, 5);
    assert_eq!(p.huntables.len(), 3);
    assert!(p.huntables.iter().all(|h| h.1 == usize::MAX));
    // The instance's secrets whatever they take, the map's roamer when
    // it fits, no aquatic animal.
    assert_eq!(p.rule_secrets, vec![(SHARK, 3)]);
    assert_eq!(p.native_secrets, vec![(ROAMER, usize::MAX)]);
    assert!(p.aquatic.is_empty() && !p.loads(FISH));
    // The area's figure holds the ambient models' main memory.
    let models = 1500 + 1200 + 1000 + 900;
    assert_eq!(p.used.linear, FIXED + 700 + models);
    assert_eq!(p.used.heap, 21500 + 100 + 800 + 500 + 500 + 600);
    // Too little memory refuses the hunt as it did.
    let tight = plan(&pack(FIXED + 700 + 3700 - 1, 30000, false), &rules, &req);
    assert!(!tight.fits);
}

#[test]
fn a_survival_and_the_trophy_room_are_not_planned() {
    let req = Request {
        hunting: false,
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, true), &rules(true), &req);
    assert!(p.fits && !p.planned);
    assert!(p.native_secrets.is_empty() && p.rule_secrets.is_empty());
}

#[test]
fn the_herd_gets_the_plans_limits() {
    let req = Request {
        rule_secrets: vec![(SHARK, 3), (REX, 2)],
        ..three_picks()
    };
    let p = plan(&pack(20000, 30000, true), &rules(false), &req);
    let Some(caps) = p.caps() else {
        panic!("a planned hunt has caps");
    };
    // The hunter and the small ambient animals have no limit of their own.
    assert_eq!((caps[HUNTER], caps[1]), (None, None));
    assert_eq!(caps[BRACH], Some(3));
    // A pick's limit, the instance's secrets of its species on top.
    assert_eq!(caps[ALLO], Some(count(&p, ALLO)));
    assert_eq!(caps[REX], Some(count(&p, REX) + 2));
    // A species loaded for the instance's secrets alone: only those.
    assert_eq!(
        caps[SHARK],
        Some(3 + p.aquatic.iter().find(|a| a.0 == SHARK).map_or(0, |a| a.1))
    );
    // Without a plan the game's own populations stand.
    assert_eq!(
        plan(&pack(20000, 30000, false), &rules(false), &req).caps(),
        None
    );
}
