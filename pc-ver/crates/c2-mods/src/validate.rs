//! Checks a recipe's settings and reports what looks wrong, as warnings
//! only: a recipe still loads and builds as before (values out of range are
//! clamped, names the game lacks are skipped, features a platform lacks are
//! left out there). Each warning names its key path in the recipe file.
//!
//! Three kinds of rule: values outside their range, names the built game
//! does not have (areas, animals, weapons as the balance editor lists
//! them), and features the chosen platform does not support.

use c2_formats::flee as flee_fmt;
use c2_vfs::Vfs;

use crate::recipe::Recipe;
use crate::ModError;

/// One finding: where it is in the recipe, and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub key: String,
    pub message: String,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.message)
    }
}

/// Where the recipe's game is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Pc,
    Android,
    ThreeDs,
    WiiU,
    Vr,
}

impl Platform {
    fn name(self) -> &'static str {
        match self {
            Platform::Pc => "PC",
            Platform::Android => "Android",
            Platform::ThreeDs => "3DS",
            Platform::WiiU => "Wii U",
            Platform::Vr => "VR",
        }
    }
}

/// The names a built game has, as the balance editor lists them.
#[derive(Debug, Clone, Default)]
pub struct Names {
    pub areas: Vec<String>,
    pub animals: Vec<String>,
    pub weapons: Vec<String>,
}

impl Names {
    pub fn read(vfs: &Vfs) -> Result<Self, ModError> {
        let b = crate::balance::read(vfs)?;
        let names = |items: Vec<crate::balance::Item>| items.into_iter().map(|i| i.name).collect();
        Ok(Names {
            areas: names(b.areas),
            animals: names(b.animals),
            weapons: names(b.weapons),
        })
    }
}

fn has(list: &[String], name: &str) -> bool {
    list.iter()
        .any(|n| n.trim().eq_ignore_ascii_case(name.trim()))
}

/// The warnings for `r`. `names`: the built game's (none: names are not
/// checked).
pub fn validate(r: &Recipe, names: Option<&Names>, platform: Platform) -> Vec<Warning> {
    let mut out = Vec::new();
    let mut warn = |key: String, message: String| out.push(Warning { key, message });
    ranges(r, &mut warn);
    hunt_ranges(r, &mut warn);
    dhc_values(r, &mut warn);
    if let Some(n) = names {
        unknown_names(r, n, &mut warn);
    }
    if let Some(w) = &r.settings.world {
        crate::world::check(w, names.map(|n| n.areas.as_slice()), &mut warn);
        crate::mission::check(w, names, &mut warn);
        crate::ecosystem::check(w, names.map(|n| n.animals.as_slice()), &mut warn);
    }
    platform_support(r, platform, &mut warn);
    out
}

