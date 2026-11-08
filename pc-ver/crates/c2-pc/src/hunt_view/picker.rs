//! The weapon and call pickers: a short press of a pick key takes the
//! next one, holding it opens a list (hexagons with the phone kit).

use super::*;

/// What a pick key picks: a weapon, or a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pick {
    Weapon,
    Call,
}

/// Seconds a pick key is held before its picker opens (a shorter press
/// takes the next one).
pub(super) const HOLD_TO_PICK: f32 = 0.35;

impl HuntView {
    /// The animal the picked call is for: the species of that AI, or (the
    /// dialect) the one answering its slot.
    pub(super) fn call_name(&self) -> Option<String> {
        self.call_name_of(self.hunt.target_call)
    }

    /// The animal call `call` (10..) calls.
    pub(super) fn call_name_of(&self, call: i32) -> Option<String> {
        let h = &self.hunt;
        let slot = call - 10;
        h.herd
            .species
            .iter()
            .find(|s| match &s.mee {
                Some(m) => m.call_no == slot,
                None => s
                    .tuning
                    .menu_slot
                    .map_or(s.info.ai == call, |k| k >= 0 && k == slot),
            })
            .map(|s| s.info.name.clone())
    }

    /// The calls of the hunted animals (ids 10..).
    pub(super) fn call_list(&self) -> Vec<i32> {
        let t = self.hunt.opts.targets;
        (10..c2_core::game::MAX_CALL)
            .filter(|&c| usize::try_from(c).is_ok_and(|c| t.has(c)))
            .collect()
    }

    /// What `kind`'s picker lists: weapons by index, calls by id.
    pub(super) fn pick_list(&self, kind: Pick) -> Vec<usize> {
        match kind {
            Pick::Weapon => self.carried(),
            Pick::Call => self.call_list().iter().map(|&c| c as usize).collect(),
        }
    }

    /// Opens `kind`'s picker on the one in use (the hunt waits).
    pub(super) fn open_pick(&mut self, kind: Pick, held: bool) {
        let cur = match kind {
            Pick::Weapon => self.hunt.weapons.current,
            Pick::Call => self.hunt.target_call.max(0) as usize,
        };
        let list = self.pick_list(kind);
        if list.is_empty() {
            return;
        }
        self.picker = Some(list.iter().position(|&i| i == cur).unwrap_or(0));
        self.picker_kind = kind;
        self.picker_held = held;
        self.acc_ms = 0.0;
    }

    /// The key that picks `kind` is down.
    pub(super) fn pick_held(&self, input: &Input, kind: Pick) -> bool {
        if self.touch_holds_picker() {
            return true;
        }
        match kind {
            Pick::Weapon => input.held(self.pick_key()),
            Pick::Call => keys::held(input, self.hunt.room.options.keys.change_call),
        }
    }

    /// The next carried weapon, its hexagon shown a moment.
    pub(super) fn next_weapon(&mut self) {
        let carried = self.carried();
        let cur = self.hunt.weapons.target;
        let at = carried.iter().position(|&w| w == cur);
        let next = at.map_or(0, |i| (i + 1) % carried.len().max(1));
        if let Some(&w) = carried.get(next) {
            self.pending.select_weapon = Some(w);
            self.weapon_flash = 1.2;
        }
    }

    /// The carried weapons, for the pick screen.
    pub(super) fn carried(&self) -> Vec<usize> {
        let w = &self.hunt.weapons;
        (0..w.info.len()).filter(|&i| w.carried(i)).collect()
    }

    /// The pick screen's key: Q, or G when Q is bound to something.
    pub(super) fn pick_key(&self) -> KeyCode {
        if self.key_bound(81) {
            KeyCode::KeyG
        } else {
            KeyCode::KeyQ
        }
    }

