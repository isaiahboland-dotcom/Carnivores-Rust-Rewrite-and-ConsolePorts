//! The main menu, the hunters list and the survival chooser in the DHC OS
//! terminal's look (its setting on): the same buttons and taps as the
//! classic pages (`menu.rs`), drawn as framed terminal rows over a plain
//! dark background.

use super::{hunter_row, pick_rect, Menu, Step, BACK, BAR_Y, HUNTER, PLAY, QUIT};
use super::{SETTINGS, SURVIVAL, TROPHIES};
use crate::dhc_look::*;
use crate::draw2d::Canvas;
use crate::gpu::{Gpu, TOP_W};

impl Menu {
    /// Whether this page is drawn by this module.
    pub(super) fn dhc_page(&self) -> bool {
        self.settings.dhc_terminal && matches!(self.step, Step::Title | Step::Hunters)
    }

    /// Both screens for the page.
    pub(super) fn draw_dhc_page(&mut self, gpu: &mut Gpu) {
        let mut c = std::mem::replace(&mut self.c, Canvas::new(None));
        gpu.begin_2d(None);
        self.dhc_top(&mut c, gpu);
        c.flush(gpu);
        gpu.begin_2d(Some([BG[0], BG[1], BG[2]]));
        c.rect([0.0, 0.0, 320.0, 240.0], BG);
        match (self.step, self.survival_pick) {
            (Step::Hunters, _) => self.dhc_hunters(&mut c, gpu),
            (_, true) => self.dhc_survival_pick(&mut c, gpu),
            _ => self.dhc_title(&mut c, gpu),
        }
        c.flush(gpu);
        self.c = c;
    }

    /// The top screen: the terminal's header and the hunter's record.
    fn dhc_top(&self, c: &mut Canvas, gpu: &mut Gpu) {
        c.rect([0.0, 0.0, TOP_W as f32, 240.0], BG);
        c.rect([0.0, 15.0, TOP_W as f32, 1.0], LINE);
        c.flush(gpu);
        c.text(gpu, 4.0, 4.0, 1.0, HI, "DINOHUNT CORP // DHC-OS");
        let name: String = String::from_utf8_lossy(&self.save.player_name_raw)
            .trim_end_matches('\0')
            .to_uppercase()
            .chars()
            .take(20)
            .collect();
        c.text(gpu, 396.0 - name.len() as f32 * 8.0, 4.0, 1.0, LO, &name);
        let ranks = ["BEGINNER", "ADVANCED", "EXPERT"];
        let rank = ranks[self.save.rank.clamp(0, 2) as usize];
        let trophies = self.save.bodies.iter().filter(|b| b.ctype != 0).count();
        let mut rows = vec![
            ("RANK", rank.to_string()),
            ("SCORE", self.save.score.to_string()),
            ("TROPHIES", trophies.to_string()),
            ("KILLS", self.save.total.success.to_string()),
        ];
        let r = &self.record;
        if let Some(k) = r.longest() {
            rows.push(("LONGEST", format!("{:.0} m", k.range)));
        }
        if let Some(k) = r.best() {
            let animal: String = k.animal.to_uppercase().chars().take(12).collect();
            rows.push(("BEST", format!("{} {animal}", k.score)));
        }
        if r.best_wave > 0 {
            rows.push(("WAVE", r.best_wave.to_string()));
        }
        c.text(gpu, 12.0, 28.0, 1.0, MD, "HUNTER RECORD");
        for (i, (k, v)) in rows.iter().enumerate() {
            let y = 48.0 + 16.0 * i as f32;
            c.rect([12.0, y + 11.0, 376.0, 1.0], LINE);
            c.flush(gpu);
            c.text(gpu, 12.0, y, 1.0, LO, k);
            c.text(gpu, 388.0 - v.len() as f32 * 8.0, y, 1.0, HI, v);
        }
    }

    /// A terminal button: a frame and a left-aligned label.
    fn dhc_button(c: &mut Canvas, gpu: &mut Gpu, r: [f32; 4], label: &str, on: bool, lit: bool) {
        frame(
            c,
            r,
            if lit { LO } else { PANEL },
            if on { MD } else { LINE },
        );
        c.flush(gpu);
        let col = match (on, lit) {
            (false, _) => FAINT,
            (_, true) => INV,
            _ => HI,
        };
        c.text(gpu, r[0] + 8.0, r[1] + (r[3] - 8.0) / 2.0, 1.0, col, label);
    }

    fn dhc_title(&mut self, c: &mut Canvas, gpu: &mut Gpu) {
        c.text(gpu, 4.0, 4.0, 1.0, HI, "MAIN MENU");
        c.rect([0.0, 15.0, 320.0, 1.0], LINE);
        let mut buttons = vec![("> PLAY", PLAY, true)];
        if !self.survival_modes().is_empty() {
            buttons.push(("> SURVIVAL", SURVIVAL, true));
        }
        buttons.extend([
            ("> TROPHY ROOM", TROPHIES, self.trophy_room),
            ("> SETTINGS", SETTINGS, true),
            ("> QUIT", QUIT, true),
            ("> PROFILE", HUNTER, true),
        ]);
        for (label, r, on) in buttons {
            Self::dhc_button(c, gpu, r, label, on, false);
        }
        if let Some(m) = &self.message {
            let m: String = m.chars().take(38).collect();
            c.text_c(gpu, 160.0, 224.0, 1.0, AMBER, &m);
        }
    }

    fn dhc_survival_pick(&mut self, c: &mut Canvas, gpu: &mut Gpu) {
        c.text(gpu, 4.0, 4.0, 1.0, HI, "SURVIVAL // SELECT");
        c.rect([0.0, 15.0, 320.0, 1.0], LINE);
        for (i, (_, label)) in self.survival_modes().iter().enumerate() {
            let t = format!("> {}", label.to_uppercase());
            Self::dhc_button(c, gpu, pick_rect(i), &t, true, false);
        }
        c.text_c(gpu, 160.0, 220.0, 1.0, LO, "TAP ELSEWHERE TO GO BACK");
    }

    fn dhc_hunters(&mut self, c: &mut Canvas, gpu: &mut Gpu) {
        c.text(gpu, 4.0, 4.0, 1.0, HI, "PROFILES");
        c.rect([0.0, 15.0, 320.0, 1.0], LINE);
        let list = crate::save::hunters();
        let me = crate::save::hunter();
        let mut rows: Vec<(String, bool)> = list
            .iter()
            .map(|(n, name)| (name.to_uppercase(), *n == me))
            .collect();
        if crate::save::free_slot().is_some() {
            rows.push(("+ NEW PROFILE".into(), false));
        }
        for (i, (name, on)) in rows.iter().enumerate() {
            let n: String = name.chars().take(26).collect();
            let mark = if *on { "(*)" } else { "( )" };
            Self::dhc_button(c, gpu, hunter_row(i), &format!("{mark} {n}"), true, *on);
        }
        Self::dhc_button(c, gpu, BACK, "L BACK", true, false);
        c.rect([0.0, BAR_Y - 5.0, 320.0, 1.0], LINE);
        if let Some(m) = &self.message {
            let m: String = m.chars().take(38).collect();
            c.text_c(gpu, 160.0, 200.0, 1.0, AMBER, &m);
        }
    }
}