fn ranges(r: &Recipe, warn: &mut impl FnMut(String, String)) {
    let s = &r.settings;
    if let Some(f) = &s.flee {
        let all = std::iter::once(("settings.flee".to_string(), &f.limits)).chain(
            f.animals
                .iter()
                .enumerate()
                .map(|(i, a)| (format!("settings.flee.animals[{i}]"), &a.limits)),
        );
        for (at, l) in all {
            for (key, v, (lo, hi)) in [
                ("max_distance", l.max_distance, flee_fmt::DISTANCE_RANGE),
                ("max_duration", l.max_duration, flee_fmt::DURATION_RANGE),
                ("chase_extension", l.chase_extension, flee_fmt::CHASE_RANGE),
                (
                    "retrigger_lockout",
                    l.retrigger_lockout,
                    flee_fmt::LOCKOUT_RANGE,
                ),
            ] {
                if let Some(v) = v.filter(|v| !(lo..=hi).contains(v)) {
                    warn(
                        format!("{at}.{key}"),
                        format!("{v} is outside {lo}..={hi}; {} is used", v.clamp(lo, hi)),
                    );
                }
            }
        }
    }
    if let Some(a) = s.behavior.as_ref().and_then(|b| b.awareness.as_ref()) {
        use c2_formats::behavior as b;
        let at = "settings.behavior.awareness";
        let all = std::iter::once((at.to_string(), a.values())).chain(
            a.animals
                .iter()
                .enumerate()
                .map(|(i, x)| (format!("{at}.animals[{i}]"), x.values())),
        );
        for (at, v) in all {
            for (key, v, (lo, hi)) in [
                ("suspicion", v.suspicion, b::SUSPICION_RANGE),
                (
                    "investigate_after",
                    v.investigate_after,
                    b::INVESTIGATE_RANGE,
                ),
                ("search", v.search, b::SEARCH_RANGE),
            ] {
                if let Some(v) = v.filter(|v| !(lo..=hi).contains(v)) {
                    warn(
                        format!("{at}.{key}"),
                        format!("{v} is outside {lo}..={hi}; {} is used", v.clamp(lo, hi)),
                    );
                }
            }
        }
    }
    if let Some(n) = s.rules.locks.secret_dinos.filter(|&n| n > 255) {
        warn(
            "settings.rules.locks.secret_dinos".into(),
            format!("{n} is over 255; 255 is used"),
        );
    }
    for (i, t) in s.rules.times.iter().enumerate() {
        if !c2_formats::rules::TIMES
            .iter()
            .any(|x| x.eq_ignore_ascii_case(t))
        {
            warn(
                format!("settings.rules.times[{i}]"),
                format!("{t:?} is not dawn, day or night; ignored"),
            );
        }
    }
    if let Some(p) = &s.predation {
        let pr = p.priority.trim();
        if !pr.is_empty() && !pr.eq_ignore_ascii_case("hunter") && !pr.eq_ignore_ascii_case("prey")
        {
            warn(
                "settings.predation.priority".into(),
                format!("{pr:?} is not hunter or prey; hunter is used"),
            );
        }
        for (i, a) in p.animals.iter().enumerate() {
            let d = a.diet.trim();
            if !d.is_empty()
                && !d.eq_ignore_ascii_case("herbivore")
                && !d.eq_ignore_ascii_case("carnivore")
            {
                warn(
                    format!("settings.predation.animals[{i}].diet"),
                    format!("{d:?} is not herbivore or carnivore; its AI's diet is used"),
                );
            }
        }
    }
    if let Some(b) = &s.balance {
        if let Some(v) = b.start.filter(|&v| v < 0) {
            warn(
                "settings.balance.start".into(),
                format!("{v} points is negative"),
            );
        }
        for (list, items) in [
            ("areas", &b.areas),
            ("animals", &b.animals),
            ("weapons", &b.weapons),
        ] {
            for (i, it) in items.iter().enumerate() {
                if let Some(v) = it.price.filter(|&v| v < 0) {
                    warn(
                        format!("settings.balance.{list}[{i}].price"),
                        format!("{v} is negative"),
                    );
                }
                if let Some(v) = it.score.filter(|&v| v < 0.0) {
                    warn(
                        format!("settings.balance.{list}[{i}].score"),
                        format!("{v} is negative"),
                    );
                }
            }
        }
    }
}

