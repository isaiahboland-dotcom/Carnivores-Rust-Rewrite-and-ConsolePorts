//! "Remember last hunt" (a PC setting): the hunt setup starts from the
//! last hunt's area, time, animals, weapons and gear, per hunter, so a
//! quick adjustment replaces picking everything again. Items are kept by
//! name, so a different pack or an edited list drops only what is gone.

use super::*;

impl Menu {
    /// The file of the signed-in hunter's last hunt, in the game's saves.
    fn last_hunt_file(&self) -> Option<std::path::PathBuf> {
        let name: String = self
            .me()?
            .player_name()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Some(crate::save::saves_dir().join(format!("last_hunt_{name}.txt")))
    }

    /// Keeps the picks the hunt about to start was set up with.
    pub(super) fn remember_hunt(&self) {
        if self.pc.remember_hunt == 0 {
            return;
        }
        let Some(path) = self.last_hunt_file() else {
            return;
        };
        let c = &self.choice;
        let stem = |i: usize| {
            self.areas
                .get(i)
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
        };
        let mut out = String::new();
        if let Some(s) = stem(c.area) {
            out += &format!("area={s}\n");
        }
        out += &format!(
            "time={}\nobserver={}\nequip={}\n",
            c.time,
            u8::from(c.observer),
            c.equip
        );
        for i in c.dinos.iter().filter(|&i| i < self.columns[1].names.len()) {
            out += &format!("dino={}\n", self.columns[1].names[i]);
        }
        for (i, n) in self.columns[2].names.iter().enumerate() {
            if c.weapons >> i & 1 != 0 {
                out += &format!("weapon={n}\n");
            }
        }
        if let Err(e) = std::fs::write(&path, out) {
            tracing::warn!("last hunt not kept: {e}");
        }
    }

    /// Starts the setup from the last hunt (when the setting is on).
    pub(super) fn recall_hunt(&mut self) {
        if self.pc.remember_hunt == 0 {
            return;
        }
        let Some(text) = self
            .last_hunt_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
        else {
            return;
        };
        let mut c = self.choice.clone();
        c.dinos = Bits::NONE;
        c.weapons = 0;
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            match k {
                "area" => {
                    let at = self.areas.iter().position(|p| {
                        p.file_stem()
                            .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(v))
                    });
                    c.area = at.unwrap_or(c.area);
                }
                "time" => c.time = v.parse::<usize>().unwrap_or(c.time).min(2),
                "observer" => c.observer = v == "1",
                "equip" => c.equip = v.parse().unwrap_or(c.equip) & 15,
                "dino" => {
                    if let Some(i) = self.columns[1].names.iter().position(|n| n == v) {
                        c.dinos.set(i);
                    }
                }
                "weapon" => {
                    if let Some(i) = self.columns[2].names.iter().position(|n| n == v) {
                        c.weapons |= 1u128.checked_shl(i as u32).unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        let old = std::mem::replace(&mut self.choice, c);
        // What the account can't pay for, or the limits bar, is dropped.
        if self.remaining() < 0 {
            self.choice.dinos = Bits::NONE;
            self.choice.weapons = 0;
            self.choice.equip = old.equip;
        }
        if !self.within_picks() {
            self.choice.dinos = Bits::NONE;
            self.choice.weapons = 0;
        }
    }
}
