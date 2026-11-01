//! The recipe page (Add-ons tab): a recipe (`recipes/*.toml`) dropped on
//! the window or opened, a folder given for each of its entries (picked,
//! dropped, or found in a downloads folder), each checked against the
//! recipe's hashes off the UI thread. Only the files the recipe lists are
//! taken from a folder. Once used, play and the 3DS and Wii U builds go by
//! the recipe (`config::RecipeUse`).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use c2_mods::recipe::{self, Base, Located, Recipe, RecipeMod};
use eframe::egui::{self, RichText};

use crate::app::{help, panel};
use crate::config::RecipeUse;
use crate::theme::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Unset,
    Checking,
    Ok(String),
    Bad(String),
}

impl Status {
    fn show(&self, ui: &mut egui::Ui) {
        match self {
            Status::Unset => ui.label(RichText::new("no folder").color(DIM)),
            Status::Checking => ui.spinner(),
            Status::Ok(s) => ui.label(RichText::new(s).color(TEXT)),
            Status::Bad(s) => ui.label(RichText::new(s).color(RED)),
        };
    }
}

pub struct Row {
    /// `overlay`, `standalone` or `mod`.
    pub kind: &'static str,
    pub entry: RecipeMod,
    /// A whole game (overlay or standalone) rather than an add-on.
    pub game: bool,
    pub folder: Option<PathBuf>,
    pub status: Status,
}

enum Msg {
    Row(usize, PathBuf, Located),
    /// A downloads folder looked through: how many entries it gave.
    Scanned(usize),
    Base(Status),
}

/// What the page asks of the launcher.
pub enum Ask {
    Nothing,
    /// Use the recipe as it is set up ([`Page::to_use`]).
    Use,
    Close,
    /// Build the game from the player's APK, for this recipe.
    ChooseApk,
}

pub struct Page {
    pub path: PathBuf,
    pub recipe: Recipe,
    base: Option<Base>,
    /// The recipe's own game folder; `None`: the Game tab's game with its
    /// overlays.
    pub game: Option<PathBuf>,
    pub rows: Vec<Row>,
    base_status: Status,
    /// Downloads folders being looked through.
    scanning: usize,
    note: Option<String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

/// Whether a recipe entry's checked folder has every listed file as the
/// recipe pins it, as a status.
fn status_of(l: &Located) -> Status {
    if l.complete() {
        Status::Ok(format!("ok ({} files)", l.ok.len()))
    } else if l.root.is_none() {
        Status::Bad("not in that folder".into())
    } else {
        Status::Bad(format!(
            "{} missing, {} different",
            l.missing.len(),
            l.mismatched.len()
        ))
    }
}

impl Page {
    /// The recipe at `path`, with the folders of `used` if it is the same
    /// recipe; `game_dirs` is the Game tab's game and overlays.
    pub fn open(
        path: &Path,
        used: Option<&RecipeUse>,
        game_dirs: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let r = Recipe::load(path).map_err(|e| e.to_string())?;
        let base = recipe::load_base(path, &r);
        let used = used.filter(|u| u.path == path);
        let rows = recipe::entries(&r)
            .map(|(kind, m, game)| Row {
                kind,
                entry: m.clone(),
                game,
                folder: used.and_then(|u| u.folders.get(&m.id).cloned()),
                status: Status::Unset,
            })
            .collect();
        let (tx, rx) = channel();
        let mut p = Page {
            path: path.to_path_buf(),
            recipe: r,
            base,
            game: used.and_then(|u| u.game.clone()),
            rows,
            base_status: Status::Unset,
            scanning: 0,
            note: None,
            tx,
            rx,
        };
        for i in 0..p.rows.len() {
            if let Some(d) = p.rows[i].folder.clone() {
                p.check_row(i, d);
            }
        }
        p.check_base(game_dirs);
        Ok(p)
    }

    pub fn is_apk_base(&self) -> bool {
        self.base.as_ref().is_some_and(|b| !b.base.apk.is_empty())
    }

