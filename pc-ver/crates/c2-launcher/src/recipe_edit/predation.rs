//! The recipe editor's Advanced page: "Carnivores hunt herbivores"
//! (`[settings.predation]`, `c2_mods::predation`): the switch, whom
//! carnivores go for first, carnivore fights, the food chain (by price, one
//! apex, or an order of the player's own) and each animal's tags.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_mods::balance::Balance;
use c2_mods::predation::{Animal, Predation};
use c2_mods::recipe::Recipe;
use eframe::egui::{self, RichText};

use super::instance::Extras;
use crate::app::{help, panel};
use crate::theme::*;

/// How the food chain is ranked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chain {
    Price,
    Apex,
    Custom,
}

/// The instance's animals and their prices (with the recipe's balance
/// edits), dearest first.
fn animals(values: &Balance, r: &Recipe) -> Vec<(String, i32)> {
    let edited = |name: &str| {
        r.settings
            .balance
            .as_ref()
            .and_then(|b| b.animals.iter().find(|a| a.name == name))
            .and_then(|a| a.price)
    };
    let mut v: Vec<(String, i32)> = values
        .animals
        .iter()
        .map(|a| (a.name.clone(), edited(&a.name).or(a.price).unwrap_or(0)))
        .collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.1));
    v
}

fn tags<'a>(p: &'a mut Predation, name: &str) -> &'a mut Animal {
    let i = match p.animals.iter().position(|a| a.name == name) {
        Some(i) => i,
        None => {
            p.animals.push(Animal {
                name: name.into(),
                ..Animal::default()
            });
            p.animals.len() - 1
        }
    };
    &mut p.animals[i]
}

/// Moves entry `from` of `list` to `to`.
fn shift(list: &mut Vec<String>, from: usize, to: usize) {
    if from < list.len() && to < list.len() && from != to {
        let x = list.remove(from);
        list.insert(to, x);
    }
}

impl Extras {
    /// The Advanced page's predation settings: whether one changed.
    pub(super) fn predation_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) -> bool {
        let mut p = r.settings.predation.clone().unwrap_or_default();
        let before = p.clone();
        ui.checkbox(
            &mut p.enabled,
            RichText::new("Carnivores hunt herbivores (experimental)")
                .color(SAND)
                .strong(),
        )
        .on_hover_text(
            "Not in the original game. Carnivores chase and kill the animals below them on the \
             food chain; their prey runs from them as from you. A kill leaves a body that is no \
             trophy of yours.",
        );
        ui.label(
            RichText::new(
                "Experimental on every platform. Where many animals spawn close together, chases \
                 and fights can pile up and get chaotic. On 3DS, Wii U, Android and VR it may also \
                 cause lag or game-breaking bugs; PC is where it is tested.",
            )
            .color(GOLD),
        );
        if p.enabled {
            ui.add_space(4.0);
            self.predation_rules(ui, r, &mut p, game, folders, handle);
        }
        // Untagged animals need not be kept.
        p.animals.retain(Animal::is_tagged);
        if p == before {
            return false;
        }
        r.settings.predation = (p != Predation::default()).then_some(p);
        true
    }

    fn predation_rules(
        &mut self,
        ui: &mut egui::Ui,
        r: &Recipe,
        p: &mut Predation,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        ui.label(RichText::new("Carnivores go for").color(TEXT));
        ui.horizontal_wrapped(|ui| {
            let prey = p.priority.eq_ignore_ascii_case("prey");
            if ui
                .radio(!prey, "The hunter first")
                .on_hover_text("An animal is hunted only while the carnivore is not after you.")
                .clicked()
            {
                p.priority.clear();
            }
            if ui
                .radio(prey, "Prey first")
                .on_hover_text("A carnivore after you turns on prey it senses.")
                .clicked()
            {
                p.priority = "prey".into();
            }
        });
        ui.checkbox(&mut p.carnivore_fights, "Carnivore on carnivore violence")
            .on_hover_text(
                "Carnivores also hunt carnivores priced at least 10% lower. Closer in price, \
                 they face off and the cheaper one backs off; at the same price they leave \
                 each other alone. Two after the same prey: the dearer one gets it, or the \
                 closer one when they are within 10%.",
            );
        ui.add_space(4.0);
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        let list = animals(&values, r);
        if list.is_empty() {
            help(ui, "This instance has no animals.");
            return;
        }
        food_chain_ui(ui, p, &list);
        ui.add_space(4.0);
        tags_ui(ui, p, &list);
    }
}

