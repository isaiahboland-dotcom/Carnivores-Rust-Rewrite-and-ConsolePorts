//! The recipe editor's Advanced page: "Custom flee limits"
//! (`[settings.flee]`, `c2_mods::flee`). Bounded fleeing itself is a switch
//! in the game's options; this page only sets the limits it uses (opt-in,
//! globally or per animal) and always shows what the original game does, as a
//! starting point.

use std::collections::BTreeMap;
use std::path::PathBuf;

use c2_formats::flee as fmt;
use c2_mods::flee::{Animal, Flee, Limits};
use c2_mods::recipe::Recipe;
use eframe::egui::{self, RichText};

use super::instance::Extras;
use crate::app::{help, panel};
use crate::theme;

/// What the original does for the animal running AI `ai`, in one line.
pub(super) fn vanilla_text(ai: i32) -> String {
    let v = fmt::vanilla(ai);
    if !v.affected {
        return "Not affected: its own chase and retreat".into();
    }
    let secs = |ms: i32| format!("{:.0}", ms as f32 / 1000.0);
    let mut s = String::new();
    if let Some((a, b)) = v.alarm_ms {
        s += &format!("alarm {}-{} s", secs(a), secs(b));
    }
    if let Some(r) = v.refresh_ms {
        if !s.is_empty() {
            s += ", ";
        }
        s += &format!("re-armed to {} s while you are near", secs(r));
    }
    s + &format!(
        "; calls {}-{} s; legs of {} cells; total distance unlimited",
        secs(v.call_ms.0),
        secs(v.call_ms.1),
        fmt::LEG_UNITS / 256
    )
}

/// One limit as an optional value: "inherit" until set.
pub(super) fn limit_ui(
    ui: &mut egui::Ui,
    v: &mut Option<u32>,
    range: (u32, u32),
    inherited: u32,
    unit: &str,
) {
    let Some(x) = v else {
        if ui
            .button(RichText::new(format!("{inherited}{unit}")).weak())
            .on_hover_text("Inherited. Click to set.")
            .clicked()
        {
            *v = Some(inherited);
        }
        return;
    };
    let mut clear = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(x)
                .range(range.0..=range.1)
                .suffix(unit),
        );
        clear = ui.small_button("✕").on_hover_text("Inherit").clicked();
    });
    if clear {
        *v = None;
    }
}

fn limits_row(ui: &mut egui::Ui, l: &mut Limits, base: [u32; 4]) {
    limit_ui(
        ui,
        &mut l.max_distance,
        fmt::DISTANCE_RANGE,
        base[0],
        " cells",
    );
    limit_ui(ui, &mut l.max_duration, fmt::DURATION_RANGE, base[1], " s");
    limit_ui(ui, &mut l.chase_extension, fmt::CHASE_RANGE, base[2], " s");
    limit_ui(
        ui,
        &mut l.retrigger_lockout,
        fmt::LOCKOUT_RANGE,
        base[3],
        " s",
    );
}

fn animal_mut<'a>(f: &'a mut Flee, name: &str) -> &'a mut Animal {
    let i = match f.animals.iter().position(|a| a.name == name) {
        Some(i) => i,
        None => {
            f.animals.push(Animal {
                name: name.into(),
                ..Animal::default()
            });
            f.animals.len() - 1
        }
    };
    &mut f.animals[i]
}

