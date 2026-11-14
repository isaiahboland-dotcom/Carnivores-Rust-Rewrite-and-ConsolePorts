//! Drawing the terminal on both screens: the step's list and the bar on
//! the bottom screen; the focused item's picture and description, the
//! budget and the earnings on the top screen. The colour scheme is the PC
//! terminal's; no scanlines, flicker or noise on the 3DS.

use c2_core::dhc::Tab;

use super::terminal::{wrap, GearRow, BOOT_LINE_FRAMES, LIST_Y, ROW, ROWS, T_BACK, T_MID, T_NEXT};
use super::{Menu, Pick, Step, EXTRA_AMMO, MAX_PICKS, TIMES};
use crate::dhc_look::*;
use crate::draw2d::{Canvas, Rgba};
use crate::gpu::{Gpu, TOP_W};

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// One list row: mark, name, and the price or why it can't be picked.
struct Row {
    mark: &'static str,
    name: String,
    right: String,
    color: Rgba,
    right_color: Rgba,
}

impl Menu {
    fn term_rows(&self) -> Vec<Row> {
        let left = self.remaining();
        let row = |mark, name: &str, price: i32, picked: bool, allowed: bool, roam: bool| {
            let afford = picked || price <= 0 || left - price >= 0 || self.settings.test_points;
            let (color, right, right_color) = if !allowed {
                (FAINT, "RESTRICTED".to_string(), FAINT)
            } else if picked {
                (HI, price.to_string(), HI)
            } else if !afford {
                (FAINT, price.to_string(), RED)
            } else if roam {
                ([120, 180, 255, 255], price.to_string(), MD)
            } else {
                (MD, price.to_string(), MD)
            };
            Row {
                mark,
                name: name.to_string(),
                right,
                color,
                right_color,
            }
        };
        let mark = |on: bool| if on { "[x]" } else { "[ ]" };
        match self.term_list() {
            0 => self
                .areas
                .iter()
                .enumerate()
                .map(|(i, (_, a))| {
                    let on = i == self.area;
                    row(
                        if on { "(*)" } else { "( )" },
                        &a.name,
                        a.price,
                        on,
                        true,
                        false,
                    )
                })
                .collect(),
            1 => self
                .term_animals()
                .into_iter()
                .map(|i| {
                    let (p, it) = &self.dinos[i];
                    let on = self.picked_dinos.has(i);
                    let allowed = on || self.island_allows(Tab::Targets, i);
                    let roam = matches!(p, Pick::Roamer(_));
                    row(mark(on), &it.name, it.price, on, allowed, roam)
                })
                .collect(),
            _ => self
                .gear_rows()
                .into_iter()
                .map(|g| match g {
                    GearRow::Weapon(i) => {
                        let it = &self.weapons[i];
                        let on = self.picked_weapons.has(i);
                        let allowed = on || self.island_allows(Tab::Arms, i);
                        row(mark(on), &it.name, it.price, on, allowed, false)
                    }
                    GearRow::Equip(i) => {
                        let it = &self.equip[i];
                        let on = self.picked_equip.has(i);
                        let allowed = on || self.island_allows(Tab::Gear, i);
                        row(mark(on), &it.name, it.price, on, allowed, false)
                    }
                    GearRow::Ammo => {
                        let name = match self.extra_ammo {
                            0 => "Extra ammo: off".to_string(),
                            n => format!("Extra ammo: x{n}"),
                        };
                        let price = self.extra().map_or(0, |e| e.0);
                        row(mark(self.extra_ammo > 0), &name, price, true, true, false)
                    }
                    GearRow::Tranq => {
                        let on = self.save.options.tranq != 0;
                        let mut r = row(mark(on), "Tranquilizer", 0, on, true, false);
                        r.right = "x1.25".into();
                        r
                    }
                    GearRow::Observe => {
                        let mut r = row(
                            mark(self.observer),
                            "Observer",
                            0,
                            self.observer,
                            true,
                            false,
                        );
                        r.right = "FREE".into();
                        r
                    }
                })
                .collect(),
        }
    }