fn unknown_names(r: &Recipe, n: &Names, warn: &mut impl FnMut(String, String)) {
    let s = &r.settings;
    let mut check = |key: String, name: &str, list: &[String], what: &str| {
        if !name.trim().is_empty() && !has(list, name) {
            warn(
                key,
                format!("the game has no {what} named {name:?}; skipped"),
            );
        }
    };
    if let Some(b) = &s.balance {
        for (list, items, names, what) in [
            ("areas", &b.areas, &n.areas, "area"),
            ("animals", &b.animals, &n.animals, "animal"),
            ("weapons", &b.weapons, &n.weapons, "weapon"),
        ] {
            for (i, it) in items.iter().enumerate() {
                check(
                    format!("settings.balance.{list}[{i}].name"),
                    &it.name,
                    names,
                    what,
                );
            }
        }
    }
    for (i, a) in s.rules.areas.iter().enumerate() {
        check(
            format!("settings.rules.areas[{i}].area"),
            &a.area,
            &n.areas,
            "area",
        );
    }
    for (i, x) in s.rules.secrets.iter().enumerate() {
        check(
            format!("settings.rules.secrets[{i}].animal"),
            &x.animal,
            &n.animals,
            "animal",
        );
        for (j, a) in x.areas.iter().enumerate() {
            check(
                format!("settings.rules.secrets[{i}].areas[{j}]"),
                a,
                &n.areas,
                "area",
            );
        }
    }
    for (i, h) in s.rules.hidden.iter().enumerate() {
        check(
            format!("settings.rules.hidden[{i}]"),
            h,
            &n.animals,
            "animal",
        );
    }
    if let Some(p) = &s.predation {
        check(
            "settings.predation.apex".into(),
            &p.apex,
            &n.animals,
            "animal",
        );
        for (i, c) in p.chain.iter().enumerate() {
            check(
                format!("settings.predation.chain[{i}]"),
                c,
                &n.animals,
                "animal",
            );
        }
        for (i, a) in p.animals.iter().enumerate() {
            check(
                format!("settings.predation.animals[{i}].name"),
                &a.name,
                &n.animals,
                "animal",
            );
        }
    }
    if let Some(f) = &s.flee {
        for (i, a) in f.animals.iter().enumerate() {
            check(
                format!("settings.flee.animals[{i}].name"),
                &a.name,
                &n.animals,
                "animal",
            );
        }
    }
    if let Some(a) = s.behavior.as_ref().and_then(|b| b.awareness.as_ref()) {
        for (i, x) in a.animals.iter().enumerate() {
            check(
                format!("settings.behavior.awareness.animals[{i}].name"),
                &x.name,
                &n.animals,
                "animal",
            );
        }
    }
    if let Some(d) = &s.dhc {
        for (i, a) in d.animals.iter().enumerate() {
            check(
                format!("settings.dhc.animals[{i}].name"),
                &a.name,
                &n.animals,
                "animal",
            );
        }
        for (i, isl) in d.islands.iter().enumerate() {
            check(
                format!("settings.dhc.islands[{i}].area"),
                &isl.area,
                &n.areas,
                "area",
            );
            for (j, a) in isl.animals.iter().enumerate() {
                check(
                    format!("settings.dhc.islands[{i}].animals[{j}]"),
                    a,
                    &n.animals,
                    "animal",
                );
            }
            for (j, w) in isl.weapons.iter().enumerate() {
                check(
                    format!("settings.dhc.islands[{i}].weapons[{j}]"),
                    w,
                    &n.weapons,
                    "weapon",
                );
            }
        }
    }
    if let Some(h) = &s.hunt {
        for (i, a) in h.predators.iter().enumerate() {
            check(
                format!("settings.hunt.predators[{i}].name"),
                &a.name,
                &n.animals,
                "animal",
            );
        }
    }
}

/// `[settings.hunt]`: values outside their ranges and unknown reactions.
fn hunt_ranges(r: &Recipe, warn: &mut impl FnMut(String, String)) {
    use c2_formats::hardcore as h;
    let Some(s) = &r.settings.hunt else { return };
    let e = s.extraction.unwrap_or_default();
    let p = s.second_predator.unwrap_or_default();
    for (key, v, range) in [
        ("max_firearms", s.max_firearms, h::FIREARMS_RANGE),
        ("ammo_drops", s.ammo_drops, h::DROPS_RANGE),
        ("hesitation_s", s.hesitation_s, (0, h::SECONDS_RANGE.1)),
        ("back_angle", s.back_angle, h::BACK_ANGLE_RANGE),
        ("extraction.arrive_s", e.arrive_s, h::SECONDS_RANGE),
        ("extraction.hover_s", e.hover_s, h::SECONDS_RANGE),
        (
            "extraction.cooldown_s",
            e.cooldown_s,
            (0, h::SECONDS_RANGE.1),
        ),
        ("extraction.min_distance", e.min_distance, h::ZONE_GAP_RANGE),
        ("second_predator.window_s", p.window_s, h::SECONDS_RANGE),
    ] {
        if let Some(v) = v.filter(|v| !(range.0..=range.1).contains(v)) {
            warn(
                format!("settings.hunt.{key}"),
                format!("{v} is outside {}..={}; it is clamped", range.0, range.1),
            );
        }
    }
    if s.knife.is_some() && !s.experimental_knife {
        warn(
            "settings.hunt.knife".into(),
            "the knife is experimental and unfinished; it stays off unless \
             experimental_knife = true is set as well"
                .into(),
        );
    }
    for (i, a) in s.predators.iter().enumerate() {
        if h::Reaction::from_key(a.reaction.trim()).is_none() {
            warn(
                format!("settings.hunt.predators[{i}].reaction"),
                format!(
                    "\"{}\" is not a reaction (circle_lunge, circle_eat, ignore); the animal is skipped",
                    a.reaction
                ),
            );
        }
    }
}