    /// Still checking something.
    pub fn working(&self) -> bool {
        self.scanning > 0
            || self.base_status == Status::Checking
            || self.rows.iter().any(|r| r.status == Status::Checking)
    }

    /// Every entry has its folder, matching.
    pub fn ready(&self) -> bool {
        self.rows.iter().all(|r| matches!(r.status, Status::Ok(_)))
    }

    pub fn to_use(&self) -> RecipeUse {
        RecipeUse {
            path: self.path.clone(),
            game: self.game.clone(),
            folders: self
                .rows
                .iter()
                .filter_map(|r| Some((r.entry.id.clone(), r.folder.clone()?)))
                .collect(),
            on: true,
        }
    }

    fn check_row(&mut self, i: usize, dir: PathBuf) {
        let Some(row) = self.rows.get_mut(i) else {
            return;
        };
        row.folder = Some(dir.clone());
        row.status = Status::Checking;
        let (m, game, tx) = (row.entry.clone(), row.game, self.tx.clone());
        std::thread::spawn(move || {
            let l = recipe::check_folder(&m, &dir, game);
            let _ = tx.send(Msg::Row(i, dir, l));
        });
    }

    /// The base game checked against the recipe's hash list (or, for an
    /// APK base, against the APK it was built from).
    pub fn check_base(&mut self, game_dirs: Vec<PathBuf>) {
        let dirs = match &self.game {
            Some(g) => vec![g.clone()],
            None => game_dirs,
        };
        let Some(base) = self.base.clone() else {
            self.base_status = if self.recipe.recipe.base.is_empty() {
                Status::Ok("any".into())
            } else {
                Status::Ok("no hash list beside the recipe: not checked".into())
            };
            return;
        };
        let Some(first) = dirs.first().cloned() else {
            self.base_status = Status::Bad("no game folder".into());
            return;
        };
        self.base_status = Status::Checking;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let s = match recipe::check_apk(&base, &first) {
                Some(true) => Status::Ok("built from that APK".into()),
                Some(false) => Status::Bad("not built from that APK".into()),
                None => match c2_mods::vanilla(&dirs) {
                    Err(e) => Status::Bad(e.to_string()),
                    Ok(v) => {
                        let (missing, bad) = recipe::check_base(&base, &v);
                        if missing.is_empty() && bad.is_empty() {
                            Status::Ok(format!("ok ({} files)", base.files.len()))
                        } else {
                            // As `c2-pc recipe verify`: a warning, not a stop.
                            Status::Bad(format!(
                                "{} missing, {} different (another version?)",
                                missing.len(),
                                bad.len()
                            ))
                        }
                    }
                },
            };
            let _ = tx.send(Msg::Base(s));
        });
    }

    /// Looks through folders (downloads folders, or entries' own folders)
    /// for the entries that have none yet: each found complete is taken.
    pub fn scan(&mut self, dirs: Vec<PathBuf>) {
        let open: Vec<usize> = (0..self.rows.len())
            .filter(|&i| !matches!(self.rows[i].status, Status::Ok(_) | Status::Checking))
            .collect();
        if open.is_empty() || dirs.is_empty() {
            return;
        }
        let mods = Recipe {
            mods: open
                .iter()
                .filter(|&&i| !self.rows[i].game)
                .map(|&i| self.rows[i].entry.clone())
                .collect(),
            ..Default::default()
        };
        let games: Vec<RecipeMod> = open
            .iter()
            .filter(|&&i| self.rows[i].game)
            .map(|&i| self.rows[i].entry.clone())
            .collect();
        let index: Vec<usize> = open
            .iter()
            .filter(|&&i| !self.rows[i].game)
            .chain(open.iter().filter(|&&i| self.rows[i].game))
            .copied()
            .collect();
        self.scanning += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let found = recipe::locate(&mods, &dirs)
                .into_iter()
                .chain(recipe::locate_games(&games, &dirs));
            let mut n = 0;
            for (i, l) in index.into_iter().zip(found) {
                if let (true, Some(root)) = (l.complete(), l.root.clone()) {
                    n += 1;
                    let _ = tx.send(Msg::Row(i, root, l));
                }
            }
            let _ = tx.send(Msg::Scanned(n));
        });
    }

    /// Takes in the checks that finished.
    pub fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Row(i, dir, l) => {
                    if let Some(r) = self.rows.get_mut(i) {
                        // The check of the row's folder, or a find for a row
                        // still to do (a pick since then wins).
                        if r.folder.as_ref() == Some(&dir)
                            || !matches!(r.status, Status::Checking | Status::Ok(_))
                        {
                            r.folder = Some(dir);
                            r.status = status_of(&l);
                        }
                    }
                }
                Msg::Scanned(n) => {
                    self.scanning = self.scanning.saturating_sub(1);
                    let left = self
                        .rows
                        .iter()
                        .filter(|r| !matches!(r.status, Status::Ok(_)))
                        .count();
                    self.note = Some(format!("{n} found there; {left} still to find."));
                }
                Msg::Base(s) => self.base_status = s,
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, game_dirs: &[PathBuf], busy: bool) -> Ask {
        let mut ask = Ask::Nothing;
        let r = &self.recipe.recipe;
        ui.label(
            RichText::new(format!("Recipe: {} {}", r.name, r.version))
                .size(18.0)
                .color(SAND)
                .strong(),
        );
        help(
            ui,
            &format!(
                "{} — give each entry its folder: Browse, drop folders on the window, or look in \
                 a downloads folder. Only the files the recipe lists are taken from each \
                 folder; nothing else in it is used.",
                self.path.display()
            ),
        );
        ui.add_space(4.0);
        let mut pick_game = false;
        let mut own_game_off = false;
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Game").color(SAND).strong());
                ui.label(&self.recipe.recipe.base);
                if let Some(b) = &self.base {
                    if !b.base.credit.is_empty() {
                        ui.label(RichText::new(&b.base.credit).color(DIM));
                    }
                    if !b.base.url.is_empty() {
                        ui.hyperlink_to("download page", &b.base.url);
                    }
                }
            });
            ui.horizontal(|ui| {
                let shown = match (&self.game, game_dirs.first()) {
                    (Some(g), _) => g.display().to_string(),
                    (None, Some(g)) if game_dirs.len() > 1 => {
                        format!("{} and its overlays (Game tab)", g.display())
                    }
                    (None, Some(g)) => format!("{} (Game tab)", g.display()),
                    (None, None) => "the Game tab's (not set)".into(),
                };
                ui.label(RichText::new(shown).color(SAND));
                self.base_status.show(ui);
            });
            ui.horizontal(|ui| {
                if self.is_apk_base()
                    && ui
                        .add_enabled(!busy, egui::Button::new("Choose the APK..."))
                        .on_hover_text(
                            "Your own copy of the Android game: the game is built from it into \
                             the data folder (or drop the APK on the window).",
                        )
                        .clicked()
                {
                    ask = Ask::ChooseApk;
                }
                if ui.button("Choose game folder...").clicked() {
                    pick_game = true;
                }
                if self.game.is_some() && ui.button("Use the Game tab's").clicked() {
                    own_game_off = true;
                }
            });
        });
        if pick_game {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                self.game = Some(p);
                self.check_base(game_dirs.to_vec());
            }
        }
        if own_game_off {
            self.game = None;
            self.check_base(game_dirs.to_vec());
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Look in a downloads folder...").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.scan(vec![d]);
                }
            }
            if self.scanning > 0 {
                ui.spinner();
                ui.label("looking...");
            } else if let Some(n) = &self.note {
                ui.label(RichText::new(n).color(DIM));
            }
        });
        ui.add_space(4.0);
        let mut browse: Option<usize> = None;
        let mut clear: Option<usize> = None;
        if self.rows.is_empty() {
            help(ui, "Nothing to download: the recipe is the game alone.");
        } else {
            panel(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .id_salt("recipe-rows")
                    .show(ui, |ui| {
                        egui::Grid::new("recipe-grid")
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for (i, row) in self.rows.iter().enumerate() {
                                    ui.label(RichText::new(row.kind).color(DIM));
                                    let credit = if row.entry.credit.is_empty() {
                                        "credit not given".to_string()
                                    } else {
                                        row.entry.credit.clone()
                                    };
                                    ui.label(&row.entry.id).on_hover_text(credit);
                                    if row.entry.url.is_empty() {
                                        ui.label(RichText::new("no link").color(DIM));
                                    } else {
                                        ui.hyperlink_to("download", &row.entry.url)
                                            .on_hover_text(&row.entry.url);
                                    }
                                    let folder = row.folder.as_ref().map_or(String::new(), |f| {
                                        f.file_name().map_or_else(
                                            || f.display().to_string(),
                                            |n| n.to_string_lossy().into_owned(),
                                        )
                                    });
                                    let l = ui.label(RichText::new(folder).color(SAND));
                                    if let Some(f) = &row.folder {
                                        l.on_hover_text(f.display().to_string());
                                    }
                                    row.status.show(ui);
                                    ui.horizontal(|ui| {
                                        if ui.small_button("Browse...").clicked() {
                                            browse = Some(i);
                                        }
                                        if row.folder.is_some() && ui.small_button("x").clicked() {
                                            clear = Some(i);
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                    });
            });
        }
        if let Some(i) = browse {
            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                self.check_row(i, d);
            }
        }
        if let Some(r) = clear.and_then(|i| self.rows.get_mut(i)) {
            r.folder = None;
            r.status = Status::Unset;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ready = self.ready() && !self.working();
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new("Use this recipe").color(GOLD).strong())
                        .fill(GREEN),
                )
                .on_disabled_hover_text("Every entry needs its folder, matching (ok).")
                .clicked()
            {
                ask = Ask::Use;
            }
            if ui.button("Close").clicked() {
                ask = Ask::Close;
            }
            let left = self
                .rows
                .iter()
                .filter(|r| !matches!(r.status, Status::Ok(_)))
                .count();
            if left > 0 {
                ui.label(RichText::new(format!("{left} to go")).color(DIM));
            }
        });
        ask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(p: &mut Page) {
        for _ in 0..500 {
            p.poll();
            if !p.working() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("checks never finished");
    }

    #[test]
    fn entries_found_in_a_downloads_folder() {
        let dir = std::env::temp_dir().join(format!("c2rs-recipe-page-{}", std::process::id()));
        let huntdat = dir.join("downloads").join("Some Mod").join("HUNTDAT");
        std::fs::create_dir_all(&huntdat).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(huntdat.join("X.CAR"), b"abc").unwrap_or_else(|e| panic!("{e}"));
        let r = format!(
            "[recipe]\nid = \"t\"\nname = \"T\"\n\n[[mods]]\nid = \"some-mod\"\n\n\
             [mods.files]\n\"HUNTDAT/X.CAR\" = \"{}\"\n",
            recipe::sha256(b"abc")
        );
        let path = dir.join("t.toml");
        std::fs::write(&path, r).unwrap_or_else(|e| panic!("{e}"));
        let mut p = Page::open(&path, None, Vec::new()).unwrap_or_else(|e| panic!("{e}"));
        settle(&mut p);
        assert!(!p.ready());
        p.scan(vec![dir.join("downloads")]);
        settle(&mut p);
        let used = p.to_use();
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert!(p.ready(), "{:?}", p.rows[0].status);
        assert_eq!(
            used.folders.get("some-mod").and_then(|f| f.file_name()),
            Some(std::ffi::OsStr::new("Some Mod"))
        );
    }
}
