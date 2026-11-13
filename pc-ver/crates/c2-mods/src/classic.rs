//! Play Vanilla (classic mode): an instance's game without its enhanced
//! rules. Which recipe sections those are, and the recipe a classic build
//! uses. The field-by-field treatment is in `docs/RECIPE-SETTINGS.md`.
//!
//! Game-defining parts (files, balance, weapon restrictions, skips, menu,
//! pictures, tuning, variants, times of day, hidden animals) always stay.

use c2_formats::rules::Lock;

use crate::recipe::Recipe;

/// Locks that turn an enhanced feature on when set to true.
pub const FORCING_LOCKS: [Lock; 5] = [
    Lock::DynamicTime,
    Lock::AdvancedObserver,
    Lock::MultiAmmo,
    Lock::Teleport,
    Lock::AllHuntable,
];

/// A recipe section that Play Vanilla leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advanced {
    /// `[settings.predation]`, enabled.
    Predation,
    /// `[settings.flee]`, enabled.
    FleeLimits,
    /// `[[settings.rules.areas]]` with a huntable cap.
    AreaCaps,
    /// `[[settings.rules.secrets]]` that spawn something.
    Secrets,
    /// `[settings.rules.locks]` forcing this feature on.
    ForcedOn(Lock),
}

impl Advanced {
    /// The section's place in the recipe file.
    pub fn key(self) -> String {
        match self {
            Advanced::Predation => "settings.predation".into(),
            Advanced::FleeLimits => "settings.flee".into(),
            Advanced::AreaCaps => "settings.rules.areas".into(),
            Advanced::Secrets => "settings.rules.secrets".into(),
            Advanced::ForcedOn(l) => format!("settings.rules.locks.{}", l.key()),
        }
    }

    /// What it is, for the launcher.
    pub fn label(self) -> String {
        match self {
            Advanced::Predation => "Predation (carnivores hunt herbivores)".into(),
            Advanced::FleeLimits => "Custom flee limits".into(),
            Advanced::AreaCaps => "Huntable animals capped per area".into(),
            Advanced::Secrets => "Secret animals per area".into(),
            Advanced::ForcedOn(l) => format!("Forces on: {}", lock_label(l)),
        }
    }
}

fn lock_label(l: Lock) -> &'static str {
    match l {
        Lock::DynamicTime => "dynamic time",
        Lock::AdvancedObserver => "advanced observer mode",
        Lock::MultiAmmo => "multi ammo",
        Lock::MobileRadar => "mobile radar",
        Lock::Teleport => "teleport",
        Lock::AllHuntable => "all dinos huntable",
        Lock::SecretDinos => "secret dinos",
        Lock::SurvivalBudget => "survival budget",
    }
}

/// The enhanced sections `r` has that change its game, in file order. Empty:
/// Play Vanilla plays the same rules as Play.
pub fn advanced_summary(r: &Recipe) -> Vec<Advanced> {
    let s = &r.settings;
    let mut out = Vec::new();
    if s.predation.as_ref().is_some_and(|p| p.enabled) {
        out.push(Advanced::Predation);
    }
    if s.flee.as_ref().is_some_and(|f| f.enabled) {
        out.push(Advanced::FleeLimits);
    }
    if s.rules.areas.iter().any(|a| a.max_huntable.is_some()) {
        out.push(Advanced::AreaCaps);
    }
    let spawns = |x: &crate::rules::SecretRule| x.count > 0 && !x.animal.trim().is_empty();
    if s.rules.secrets.iter().any(spawns) {
        out.push(Advanced::Secrets);
    }
    let locks = s.rules.locks.list();
    for l in FORCING_LOCKS {
        if locks.iter().any(|&(k, v)| k == l && v != 0) {
            out.push(Advanced::ForcedOn(l));
        }
    }
    out
}