/// `[settings.dhc]`: discovery modes, licence costs, equipment and times.
fn dhc_values(r: &Recipe, warn: &mut impl FnMut(String, String)) {
    use c2_formats::dhc as d;
    let Some(s) = &r.settings.dhc else { return };
    let mode = |key: String, v: &str, fallback: &str, warn: &mut dyn FnMut(String, String)| {
        if !v.trim().is_empty() && d::Discovery::from_key(v).is_none() {
            warn(
                key,
                format!("{v:?} is not off, sighting, scan or kill; {fallback} is used"),
            );
        }
    };
    let (lo, hi) = d::LICENCE_RANGE;
    let cost = |key: String, v: Option<u32>, warn: &mut dyn FnMut(String, String)| {
        if let Some(v) = v.filter(|v| !(lo..=hi).contains(v)) {
            warn(key, format!("{v} is outside {lo}..={hi}; {hi} is used"));
        }
    };
    mode("settings.dhc.discovery".into(), &s.discovery, "off", warn);
    cost("settings.dhc.licence_cost".into(), s.licence_cost, warn);
    for (i, a) in s.animals.iter().enumerate() {
        let at = format!("settings.dhc.animals[{i}]");
        mode(format!("{at}.discovery"), &a.discovery, "the default", warn);
        cost(format!("{at}.licence_cost"), a.licence_cost, warn);
    }
    for (i, isl) in s.islands.iter().enumerate() {
        for (j, e) in isl.equipment.iter().enumerate() {
            if !crate::dhc::EQUIPMENT
                .iter()
                .any(|x| x.eq_ignore_ascii_case(e.trim()))
            {
                warn(
                    format!("settings.dhc.islands[{i}].equipment[{j}]"),
                    format!("{e:?} is not equipment the menus offer; ignored"),
                );
            }
        }
        for (j, t) in isl.times.iter().enumerate() {
            if !d::TIMES.iter().any(|x| x.eq_ignore_ascii_case(t.trim())) {
                warn(
                    format!("settings.dhc.islands[{i}].times[{j}]"),
                    format!("{t:?} is not dawn, day or night; ignored"),
                );
            }
        }
    }
}

