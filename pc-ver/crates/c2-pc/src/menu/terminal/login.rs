//! The hunter list, the waiver and deleting a hunter in the terminal's look
//! ("hunter authentication"). The saves are handled as the original screens
//! handle them (`register.rs`); only the drawing and the keys differ.

use c2_core::dhc::terminal::{Action, Screen as TScreen};

use super::draw::{AMBER, FAINT, HI, INV, LINE, LO, MD, PANEL, PX, RED, SMALL};
use super::*;

const REC_ROW: f32 = 28.0;
/// The waiver's and the delete prompt's answers.
pub(super) const YES: Rect = (220.0, 440.0, 170.0, 44.0);
pub(super) const NO: Rect = (410.0, 440.0, 170.0, 44.0);

/// The waiver, ours (the original's is a picture).
const WAIVER: [&str; 10] = [
    "DINOHUNT CORP // HUNTING LICENCE AND LIABILITY WAIVER",
    "",
    "The bearer is licensed to hunt on DinoHunt Corp islands",
    "under the terms of the permits issued for each hunt.",
    "",
    "The animals on these islands are wild and dangerous.",
    "DinoHunt Corp accepts no liability for injury or death",
    "of the bearer, howsoever caused, on or off the islands.",
    "",
    "Accept these terms to open a hunter record.",
];

fn record_rect(records: Rect, i: usize) -> Rect {
    (
        records.0 + 6.0,
        records.1 + 34.0 + i as f32 * REC_ROW,
        records.2 - 12.0,
        REC_ROW - 4.0,
    )
}

impl Menu {
    /// A yes/no prompt's answer this frame: Y, Enter or the left button;
    /// N, Esc or the right button.
    pub(super) fn term_prompt_answer(&self, input: &Input) -> Option<bool> {
        let at = |r: Rect| input.clicked && inside(self.cursor, r);
        if input.pressed(KeyCode::KeyY) || input.pressed(KeyCode::Enter) || at(YES) {
            Some(true)
        } else if input.pressed(KeyCode::KeyN) || input.pressed(KeyCode::Escape) || at(NO) {
            Some(false)
        } else {
            None
        }
    }

    /// The quit prompt (the terminal's LOG OFF).
    pub(in crate::menu) fn update_term_quit(&mut self, input: &Input) {
        match self.term_prompt_answer(input) {
            Some(true) => self.quit = true,
            Some(false) => self.go_home(),
            None => {}
        }
    }

    pub(in crate::menu) fn draw_term_quit(&self, out: &mut Vec<Sprite>) {
        self.term_frame(out);
        let lines = [
            "LOG OFF",
            "",
            "Close the DinoHunt Corp terminal and leave the game?",
            "Your account and trophies are saved.",
        ];
        self.draw_term_prompt(out, &lines, "[Y] LOG OFF", "[N] STAY", false);
        self.term_status(out, "Y / ENTER LOG OFF   N / ESC STAY", "");
    }

    /// The picked hunter's name in the field (or nothing picked).
    fn pick_hunter(&mut self, i: Option<usize>) {
        self.picked = i.filter(|&i| i < self.players.len());
        if let Some(i) = self.picked {
            self.name = self.players[i].player_name();
        }
    }

