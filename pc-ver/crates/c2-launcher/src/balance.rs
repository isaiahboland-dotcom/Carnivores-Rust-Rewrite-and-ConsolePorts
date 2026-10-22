//! The balance editor: the chosen game's areas, animals and weapons with
//! their prices and scores (read with `c2-mods`, as `c2-pc balance` does),
//! edited into `balance.toml` in the data folder. Play and the 3DS build
//! pass that file on (`--balance`); only values that differ from the game's
//! are kept in it, by name, so it survives add-ons being reordered.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use c2_mods::balance::{Balance, Item};
use eframe::egui::{self, RichText};

use crate::app::{help, panel};
use crate::theme::*;

/// The game's own values, read off the UI thread.
type Pending = Arc<Mutex<Option<Result<Balance, String>>>>;

pub fn path() -> PathBuf {
    crate::config::data_dir().join("balance.toml")
}

/// The game (base, overlays, add-ons) combined in memory, and its values.
fn read_game(bases: &[PathBuf], mods: &[PathBuf]) -> Result<Balance, String> {
    let v = c2_mods::vanilla(bases).map_err(|e| e.to_string())?;
    let sources = mods
        .iter()
        .map(|m| c2_mods::load_mod(&v, m).map_err(|e| format!("{}: {e}", m.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let pack = c2_mods::build(bases, sources).map_err(|e| e.to_string())?;
    c2_mods::balance::read(&pack.vfs).map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct Editor {
    /// The player's changes.
    pub edits: Balance,
    /// The game's own values, and the folders they are for.
    values: Option<Balance>,
    for_game: Vec<PathBuf>,
    pending: Option<Pending>,
    error: Option<String>,
    filter: String,
    /// The mobile game's values for this game's entries (written by the
    /// APK import, `mobile-values.toml`), offered where they differ.
    pub mobile: Option<Balance>,
}

/// The values a balance item can carry: name, read, write.
type Field = (
    &'static str,
    fn(&Item) -> Option<f64>,
    fn(&mut Item, Option<f64>),
);
const FIELDS: [Field; 7] = [
    (
        "price",
        |i| i.price.map(f64::from),
        |i, v| i.price = v.map(|v| v.round() as i32),
    ),
    (
        "score",
        |i| i.score.map(f64::from),
        |i, v| i.score = v.map(|v| v as f32),
    ),
    (
        "health",
        |i| i.health.map(f64::from),
        |i, v| i.health = v.map(|v| v as f32),
    ),
    (
        "power",
        |i| i.power.map(f64::from),
        |i, v| i.power = v.map(|v| v as f32),
    ),
    (
        "precision",
        |i| i.prec.map(f64::from),
        |i, v| i.prec = v.map(|v| v as f32),
    ),
    (
        "rate",
        |i| i.rate.map(f64::from),
        |i, v| i.rate = v.map(|v| v as f32),
    ),
    (
        "shots",
        |i| i.shots.map(f64::from),
        |i, v| i.shots = v.map(|v| v as f32),
    ),
];

/// An entry where the mobile game differs: its list, name, and per field
/// (index in [`FIELDS`]) the game's and the mobile value.
type Difference = (List, String, Vec<(usize, f64, f64)>);

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}").trim_end_matches('0').to_string()
    }
}

/// Which list a row is in.
#[derive(Clone, Copy, PartialEq)]
enum List {
    Areas,
    Animals,
    Weapons,
}

impl Editor {
    pub fn load() -> Self {
        let edits = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| Balance::from_toml(&t).ok())
            .unwrap_or_default();
        Self {
            edits,
            ..Default::default()
        }
    }

    /// The balance file to pass on, if it changes anything.
    pub fn file(&self) -> Option<PathBuf> {
        let p = path();
        (!self.edits.is_empty() && p.exists()).then_some(p)
    }

    fn store(&self) -> Result<(), String> {
        let p = path();
        if self.edits.is_empty() {
            if p.exists() {
                std::fs::remove_file(&p).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        std::fs::create_dir_all(crate::config::data_dir()).map_err(|e| e.to_string())?;
        std::fs::write(&p, self.edits.to_toml()).map_err(|e| e.to_string())
    }

    /// Reads the game's values again when the folders changed.
    fn refresh(&mut self, bases: &[PathBuf], mods: &[PathBuf], ctx: &egui::Context) {
        if let Some(p) = &self.pending {
            let done = p.lock().ok().and_then(|mut s| s.take());
            match done {
                Some(Ok(v)) => {
                    (self.values, self.error, self.pending) = (Some(v), None, None);
                }
                Some(Err(e)) => (self.values, self.error, self.pending) = (None, Some(e), None),
                None => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
            }
        }
        let key: Vec<PathBuf> = bases.iter().chain(mods).cloned().collect();
        if key == self.for_game || bases.is_empty() {
            return;
        }
        self.for_game = key;
        let p: Pending = Arc::default();
        self.pending = Some(p.clone());
        let (bases, mods) = (bases.to_vec(), mods.to_vec());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = read_game(&bases, &mods);
            if let Ok(mut s) = p.lock() {
                *s = Some(r);
            }
            ctx.request_repaint();
        });
    }

    fn edits_mut(&mut self, list: List) -> &mut Vec<Item> {
        match list {
            List::Areas => &mut self.edits.areas,
            List::Animals => &mut self.edits.animals,
            List::Weapons => &mut self.edits.weapons,
        }
    }

    fn edit_of(&self, list: List, name: &str) -> Option<&Item> {
        let l = match list {
            List::Areas => &self.edits.areas,
            List::Animals => &self.edits.animals,
            List::Weapons => &self.edits.weapons,
        };
        l.iter().find(|i| i.name.eq_ignore_ascii_case(name))
    }

    /// Records a value (`None`: back to the game's), dropping rows that no
    /// longer change anything.
    fn set(
        &mut self,
        list: List,
        name: &str,
        price: Option<Option<i32>>,
        score: Option<Option<f32>>,
    ) {
        let l = self.edits_mut(list);
        let i = match l.iter().position(|i| i.name.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                l.push(Item {
                    name: name.to_string(),
                    ..Default::default()
                });
                l.len() - 1
            }
        };
        if let Some(p) = price {
            l[i].price = p;
        }
        if let Some(s) = score {
            l[i].score = s;
        }
        l.retain(Item::changes);
    }

    /// Where the mobile game differs from this game: per entry, the fields
    /// and both values (game, mobile).
    fn differences(&self, game: &Balance) -> Vec<Difference> {
        let Some(m) = &self.mobile else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (list, mine, theirs) in [
            (List::Areas, &game.areas, &m.areas),
            (List::Animals, &game.animals, &m.animals),
            (List::Weapons, &game.weapons, &m.weapons),
        ] {
            for t in theirs {
                let Some(g) = mine.iter().find(|g| g.name.eq_ignore_ascii_case(&t.name)) else {
                    continue;
                };
                let diffs: Vec<(usize, f64, f64)> = FIELDS
                    .iter()
                    .enumerate()
                    .filter_map(|(k, (_, get, _))| {
                        let (a, b) = (get(g)?, get(t)?);
                        ((a - b).abs() > 1e-3).then_some((k, a, b))
                    })
                    .collect();
                if !diffs.is_empty() {
                    out.push((list, t.name.clone(), diffs));
                }
            }
        }
        out
    }

    /// Takes the mobile values of one entry (`true`) or the game's.
    fn choose(&mut self, list: List, name: &str, diffs: &[(usize, f64, f64)], mobile: bool) {
        let l = self.edits_mut(list);
        let i = match l.iter().position(|i| i.name.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                l.push(Item {
                    name: name.to_string(),
                    ..Default::default()
                });
                l.len() - 1
            }
        };
        for &(k, _, m) in diffs {
            (FIELDS[k].2)(&mut l[i], mobile.then_some(m));
        }
        l.retain(Item::changes);
    }

    /// Whether an entry has the mobile values.
    fn is_mobile(&self, list: List, name: &str, diffs: &[(usize, f64, f64)]) -> bool {
        self.edit_of(list, name).is_some_and(|e| {
            diffs
                .iter()
                .all(|&(k, _, m)| (FIELDS[k].1)(e).is_some_and(|v| (v - m).abs() < 0.01))
        })
    }

    /// PC or mobile, per entry where they differ (the game's by default).
    fn mobile_ui(&mut self, ui: &mut egui::Ui, game: &Balance) -> bool {
        let diffs = self.differences(game);
        if diffs.is_empty() {
            return false;
        }
        let mut changed = false;
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("PC or mobile").color(GOLD));
                help(
                    ui,
                    "where the mobile game's values differ; PC unless you pick mobile",
                );
            });
            ui.horizontal(|ui| {
                if ui.button("All PC").clicked() {
                    for (l, n, d) in &diffs {
                        self.choose(*l, n, d, false);
                    }
                    changed = true;
                }
                if ui.button("All mobile").clicked() {
                    for (l, n, d) in &diffs {
                        self.choose(*l, n, d, true);
                    }
                    changed = true;
                }
            });
            egui::Grid::new("pc-or-mobile")
                .striped(true)
                .num_columns(3)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for (list, name, d) in &diffs {
                        let mobile = self.is_mobile(*list, name, d);
                        ui.label(RichText::new(name).color(if mobile { GOLD } else { TEXT }));
                        let text: Vec<String> = d
                            .iter()
                            .map(|&(k, a, b)| format!("{} {} / {}", FIELDS[k].0, fmt(a), fmt(b)))
                            .collect();
                        ui.label(RichText::new(text.join(", ")).color(DIM))
                            .on_hover_text("PC / mobile");
                        ui.horizontal(|ui| {
                            if ui.selectable_label(!mobile, "PC").clicked() && mobile {
                                self.choose(*list, name, d, false);
                                changed = true;
                            }
                            if ui.selectable_label(mobile, "Mobile").clicked() && !mobile {
                                self.choose(*list, name, d, true);
                                changed = true;
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        changed
    }

    /// Edits for names this game doesn't have (kept for other games).
    fn elsewhere(&self) -> usize {
        let Some(v) = &self.values else { return 0 };
        let has = |l: &[Item], n: &str| l.iter().any(|i| i.name.eq_ignore_ascii_case(n));
        [
            (&self.edits.areas, &v.areas),
            (&self.edits.animals, &v.animals),
            (&self.edits.weapons, &v.weapons),
        ]
        .iter()
        .map(|(e, g)| e.iter().filter(|i| !has(g, &i.name)).count())
        .sum()
    }

    /// The tab. Returns a note for the status line when saving failed.
    pub fn ui(&mut self, ui: &mut egui::Ui, bases: &[PathBuf], mods: &[PathBuf]) -> Option<String> {
        self.refresh(bases, mods, &ui.ctx().clone());
        if bases.is_empty() {
            ui.label(RichText::new("Set the game folder first (The game tab).").color(RED));
            return None;
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(format!("Can't read the game: {e}")).color(RED));
            return None;
        }
        let Some(values) = self.values.clone() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading the game (with the add-ons that are on)...");
            });
            return None;
        };
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Find:");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(180.0));
            let n = [&self.edits.areas, &self.edits.animals, &self.edits.weapons]
                .iter()
                .map(|l| l.len())
                .sum::<usize>()
                + usize::from(self.edits.start.is_some());
            ui.label(RichText::new(format!("{n} changed")).color(if n > 0 { GOLD } else { DIM }));
            if n > 0 && ui.button("Reset all").clicked() {
                self.edits = Balance::default();
                changed = true;
            }
        });
        let others = self.elsewhere();
        if others > 0 {
            help(
                ui,
                &format!("{others} changes are for entries this game doesn't have (kept for when it does)."),
            );
        }
        ui.add_space(6.0);
        // A new hunter's points.
        let game_start = values.start.unwrap_or(100);
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("New hunter's points").color(GOLD));
                let mut v = self.edits.start.unwrap_or(game_start);
                if ui
                    .add(egui::DragValue::new(&mut v).range(0..=1_000_000))
                    .changed()
                {
                    self.edits.start = (v != game_start).then_some(v);
                    changed = true;
                }
                if self.edits.start.is_some() {
                    ui.label(RichText::new(format!("game: {game_start}")).color(DIM));
                    if ui.small_button("Reset").clicked() {
                        self.edits.start = None;
                        changed = true;
                    }
                }
            });
        });
        ui.add_space(8.0);
        changed |= self.mobile_ui(ui, &values);
        for (list, title, items) in [
            (List::Areas, "Areas", &values.areas),
            (List::Animals, "Animals", &values.animals),
            (List::Weapons, "Weapons", &values.weapons),
        ] {
            ui.add_space(8.0);
            changed |= self.list_ui(ui, list, title, items);
        }
        if changed {
            if let Err(e) = self.store() {
                return Some(format!("Balance not saved: {e}"));
            }
        }
        None
    }

    fn list_ui(&mut self, ui: &mut egui::Ui, list: List, title: &str, items: &[Item]) -> bool {
        let f = self.filter.trim().to_ascii_lowercase();
        let shown: Vec<&Item> = items
            .iter()
            .filter(|i| f.is_empty() || i.name.to_ascii_lowercase().contains(&f))
            .collect();
        if shown.is_empty() {
            return false;
        }
        let mut changed = false;
        panel(ui, |ui| {
            ui.label(RichText::new(title).color(GOLD));
            egui::Grid::new(title)
                .striped(true)
                .num_columns(4)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Name").color(DIM));
                    ui.label(RichText::new("Price").color(DIM));
                    ui.label(
                        RichText::new(if list == List::Animals { "Score" } else { "" }).color(DIM),
                    );
                    ui.label("");
                    ui.end_row();
                    for it in shown {
                        let edit = self.edit_of(list, &it.name).cloned();
                        let is_edited = edit.is_some();
                        ui.label(RichText::new(&it.name).color(if is_edited {
                            GOLD
                        } else {
                            TEXT
                        }));
                        match it.price {
                            Some(game) => {
                                let mut v = edit.as_ref().and_then(|e| e.price).unwrap_or(game);
                                let r = ui
                                    .add(egui::DragValue::new(&mut v).range(0..=1_000_000))
                                    .on_hover_text(format!("game: {game}"));
                                if r.changed() {
                                    self.set(list, &it.name, Some((v != game).then_some(v)), None);
                                    changed = true;
                                }
                            }
                            None => {
                                ui.label(RichText::new("-").color(DIM)).on_hover_text(
                                    "Roams (no menu entry): on the 3DS it costs as the menu animal \
                                     with its AI",
                                );
                            }
                        }
                        match it.score {
                            Some(game) => {
                                let mut v = edit.as_ref().and_then(|e| e.score).unwrap_or(game);
                                let r = ui
                                    .add(
                                        egui::DragValue::new(&mut v)
                                            .range(0.0..=100_000.0)
                                            .speed(0.1)
                                            .max_decimals(1),
                                    )
                                    .on_hover_text(format!("game: {game}"));
                                if r.changed() {
                                    self.set(list, &it.name, None, Some((v != game).then_some(v)));
                                    changed = true;
                                }
                            }
                            None => {
                                ui.label("");
                            }
                        }
                        if is_edited {
                            if ui.small_button("Reset").clicked() {
                                self.set(list, &it.name, Some(None), Some(None));
                                changed = true;
                            }
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        });
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_values_are_offered_and_chosen_per_entry() {
        let item = |name: &str, price: i32, shots: f32| Item {
            name: name.into(),
            price: Some(price),
            shots: Some(shots),
            ..Default::default()
        };
        let game = Balance {
            weapons: vec![item("Shotgun", 100, 8.0), item("Pistol", 20, 16.0)],
            ..Default::default()
        };
        let mut e = Editor {
            mobile: Some(Balance {
                weapons: vec![item("Shotgun", 40, 10.0), item("Pistol", 20, 16.0)],
                ..Default::default()
            }),
            ..Default::default()
        };
        // Only the shotgun differs, in price and shots.
        let d = e.differences(&game);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].1, "Shotgun");
        assert_eq!(d[0].2.len(), 2);
        assert!(!e.is_mobile(List::Weapons, "Shotgun", &d[0].2));
        e.choose(List::Weapons, "Shotgun", &d[0].2, true);
        assert!(e.is_mobile(List::Weapons, "Shotgun", &d[0].2));
        assert_eq!(e.edits.weapons[0].shots, Some(10.0));
        assert_eq!(e.edits.weapons[0].price, Some(40));
        // Back to PC: nothing left to change.
        e.choose(List::Weapons, "Shotgun", &d[0].2, false);
        assert!(e.edits.weapons.is_empty());
    }
}
