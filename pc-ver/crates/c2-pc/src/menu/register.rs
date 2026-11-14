//! Registration: the hunter list, the waiver and deleting a hunter.

use super::*;

impl Menu {
    pub(super) fn update_register(&mut self, input: &Input, region: u8) {
        self.type_name(input);
        // The hunter list.
        if input.clicked {
            for i in 0..self.players.len() {
                if inside(self.cursor, (304.0, 368.0 + i as f32 * 18.0, 192.0, 18.0)) {
                    self.picked = Some(i);
                    self.name = self.players[i].player_name();
                }
            }
        }
        let ok = (input.clicked && region == 1) || input.pressed(KeyCode::Enter);
        if ok {
            self.submit_name();
        }
        if input.clicked && region == 2 && self.picked.is_some() {
            self.screen = Screen::Delete;
        }
        if input.pressed(KeyCode::Escape) && self.player.is_some() {
            self.go_home();
        }
    }

    /// Typing for the name field: printable characters and Backspace.
    pub(super) fn type_name(&mut self, input: &Input) {
        for ch in input.typed.chars() {
            if self.name.len() < 24 && (ch.is_ascii_graphic() || ch == ' ') {
                self.name.push(ch);
                self.picked = None;
            }
        }
        if input.pressed(KeyCode::Backspace) {
            self.name.pop();
            self.picked = None;
        }
    }

    /// The name entered: signs that hunter in, or a new one goes to the
    /// waiver.
    pub(super) fn submit_name(&mut self) {
        let name = self.name.trim().to_string();
        if let Some(i) = self.players.iter().position(|p| p.player_name() == name) {
            self.sign_in(i);
        } else if !name.is_empty() {
            self.screen = Screen::Waiver;
        }
    }

    /// The waiver accepted: the new hunter is saved and signed in.
    pub(super) fn accept_waiver(&mut self) {
        let used: Vec<i32> = self.players.iter().map(|p| p.reg_number).collect();
        let Some(reg) = (0..save::MAX_PLAYERS).find(|r| !used.contains(r)) else {
            tracing::warn!("all {} player slots are used", save::MAX_PLAYERS);
            self.screen = Screen::Register;
            return;
        };
        let p = save::new_player(self.name.trim(), reg, self.start_score);
        if let Err(e) = save::store(&p) {
            tracing::error!("{e:#}");
        }
        self.players.push(p);
        self.players.sort_by_key(|p| p.reg_number);
        let i = self.players.iter().position(|p| p.reg_number == reg);
        if let Some(i) = i {
            self.sign_in(i);
        }
    }

    /// Deletes the picked hunter (confirmed).
    pub(super) fn delete_picked(&mut self) {
        if let Some(i) = self.picked.take() {
            let p = self.players.remove(i);
            if let Err(e) = save::delete(p.reg_number) {
                tracing::error!("{e:#}");
            }
            self.player = None;
            self.term = None;
            self.name.clear();
        }
        self.screen = Screen::Register;
    }

    pub(super) fn update_waiver(&mut self, input: &Input, region: u8) {
        if input.clicked && region == 1 {
            self.accept_waiver();
        } else if (input.clicked && region == 2) || input.pressed(KeyCode::Escape) {
            self.screen = Screen::Register;
        }
    }

    pub(super) fn update_delete(&mut self, input: &Input, region: u8) {
        if input.clicked && region == 1 {
            self.delete_picked();
        } else if (input.clicked && region == 2) || input.pressed(KeyCode::Escape) {
            self.screen = Screen::Register;
        }
    }

    pub(super) fn draw_register(&self, out: &mut Vec<Sprite>, region: u8) {
        self.board(out, &self.reg, &[region]);
        let caret = if (self.frame_time() * 2.0) as i32 % 2 == 0 {
            "_"
        } else {
            ""
        };
        self.text(
            out,
            306.0,
            322.0,
            12.0,
            BLUE,
            &format!("{}{caret}", self.name),
        );
        for (i, p) in self.players.iter().enumerate() {
            let y = 370.0 + i as f32 * 18.0;
            let hover = inside(self.cursor, (304.0, y - 2.0, 192.0, 18.0));
            let c = if self.picked == Some(i) {
                PICKED
            } else if hover {
                HOVER
            } else {
                BLUE
            };
            self.text(out, 308.0, y, 12.0, c, &p.player_name());
        }
    }
}