    /// The bottom screen.
    pub(super) fn term_draw_bottom(&self, c: &mut Canvas, gpu: &mut Gpu) {
        c.rect([0.0, 0.0, 320.0, 240.0], BG);
        if self.term.boot.is_some() {
            c.flush(gpu);
            c.text_c(gpu, 160.0, 112.0, 1.0, LO, "ANY BUTTON: SKIP");
            return;
        }
        let (title, n) = match self.step {
            Step::Area => ("AREA", 1),
            Step::Animals => ("TARGETS", 2),
            _ => ("ARMS & GEAR", 3),
        };
        c.rect([0.0, 15.0, 320.0, 1.0], LINE);
        let rows = self.term_rows();
        let first = self.term_first();
        let focus = self.term.rows[self.term_list()];
        if self.tranq_ask {
            self.term_draw_tranq(c, gpu);
        } else {
            for k in (first..rows.len()).take(ROWS) {
                let y = LIST_Y + (k - first) as f32 * ROW;
                if k == focus {
                    c.rect([2.0, y, 316.0, ROW - 1.0], LO);
                }
            }
            c.flush(gpu);
            for (k, r) in rows.iter().enumerate().skip(first).take(ROWS) {
                let y = LIST_Y + (k - first) as f32 * ROW + 4.0;
                let on = k == focus;
                let (col, rcol) = if on {
                    (INV, INV)
                } else {
                    (r.color, r.right_color)
                };
                let room = 38 - r.right.len().min(12);
                let text = cut(&format!("{} {}", r.mark, r.name), room.saturating_sub(1));
                c.text(gpu, 6.0, y, 1.0, col, &text);
                c.text(
                    gpu,
                    314.0 - r.right.len() as f32 * 8.0,
                    y,
                    1.0,
                    rcol,
                    &r.right,
                );
            }
            if rows.is_empty() {
                c.text(gpu, 8.0, LIST_Y + 4.0, 1.0, LO, "NOTHING ON FILE");
            }
            let pages = rows.len().div_ceil(ROWS).max(1);
            if pages > 1 {
                let p = format!("{}/{pages}", first / ROWS + 1);
                c.text(gpu, 314.0 - p.len() as f32 * 8.0, 204.0 - 8.0, 1.0, LO, &p);
            }
        }
        // The header: step, page of the steps, balance.
        c.text(gpu, 4.0, 4.0, 1.0, HI, &format!("{n}/3 {title}"));
        let bal = if self.settings.test_points {
            "TEST PTS".to_string()
        } else {
            format!("BAL {}", self.remaining())
        };
        let bc = if self.remaining() < 0 { RED } else { MD };
        c.text(gpu, 316.0 - bal.len() as f32 * 8.0, 4.0, 1.0, bc, &bal);
        // The bar: L Back, the middle, R Next.
        c.rect([0.0, 207.0, 320.0, 1.0], LINE);
        frame(c, T_BACK, PANEL, LINE);
        frame(c, T_NEXT, MD, HI);
        let mid = match self.step {
            Step::Area => Some(format!(
                "X TIME: {}",
                TIMES[self.time.min(2)].1.to_uppercase()
            )),
            _ => None,
        };
        if mid.is_some() {
            frame(c, T_MID, PANEL, LINE);
        }
        c.flush(gpu);
        let centre = |r: [f32; 4]| r[0] + r[2] / 2.0;
        c.text_c(gpu, centre(T_BACK), T_BACK[1] + 8.0, 1.0, MD, "L BACK");
        c.text_c(gpu, centre(T_NEXT), T_NEXT[1] + 8.0, 1.0, INV, "R NEXT");
        match mid {
            Some(t) => c.text_c(gpu, centre(T_MID), T_MID[1] + 8.0, 1.0, MD, &t),
            None => c.text_c(gpu, centre(T_MID), T_MID[1] + 8.0, 1.0, LO, "A PICK"),
        }
    }

    /// Multi ammo's question, in the terminal's look (the Tranq / Observe
    /// rectangles answer, as on the grid page).
    fn term_draw_tranq(&self, c: &mut Canvas, gpu: &mut Gpu) {
        frame(c, Self::TRANQ, MD, HI);
        frame(c, Self::OBSERVE, PANEL, HI);
        c.flush(gpu);
        c.text_c(gpu, 160.0, 60.0, 1.0, HI, "TRANQUILIZER ROUNDS");
        c.text_c(gpu, 160.0, 80.0, 1.0, MD, "Half: lethal and darts, switch");
        c.text_c(gpu, 160.0, 92.0, 1.0, MD, "in the hunt. Full: darts only.");
        let t = Self::TRANQ;
        c.text_c(gpu, t[0] + t[2] / 2.0, t[1] + 8.0, 1.0, INV, "A HALF");
        let o = Self::OBSERVE;
        c.text_c(gpu, o[0] + o[2] / 2.0, o[1] + 8.0, 1.0, MD, "Y FULL");
    }

