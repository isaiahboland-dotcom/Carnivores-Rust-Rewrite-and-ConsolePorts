//! The recipe editor's instance settings (owner request 2026-11-05): a new
//! hunter's points and each area's, animal's and weapon's price and score
//! (the recipe's `[settings.balance]`, edited as on the Balance tab), and
//! the areas, animals and weapons of each standalone game left out (its
//! entry's `skip`, picked as on the Add-ons tab).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use c2_mods::balance::Balance;
use c2_mods::recipe::{Recipe, RecipeMod};
use c2_mods::standalone::Parts;
use eframe::egui::{self, RichText};

use crate::app::{help, panel};
use crate::jobs::balance::Instance;
use crate::theme::*;

#[derive(Default)]
pub(super) struct Extras {
    /// Edits the recipe's balance (copied back on each change).
    balance: crate::balance::Editor,
    /// The instance's own values, and the game they were read for.
    values: Option<Balance>,
    read_for: Option<Instance>,
    reading: bool,
    error: Option<String>,
    /// Each standalone folder's parts (`None`: not a game).
    parts: HashMap<PathBuf, Option<Parts>>,
}

/// The game folder in `dir` (the folder as given, if none is found).
fn root(dir: &Path) -> PathBuf {
    c2_mods::pack::find_game_root(dir).unwrap_or_else(|| dir.to_path_buf())
}

/// The game `r` makes over `game` (the game, then its overlays) with the
/// entries' `folders`, as Play builds it; and the ids of the entries whose
/// folder isn't known (left out).
pub(super) fn instance(
    r: &Recipe,
    game: &[PathBuf],
    folders: &BTreeMap<String, PathBuf>,
) -> (Option<Instance>, Vec<String>) {
    let mut missing = Vec::new();
    let mut found = |m: &RecipeMod| {
        let f = folders.get(&m.id).cloned();
        if f.is_none() {
            missing.push(m.id.clone());
        }
        f
    };
    let Some(first) = game.first() else {
        return (None, Vec::new());
    };
    // The recipe's own overlays, when it lists them, over the game.
    let bases: Vec<PathBuf> = if r.overlays.is_empty() {
        game.to_vec()
    } else {
        let overlays: Vec<PathBuf> = r.overlays.iter().filter_map(&mut found).collect();
        std::iter::once(first.clone())
            .chain(overlays.iter().map(|d| root(d)))
            .collect()
    };
    let standalones = r
        .standalones
        .iter()
        .filter_map(|m| Some((root(&found(m)?), m.skip.clone())))
        .collect();
    let by_id: HashMap<&str, &RecipeMod> = r.mods.iter().map(|m| (m.id.as_str(), m)).collect();
    let mods = super::add_on_order(r)
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).and_then(|m| found(m)))
        .collect();
    let i = Instance {
        bases,
        standalones,
        mods,
        random_ambients: r.combine.as_ref().is_some_and(|c| c.ambients == "random"),
        scratch: crate::config::data_dir().join("recipe-balance"),
    };
    (Some(i), missing)
}

impl Extras {
    pub(super) fn new(r: &Recipe) -> Self {
        Extras {
            balance: crate::balance::Editor::over(r.settings.balance.clone().unwrap_or_default()),
            ..Default::default()
        }
    }

    pub(super) fn reading(&self) -> bool {
        self.reading
    }

    pub(super) fn values_read(&mut self, values: Balance) {
        self.values = Some(values);
        self.reading = false;
        self.error = None;
    }

    pub(super) fn failed(&mut self, error: &str) {
        if self.reading {
            self.error = Some(error.into());
            self.values = None;
            // Not read again until something changes.
        }
        self.reading = false;
    }