    pub(in crate::menu) fn update_term_login(&mut self, dt: f32, input: &Input) {
        // The boot log, before the first screen of the run.
        if let Some(v) = self
            .term
            .as_mut()
            .filter(|v| v.term.screen == TScreen::Boot)
        {
            v.term.step(dt);
            if input.last_pressed.is_some() || input.clicked {
                v.term.act(Action::Skip);
            }
            return;
        }
        let cursor = self.cursor;
        let at = |r: Rect| input.clicked && inside(cursor, r);
        let back = input.pressed(KeyCode::Escape) || at(self.geom().back());
        match self.screen {
            Screen::Waiver => {
                if input.pressed(KeyCode::KeyY) || input.pressed(KeyCode::Enter) || at(YES) {
                    self.accept_waiver();
                } else if back || input.pressed(KeyCode::KeyN) || at(NO) {
                    self.screen = Screen::Register;
                }
            }
            Screen::Delete => {
                if input.pressed(KeyCode::KeyY) || at(YES) {
                    self.delete_picked();
                } else if back || input.pressed(KeyCode::KeyN) || at(NO) {
                    self.screen = Screen::Register;
                }
            }
            _ => {
                self.type_name(input);
                let n = self.players.len();
                let down = input.pressed(KeyCode::ArrowDown) || input.scroll < 0.0;
                let up = input.pressed(KeyCode::ArrowUp) || input.scroll > 0.0;
                if n > 0 && down {
                    self.pick_hunter(Some(self.picked.map_or(0, |i| (i + 1).min(n - 1))));
                } else if n > 0 && up {
                    self.pick_hunter(Some(self.picked.map_or(0, |i| i.saturating_sub(1))));
                }
                if let Some(i) = (0..n).find(|&i| at(record_rect(self.geom().records(), i))) {
                    if self.picked == Some(i) {
                        // A second click on the same hunter logs in.
                        self.submit_name();
                        return;
                    }
                    self.pick_hunter(Some(i));
                }
                if input.pressed(KeyCode::Enter)
                    || input.pressed(KeyCode::NumpadEnter)
                    || at(self.geom().login())
                {
                    self.submit_name();
                } else if self.picked.is_some()
                    && (input.pressed(KeyCode::Delete) || at(self.geom().delete()))
                {
                    self.screen = Screen::Delete;
                } else if back && self.player.is_some() {
                    self.go_home();
                }
            }
        }
    }

    pub(in crate::menu) fn draw_term_login(&self, out: &mut Vec<Sprite>) {
        self.term_frame(out);
        if let Some(v) = self
            .term
            .as_ref()
            .filter(|v| v.term.screen == TScreen::Boot)
        {
            self.draw_term_boot(out, &v.term);
            self.term_status(out, "ANY KEY: SKIP", "");
            return;
        }
        let (hints, right) = match self.screen {
            Screen::Waiver => {
                self.draw_term_prompt(out, &WAIVER, "[Y] ACCEPT", "[N] DECLINE", false);
                ("Y / ENTER ACCEPT   N / ESC DECLINE", String::new())
            }
            Screen::Delete => {
                let name = self
                    .picked
                    .and_then(|i| self.players.get(i))
                    .map(|p| p.player_name().to_uppercase())
                    .unwrap_or_default();
                let lines = [
                    "DELETE HUNTER RECORD".to_string(),
                    String::new(),
                    format!("HUNTER: {name}"),
                    String::new(),
                    "The record, its account and its trophies are erased.".into(),
                    "This cannot be undone.".into(),
                ];
                self.draw_term_prompt(out, &lines, "[Y] DELETE", "[N] CANCEL", true);
                ("Y DELETE   N / ESC CANCEL", String::new())
            }
            _ => {
                self.draw_term_records(out);
                let right = if self.player.is_some() {
                    "[ESC] BACK"
                } else {
                    ""
                };
                (
                    "TYPE A NAME   UP/DOWN SELECT   ENTER LOG IN   DEL DELETE",
                    right.to_string(),
                )
            }
        };
        self.term_status(out, hints, &right);
    }

    pub(super) fn term_button(
        &self,
        out: &mut Vec<Sprite>,
        r: Rect,
        label: &str,
        on: bool,
        danger: bool,
    ) {
        let hover = on && inside(self.cursor, r);
        let (fill, line, tc) = match (on, hover, danger) {
            (false, _, _) => (PANEL, LINE, FAINT),
            (true, true, true) => (RED, RED, INV),
            (true, true, false) => (HI, HI, INV),
            (true, false, true) => (PANEL, RED, RED),
            (true, false, false) => (MD, HI, INV),
        };
        self.term_box(out, r, fill, line);
        let x = r.0 + (r.2 - self.text_width(PX, label)) / 2.0;
        self.text(out, x, r.1 + (r.3 - PX) / 2.0, PX, tc, label);
    }