    /// The top screen.
    pub(super) fn term_draw_top(&self, c: &mut Canvas, gpu: &mut Gpu) {
        c.rect([0.0, 0.0, TOP_W as f32, 240.0], BG);
        c.rect([0.0, 15.0, TOP_W as f32, 1.0], LINE);
        c.flush(gpu);
        c.text(gpu, 4.0, 4.0, 1.0, HI, "DINOHUNT CORP // DHC-OS");
        let name = String::from_utf8_lossy(&self.save.player_name_raw)
            .trim_end_matches('\0')
            .to_uppercase();
        let name = cut(&name, 20);
        c.text(gpu, 396.0 - name.len() as f32 * 8.0, 4.0, 1.0, LO, &name);
        if let Some((lines, f)) = &self.term.boot {
            let shown = (*f / BOOT_LINE_FRAMES + 1) as usize;
            for (i, l) in lines.iter().take(shown).enumerate() {
                let col = if i == 0 { HI } else { MD };
                c.text(
                    gpu,
                    8.0,
                    26.0 + i as f32 * 14.0,
                    1.0,
                    col,
                    &cut(&format!("> {l}"), 48),
                );
            }
            return;
        }
        self.term_draw_item(c, gpu);
        self.term_draw_budget(c, gpu);
    }

    /// The focused item: picture, name, price, status, description.
    fn term_draw_item(&self, c: &mut Canvas, gpu: &mut Gpu) {
        let row = self.term.rows[self.term_list()];
        let (name, price, pic) = match self.term_list() {
            0 => match self.areas.get(row) {
                Some((_, a)) => (a.name.clone(), a.price, a.pic.as_ref()),
                None => (String::new(), 0, None),
            },
            1 => match self
                .term_animals()
                .get(row)
                .and_then(|&i| self.dinos.get(i))
            {
                Some((_, it)) => (it.name.clone(), it.price, it.pic.as_ref()),
                None => (String::new(), 0, None),
            },
            _ => match self.gear_rows().get(row) {
                Some(GearRow::Weapon(i)) => {
                    let it = &self.weapons[*i];
                    (it.name.clone(), it.price, it.pic.as_ref())
                }
                Some(GearRow::Equip(i)) => {
                    let it = &self.equip[*i];
                    (it.name.clone(), it.price, it.pic.as_ref())
                }
                Some(GearRow::Ammo) => {
                    let p = EXTRA_AMMO
                        .get(self.extra_ammo.saturating_sub(1))
                        .map_or(0, |e| e.0);
                    ("Extra ammo".into(), p, self.ammo_pic.as_ref())
                }
                Some(GearRow::Tranq) => ("Tranquilizer".into(), 0, None),
                Some(GearRow::Observe) => ("Observer".into(), 0, None),
                None => (String::new(), 0, None),
            },
        };
        let pic_r = [8.0, 22.0, 192.0, 112.0];
        frame(
            c,
            [
                pic_r[0] - 1.0,
                pic_r[1] - 1.0,
                pic_r[2] + 2.0,
                pic_r[3] + 2.0,
            ],
            PANEL,
            LINE,
        );
        c.flush(gpu);
        match pic {
            // Menu pictures are fitted to the texture's width: their top
            // 0.58 (`draw_hex_item`).
            Some(t) => c.image(gpu, t, pic_r, [0.0, 0.0, 1.0, 0.58], [255; 4]),
            None => c.text_c(gpu, 104.0, 74.0, 1.0, FAINT, "NO IMAGE ON FILE"),
        }
        // Right of the picture: name, price, status, the picks so far.
        let x = 210.0;
        c.text(gpu, x, 22.0, 1.0, HI, &cut(&name.to_uppercase(), 23));
        if price > 0 {
            c.text(gpu, x, 34.0, 1.0, MD, &format!("{price} PTS"));
        }
        let status = match self.term_focus() {
            Some((tab, i)) if tab != Tab::Area && !self.island_allows(tab, i) => {
                let picked = match tab {
                    Tab::Targets => self.picked_dinos.has(i),
                    Tab::Arms => self.picked_weapons.has(i),
                    _ => self.picked_equip.has(i),
                };
                if picked {
                    ("ISSUED", HI)
                } else {
                    ("RESTRICTED HERE", AMBER)
                }
            }
            Some((Tab::Area, i)) if i == self.area => ("SELECTED", HI),
            Some((Tab::Targets, i)) if self.picked_dinos.has(i) => ("ISSUED", HI),
            Some((Tab::Arms, i)) if self.picked_weapons.has(i) => ("ISSUED", HI),
            Some((Tab::Gear, i)) if self.picked_equip.has(i) => ("ISSUED", HI),
            _ if price > 0 && self.remaining() < price && !self.settings.test_points => {
                ("INSUFFICIENT FUNDS", RED)
            }
            _ => ("AVAILABLE", MD),
        };
        c.text(gpu, x, 46.0, 1.0, status.1, status.0);
        let area = self
            .areas
            .get(self.area)
            .map_or(String::new(), |a| a.1.name.clone());
        let lines = [
            format!("AREA  {}", cut(&area.to_uppercase(), 17)),
            format!("TIME  {}", TIMES[self.time.min(2)].1.to_uppercase()),
            format!("TARGETS {}/{MAX_PICKS}", self.picked_dinos.count()),
            format!("ARMS    {}/{MAX_PICKS}", self.picked_weapons.count()),
        ];
        for (i, l) in lines.iter().enumerate() {
            c.text(gpu, x, 70.0 + i as f32 * 12.0, 1.0, LO, l);
        }
        // The description, under the picture.
        let desc = self
            .term
            .desc
            .as_ref()
            .map(|(_, t)| wrap(t, 48))
            .unwrap_or_default();
        let desc: Vec<&String> = desc.iter().filter(|l| !l.trim().is_empty()).collect();
        for (i, l) in desc.iter().take(5).enumerate() {
            c.text(gpu, 8.0, 140.0 + i as f32 * 10.0, 1.0, MD, l);
        }
    }