impl Extras {
    /// The Advanced page's flee limits: whether one changed.
    pub(super) fn flee_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) -> bool {
        let mut f = r.settings.flee.clone().unwrap_or_default();
        let before = f.clone();
        ui.label(
            RichText::new("Fleeing: custom limits (experimental)")
                .color(theme::sand())
                .strong(),
        );
        ui.label(
            RichText::new(
                "Experimental on Nintendo 3DS and Wii U; PC is the tested platform. Players \
                 switch Bounded fleeing on in the game's options; these limits apply only then.",
            )
            .color(theme::gold()),
        );
        help(
            ui,
            "With Bounded fleeing on, scared animals aim inside the map and clear of cliffs and \
             water, a flight ends at a distance or time limit, and repeated noises cannot keep \
             an animal afraid. Without custom limits the built-in ones apply (24 cells, 20 s, \
             10 s chase extension, 4 s noise lockout).",
        );
        egui::CollapsingHeader::new("What the original game does")
            .id_salt("flee-vanilla")
            .show(ui, |ui| {
                help(
                    ui,
                    "The original has no distance or duration limit: an animal aims 8 cells \
                     straight away from you every frame, and its fear timer is re-armed by \
                     closeness and by every call or shot. Use these as a starting point.",
                );
                for (name, ai) in [
                    ("Parasaurolophus, Ankylosaurus, Stegosaurus", 10),
                    ("Triceratops (defensive)", 14),
                    ("Small animals (Moschops, Gallimimus, Dimetrodon)", 1),
                    ("Dialect hunting AIs", 99),
                    ("Carnivores", 13),
                ] {
                    ui.label(
                        RichText::new(format!("{name}: {}", vanilla_text(ai))).color(theme::text()),
                    );
                }
            });
        ui.checkbox(&mut f.enabled, "Custom flee limits")
            .on_hover_text("Off: the built-in limits apply when Bounded fleeing is on.");
        if f.enabled {
            ui.add_space(4.0);
            self.flee_limits(ui, r, &mut f, game, folders, handle);
        }
        if f == before {
            return false;
        }
        r.settings.flee = (f != Flee::default()).then_some(f);
        true
    }

    fn flee_limits(
        &mut self,
        ui: &mut egui::Ui,
        r: &Recipe,
        f: &mut Flee,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let defaults = fmt::Limits::default().resolved();
        egui::Grid::new("flee-global")
            .num_columns(5)
            .show(ui, |ui| {
                for h in [
                    "",
                    "Distance",
                    "Duration",
                    "Chase extension",
                    "Noise lockout",
                ] {
                    ui.label(RichText::new(h).color(theme::sand()));
                }
                ui.end_row();
                ui.label("All animals");
                limits_row(ui, &mut f.limits, defaults);
                ui.end_row();
            });
        if ui.button("Reset").clicked() {
            *f = Flee {
                enabled: true,
                ..Flee::default()
            };
        }
        let global = f.limits;
        let base = [
            global.max_distance.unwrap_or(defaults[0]),
            global.max_duration.unwrap_or(defaults[1]),
            global.chase_extension.unwrap_or(defaults[2]),
            global.retrigger_lockout.unwrap_or(defaults[3]),
        ];
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        if values.animals.is_empty() {
            return;
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Per animal").color(theme::text()));
        help(
            ui,
            "Unset values inherit the one above, then the built-in default.",
        );
        panel(ui, |ui| {
            egui::Grid::new("flee-animals")
                .striped(true)
                .num_columns(6)
                .show(ui, |ui| {
                    for h in [
                        "Animal",
                        "Distance",
                        "Duration",
                        "Chase extension",
                        "Noise lockout",
                        "Original",
                    ] {
                        ui.label(RichText::new(h).color(theme::sand()));
                    }
                    ui.end_row();
                    for a in &values.animals {
                        ui.label(&a.name);
                        let mut l = f
                            .animals
                            .iter()
                            .find(|x| x.name == a.name)
                            .map(|x| x.limits)
                            .unwrap_or_default();
                        let before = l;
                        limits_row(ui, &mut l, base);
                        ui.label(RichText::new(vanilla_text(a.ai.unwrap_or(99))).small());
                        ui.end_row();
                        if l != before {
                            animal_mut(f, &a.name).limits = l;
                        }
                    }
                });
        });
        f.animals.retain(|a| !a.limits.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_is_described_per_family() {
        assert!(vanilla_text(10).contains("alarm 6-13 s"));
        assert!(vanilla_text(14).contains("re-armed to 8 s"));
        assert!(vanilla_text(18).starts_with("Not affected"));
        assert!(vanilla_text(10).contains("unlimited"));
    }

    #[test]
    fn setting_a_value_adds_the_animal_and_clearing_drops_it() {
        let mut f = Flee::default();
        animal_mut(&mut f, "Hadro").limits.max_distance = Some(30);
        assert_eq!(f.animals.len(), 1);
        animal_mut(&mut f, "Hadro").limits.max_distance = None;
        f.animals.retain(|a| !a.limits.is_empty());
        assert!(f.animals.is_empty());
    }
}
