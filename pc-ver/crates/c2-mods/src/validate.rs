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
    if let Some(n) = names {
        unknown_names(r, n, &mut warn);
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
}

fn platform_support(r: &Recipe, p: Platform, warn: &mut impl FnMut(String, String)) {
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