    /// Account, permits, balance, the meter, earnings and the memory line.
    fn term_draw_budget(&self, c: &mut Canvas, gpu: &mut Gpu) {
        let score = self.points();
        let left = self.remaining();
        let spent = score - left;
        let meter = [8.0, 206.0, 384.0, 6.0];
        frame(
            c,
            [
                meter[0] - 1.0,
                meter[1] - 1.0,
                meter[2] + 2.0,
                meter[3] + 2.0,
            ],
            PANEL,
            LINE,
        );
        let share = if score > 0 {
            (spent as f32 / score as f32).clamp(0.0, 1.0)
        } else if spent > 0 {
            1.0
        } else {
            0.0
        };
        let col = if left < 0 {
            RED
        } else if share > 0.85 {
            AMBER
        } else {
            MD
        };
        c.rect([meter[0], meter[1], meter[2] * share, meter[3]], col);
        c.rect([0.0, 191.0, TOP_W as f32, 1.0], LINE);
        c.flush(gpu);
        let line = if self.settings.test_points {
            "ACCOUNT: TEST (UNLIMITED)".to_string()
        } else {
            format!("ACCOUNT {score}  PERMITS -{spent}  BALANCE {left}")
        };
        c.text(gpu, 8.0, 195.0, 1.0, if left < 0 { RED } else { MD }, &line);
        // The kill-score multiplier of the picks.
        let names = self
            .equip
            .iter()
            .enumerate()
            .filter(|(i, _)| self.picked_equip.has(*i))
            .map(|(_, e)| e.name.as_str());
        let mut k = c2_core::dhc::earnings_factor(names, self.save.options.tranq != 0);
        if let Some((_, _, f)) = self.extra() {
            k *= f;
        }
        let earn = format!("EARNINGS x{k:.2}");
        c.text(
            gpu,
            8.0,
            218.0,
            1.0,
            if k < 1.0 { AMBER } else { MD },
            &earn,
        );
        // A notice, the menu's message, or what the step wants.
        let (msg, mc) = if let Some(n) = &self.term.notice {
            (n.clone(), AMBER)
        } else if let Some(m) = &self.message {
            (m.to_uppercase(), AMBER)
        } else {
            // The memory plan: animals and room to spare, or what to drop.
            if self.memory().fits {
                let line = self.plan_line().to_uppercase();
                (line.trim_start_matches("READY. ").to_string(), LO)
            } else {
                (self.too_much_line().to_uppercase(), RED)
            }
        };
        c.text(gpu, 8.0, 229.0, 1.0, mc, &cut(&msg, 48));
        if self.roamer_picked() && !self.observer {
            c.text(
                gpu,
                260.0,
                218.0,
                1.0,
                [120, 180, 255, 255],
                "NO CALLS +10%",
            );
        }
    }
}
