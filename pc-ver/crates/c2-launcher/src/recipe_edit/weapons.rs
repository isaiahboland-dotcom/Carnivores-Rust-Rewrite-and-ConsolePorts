//! The recipe editor's Advanced page: weapon restrictions
//! (`[[settings.weapon_restrictions]]`, `c2_mods::weapon_rules`): per
//! weapon, whether it may fire tranquilizer darts and whether its kills
//! count as trophies. Unset, a weapon is as the game has it (both allowed).

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_mods::recipe::Recipe;
use c2_mods::weapon_rules::WeaponRule;
use eframe::egui::{self, RichText};

use super::instance::Extras;
use super::rules::with_missing;
use crate::app::help;
use crate::theme::*;

const CHOICES: [(Option<bool>, &str); 3] = [
    (None, "As the game has it"),
    (Some(true), "Allowed"),
    (Some(false), "Not allowed"),
];

fn label(v: Option<bool>) -> &'static str {
    CHOICES.iter().find(|c| c.0 == v).map_or("", |c| c.1)
}

/// Weapon `name`'s restrictions (none kept: both unset).
pub(super) fn rule_of(rules: &[WeaponRule], name: &str) -> (Option<bool>, Option<bool>) {
    rules
        .iter()
        .find(|r| r.name.trim().eq_ignore_ascii_case(name.trim()))
        .map_or((None, None), |r| (r.allow_tranq, r.allow_trophy))
}

/// Keeps `name`'s restrictions (its entry removed when both are unset).
pub(super) fn set_rule(
    rules: &mut Vec<WeaponRule>,
    name: &str,
    (allow_tranq, allow_trophy): (Option<bool>, Option<bool>),
) {
    let i = rules
        .iter()
        .position(|r| r.name.trim().eq_ignore_ascii_case(name.trim()));
    match (i, allow_tranq.is_none() && allow_trophy.is_none()) {
        (Some(i), true) => {
            rules.remove(i);
        }
        (Some(i), false) => {
            (rules[i].allow_tranq, rules[i].allow_trophy) = (allow_tranq, allow_trophy)
        }
        (None, true) => {}
        (None, false) => rules.push(WeaponRule {
            name: name.trim().into(),
            allow_tranq,
            allow_trophy,
        }),
    }
}

fn choice(ui: &mut egui::Ui, salt: (&str, &str), v: &mut Option<bool>) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(label(*v))
        .show_ui(ui, |ui| {
            for (value, text) in CHOICES {
                ui.selectable_value(v, value, text);
            }
        });
}

impl Extras {
    /// The Advanced page's weapon restrictions.
    pub(super) fn weapons_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        ui.label(RichText::new("Weapon restrictions").color(SAND).strong());
        help(
            ui,
            "Per weapon: whether it may be loaded with tranquilizer darts, and whether its \
             kills count as trophies. A recipe that names a weapon the game lacks does not \
             build, so remove entries marked as not in this game. Kept by Play Vanilla.",
        );
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        let weapons: Vec<String> = values.weapons.iter().map(|w| w.name.clone()).collect();
        let rules = &mut r.settings.weapon_restrictions;
        let kept: Vec<String> = rules.iter().map(|w| w.name.clone()).collect();
        egui::Grid::new("weapon-restrictions")
            .striped(true)
            .num_columns(3)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Weapon").color(DIM));
                ui.label(RichText::new("Tranquilizer darts").color(DIM));
                ui.label(RichText::new("Trophies").color(DIM));
                ui.end_row();
                for (name, here) in with_missing(&weapons, &kept) {
                    let was = rule_of(rules, &name);
                    let mut now = was;
                    if here {
                        ui.label(RichText::new(&name).color(TEXT));
                    } else {
                        ui.label(RichText::new(format!("{name} (not in this game)")).color(RED));
                    }
                    choice(ui, ("weapon-tranq", &name), &mut now.0);
                    choice(ui, ("weapon-trophy", &name), &mut now.1);
                    ui.end_row();
                    if now != was {
                        set_rule(rules, &name, now);
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_with_nothing_set_keeps_no_entry() {
        let mut rules = Vec::new();
        set_rule(&mut rules, "Pistol", (None, None));
        assert!(rules.is_empty());
        set_rule(&mut rules, "Pistol", (Some(false), None));
        set_rule(&mut rules, " pistol ", (Some(false), Some(true)));
        assert_eq!(rules.len(), 1);
        assert_eq!(rule_of(&rules, "PISTOL"), (Some(false), Some(true)));
        assert_eq!(rule_of(&rules, "Shotgun"), (None, None));
        set_rule(&mut rules, "Pistol", (None, None));
        assert!(rules.is_empty());
    }
}