fn platform_support(r: &Recipe, p: Platform, warn: &mut impl FnMut(String, String)) {
    // The DHC terminal is in the PC game (and the Android build of it) and
    // on the 3DS.
    if r.settings.dhc.as_ref().is_some_and(|d| d.enabled)
        && matches!(p, Platform::WiiU | Platform::Vr)
    {
        warn(
            "settings.dhc".into(),
            format!(
                "{} has no DHC OS terminal yet; its original menu is used and these rules are unused",
                p.name()
            ),
        );
    }
    // Missions are listed by the DHC terminal only.
    if r.settings
        .world
        .as_ref()
        .is_some_and(|w| w.enabled && !w.missions.is_empty())
        && matches!(p, Platform::WiiU | Platform::Vr)
    {
        warn(
            "settings.world.missions".into(),
            format!(
                "{} has no DHC OS terminal yet; missions are hidden there",
                p.name()
            ),
        );
    }
    // Awareness is read by the PC game (and its Android build).
    if r.settings.behavior.as_ref().is_some_and(|b| b.is_active())
        && matches!(p, Platform::ThreeDs | Platform::WiiU | Platform::Vr)
    {
        warn(
            "settings.behavior.awareness".into(),
            format!(
                "{} does not apply awareness yet; animals notice and chase there as in the original",
                p.name()
            ),
        );
    }
    // The spawn ecosystem is read by the PC game (and its Android build).
    if r.settings
        .world
        .as_ref()
        .is_some_and(|w| w.enabled && w.ecosystem.as_ref().is_some_and(|e| e.is_active()))
        && matches!(p, Platform::ThreeDs | Platform::WiiU | Platform::Vr)
    {
        warn(
            "settings.world.ecosystem".into(),
            format!(
                "{} does not apply the spawn ecosystem yet; animals appear there as in the original",
                p.name()
            ),
        );
    }
    // The hardcore hunt ruleset runs in the PC game only so far.
    if r.settings.hunt.as_ref().is_some_and(|h| h.is_active())
        && matches!(p, Platform::ThreeDs | Platform::WiiU | Platform::Vr)
    {
        warn(
            "settings.hunt".into(),
            format!(
                "{} does not play the hardcore hunt ruleset yet; hunts there are regular hunts",
                p.name()
            ),
        );
    }
    // `c2-cook` refuses variant packs: their runtime is the PC game's.
    if !r.variants.is_empty() && matches!(p, Platform::ThreeDs | Platform::WiiU) {
        warn(
            "variants".into(),
            format!(
                "variants need the PC runtime; a {} pack can't be cooked with them",
                p.name()
            ),
        );
    }
    // Wii U and VR have no Bounded fleeing option yet: the limits are unused.
    if r.settings.flee.as_ref().is_some_and(|f| f.enabled)
        && matches!(p, Platform::WiiU | Platform::Vr)
    {
        warn(
            "settings.flee".into(),
            format!(
                "{} has no Bounded fleeing option yet; these limits are unused",
                p.name()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra: &str) -> Recipe {
        Recipe::parse(&format!("[recipe]\nid = \"r\"\n{extra}")).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn a_knife_without_its_experimental_switch_is_reported() {
        let off = parse("[settings.hunt]\nenabled = true\nknife = { dodge = true }\n");
        assert_eq!(keys(&off, Platform::Pc), ["settings.hunt.knife"]);
        let on = parse(
            "[settings.hunt]\nenabled = true\nexperimental_knife = true\n\
             knife = { dodge = true }\n",
        );
        assert!(keys(&on, Platform::Pc).is_empty());
    }

    #[test]
    fn hardcore_hunt_ranges_names_and_platforms() {
        let r = parse(
            "[settings.hunt]\nenabled = true\nmax_firearms = 99\nback_angle = 5\n\
             extraction = { hover_s = 0 }\n\
             [[settings.hunt.predators]]\nname = \"Velociraptor\"\nreaction = \"leap\"\n\
             [[settings.hunt.predators]]\nname = \"Gone\"\nreaction = \"ignore\"\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.hunt.max_firearms",
                "settings.hunt.back_angle",
                "settings.hunt.extraction.hover_s",
                "settings.hunt.predators[0].reaction",
                "settings.hunt.predators[1].name",
            ]
        );
        assert!(keys(&r, Platform::ThreeDs).contains(&"settings.hunt".to_string()));
        assert!(!keys(&r, Platform::Android).contains(&"settings.hunt".to_string()));
    }

    fn names() -> Names {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
        Names {
            areas: s(&["Vengar Peninsula", "Manya Jungle"]),
            animals: s(&["Velociraptor", "Tyrannosaurus Rex"]),
            weapons: s(&["Pistol"]),
        }
    }

    fn keys(r: &Recipe, p: Platform) -> Vec<String> {
        validate(r, Some(&names()), p)
            .into_iter()
            .map(|w| w.key)
            .collect()
    }

    #[test]
    fn dhc_values_names_and_platforms() {
        let r = parse(
            "[settings.dhc]\nenabled = true\ndiscovery = \"smell\"\nlicence_cost = 200000\n\
             [[settings.dhc.animals]]\nname = \"Moa\"\ndiscovery = \"kill\"\n\
             [[settings.dhc.islands]]\narea = \"Atlantis\"\nanimals = [\"Velociraptor\", \"Dodo\"]\n\
             weapons = [\"Pistol\", \"Laser\"]\nequipment = [\"Radar\", \"Jetpack\"]\n\
             times = [\"day\", \"noon\"]\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.dhc.discovery",
                "settings.dhc.licence_cost",
                "settings.dhc.islands[0].equipment[1]",
                "settings.dhc.islands[0].times[1]",
                "settings.dhc.animals[0].name",
                "settings.dhc.islands[0].area",
                "settings.dhc.islands[0].animals[1]",
                "settings.dhc.islands[0].weapons[1]",
            ]
        );
        assert!(keys(&r, Platform::WiiU).contains(&"settings.dhc".to_string()));
        assert!(!keys(&r, Platform::ThreeDs).contains(&"settings.dhc".to_string()));
        assert!(!keys(&r, Platform::Android).contains(&"settings.dhc".to_string()));
    }

    #[test]
    fn missions_are_checked_and_hidden_without_a_terminal() {
        let r = parse(
            "[settings.world]\nenabled = true\n\
             [[settings.world.missions]]\nid = \"m\"\narea = \"Atlantis\"\n",
        );
        let k = keys(&r, Platform::WiiU);
        assert!(k.contains(&"settings.world.missions".to_string()), "{k:?}");
        assert!(k.contains(&"settings.world.missions[0].entries".to_string()));
        let k = keys(&r, Platform::Pc);
        assert!(!k.contains(&"settings.world.missions".to_string()), "{k:?}");
    }

    #[test]
    fn awareness_is_checked_and_warned_about_where_it_is_not_applied() {
        let r = parse(
            "[settings.behavior.awareness]\nenabled = true\nsearch = 999\n\
             [[settings.behavior.awareness.animals]]\nname = \"Nobody\"\nsuspicion = 5\n",
        );
        let at = "settings.behavior.awareness".to_string();
        for p in [Platform::ThreeDs, Platform::WiiU, Platform::Vr] {
            assert!(keys(&r, p).contains(&at), "{p:?}");
        }
        let k = keys(&r, Platform::Pc);
        assert!(!k.contains(&at), "{k:?}");
        assert!(k.contains(&format!("{at}.search")), "{k:?}");
        assert!(k.contains(&format!("{at}.animals[0].suspicion")), "{k:?}");
    }

    #[test]
    fn the_ecosystem_is_checked_and_warned_about_where_it_is_not_applied() {
        let r = parse(
            "[settings.world]\nenabled = true\n\
             [settings.world.ecosystem]\nenabled = true\nmax_animals = 999\n",
        );
        let at = "settings.world.ecosystem".to_string();
        for p in [Platform::ThreeDs, Platform::WiiU, Platform::Vr] {
            assert!(keys(&r, p).contains(&at), "{p:?}");
        }
        for p in [Platform::Pc, Platform::Android] {
            let k = keys(&r, p);
            assert!(!k.contains(&at), "{p:?}: {k:?}");
            assert!(k.contains(&format!("{at}.max_animals")), "{k:?}");
        }
    }

    #[test]
    fn world_zones_are_checked_with_the_games_areas() {
        let r = parse(
            "[settings.world]\nenabled = true\n\
             [[settings.world.zones]]\nid = \"a\"\narea = \"Atlantis\"\nshape = \"circle\"\n\
             center = [10.0, 10.0]\nradius = 2000.0\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.world.zones[0].area",
                "settings.world.zones[0].shape"
            ]
        );
    }

    #[test]
    fn a_plain_recipe_has_no_warnings() {
        assert!(keys(&parse(""), Platform::Pc).is_empty());
        assert!(keys(&parse(""), Platform::WiiU).is_empty());
    }

    #[test]
    fn flee_limits_out_of_range() {
        let r = parse(
            "[settings.flee]\nenabled = true\nmax_distance = 500\nmax_duration = 20\n\
             [[settings.flee.animals]]\nname = \"Velociraptor\"\nretrigger_lockout = 99\n",
        );
        let w = validate(&r, None, Platform::Pc);
        assert_eq!(
            w.iter().map(|w| w.key.as_str()).collect::<Vec<_>>(),
            [
                "settings.flee.max_distance",
                "settings.flee.animals[0].retrigger_lockout"
            ]
        );
        assert!(w[0].message.contains("200 is used"), "{}", w[0]);
    }

    #[test]
    fn secret_dinos_over_255() {
        let r = parse("[settings.rules.locks]\nsecret_dinos = 300\n");
        assert_eq!(
            keys(&r, Platform::Pc),
            ["settings.rules.locks.secret_dinos"]
        );
    }

    #[test]
    fn unknown_times_of_day() {
        let r = parse("[settings.rules]\ntimes = [\"day\", \"noon\"]\n");
        assert_eq!(keys(&r, Platform::Pc), ["settings.rules.times[1]"]);
    }

    #[test]
    fn predation_priority_and_diet_values() {
        let r = parse(
            "[settings.predation]\nenabled = true\npriority = \"everyone\"\n\
             [[settings.predation.animals]]\nname = \"Velociraptor\"\ndiet = \"fish\"\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.predation.priority",
                "settings.predation.animals[0].diet"
            ]
        );
    }

    #[test]
    fn negative_balance_values() {
        let r = parse(
            "[settings.balance]\nstart = -5\n[[settings.balance.weapons]]\nname = \"Pistol\"\n\
             price = -1\n[[settings.balance.animals]]\nname = \"Velociraptor\"\nscore = -2.0\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.balance.start",
                "settings.balance.animals[0].score",
                "settings.balance.weapons[0].price"
            ]
        );
    }

    #[test]
    fn balance_entry_naming_an_absent_animal() {
        let r = parse(
            "[settings.balance]\n[[settings.balance.animals]]\nname = \"Velociraptor\"\nprice = 5\n\
             [[settings.balance.animals]]\nname = \"Unicorn\"\nprice = 9\n",
        );
        let w = validate(&r, Some(&names()), Platform::Pc);
        assert_eq!(w.len(), 1, "{w:?}");
        assert_eq!(w[0].key, "settings.balance.animals[1].name");
        assert!(w[0].message.contains("\"Unicorn\""), "{}", w[0]);
        // Without the game's names, names are not checked.
        assert!(validate(&r, None, Platform::Pc).is_empty());
    }

    #[test]
    fn spawn_rules_and_hidden_naming_absent_areas_or_animals() {
        let r = parse(
            "[settings.rules]\nhidden = [\"Dodo\"]\n\
             [[settings.rules.areas]]\narea = \"Atlantis\"\nmax_huntable = 2\n\
             [[settings.rules.secrets]]\nanimal = \"Dodo\"\ncount = 1\n\
             areas = [\"Manya Jungle\", \"Atlantis\"]\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.rules.areas[0].area",
                "settings.rules.secrets[0].animal",
                "settings.rules.secrets[0].areas[1]",
                "settings.rules.hidden[0]"
            ]
        );
    }

    #[test]
    fn predation_and_flee_naming_absent_animals() {
        let r = parse(
            "[settings.predation]\nenabled = true\napex = \"Dodo\"\nchain = [\"Velociraptor\", \"Moa\"]\n\
             [[settings.predation.animals]]\nname = \"Emu\"\nscavenger = true\n\
             [settings.flee]\nenabled = true\n[[settings.flee.animals]]\nname = \"Kiwi\"\n",
        );
        assert_eq!(
            keys(&r, Platform::Pc),
            [
                "settings.predation.apex",
                "settings.predation.chain[1]",
                "settings.predation.animals[0].name",
                "settings.flee.animals[0].name"
            ]
        );
    }

    #[test]
    fn variants_on_consoles() {
        let mut r = parse("");
        r.variants.push(crate::variants::Group {
            id: "rex".into(),
            menu_owner: String::new(),
            members: Vec::new(),
        });
        assert!(keys(&r, Platform::Pc).is_empty());
        assert!(keys(&r, Platform::Android).is_empty());
        assert_eq!(keys(&r, Platform::ThreeDs), ["variants"]);
        assert_eq!(keys(&r, Platform::WiiU), ["variants"]);
    }

    #[test]
    fn flee_limits_where_there_is_no_bounded_fleeing() {
        let r = parse("[settings.flee]\nenabled = true\n");
        assert!(keys(&r, Platform::Pc).is_empty());
        assert!(keys(&r, Platform::ThreeDs).is_empty());
        assert_eq!(keys(&r, Platform::WiiU), ["settings.flee"]);
        assert_eq!(keys(&r, Platform::Vr), ["settings.flee"]);
    }
}