    fn draw_term_records(&self, out: &mut Vec<Sprite>) {
        let g = self.geom();
        let (records, id_panel, field) = (g.records(), g.id_panel(), g.id_field());
        self.text(out, g.l, 50.0, PX, HI, "HUNTER AUTHENTICATION");
        self.term_box(out, records, PANEL, LINE);
        self.text(
            out,
            records.0 + 10.0,
            records.1 + 10.0,
            SMALL,
            LO,
            "HUNTER RECORDS",
        );
        for (i, p) in self.players.iter().enumerate() {
            let r = record_rect(records, i);
            let picked = self.picked == Some(i);
            let hover = inside(self.cursor, r);
            let c = if picked {
                self.fill(out, r, MD);
                INV
            } else if hover {
                HI
            } else {
                MD
            };
            let score = format!("{} PTS", p.score);
            let w = self.text_width(SMALL, &score);
            let name = format!("{:>2}  {}", i + 1, p.player_name().to_uppercase());
            self.term_text(out, r.0 + 8.0, r.1 + 6.0, PX, c, &name, r.2 - w - 30.0);
            self.text(out, r.0 + r.2 - w - 8.0, r.1 + 7.0, SMALL, c, &score);
        }
        if self.players.is_empty() {
            let t = "NO RECORDS ON FILE";
            self.text(out, records.0 + 10.0, records.1 + 40.0, SMALL, FAINT, t);
        }
        // The ID field and what it will do.
        self.term_box(out, id_panel, PANEL, LINE);
        self.text(
            out,
            id_panel.0 + 12.0,
            id_panel.1 + 14.0,
            SMALL,
            LO,
            "HUNTER ID",
        );
        self.term_box(out, field, [0.0, 0.0, 0.0, 0.5], HI);
        let caret = if (self.frame_time() * 2.0) as i32 % 2 == 0 {
            "_"
        } else {
            ""
        };
        let shown = format!("{}{caret}", self.name.to_uppercase());
        self.term_text(
            out,
            field.0 + 8.0,
            field.1 + 8.0,
            PX,
            HI,
            &shown,
            field.2 - 16.0,
        );
        let name = self.name.trim();
        let known = self.players.iter().position(|p| p.player_name() == name);
        let mut y = field.1 + field.3 + 20.0;
        let mut line = |t: &str, c: [f32; 4]| {
            self.text(out, id_panel.0 + 12.0, y, PX, c, t);
            y += 22.0;
        };
        match known.and_then(|i| self.players.get(i)) {
            Some(p) => {
                let rank = ["BEGINNER", "ADVANCED", "EXPERT"]
                    .get(p.rank.clamp(0, 2) as usize)
                    .copied()
                    .unwrap_or("");
                line("RECORD FOUND", HI);
                line(&format!("ACCOUNT  {} PTS", p.score), MD);
                line(&format!("RANK     {rank}"), MD);
                if self.player == known {
                    line("LOGGED IN NOW", LO);
                }
            }
            None if name.is_empty() => {
                line("Type a new hunter ID, or pick", LO);
                line("a record on the left.", LO);
            }
            None if self.players.len() as i32 >= save::MAX_PLAYERS => {
                line("ALL HUNTER RECORDS IN USE", AMBER);
                line("Delete one to register.", LO);
            }
            None => {
                line("NEW HUNTER", AMBER);
                line(&format!("Opening account: {} PTS", self.start_score), MD);
                line("A liability waiver follows.", LO);
            }
        }
        let label = if known.is_some() || name.is_empty() {
            "[ENTER] LOG IN"
        } else {
            "[ENTER] REGISTER"
        };
        self.term_button(out, g.login(), label, !name.is_empty(), false);
        self.term_button(out, g.delete(), "[DEL] DELETE", self.picked.is_some(), true);
    }

    /// A centred notice with two answers (waiver, delete).
    pub(super) fn draw_term_prompt<S: AsRef<str>>(
        &self,
        out: &mut Vec<Sprite>,
        lines: &[S],
        yes: &str,
        no: &str,
        danger: bool,
    ) {
        let r = (100.0, 90.0, 600.0, 410.0);
        self.term_box(out, r, PANEL, LINE);
        // The title on its own line, the rest wrapped to the box.
        let w = r.2 - 40.0;
        let mut y = r.1 + 24.0;
        if let Some(t) = lines.first() {
            self.term_text(out, r.0 + 20.0, y, PX, HI, t.as_ref(), w);
            y += 30.0;
        }
        let rest: Vec<String> = lines
            .iter()
            .skip(1)
            .map(|l| l.as_ref().to_string())
            .collect();
        for l in wrap(&self.hud, PX, w, &rest) {
            self.text(out, r.0 + 20.0, y, PX, MD, &l);
            y += 22.0;
        }
        self.term_button(out, YES, yes, true, danger);
        self.term_button(out, NO, no, true, false);
    }
}