fn food_chain_ui(ui: &mut egui::Ui, p: &mut Predation, list: &[(String, i32)]) {
    let mut mode = if !p.chain.is_empty() {
        Chain::Custom
    } else if !p.apex.is_empty() {
        Chain::Apex
    } else {
        Chain::Price
    };
    ui.label(RichText::new("Food chain").color(TEXT));
    ui.horizontal_wrapped(|ui| {
        ui.radio_value(&mut mode, Chain::Price, "By price")
            .on_hover_text("A carnivore hunts herbivores priced the same or lower.");
        ui.radio_value(&mut mode, Chain::Apex, "One apex")
            .on_hover_text(
                "One animal hunts every other, whatever the prices; the rest go by price.",
            );
        ui.radio_value(&mut mode, Chain::Custom, "Custom order...")
            .on_hover_text("Put every animal in order yourself: each hunts the ones below it.");
    });
    match mode {
        Chain::Price => {
            p.apex.clear();
            p.chain.clear();
        }
        Chain::Apex => {
            p.chain.clear();
            if p.apex.is_empty() {
                p.apex = list[0].0.clone();
            }
            egui::ComboBox::from_id_salt("predation-apex")
                .selected_text(p.apex.as_str())
                .show_ui(ui, |ui| {
                    for (name, price) in list {
                        ui.selectable_value(&mut p.apex, name.clone(), format!("{name} ({price})"));
                    }
                });
        }
        Chain::Custom => {
            if p.chain.is_empty() {
                p.chain = list.iter().map(|(n, _)| n.clone()).collect();
            }
            // Animals added to the instance since go to the bottom; gone
            // ones leave.
            p.chain.retain(|n| list.iter().any(|(m, _)| m == n));
            for (n, _) in list {
                if !p.chain.contains(n) {
                    p.chain.push(n.clone());
                }
            }
            chain_list_ui(ui, &mut p.chain);
        }
    }
}

/// The custom order: drag a row, or use its arrows.
fn chain_list_ui(ui: &mut egui::Ui, chain: &mut Vec<String>) {
    help(
        ui,
        "Top: the apex, hunts everything. Bottom: prey to every carnivore. Drag to reorder.",
    );
    let mut moved: Option<(usize, usize)> = None;
    panel(ui, |ui| {
        let n = chain.len();
        for (i, name) in chain.iter().enumerate() {
            let row = ui.horizontal(|ui| {
                if ui
                    .add_enabled(i > 0, egui::Button::new("▲").small())
                    .clicked()
                {
                    moved = Some((i, i - 1));
                }
                if ui
                    .add_enabled(i + 1 < n, egui::Button::new("▼").small())
                    .clicked()
                {
                    moved = Some((i, i + 1));
                }
                let id = ui.scope_id().with(("predation-chain", i));
                ui.dnd_drag_source(id, i, |ui| {
                    ui.label(RichText::new(format!("{}. {name}", i + 1)).color(TEXT));
                });
            });
            let r = row.response;
            if let Some(from) = r.dnd_release_payload::<usize>() {
                moved = Some((*from, i));
            } else if r.dnd_hover_payload::<usize>().is_some() {
                ui.painter()
                    .hline(r.rect.x_range(), r.rect.top(), (2.0, GOLD));
            }
        }
    });
    if let Some((from, to)) = moved {
        shift(chain, from, to);
    }
}

/// Each animal's diet, scavenger and defensive tags.
fn tags_ui(ui: &mut egui::Ui, p: &mut Predation, list: &[(String, i32)]) {
    ui.label(RichText::new("Animals").color(TEXT));
    help(
        ui,
        "Diet Auto goes by the animal's AI: the raptor, Allosaurus, Spinosaurus, Ceratosaurus \
         and T-Rex AIs (and modded carnivore AIs) hunt; the rest are herbivores. Scavenger: \
         seeks out dead herbivores it senses, unless it senses a bigger carnivore, eats once \
         and runs off. Defensive (herbivores): charges a carnivore below it for 5 to 10 \
         seconds and kills it if it catches it, then rests 20 seconds.",
    );
    egui::Grid::new("predation-animals")
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            for h in ["Animal", "Price", "Diet", "Scavenger", "Defensive"] {
                ui.label(RichText::new(h).color(SAND));
            }
            ui.end_row();
            for (name, price) in list {
                let t = tags(p, name);
                ui.label(name);
                ui.label(price.to_string());
                let shown = match t.diet.as_str() {
                    "herbivore" => "Herbivore",
                    "carnivore" => "Carnivore",
                    _ => "Auto",
                };
                egui::ComboBox::from_id_salt(("predation-diet", name))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut t.diet, String::new(), "Auto");
                        ui.selectable_value(&mut t.diet, "herbivore".into(), "Herbivore");
                        ui.selectable_value(&mut t.diet, "carnivore".into(), "Carnivore");
                    });
                ui.checkbox(&mut t.scavenger, "");
                let can_defend = t.diet != "carnivore";
                ui.add_enabled(can_defend, egui::Checkbox::without_text(&mut t.defensive))
                    .on_disabled_hover_text("Herbivores only.");
                if !can_defend {
                    t.defensive = false;
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_move_up_down_and_by_drag() {
        let mut v: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        shift(&mut v, 3, 0);
        assert_eq!(v, ["d", "a", "b", "c"]);
        shift(&mut v, 1, 2);
        assert_eq!(v, ["d", "b", "a", "c"]);
        shift(&mut v, 9, 0);
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn animals_are_dearest_first_with_balance_edits() {
        let item = |n: &str, p: i32| c2_mods::balance::Item {
            name: n.into(),
            price: Some(p),
            ..Default::default()
        };
        let values = Balance {
            animals: vec![item("Raptor", 10), item("Rex", 30), item("Stego", 5)],
            ..Default::default()
        };
        let mut r = Recipe::default();
        r.settings.balance = Some(Balance {
            animals: vec![item("Stego", 50)],
            ..Default::default()
        });
        let names: Vec<String> = animals(&values, &r).into_iter().map(|a| a.0).collect();
        assert_eq!(names, ["Stego", "Rex", "Raptor"]);
    }
}
