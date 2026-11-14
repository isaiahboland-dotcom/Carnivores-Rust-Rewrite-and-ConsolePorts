//! A list of names picked from the instance's own (animals, weapons, areas,
//! ids), as a button that opens a menu of tick boxes. Names kept from
//! another game stay listed, marked, so they can be unticked.

use std::hash::Hash;

use eframe::egui;

use super::rules::with_missing;

/// The button's text: what `list` holds, or `none` when it is empty.
pub(super) fn summary(list: &[String], none: &str) -> String {
    match list {
        [] => none.to_string(),
        [a] => a.clone(),
        [a, b] => format!("{a}, {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

/// Ticks `name` in `list` on or off (case ignored).
pub(super) fn set(list: &mut Vec<String>, name: &str, on: bool) {
    let has = list.iter().any(|x| x.eq_ignore_ascii_case(name));
    if on && !has {
        list.push(name.to_string());
    } else if !on {
        list.retain(|x| !x.eq_ignore_ascii_case(name));
    }
}

/// The menu for `list` over `all`: whether it changed.
pub(super) fn names_menu(
    ui: &mut egui::Ui,
    salt: impl Hash + std::fmt::Debug,
    list: &mut Vec<String>,
    all: &[String],
    none: &str,
) -> bool {
    let mut changed = false;
    ui.push_id(salt, |ui| {
        ui.menu_button(summary(list, none), |ui| {
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    if all.is_empty() && list.is_empty() {
                        ui.label("Nothing to pick from.");
                    }
                    for (n, here) in with_missing(all, list) {
                        let mut on = list.iter().any(|x| x.eq_ignore_ascii_case(&n));
                        let text = if here {
                            n.clone()
                        } else {
                            format!("{n} (not in this game)")
                        };
                        if ui.checkbox(&mut on, text).changed() {
                            set(list, &n, on);
                            changed = true;
                        }
                    }
                });
        });
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_names_two_and_counts_the_rest() {
        let l: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        assert_eq!(summary(&[], "every animal"), "every animal");
        assert_eq!(summary(&l[..2], ""), "a, b");
        assert_eq!(summary(&l, ""), "a, b and 2 more");
    }

    #[test]
    fn a_name_is_ticked_once_whatever_its_case() {
        let mut l = vec!["Rex".to_string()];
        set(&mut l, "rex", true);
        set(&mut l, "Moa", true);
        assert_eq!(l, ["Rex", "Moa"]);
        set(&mut l, "REX", false);
        assert_eq!(l, ["Moa"]);
    }
}
