//! "Pick parts" (owner request 2026-11-05): which of a game's areas,
//! huntables and weapons an instance keeps (`c2_mods::standalone::Skip`,
//! `c2-pc --skip`), for the base game and each standalone game in the
//! instance editor.
use super::*;

/// The dropdown for one standalone (`salt`: its id): whether a choice
/// changed. Also the recipe editor's, for an instance's standalones.
pub(crate) fn pick_parts(
    ui: &mut egui::Ui,
    salt: egui::IdSalt,
    p: &c2_mods::standalone::Parts,
    s: &mut c2_mods::standalone::Skip,
) -> bool {
    let kept = |all: usize, out: &[u32]| all - out.iter().filter(|&&n| n as usize <= all).count();
    let title = format!(
        "Pick parts: {} of {} areas, {} of {} animals, {} of {} weapons",
        kept(p.areas, &s.areas),
        p.areas,
        kept(p.huntables.len(), &s.huntables),
        p.huntables.len(),
        kept(p.weapons.len(), &s.weapons),
        p.weapons.len()
    );
    let mut changed = false;
    egui::CollapsingHeader::new(RichText::new(title).color(if s.is_empty() { DIM } else { GOLD }))
        .id_salt(salt)
        .show(ui, |ui| {
            let areas: Vec<String> = (1..=p.areas).map(|n| format!("Area {n}")).collect();
            changed |= pick_list(ui, "Areas", &areas, &mut s.areas);
            changed |= pick_list(ui, "Animals", &p.huntables, &mut s.huntables);
            changed |= pick_list(ui, "Weapons", &p.weapons, &mut s.weapons);
            if !s.is_empty() && ui.small_button("Take everything").clicked() {
                *s = Default::default();
                changed = true;
            }
        });
    changed
}

/// A row of checkboxes, one per name; `left_out` holds the unticked ones'
/// numbers (from 1).
fn pick_list(ui: &mut egui::Ui, title: &str, names: &[String], left_out: &mut Vec<u32>) -> bool {
    if names.is_empty() {
        return false;
    }
    let mut changed = false;
    ui.label(RichText::new(title).color(SAND));
    ui.horizontal_wrapped(|ui| {
        for (k, name) in names.iter().enumerate() {
            let n = k as u32 + 1;
            let mut on = !left_out.contains(&n);
            let label = if name.is_empty() {
                format!("#{n}")
            } else {
                name.clone()
            };
            if ui.checkbox(&mut on, label).changed() {
                if on {
                    left_out.retain(|&x| x != n);
                } else {
                    left_out.push(n);
                    left_out.sort_unstable();
                }
                changed = true;
            }
        }
    });
    changed
}
