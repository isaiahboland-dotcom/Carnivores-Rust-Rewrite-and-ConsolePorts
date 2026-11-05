//! The recipe page (Add-ons tab): a recipe (`recipes/*.toml`) dropped on
//! the window or opened, a folder given for each of its entries (picked,
//! dropped, or found in a downloads folder), each checked against the
//! recipe's hashes off the UI thread. Only the files the recipe lists are
//! taken from a folder. Once used, play and the 3DS and Wii U builds go by
//! the recipe (`config::RecipeUse`).

use std::path::{Path, PathBuf};

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

pub(crate) enum Msg {
    Row(usize, PathBuf, Located),
    /// A downloads folder looked through: how many entries it gave.
    Scanned(usize),
    Base(Status),
    /// A game elsewhere (a standalone, or one in the search folders) that
    /// matches the base the Game tab's does not: the recipe's game.
    Found(PathBuf, Status),
}

/// What the page asks of the launcher.
pub enum Ask {
    Nothing,
    /// Use the recipe as it is set up ([`Page::to_use`]).
    Use,
    /// ... and play it.
    Play,
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
    /// Continue to Play or a console builder once the downloads are ready.
    pub next: Option<crate::instances::Destination>,
    /// Folders to look through (with the folders in them) when the Game
    /// tab's game is not the recipe's base: a standalone recipe's game is
    /// rarely the Game tab's.
    candidates: Vec<PathBuf>,
    /// The launcher's game the base was last checked against, while the
    /// instance had none of its own: kept as the instance's own game once
    /// used, so it never follows another game later (owner's report
    /// 2026-11-05: an instance played the APK game set up afterwards).
    fallback: Option<PathBuf>,
    /// The player chose to play a game whose files differ from the
    /// recipe's base (another version of it).
    pub accept_base: bool,
    requests: Vec<crate::jobs::recipe::Check>,
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
    /// recipe; `game_dirs` is the Game tab's game and overlays, and
    /// `candidates` other folders to look through for the recipe's base.
    pub fn open(
        path: &Path,
        used: Option<&RecipeUse>,
        game_dirs: Vec<PathBuf>,
        candidates: Vec<PathBuf>,
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
        let mut p = Page {
            path: path.to_path_buf(),
            recipe: r,
            base,
            game: used.and_then(|u| u.game.clone()),
            rows,
            base_status: Status::Unset,
            scanning: 0,
            note: None,
            next: None,
            candidates,
            fallback: None,
            accept_base: false,
            requests: Vec::new(),
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

    /// Every entry has its folder, matching, and the game is the recipe's
    /// base (its hash list), or the player accepted a different version.
    /// Launch protection: a wrong game is never played or built silently.
    pub fn ready(&self) -> bool {
        (matches!(self.base_status, Status::Ok(_)) || self.base_accepted())
            && self.rows.iter().all(|r| matches!(r.status, Status::Ok(_)))
    }

    /// The game differs from the recipe's base, and the player said to use
    /// it anyway (never for a game built from an APK: that is another game).
    fn base_accepted(&self) -> bool {
        self.accept_base && !self.is_apk_base() && self.base_differs()
    }

    /// The game was checked and isn't the recipe's base.
    pub fn base_differs(&self) -> bool {
        matches!(self.base_status, Status::Bad(_))
    }

    /// The instance's game as it will be used: its own, else the one it was
    /// checked against.
    pub fn game(&self) -> Option<PathBuf> {
        self.game.clone().or_else(|| self.fallback.clone())
    }

    pub fn to_use(&self) -> RecipeUse {
        RecipeUse {
            path: self.path.clone(),
            game: self.game(),
            overlays: None,
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
        self.requests.push(crate::jobs::recipe::Check::Row {
            index: i,
            dir,
            entry: row.entry.clone(),
            game: row.game,
        });
    }

    /// The base game checked against the recipe's hash list (or, for an
    /// APK base, against the APK it was built from).
    pub fn check_base(&mut self, game_dirs: Vec<PathBuf>) {
        let dirs = match &self.game {
            Some(g) => vec![g.clone()],
            None => game_dirs,
        };
        self.accept_base = false;
        self.fallback = match &self.game {
            None => dirs.first().map(|g| crate::config::game_root(g)),
            Some(_) => None,
        };
        let no_game = || Status::Bad("no game folder: pick one with Edit, Base game".into());
        let Some(base) = self.base.clone() else {
            self.base_status = if dirs.is_empty() {
                no_game()
            } else if self.recipe.recipe.base.is_empty() {
                Status::Ok("any".into())
            } else {
                Status::Ok("no hash list beside the recipe: not checked".into())
            };
            return;
        };
        // An instance without a game of its own looks for its base in the
        // candidates; a folder the player chose is theirs.
        let others = if self.game.is_none() {
            self.candidates.clone()
        } else {
            Vec::new()
        };
        if dirs.is_empty() && others.is_empty() {
            self.base_status = no_game();
            return;
        }
        let first = dirs.first().cloned();
        self.base_status = Status::Checking;
        self.requests.push(crate::jobs::recipe::Check::Base {
            base,
            first,
            dirs,
            others,
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
        self.requests.push(crate::jobs::recipe::Check::Scan {
            mods: Box::new(mods),
            games,
            index,
            dirs,
        });
    }

    /// Takes in the checks that finished.
    pub(crate) fn apply(&mut self, messages: Vec<Msg>) {
        for m in messages {
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
                Msg::Found(g, s) => {
                    self.game = Some(crate::config::game_root(&g));
                    self.base_status = s;
                }
            }
        }
    }

    /// Submit the page's queued checks together to the launcher's one worker.
    pub(crate) fn dispatch(&mut self, handle: &mut crate::jobs::Handle) {
        if self.requests.is_empty() || handle.busy() {
            return;
        }
        let checks = std::mem::take(&mut self.requests);
        if let Err(e) = handle.start_task("Checking recipe files", move |_| {
            Ok(crate::jobs::Completion::Recipe(
                checks
                    .into_iter()
                    .flat_map(crate::jobs::recipe::Check::run)
                    .collect(),
            ))
        }) {
            self.failed(&e);
        }
    }

    pub(crate) fn failed(&mut self, error: &str) {
        self.requests.clear();
        self.scanning = 0;
        if self.base_status == Status::Checking {
            self.base_status = Status::Bad(error.into());
        }
        for row in &mut self.rows {
            if row.status == Status::Checking {
                row.status = Status::Bad(error.into());
            }
        }
        self.note = Some(error.into());
    }

    /// The entries not found yet (once nothing is being checked).
    pub fn missing(&self) -> Vec<&Row> {
        if self.working() {
            return Vec::new();
        }
        self.rows
            .iter()
            .filter(|r| !matches!(r.status, Status::Ok(_)))
            .collect()
    }

    /// The downloads still to get, each with its author and download page.
    fn missing_ui(&self, ui: &mut egui::Ui) {
        let missing = self.missing();
        if missing.is_empty() {
            return;
        }
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Missing: {}", missing.len()))
                        .color(RED)
                        .strong(),
                );
                let links: Vec<&str> = missing
                    .iter()
                    .map(|r| r.entry.url.as_str())
                    .filter(|u| !u.is_empty())
                    .collect();
                if !links.is_empty() && ui.button("Copy the links").clicked() {
                    ui.ctx().copy_text(links.join("\n"));
                }
            });
            help(
                ui,
                "Download each from its page and unpack it into a mod or add-on folder \
                 (Settings), then use Fix missing; or give its folder below.",
            );
            for r in missing {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&r.entry.id).color(SAND));
                    if !r.entry.credit.is_empty() {
                        ui.label(RichText::new(format!("by {}", r.entry.credit)).color(DIM));
                    }
                    if r.entry.url.is_empty() {
                        ui.label(RichText::new("no link in the recipe").color(DIM));
                    } else {
                        ui.hyperlink_to(&r.entry.url, &r.entry.url);
                    }
                });
            }
        });
        ui.add_space(4.0);
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        game_dirs: &[PathBuf],
        busy: bool,
        play: bool,
        handle: &mut crate::jobs::Handle,
    ) -> Ask {
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
                let shown = match self.game() {
                    Some(g) => g.display().to_string(),
                    None => "none yet".into(),
                };
                ui.label(RichText::new(shown).color(SAND));
                self.base_status.show(ui);
            });
            if self.base_differs() && !self.is_apk_base() {
                ui.label(
                    RichText::new(
                        "That folder isn't this instance's base game (its files differ from \
                         the recipe's hashes). Choose the right folder, or use it anyway if it is \
                         another version of the same game.",
                    )
                    .color(RED),
                );
                ui.checkbox(&mut self.accept_base, "Use this game anyway");
            }
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
            });
        });
        if pick_game {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                // The game folder in it (a download's parent picked).
                self.game = Some(crate::config::game_root(&p));
                self.check_base(game_dirs.to_vec());
            }
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
        self.missing_ui(ui);
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
                    egui::Button::new(RichText::new("Use this instance").color(GOLD).strong())
                        .fill(GREEN),
                )
                .on_disabled_hover_text("Every entry needs its folder, matching (ok).")
                .clicked()
            {
                ask = Ask::Use;
            }
            if play
                && ui
                    .add_enabled(ready && !busy, egui::Button::new("Play").fill(GREEN))
                    .clicked()
            {
                ask = Ask::Play;
            }
            if ui.button("Cancel").clicked() {
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
        self.dispatch(handle);
        ask
    }
}

#[cfg(test)]
mod tests;
