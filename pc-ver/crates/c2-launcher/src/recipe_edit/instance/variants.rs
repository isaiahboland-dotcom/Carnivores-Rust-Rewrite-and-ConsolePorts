//! The Variant manager, at the foot of the Balance page. Each card is one
//! dinosaur of the hunt menu (its parent) with the dinosaurs that can spawn
//! in its place. Percentages are spawn chances and must total 100 on every
//! map. Edits go straight into the recipe, which saves them. The rules are
//! in `model`; map settings open in `window`.

mod model;
mod window;

use c2_mods::recipe::Recipe;
use c2_mods::variants::{distributions, Catalog, Dino, TOTAL};
use eframe::egui::{self, RichText};

use crate::app::{help, panel};
use crate::theme;

/// The window's open state: the member whose map settings are shown, as
/// (group ID, member ID).
#[derive(Default)]
pub(super) struct Editor {
    maps_open: Option<(String, String)>,
}

/// What the Balance page knows about the instance's dinosaurs and maps.
pub(super) struct Dinos<'a> {
    pub catalog: Option<&'a Catalog>,
    pub reading: bool,
    pub error: Option<&'a str>,
}

/// A change a card asks for. Applied after the cards are drawn.
enum Act {
    NewCard(Dino),
    Parent(usize, Dino),
    RemoveCard(usize),
    AddVariant(usize),
    SetVariant(usize, usize, Dino),
    RemoveVariant(usize, usize),
    OpenMaps(usize, usize),
}

/// `Name — Source`, so that same-named dinosaurs from different sources
/// can be told apart.
fn label_dino(d: &Dino) -> String {
    format!("{} — {}", d.name, d.source_name)
}

/// A member's dinosaur for display: its catalog entry if it has one.
fn label_member(catalog: &Catalog, m: &c2_mods::variants::Member) -> String {
    let key = model::key_of(m);
    match catalog.dinos.iter().find(|d| d.key() == key) {
        Some(d) => label_dino(d),
        None => format!("{} — {}", m.character, m.source),
    }
}

/// A map for display: its title and source, or its raw identity.
fn label_map(catalog: &Catalog, map: &c2_mods::variants::AreaId) -> String {
    match catalog.maps.iter().find(|c| &c.map == map) {
        Some(c) => format!("{} — {}", c.title, c.source_name),
        None => format!("{}:{}", map.source, map.area),
    }
}