/// The recipe a classic build uses: `r` without predation, custom flee
/// limits, spawn rules and locks. Everything else, `times` and `hidden`
/// included, is kept, so the classic `_RULES.TXT` holds only those.
pub fn classic_recipe(r: &Recipe) -> Recipe {
    let mut c = r.clone();
    let s = &mut c.settings;
    s.predation = None;
    s.flee = None;
    s.rules.areas.clear();
    s.rules.secrets.clear();
    s.rules.locks = Default::default();
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra: &str) -> Recipe {
        Recipe::parse(&format!(
            "[recipe]\nid = \"r\"\nbase = \"c2-vanilla\"\n{extra}"
        ))
        .unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn a_plain_recipe_has_none() {
        assert!(advanced_summary(&parse("")).is_empty());
    }

    #[test]
    fn game_defining_sections_are_never_listed() {
        let r = parse(
            "[recipe.skip]\nareas = [2]\n\
             [settings]\nmenu = \"mod\"\n\
             [settings.balance]\nstart = 200\n\
             [[settings.weapon_restrictions]]\nname = \"Pistol\"\nallow_trophy = false\n\
             [settings.rules]\ntimes = [\"day\"]\nhidden = [\"Velociraptor\"]\n\
             [settings.rules.locks]\nsecret_dinos = 4\nsurvival_budget = 50\n\
             mobile_radar = true\nteleport = false\n",
        );
        assert!(!r.recipe.skip.is_empty() && !r.settings.weapon_restrictions.is_empty());
        assert!(r.settings.balance.is_some());
        assert_eq!(advanced_summary(&r), []);
    }

    #[test]
    fn every_enhanced_section_and_forcing_lock_is_listed() {
        let r = parse(
            "[settings.predation]\nenabled = true\n\
             [settings.flee]\nenabled = true\n\
             [settings.rules.locks]\ndynamic_time = true\nteleport = true\nmulti_ammo = false\n\
             [[settings.rules.areas]]\narea = \"Vengar Peninsula\"\nmax_huntable = 3\n\
             [[settings.rules.secrets]]\nanimal = \"Tyrannosaurus Rex\"\ncount = 2\n",
        );
        assert_eq!(
            advanced_summary(&r),
            [
                Advanced::Predation,
                Advanced::FleeLimits,
                Advanced::AreaCaps,
                Advanced::Secrets,
                Advanced::ForcedOn(Lock::DynamicTime),
                Advanced::ForcedOn(Lock::Teleport),
            ]
        );
        assert_eq!(
            Advanced::ForcedOn(Lock::Teleport).key(),
            "settings.rules.locks.teleport"
        );
    }

    #[test]
    fn the_classic_recipe_keeps_only_game_defining_settings() {
        let r = parse(
            "[recipe.skip]\nareas = [2]\n\
             [settings]\nmenu = \"mod\"\n\
             [settings.balance]\nstart = 200\n\
             [[settings.weapon_restrictions]]\nname = \"Pistol\"\nallow_trophy = false\n\
             [settings.predation]\nenabled = true\n\
             [settings.flee]\nenabled = true\n\
             [settings.rules]\ntimes = [\"day\"]\nhidden = [\"Velociraptor\"]\n\
             [settings.rules.locks]\nteleport = true\nsecret_dinos = 4\n\
             [[settings.rules.areas]]\narea = \"Vengar Peninsula\"\nmax_huntable = 3\n\
             [[settings.rules.secrets]]\nanimal = \"Tyrannosaurus Rex\"\ncount = 2\n",
        );
        let c = classic_recipe(&r);
        assert!(advanced_summary(&c).is_empty());
        assert_eq!(c.recipe, r.recipe);
        assert_eq!(c.mods, r.mods);
        let (s, o) = (&c.settings, &r.settings);
        assert_eq!((&s.menu, &s.balance), (&o.menu, &o.balance));
        assert_eq!(s.weapon_restrictions, o.weapon_restrictions);
        assert_eq!((&s.pictures, &s.tuning), (&o.pictures, &o.tuning));
        assert_eq!(
            (&s.rules.times, &s.rules.hidden),
            (&o.rules.times, &o.rules.hidden)
        );
        assert!(s.predation.is_none() && s.flee.is_none());
        assert!(s.rules.locks.is_empty());
        assert!(s.rules.areas.is_empty() && s.rules.secrets.is_empty());
        // The rules file it writes: the times and hidden animals only.
        let f = s.rules.to_format(&[]);
        assert_eq!(f.times, 0b010);
        assert_eq!(f.hidden, ["Velociraptor"]);
        assert!(f.locks.is_empty() && f.huntable.is_empty() && f.secrets.is_empty());
    }

    #[test]
    fn sections_that_change_nothing_are_not_listed() {
        let r = parse(
            "[settings.predation]\nenabled = false\ncarnivore_fights = true\n\
             [settings.flee]\nenabled = false\n\
             [[settings.rules.areas]]\narea = \"Vengar Peninsula\"\n\
             [[settings.rules.secrets]]\nanimal = \"Tyrannosaurus Rex\"\ncount = 0\n",
        );
        assert_eq!(advanced_summary(&r), []);
    }
}
