//! The recipe editor's instance settings: a new hunter's points and each
//! area's, animal's and weapon's price and score (the recipe's
//! `[settings.balance]`, edited as on the Balance tab), the areas, animals
//! and weapons of the base game left out (`[recipe.skip]`, so add-ons or
//! standalones can stand in for them), and those of each standalone game
//! (its entry's `skip`, picked as on the Add-ons tab).

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
    /// The base game's parts, and the folders (game, overlays) read.
    base_parts: Option<(Vec<PathBuf>, Option<Parts>)>,
    addon_names: Option<(Instance, Vec<(String, String)>)>,
}

/// The game folder in `dir` (the folder as given, if none is found).
fn root(dir: &Path) -> PathBuf {
    c2_mods::pack::find_game_root(dir).unwrap_or_else(|| dir.to_path_buf())
}

/// The base game Play builds on: `game` (the game, then its overlays), or
/// the game and the recipe's own overlays when it lists them (those with a
/// known folder).
fn bases(r: &Recipe, game: &[PathBuf], folders: &BTreeMap<String, PathBuf>) -> Vec<PathBuf> {
    match game.first() {
        Some(first) if !r.overlays.is_empty() => std::iter::once(first.clone())
            .chain(
                r.overlays
                    .iter()
                    .filter_map(|m| folders.get(&m.id))
                    .map(|d| root(d)),
            )
            .collect(),
        _ => game.to_vec(),
    }
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
    if game.is_empty() {
        return (None, Vec::new());
    }
    r.overlays.iter().for_each(|m| {
        found(m);
    });
    let bases = bases(r, game, folders);
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
        base_skip: r.recipe.skip.clone(),
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

    /// Reads the instance's values and parts again on the next frame.
    pub(super) fn refresh_list(&mut self) {
        self.read_for = None;
        self.error = None;
        self.parts.clear();
        self.base_parts = None;
        self.addon_names = None;
    }

    /// Inspect source entries before merging: merging discards duplicate names.
    pub(super) fn addon_collisions(
        &mut self,
        r: &Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
    ) -> Vec<(String, String)> {
        let (Some(i), _) = instance(r, game, folders) else {
            return Vec::new();
        };
        if let Some((previous, names)) = &self.addon_names {
            if previous == &i {
                return names.clone();
            }
        }
        let mut names = Vec::new();
        if let Ok(base) = c2_mods::pack::vanilla(&i.bases) {
            for m in &r.mods {
                let Some(dir) = folders.get(&m.id) else {
                    continue;
                };
                for path in c2_mods::pack::expand_addons(dir) {
                    let Ok(source) = c2_mods::pack::load_mod(&base, &path) else {
                        continue;
                    };
                    for change in &source.res_changes {
                        use c2_mods::res_merge::{block_id, Change};
                        if let Change::Add { section, block }
                        | Change::Modify { section, block, .. } = change
                        {
                            if section.eq_ignore_ascii_case("characters") {
                                if let Some(name) = block_id(block) {
                                    names.push((m.id.clone(), name));
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut counts = HashMap::new();
        for (_, name) in &names {
            *counts.entry(name.clone()).or_insert(0usize) += 1;
        }
        names.retain(|(_, name)| counts[name] > 1);
        names.sort();
        names.dedup();
        self.addon_names = Some((i, names.clone()));
        names
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

    /// The parts of the base game picked (its own areas, animals and
    /// weapons, left out so add-ons or standalones can stand in for them):
    /// whether a choice changed.
    pub(super) fn base_parts_ui(
        &mut self,
        ui: &mut egui::Ui,
        r: &mut Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
    ) -> bool {
        let dirs = bases(r, game, folders);
        if dirs.is_empty() {
            return false;
        }
        if self.base_parts.as_ref().is_none_or(|(d, _)| *d != dirs) {
            let p = c2_mods::standalone::base_parts(&dirs);
            self.base_parts = Some((dirs.clone(), p));
        }
        let Some((_, Some(p))) = &self.base_parts else {
            return false;
        };
        let tip = dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ui.label(RichText::new("The base game's parts").color(SAND).strong())
            .on_hover_text(tip);
        help(
            ui,
            "Untick its areas, animals or weapons to leave them out of this instance, and let \
             add-ons or standalone games stand in for them. What is kept is renumbered; add-ons \
             and standalone games come after it. Used on PC, in 3DS and Wii U packs.",
        );
        let s = &r.recipe.skip;
        let all_out = |n: usize, out: &[u32]| n > 0 && (1..=n as u32).all(|i| out.contains(&i));
        let gone: Vec<&str> = [
            ("areas", all_out(p.areas, &s.areas)),
            ("animals", all_out(p.huntables.len(), &s.huntables)),
            ("weapons", all_out(p.weapons.len(), &s.weapons)),
        ]
        .into_iter()
        .filter_map(|(k, all)| all.then_some(k))
        .collect();
        if !gone.is_empty() {
            ui.label(
                RichText::new(format!(
                    "All of the base game's {} are left out: add-ons or standalone games must \
                     bring their own.",
                    gone.join(" and ")
                ))
                .color(GOLD),
            );
        }
        crate::app::standalones::pick_parts(
            ui,
            egui::IdSalt::new("recipe-base-parts"),
            p,
            &mut r.recipe.skip,
        )
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
                        "{}: no folder yet (Browse, or Fix missing) to pick its parts.",
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

    /// Starting points, prices and scores (read from the instance's game
    /// when shown, again after "Refresh list").
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
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("Points and scores ({n} changed)")).color(if n > 0 {
                    GOLD
                } else {
                    SAND
                }),
            );
            if ui
                .add_enabled(!self.reading, egui::Button::new("Refresh list"))
                .on_hover_text(
                    "Read the areas, animals and weapons again from the base game and add-ons \
                     as set up on the other pages.",
                )
                .clicked()
            {
                self.refresh_list();
            }
        });
        help(
            ui,
            "A new hunter's points, and the price and score of each area, animal and weapon in \
             this instance (its parts left out are not listed). Kept in the recipe; used when the \
             instance is played or built.",
        );
        let Some(values) = self.read_values(ui, r, game, folders, handle) else {
            return;
        };
        panel(ui, |ui| {
            if self.balance.edit_ui(ui, &values) {
                let e = &self.balance.edits;
                r.settings.balance = (!e.is_empty()).then(|| e.clone());
            }
        });
    }

    /// The instance's areas, animals and weapons with their values, read
    /// off the UI thread (showing progress or errors until they are).
    pub(super) fn read_values(
        &mut self,
        ui: &mut egui::Ui,
        r: &Recipe,
        game: &[PathBuf],
        folders: &BTreeMap<String, PathBuf>,
        handle: &mut crate::jobs::Handle,
    ) -> Option<Balance> {
        let (inst, missing) = instance(r, game, folders);
        let Some(inst) = inst else {
            ui.label(RichText::new("Choose the base game first (Base game page).").color(RED));
            return None;
        };
        if !missing.is_empty() {
            help(
                ui,
                &format!(
                    "Not read (no folder yet: Add-ons page or Fix missing): {}",
                    missing.join(", ")
                ),
            );
        }
        self.refresh(&inst, handle);
        if let Some(e) = &self.error {
            ui.label(RichText::new(format!("Can't read the instance: {e}")).color(RED));
            if ui.small_button("Try again").clicked() {
                self.refresh_list();
            }
            return None;
        }
        let Some(values) = self.values.clone() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading the instance's game...");
            });
            return None;
        };
        if self.reading {
            ui.horizontal(|ui| {
                ui.spinner();
                help(ui, "Reading the changed instance...");
            });
        }
        Some(values)
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