    /// Reads the instance's values off the UI thread when it changed.
    fn refresh(&mut self, i: &Instance, handle: &mut crate::jobs::Handle) {
        if self.reading || handle.busy() || self.read_for.as_ref() == Some(i) {
            return;
        }
        let job = i.clone();
        match handle.start_task("Reading the instance's prices and scores", move |_| {
            crate::jobs::balance::read_instance(job)
        }) {
            Ok(()) => {
                self.read_for = Some(i.clone());
                self.reading = true;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// The parts of each standalone game picked (in the Standalone
    /// games list): whether a choice changed.
    pub(super) fn parts_ui(
        &mut self,
        ui: &mut egui::Ui,
        standalones: &mut [&mut RecipeMod],
        folders: &BTreeMap<String, PathBuf>,
    ) -> bool {
        let mut changed = false;
        for m in standalones.iter_mut() {
            let Some(dir) = folders.get(&m.id) else {
                help(
                    ui,
                    &format!(
                        "{}: its folder isn't known yet (use the instance once) to pick its parts.",
                        m.id
                    ),
                );
                continue;
            };
            let p = self
                .parts
                .entry(dir.clone())
                .or_insert_with(|| c2_mods::standalone::parts(dir));
            match p {
                Some(p) => {
                    ui.label(RichText::new(&m.id).color(TEXT));
                    ui.indent(("recipe-parts", &m.id), |ui| {
                        changed |= crate::app::standalones::pick_parts(
                            ui,
                            egui::IdSalt::new(("recipe-pick-parts", &m.id)),
                            p,
                            &mut m.skip,
                        )
                    });
                }
                None => help(ui, &format!("{}: no game in {}.", m.id, dir.display())),
            }
        }
        changed
    }

    /// Starting points, prices and scores (a section that reads the
    /// instance's game when opened).
    pub(super) fn balance_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) {
        let n = r.settings.balance.as_ref().map_or(0, |b| {
            b.areas.len() + b.animals.len() + b.weapons.len() + usize::from(b.start.is_some())
        });
        let title = format!("Points and scores ({n} changed)");
        egui::CollapsingHeader::new(RichText::new(title).color(if n > 0 { GOLD } else { SAND }))
            .id_salt("recipe-edit-balance")
            .show(ui, |ui| {
                help(
                    ui,
                    "A new hunter's points, and the price and score of each area, animal and \
                     weapon in this instance (its parts left out are not listed). Kept in the \
                     recipe; used when the instance is played or built.",
                );
                let (inst, missing) = instance(r, game, folders);
                let Some(inst) = inst else {
                    ui.label(RichText::new("Set the game first (the Game tab).").color(RED));
                    return;
                };
                if !missing.is_empty() {
                    help(
                        ui,
                        &format!(
                            "Not read (folder not known yet: use the instance once): {}",
                            missing.join(", ")
                        ),
                    );
                }
                self.refresh(&inst, handle);
                if let Some(e) = &self.error {
                    ui.label(RichText::new(format!("Can't read the instance: {e}")).color(RED));
                    if ui.small_button("Try again").clicked() {
                        self.read_for = None;
                        self.error = None;
                    }
                    return;
                }
                let Some(values) = self.values.clone() else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Reading the instance's game...");
                    });
                    return;
                };
                if self.reading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        help(ui, "Reading the changed instance...");
                    });
                }
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .id_salt("recipe-edit-balance-rows")
                    .show(ui, |ui| {
                        panel(ui, |ui| {
                            if self.balance.edit_ui(ui, &values) {
                                let e = &self.balance.edits;
                                r.settings.balance = (!e.is_empty()).then(|| e.clone());
                            }
                        });
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> RecipeMod {
        RecipeMod {
            id: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_instance_takes_known_folders_in_load_order() {
        let mut r = Recipe {
            mods: vec![entry("a"), entry("b"), entry("c")],
            standalones: vec![entry("g")],
            ..Default::default()
        };
        r.load_order.order = vec!["c".into(), "a".into(), "b".into()];
        r.standalones[0].skip.areas = vec![2];
        let folders: BTreeMap<String, PathBuf> = [("a", "/m/a"), ("c", "/m/c"), ("g", "/m/g")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), PathBuf::from(v)))
            .collect();
        let (i, missing) = instance(&r, &["/game".into()], &folders);
        let i = i.expect("a game");
        assert_eq!(missing, ["b"]);
        assert_eq!(i.mods, [PathBuf::from("/m/c"), PathBuf::from("/m/a")]);
        assert_eq!(i.standalones.len(), 1);
        assert_eq!(i.standalones[0].1.areas, [2]);
        assert_eq!(i.bases, [PathBuf::from("/game")]);
        assert!(instance(&r, &[], &folders).0.is_none());
    }
}