/// A drop-down of dinosaurs; returns the one picked this frame.
fn dino_pick(
    ui: &mut egui::Ui,
    salt: (&'static str, usize, usize),
    current: &str,
    options: &[Dino],
) -> Option<Dino> {
    let mut picked = None;
    egui::ComboBox::from_id_salt(salt)
        .width(260.0)
        .selected_text(current)
        .show_ui(ui, |ui| {
            if options.is_empty() {
                ui.label(RichText::new("No dinosaurs left to choose").color(theme::dim()));
            }
            for d in options {
                if ui.selectable_label(false, label_dino(d)).clicked() {
                    picked = Some(d.clone());
                }
            }
        });
    picked
}

/// The Variant manager section.
pub(super) fn section_ui(ui: &mut egui::Ui, r: &mut Recipe, dinos: Dinos, state: &mut Editor) {
    let n = r.variants.len();
    egui::CollapsingHeader::new(
        RichText::new(format!("Variant manager ({n} cards)")).color(if n > 0 {
            theme::gold()
        } else {
            theme::sand()
        }),
    )
    .default_open(false)
    .show(ui, |ui| body(ui, r, dinos, state));
}

fn body(ui: &mut egui::Ui, r: &mut Recipe, dinos: Dinos, state: &mut Editor) {
    help(
        ui,
        "Each card is a dinosaur of the hunt menu. The variants on it can spawn in its place, \
         chosen by their percentages, which must total 100. A map setting changes a \
         dinosaur's percentage on one map; 0% turns it off there. Removing a card removes \
         only its recipe entry; no dinosaur's files are touched.",
    );
    ui.add_space(6.0);
    let Some(catalog) = dinos.catalog else {
        if let Some(e) = dinos.error {
            ui.label(
                RichText::new(format!("Can't read the dinosaurs and maps: {e}"))
                    .color(theme::red()),
            );
        } else if dinos.reading {
            ui.label(RichText::new("Reading the dinosaurs and maps…").color(theme::dim()));
        } else {
            ui.label(
                RichText::new("Choose the base game first (Base game page).").color(theme::dim()),
            );
        }
        return;
    };
    let mut acts = Vec::new();
    for gi in 0..r.variants.len() {
        card(ui, r, gi, catalog, &mut acts);
        ui.add_space(6.0);
    }
    new_card(ui, r, catalog, &mut acts);
    for act in acts {
        apply(r, act, state);
    }
    window::show(ui, r, catalog, &mut state.maps_open);
}

/// Changes the recipe as asked. Structural changes close the map settings,
/// whose member may no longer exist.
fn apply(r: &mut Recipe, act: Act, state: &mut Editor) {
    match act {
        Act::OpenMaps(gi, mi) => {
            if let Some(g) = r.variants.get(gi) {
                if let Some(m) = g.members.get(mi) {
                    state.maps_open = Some((g.id.clone(), m.id.clone()));
                }
            }
            return;
        }
        Act::NewCard(d) => model::add_group(r, &d),
        Act::Parent(gi, d) => model::set_parent(r, gi, &d),
        Act::RemoveCard(gi) => model::remove_group(r, gi),
        Act::AddVariant(gi) => model::add_variant(r, gi),
        Act::SetVariant(gi, mi, d) => model::set_variant(r, gi, mi, &d),
        Act::RemoveVariant(gi, mi) => model::remove_variant(r, gi, mi),
    }
    state.maps_open = None;
}

/// One dinosaur's card: the menu dinosaur and its percentage, then its
/// variants with theirs.
fn card(ui: &mut egui::Ui, r: &mut Recipe, gi: usize, catalog: &Catalog, acts: &mut Vec<Act>) {
    // What each row may choose, read before the card borrows the recipe.
    let options: Vec<Vec<Dino>> = (0..r.variants[gi].members.len())
        .map(|mi| {
            model::choices(&catalog.dinos, r, Some((gi, mi)))
                .into_iter()
                .cloned()
                .collect()
        })
        .collect();
    let g = &mut r.variants[gi];
    let owner = g.members.iter().position(|m| m.id == g.menu_owner);
    let total = distributions(g).first().map_or(0.0, |d| d.total);
    let on_track = (total - TOTAL).abs() < 1e-6;
    let parent = match owner {
        Some(oi) => label_member(catalog, &g.members[oi]),
        None => "No menu dinosaur".to_string(),
    };
    let card_id = g.id.clone();
    ui.push_id(&card_id, |ui| {
        panel(ui, |ui| {
            let header = RichText::new(format!("{parent}   ·   {total:.1}%")).color(if on_track {
                theme::text()
            } else {
                theme::red()
            });
            egui::CollapsingHeader::new(header)
                .default_open(true)
                .show(ui, |ui| {
                    let Some(oi) = owner else {
                        ui.label(
                            RichText::new("This card has no menu dinosaur: remove it.")
                                .color(theme::red()),
                        );
                        if ui.small_button("Remove card").clicked() {
                            acts.push(Act::RemoveCard(gi));
                        }
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.label("Menu dinosaur");
                        if let Some(d) = dino_pick(ui, ("parent", 0, 0), &parent, &options[oi]) {
                            acts.push(Act::Parent(gi, d));
                        }
                        ui.label("Spawn");
                        ui.add(
                            egui::DragValue::new(&mut g.members[oi].weight)
                                .range(0.0..=TOTAL)
                                .speed(0.5)
                                .suffix("%"),
                        );
                        if ui.small_button("Remove card").clicked() {
                            acts.push(Act::RemoveCard(gi));
                        }
                    });
                    for (mi, row_options) in options.iter().enumerate() {
                        if mi == oi {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            let current = if g.members[mi].character.trim().is_empty() {
                                "Choose a dinosaur".to_string()
                            } else {
                                label_member(catalog, &g.members[mi])
                            };
                            if let Some(d) =
                                dino_pick(ui, ("variant", mi, 0), &current, row_options)
                            {
                                acts.push(Act::SetVariant(gi, mi, d));
                            }
                            ui.add(
                                egui::DragValue::new(&mut g.members[mi].weight)
                                    .range(0.0..=TOTAL)
                                    .speed(0.5)
                                    .suffix("%"),
                            );
                            let m = &g.members[mi];
                            let maps = match (m.maps.is_some(), m.map_weights.len()) {
                                (false, 0) => "Map settings".to_string(),
                                (true, n) => format!("Map settings ({n}, limited)"),
                                (false, n) => format!("Map settings ({n})"),
                            };
                            if ui.button(maps).clicked() {
                                acts.push(Act::OpenMaps(gi, mi));
                            }
                            if ui.small_button("Remove").clicked() {
                                acts.push(Act::RemoveVariant(gi, mi));
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Add variant").clicked() {
                            acts.push(Act::AddVariant(gi));
                        }
                        let (text, color) = if on_track {
                            (format!("Total {total:.1}%"), theme::dim())
                        } else {
                            (
                                format!("Total {total:.1}%: must be {TOTAL:.0}%"),
                                theme::red(),
                            )
                        };
                        ui.label(RichText::new(text).color(color));
                    });
                    for d in distributions(g) {
                        if let Some(map) = d.map.as_ref().filter(|_| (d.total - TOTAL).abs() > 1e-6)
                        {
                            ui.label(
                                RichText::new(format!(
                                    "On {}: {:.1}% (must be {TOTAL:.0}%)",
                                    label_map(catalog, map),
                                    d.total
                                ))
                                .color(theme::red()),
                            );
                        }
                    }
                });
        });
    });
}

/// The bottom picker: choosing a dinosaur starts a new card for it.
fn new_card(ui: &mut egui::Ui, r: &Recipe, catalog: &Catalog, acts: &mut Vec<Act>) {
    let options: Vec<Dino> = model::choices(&catalog.dinos, r, None)
        .into_iter()
        .cloned()
        .collect();
    ui.horizontal(|ui| {
        ui.label("New card for");
        if let Some(d) = dino_pick(ui, ("new", 0, 0), "Choose a dinosaur", &options) {
            acts.push(Act::NewCard(d));
        }
    });
}
