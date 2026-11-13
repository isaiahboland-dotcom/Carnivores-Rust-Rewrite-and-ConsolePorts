//! The map settings window of one variant: its percentage on each map that
//! differs from its default, with each such map's total.

use c2_mods::recipe::Recipe;
use c2_mods::variants::{distributions, weight_on, Catalog, TOTAL};
use eframe::egui::{self, RichText};

use super::{label_map, label_member, model};
use crate::app::help;
use crate::theme::*;

/// Shows the window for the open member, if any. Closing it, or a member
/// that no longer exists, clears `open`.
pub(super) fn show(
    ui: &mut egui::Ui,
    r: &mut Recipe,
    catalog: &Catalog,
    open: &mut Option<(String, String)>,
) {
    let Some((gid, mid)) = open.clone() else {
        return;
    };
    let Some(gi) = r.variants.iter().position(|g| g.id == gid) else {
        *open = None;
        return;
    };
    let Some(mi) = r.variants[gi].members.iter().position(|m| m.id == mid) else {
        *open = None;
        return;
    };
    let title = format!(
        "Map settings: {}",
        label_member(catalog, &r.variants[gi].members[mi])
    );
    let mut shown = true;
    egui::Window::new(title)
        .open(&mut shown)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ui.ctx(), |ui| content(ui, r, gi, mi, catalog));
    if !shown {
        *open = None;
    }
}

fn content(ui: &mut egui::Ui, r: &mut Recipe, gi: usize, mi: usize, catalog: &Catalog) {
    help(
        ui,
        "A percentage here replaces this dinosaur's default on that map. 0% turns it off there. \
         Each map's percentages must total 100.",
    );
    ui.add_space(4.0);
    {
        let g = &mut r.variants[gi];
        let m = &mut g.members[mi];
        ui.label(format!("Default: {:.1}%", weight_on(m, None)));
        if m.maps.is_some() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Limited to the maps set in the recipe file.").color(GOLD));
                if ui.small_button("Lift limit").clicked() {
                    model::clear_limit(m);
                }
            });
        }
        ui.add_space(6.0);

        let mut drop = None;
        for k in 0..m.map_weights.len() {
            // Maps other overrides already use are not offered again.
            let taken: Vec<_> = m
                .map_weights
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .map(|(_, w)| w.map.clone())
                .collect();
            ui.horizontal(|ui| {
                let current = label_map(catalog, &m.map_weights[k].map);
                egui::ComboBox::from_id_salt(("map", k))
                    .width(230.0)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for c in &catalog.maps {
                            if taken.contains(&c.map) {
                                continue;
                            }
                            let picked = m.map_weights[k].map == c.map;
                            if ui
                                .selectable_label(picked, label_map(catalog, &c.map))
                                .clicked()
                            {
                                m.map_weights[k].map = c.map.clone();
                            }
                        }
                    });
                ui.add(
                    egui::DragValue::new(&mut m.map_weights[k].weight)
                        .range(0.0..=TOTAL)
                        .speed(0.5)
                        .suffix("%"),
                );
                if ui.small_button("Remove").clicked() {
                    drop = Some(k);
                }
            });
        }
        if let Some(k) = drop {
            m.map_weights.remove(k);
        }

        let free = model::free_maps(&catalog.maps, m);
        match free.first() {
            Some(first) => {
                if ui.button("Add map setting").clicked() {
                    let default = weight_on(m, None);
                    model::set_override(m, first.clone(), default);
                }
            }
            None => {
                ui.label(RichText::new("Every map already has a setting.").color(DIM));
            }
        }
    }

    // The member's edits are done: each map's total is read after them.
    ui.separator();
    let g = &r.variants[gi];
    let totals = distributions(g);
    let m = &g.members[mi];
    if m.map_weights.is_empty() {
        ui.label(RichText::new("No map settings: the default applies on every map.").color(DIM));
    }
    for w in &m.map_weights {
        let total = totals
            .iter()
            .find(|d| d.map.as_ref() == Some(&w.map))
            .map_or(0.0, |d| d.total);
        let (text, color) = if (total - TOTAL).abs() < 1e-6 {
            (format!("{}: {total:.1}%", label_map(catalog, &w.map)), DIM)
        } else {
            (
                format!(
                    "{}: {total:.1}% (must be {TOTAL:.0}%)",
                    label_map(catalog, &w.map)
                ),
                RED,
            )
        };
        ui.label(RichText::new(text).color(color));
    }
}