    /// Input while the pick screen is up: arrows/W/S or the wheel move,
    /// 1-9 jump, Enter/click picks, the key again or Esc closes; opened by
    /// holding the key, letting it go picks.
    pub(super) fn update_picker(&mut self, input: &Input, row: usize) {
        let kind = self.picker_kind;
        let list = self.pick_list(kind);
        let n = list.len();
        let key_again = match kind {
            Pick::Weapon => input.pressed(self.pick_key()),
            Pick::Call => keys::pressed(input, self.hunt.room.options.keys.change_call),
        };
        if n == 0 || input.pressed(KeyCode::Escape) || (key_again && !self.picker_held) {
            self.picker = None;
            return;
        }
        let mut row = row.min(n - 1);
        let up =
            input.pressed(KeyCode::ArrowUp) || input.pressed(KeyCode::KeyW) || input.scroll > 0.0;
        let down =
            input.pressed(KeyCode::ArrowDown) || input.pressed(KeyCode::KeyS) || input.scroll < 0.0;
        if up {
            row = (row + n - 1) % n;
        }
        if down {
            row = (row + 1) % n;
        }
        const DIGITS: [KeyCode; 9] = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ];
        let digit = DIGITS
            .iter()
            .position(|&k| input.pressed(k))
            .filter(|&d| d < n);
        if let Some(d) = digit {
            row = d;
        }
        // By touch: the hexagon under the finger, picked when it lets go.
        let Ok((touched, tap)) = self.touch_pick(n) else {
            self.picker = None;
            return;
        };
        if let Some(r) = touched {
            row = r;
        }
        let let_go = self.picker_held && !self.pick_held(input, kind);
        if digit.is_some() || input.pressed(KeyCode::Enter) || let_go || tap {
            let pick = list.get(row).copied();
            match kind {
                Pick::Weapon => {
                    let w = &self.hunt.weapons;
                    // Picking the weapon in hand leaves it there
                    // (selecting it would put it away).
                    if !(pick == Some(w.current) && w.pose().is_some()) {
                        self.pending.select_weapon = pick;
                    }
                }
                Pick::Call => {
                    if let Some(c) = pick {
                        self.hunt.target_call = c as i32;
                        self.hunt.call_icon_time = 2048;
                    }
                }
            }
            self.picker = None;
            return;
        }
        self.picker = Some(row);
    }

    /// The call-pick screen: as the weapons', the hunted animals' names.
    pub(super) fn call_picker_sprites(
        &self,
        row: usize,
        screen: (u32, u32),
        scale: f32,
    ) -> Vec<Sprite> {
        let mut out = Vec::new();
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        out.extend(self.hud.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]));
        let list = self.call_list();
        let (px, lh, bw) = (8.0 * scale, 14.0 * scale, 300.0 * scale);
        let shown = list.len().min(12);
        let bh = (shown as f32 + 2.0) * lh + 8.0 * scale;
        let (x, y) = ((sw - bw) / 2.0, (sh - bh) / 2.0);
        out.extend(self.hud.rect(x, y, bw, bh, [0.05, 0.05, 0.05, 0.85]));
        let white = [0.9, 0.9, 0.9, 1.0];
        out.extend(
            self.hud
                .label(x + 8.0 * scale, y + 6.0 * scale, px, white, "Choose a call"),
        );
        let first = row.saturating_sub(11);
        for (k, &c) in list.iter().enumerate().skip(first).take(12) {
            let ry = y + 6.0 * scale + (k - first + 1) as f32 * lh + 4.0 * scale;
            if k == row {
                let hl = [0.2, 0.45, 0.2, 0.9];
                out.extend(self.hud.rect(
                    x + 4.0 * scale,
                    ry - 3.0 * scale,
                    bw - 8.0 * scale,
                    lh,
                    hl,
                ));
            }
            let name = self
                .call_name_of(c)
                .unwrap_or_else(|| format!("Call {}", c - 9));
            let t = format!("{}. {name}", k + 1);
            out.extend(self.hud.label(x + 10.0 * scale, ry, px, white, &t));
        }
        out
    }

    /// The weapon-pick screen: a dimmed world and a list of the carried
    /// weapons with their ammo, scrolled to keep `row` in view.
    pub(super) fn picker_sprites(&self, row: usize, screen: (u32, u32), scale: f32) -> Vec<Sprite> {
        if self.picker_kind == Pick::Call {
            return self.call_picker_sprites(row, screen, scale);
        }
        let mut out = Vec::new();
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        out.extend(self.hud.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]));
        let w = &self.hunt.weapons;
        let list = self.carried();
        const ROWS: usize = 12;
        let first = row
            .saturating_sub(ROWS - 1)
            .min(list.len().saturating_sub(ROWS));
        let px = 8.0 * scale;
        let lh = 14.0 * scale;
        let bw = 300.0 * scale;
        let shown = list.len().min(ROWS);
        let bh = (shown as f32 + 3.0) * lh + 8.0 * scale;
        let (x, y) = ((sw - bw) / 2.0, (sh - bh) / 2.0);
        out.extend(self.hud.rect(x, y, bw, bh, [0.05, 0.05, 0.05, 0.85]));
        let white = [0.9, 0.9, 0.9, 1.0];
        let grey = [0.5, 0.5, 0.5, 1.0];
        out.extend(self.hud.label(
            x + 8.0 * scale,
            y + 6.0 * scale,
            px,
            white,
            "Choose a weapon",
        ));
        for (k, &i) in list.iter().enumerate().skip(first).take(ROWS) {
            let ry = y + 6.0 * scale + (k - first + 1) as f32 * lh + 4.0 * scale;
            if k == row {
                out.extend(self.hud.rect(
                    x + 4.0 * scale,
                    ry - 3.0 * scale,
                    bw - 8.0 * scale,
                    lh,
                    [0.2, 0.45, 0.2, 0.9],
                ));
            }
            let shots = w.rounds(i);
            let color = if shots > 0 { white } else { grey };
            let num = if k < 9 {
                format!("{}.", k + 1)
            } else {
                "  ".into()
            };
            let mark = if i == w.current && w.pose().is_some() {
                " *"
            } else {
                ""
            };
            let name = format!("{num} {}{mark}", w.info[i].name);
            out.extend(self.hud.label(x + 10.0 * scale, ry, px, color, &name));
            let ammo = if shots > 0 {
                format!("{shots}")
            } else {
                "empty".into()
            };
            let aw = self.hud.label_width(px, &ammo);
            out.extend(
                self.hud
                    .label(x + bw - aw - 12.0 * scale, ry, px, color, &ammo),
            );
        }
        let hint = "Up/Down or 1-9, Enter (or let go) to take, Esc";
        let hy = y + bh - lh;
        out.extend(self.hud.label(x + 8.0 * scale, hy, px, grey, hint));
        out
    }

    /// The open picker: the phone's hexagons of the carried weapons' (or
    /// the calls') pictures, or the PC's list.
    pub(super) fn draw_picker(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let Some(row) = self.picker else {
            return;
        };
        let Some(k) = &self.kit else {
            out.extend(self.picker_sprites(row, screen, scale));
            return;
        };
        let items: Vec<_> = self
            .pick_list(self.picker_kind)
            .iter()
            .enumerate()
            .map(|(n, &i)| {
                let pic = match self.picker_kind {
                    Pick::Weapon => k.weapon_picture(i),
                    Pick::Call => self.call_pics.get(i.saturating_sub(10)).copied().flatten(),
                };
                (pic, n == row)
            })
            .collect();
        k.hexes(&self.hud, screen, &items, out);
    }

    /// Whether key `vk` is bound to one of the hunt's actions.
    pub(super) fn key_bound(&self, vk: i32) -> bool {
        let k = &self.hunt.room.options.keys;
        [
            k.forward,
            k.backward,
            k.up,
            k.down,
            k.left,
            k.right,
            k.fire,
            k.show,
            k.strafe_left,
            k.strafe_right,
            k.strafe,
            k.jump,
            k.run,
            k.crouch,
            k.call,
            k.change_call,
            k.binoculars,
        ]
        .contains(&vk)
    }
}
