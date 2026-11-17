//! Drawing the terminal on the menus' 800x600 canvas, and where its
//! pointer targets are ([`Menu::terminal_hits`], from the same layout).

use c2_core::dhc::terminal::{MainItem, Screen as TScreen, Terminal};
use c2_core::dhc::Tab;

pub(super) use super::palette::*;
use super::*;

pub(super) const PX: f32 = 14.0;
pub(super) const SMALL: f32 = 12.0;

/// Hunt setup list rows (the panels' geometry is in `geom.rs`).
const TAB_Y: f32 = 44.0;
const ROW: f32 = 20.0;
const LIST_H: f32 = 392.0;

pub(super) fn visible_rows() -> usize {
    ((LIST_H - 16.0) / ROW) as usize
}

fn list_rect(list: Rect, first: usize, i: usize) -> Option<Rect> {
    (i >= first && i < first + visible_rows()).then(|| {
        (
            list.0 + 4.0,
            list.1 + 8.0 + (i - first) as f32 * ROW,
            list.2 - 8.0,
            ROW,
        )
    })
}

const TIMES: [&str; 3] = ["DAWN", "DAY", "NIGHT"];

impl Menu {
    /// The terminal's pointer targets on this screen.
    pub(super) fn terminal_hits(&self) -> Vec<(Rect, Hit)> {
        let Some(v) = &self.term else {
            return Vec::new();
        };
        let t = &v.term;
        let g = self.geom();
        let mut out = vec![(g.back(), Hit::Back)];
        match t.screen {
            TScreen::Main => {
                out.extend((0..t.main_items().len()).map(|i| (g.main_row(i), Hit::Row(i))));
            }
            TScreen::Survival => {
                out.extend((0..t.survival.len()).map(|i| (g.main_row(i), Hit::Row(i))));
            }
            TScreen::Setup => {
                out.extend(
                    Tab::ALL
                        .iter()
                        .enumerate()
                        .map(|(i, &tab)| (g.tab(i), Hit::Tab(tab))),
                );
                let n = t.cat.list(t.tab).len();
                out.extend((0..n).filter_map(|i| {
                    Some((list_rect(g.list(), v.first[t.tab.index()], i)?, Hit::Row(i)))
                }));
                out.push((g.time(), Hit::Time));
                out.push((g.tranq(), Hit::Tranq));
                out.push((g.observer(), Hit::Observer));
                out.push((g.deploy(), Hit::Deploy));
            }
            TScreen::Specimens => out.extend(self.index_hits(v)),
            TScreen::Specimen => out.extend(self.file_hits(v)),
            _ => {}
        }
        out
    }

    /// A one-pixel frame with a panel inside.
    pub(super) fn term_box(&self, out: &mut Vec<Sprite>, r: Rect, fill: [f32; 4], line: [f32; 4]) {
        self.fill(out, r, fill);
        for s in [
            (r.0, r.1, r.2, 1.0),
            (r.0, r.1 + r.3 - 1.0, r.2, 1.0),
            (r.0, r.1, 1.0, r.3),
            (r.0 + r.2 - 1.0, r.1, 1.0, r.3),
        ] {
            self.fill(out, s, line);
        }
    }

    /// Text clipped to `w` canvas units.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn term_text(
        &self,
        out: &mut Vec<Sprite>,
        x: f32,
        y: f32,
        px: f32,
        c: [f32; 4],
        t: &str,
        w: f32,
    ) {
        let mut s = t.to_string();
        while self.text_width(px, &s) > w && s.pop().is_some() {}
        self.text(out, x, y, px, c, &s);
    }

    pub(in crate::menu) fn draw_terminal(&self, out: &mut Vec<Sprite>) {
        let Some(v) = &self.term else { return };
        let t = &v.term;
        self.term_frame(out);
        match t.screen {
            TScreen::Boot => self.draw_term_boot(out, t),
            TScreen::Main => self.draw_term_main(out, t),
            TScreen::Setup => self.draw_term_setup(out, v),
            // The field manual is the specimen database here; the text
            // manual is for platforms without one.
            TScreen::Manual | TScreen::Specimens => self.draw_term_index(out, v),
            TScreen::Specimen => self.draw_term_file(out, v),
            TScreen::Deploy => self.draw_term_deploy(out, t),
            TScreen::Survival => self.draw_term_survival(out, t),
        }
        let pad = self.pad_used;
        let files = t.specimen_db && t.tab != Tab::Gear;
        let links = t.screen == TScreen::Specimen && !t.links().is_empty();
        let hints = match (t.screen, pad) {
            (TScreen::Boot, _) => "ANY KEY: SKIP",
            (TScreen::Main, false) => "[ESC] LOG OFF   UP/DOWN  ENTER  1-9",
            (TScreen::Main, true) => "[B] LOG OFF   D-PAD MOVE   [A] SELECT",
            (TScreen::Survival, false) => "[ESC] BACK   UP/DOWN  ENTER  1-3",
            (TScreen::Survival, true) => "[B] BACK   D-PAD MOVE   [A] SELECT",
            (TScreen::Setup, false) if files => {
                "[ESC] BACK  L/R 1-4 TABS  ENTER PICK  V FILE  T TIME  F DEPLOY"
            }
            (TScreen::Setup, false) => "[ESC] BACK  L/R 1-4 TABS  ENTER PICK  T TIME  F DEPLOY",
            (TScreen::Setup, true) if files => {
                "[B] BACK  [L]/[R] TABS  [A] PICK  [X] FILE  [Y] OBSERVER  [START] DEPLOY"
            }
            (TScreen::Setup, true) => {
                "[B] BACK  [L]/[R] TABS  [A] PICK  [X] TIME  [Y] OBSERVER  [START] DEPLOY"
            }
            (TScreen::Manual, false) => "[ESC] BACK   UP/DOWN or wheel  ENTER OPEN FILE",
            (TScreen::Manual, true) => "[B] BACK   D-PAD MOVE   [A] OPEN FILE",
            (TScreen::Specimens, false) => {
                "[ESC] BACK   L/R 1-3 LISTS   UP/DOWN or wheel   ENTER OPEN FILE"
            }
            (TScreen::Specimens, true) => "[B] BACK   [L]/[R] LISTS   D-PAD MOVE   [A] OPEN FILE",
            (TScreen::Specimen, _) if t.retrieving() => "ANY KEY: SKIP",
            (TScreen::Specimen, false) if links => {
                "[ESC] CLOSE FILE   UP/DOWN LINKS   ENTER OPEN   wheel SCROLL"
            }
            (TScreen::Specimen, true) if links => {
                "[B] CLOSE FILE   D-PAD LINKS   [A] OPEN   RIGHT STICK TURN"
            }
            (TScreen::Specimen, false) => "[ESC] CLOSE FILE   UP/DOWN or wheel  SCROLL",
            (TScreen::Specimen, true) => "[B] CLOSE FILE   D-PAD SCROLL   RIGHT STICK TURN",
            (TScreen::Deploy, false) => "ANY KEY: SKIP   [ESC] ABORT",
            (TScreen::Deploy, true) => "ANY KEY: SKIP   [B] ABORT",
        };
        self.term_status(out, hints, &format!("CREDITS {}", t.score));
    }

    /// The background and the top bar: the corporation, the hunter.
    pub(super) fn term_frame(&self, out: &mut Vec<Sprite>) {
        let g = self.geom();
        self.fill(out, self.cover(), BG);
        self.text(out, g.l, 12.0, PX, HI, "DINOHUNT CORP // DHC-OS");
        let who = self
            .me()
            .map(|p| format!("HUNTER: {}", p.player_name().to_uppercase()))
            .unwrap_or_else(|| "NOT LOGGED IN".into());
        let w = self.text_width(SMALL, &who);
        self.text(out, g.right() - w, 14.0, SMALL, LO, &who);
        self.fill(out, g.rule(34.0), LINE);
    }

    /// The status bar (key hints left, `right` at the right), then the
    /// CRT layer over everything.
    pub(super) fn term_status(&self, out: &mut Vec<Sprite>, hints: &str, right: &str) {
        let g = self.geom();
        self.fill(out, g.rule(568.0), LINE);
        let w = self.text_width(SMALL, right);
        let hc = if inside(self.cursor, g.back()) {
            HI
        } else {
            LO
        };
        self.term_text(out, g.l, 576.0, SMALL, hc, hints, g.w - w - 16.0);
        self.text(out, g.right() - w, 576.0, SMALL, MD, right);
        // The CRT layers go over everything, after the sprites (`crt.rs`).
        self.crt_frame.set(true);
    }

    pub(super) fn draw_term_boot(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let shown = t.boot_shown();
        let x = self.geom().l + 24.0;
        for (i, l) in t.boot_lines.iter().take(shown).enumerate() {
            let c = if i == 0 { HI } else { MD };
            self.text(out, x, 64.0 + i as f32 * 24.0, PX, c, &format!("> {l}"));
        }
        if (t.clock() * 2.0) as i32 % 2 == 0 {
            self.text(out, x, 64.0 + shown as f32 * 24.0, PX, HI, "_");
        }
    }

    fn draw_term_main(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let g = self.geom();
        let main_x = g.main_x();
        self.text(out, main_x, 76.0, PX, LO, "MAIN MENU");
        let items = t.main_items();
        for (i, item) in items.iter().enumerate() {
            let r = g.main_row(i);
            let label = format!("[{}] {}", i + 1, item.label());
            if i == t.main_row {
                self.fill(out, r, MD);
                self.text(out, main_x, r.1 + 8.0, PX, INV, &label);
            } else {
                self.text(out, main_x, r.1 + 8.0, PX, MD, &label);
            }
        }
        // The hunter's file and the corporate rules in force.
        let panel = g.file_panel();
        self.term_box(out, panel, PANEL, LINE);
        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
        if let Some(p) = self.me() {
            let rank = ["BEGINNER", "ADVANCED", "EXPERT"]
                .get(p.rank.clamp(0, 2) as usize)
                .copied()
                .unwrap_or("");
            lines.push((format!("HUNTER   {}", p.player_name().to_uppercase()), HI));
            lines.push((format!("ACCOUNT  {} PTS", p.score), MD));
            lines.push((format!("RANK     {rank}"), MD));
        }
        lines.push((String::new(), MD));
        match &t.rules.dhc {
            Some(d) => {
                let islands = d.islands.iter().filter(|i| !i.is_open()).count();
                lines.push(("RULES    RECIPE".into(), AMBER));
                lines.push((format!("ISLANDS  {islands} RESTRICTED"), MD));
                lines.push((format!("LICENCE  {} PTS", d.licence_cost), MD));
            }
            None => lines.push(("RULES    STANDARD".into(), MD)),
        }
        for (i, (l, c)) in lines.iter().enumerate() {
            self.text(
                out,
                panel.0 + 12.0,
                panel.1 + 14.0 + i as f32 * 22.0,
                PX,
                *c,
                l,
            );
        }
        let about = match items[t.main_row.min(items.len() - 1)] {
            MainItem::Hunt => "Plan a hunt: area, targets, arms and gear.",
            MainItem::HardcoreHunt => {
                "The same setup under the hardcore rules: a leaner loadout, a drop-off and a pickup zone."
            }
            MainItem::Survival => "Survive the waves of animals.",
            MainItem::Manual if t.specimen_db => "The specimen database: every animal's file, with your records.",
            MainItem::Manual => "How permits, budgets and earnings work.",
            MainItem::Trophy => "Visit your trophy room.",
            MainItem::Options => "Game, video and control settings.",
            MainItem::Hunters => "Sign in as another hunter, register or delete one.",
            MainItem::Classic => "The original menu, until the game restarts.",
            MainItem::Quit => "Leave the game.",
        };
        for (i, l) in wrap(&self.hud, SMALL, panel.2, &[about.to_string()])
            .iter()
            .enumerate()
        {
            self.text(out, panel.0, 364.0 + i as f32 * 16.0, SMALL, LO, l);
        }
    }

    /// The survival modes: one row each, as the main menu's.
    fn draw_term_survival(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let g = self.geom();
        self.text(out, g.main_x(), 76.0, PX, LO, "SURVIVAL");
        for (i, label) in t.survival.iter().enumerate() {
            let r = g.main_row(i);
            let text = format!("[{}] {label}", i + 1);
            if i == t.survival_row() {
                self.fill(out, r, MD);
                self.text(out, g.main_x(), r.1 + 8.0, PX, INV, &text);
            } else {
                self.text(out, g.main_x(), r.1 + 8.0, PX, MD, &text);
            }
        }
    }

    /// The terminal's deploy log at full progress, standing in for the
    /// loading picture while the hunt loads.
    pub(in crate::menu) fn draw_term_loading(&self, out: &mut Vec<Sprite>) {
        let Some(log) = &self.term_loading else {
            return;
        };
        self.term_frame(out);
        for (i, l) in log.iter().enumerate() {
            self.text(out, 60.0, 120.0 + i as f32 * 28.0, PX, MD, l);
        }
        let bar = (200.0, 400.0, 400.0, 14.0);
        self.term_box(out, bar, PANEL, LINE);
        self.fill(
            out,
            (bar.0 + 2.0, bar.1 + 2.0, bar.2 - 4.0, bar.3 - 4.0),
            HI,
        );
        self.term_status(out, "LOADING...", "");
    }

    fn draw_term_deploy(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let log = t.deploy_log();
        let p = t.deploy_progress();
        let shown = ((p * log.len() as f32).ceil() as usize).clamp(1, log.len());
        for (i, l) in log.iter().take(shown).enumerate() {
            self.text(out, 60.0, 120.0 + i as f32 * 28.0, PX, MD, l);
        }
        let bar = (200.0, 400.0, 400.0, 14.0);
        self.term_box(out, bar, PANEL, LINE);
        self.fill(
            out,
            (bar.0 + 2.0, bar.1 + 2.0, (bar.2 - 4.0) * p, bar.3 - 4.0),
            HI,
        );
    }

    fn draw_term_setup(&self, out: &mut Vec<Sprite>, v: &TermView) {
        let t = &v.term;
        let g = self.geom();
        let list_r = g.list();
        for (i, tab) in Tab::ALL.iter().enumerate() {
            let r = g.tab(i);
            let label = format!("{} {}", i + 1, tab.title());
            let on = *tab == t.tab;
            let hover = inside(self.cursor, r);
            self.term_box(
                out,
                r,
                if on { MD } else { PANEL },
                if hover { HI } else { LINE },
            );
            let x = r.0 + (r.2 - self.text_width(PX, &label)) / 2.0;
            self.text(out, x, r.1 + 6.0, PX, if on { INV } else { MD }, &label);
        }
        if t.hardcore {
            self.text(out, g.l + 544.0, TAB_Y + 6.0, PX, AMBER, "HARDCORE HUNT");
        }
        // The list: [x] name ... price, or why it can't be picked.
        self.term_box(out, list_r, PANEL, LINE);
        let list = t.cat.list(t.tab);
        let focus = t.rows[t.tab.index()];
        for (i, it) in list.iter().enumerate() {
            let Some(r) = list_rect(list_r, v.first[t.tab.index()], i) else {
                continue;
            };
            let picked = t.picked(t.tab, i);
            let allowed = t.allowed(t.tab, i);
            let afford = t.affordable(t.tab, i);
            let mark = match (t.tab, picked) {
                (Tab::Area, true) => "(*)",
                (Tab::Area, false) => "( )",
                (_, true) => "[x]",
                (_, false) => "[ ]",
            };
            let (text_c, right) = if !allowed {
                (FAINT, "RESTRICTED".to_string())
            } else if !afford {
                (FAINT, it.price.to_string())
            } else if picked {
                (HI, it.price.to_string())
            } else {
                (MD, it.price.to_string())
            };
            let (text_c, right_c) = if i == focus {
                self.fill(out, r, LO);
                (INV, INV)
            } else {
                (text_c, if allowed && !afford { RED } else { text_c })
            };
            let rw = self.text_width(SMALL, &right);
            self.term_text(
                out,
                r.0 + 6.0,
                r.1 + 4.0,
                SMALL,
                text_c,
                &format!("{mark} {}", it.name),
                r.2 - rw - 24.0,
            );
            self.text(out, r.0 + r.2 - rw - 6.0, r.1 + 4.0, SMALL, right_c, &right);
        }
        if list.is_empty() {
            self.text(
                out,
                list_r.0 + 12.0,
                list_r.1 + 12.0,
                SMALL,
                LO,
                "NOTHING ON FILE",
            );
        }
        self.draw_term_detail(out, v, focus);
        self.draw_term_budget(out, t);
    }

    /// The focused item's picture and description, from the menu's files.
    fn draw_term_detail(&self, out: &mut Vec<Sprite>, v: &TermView, focus: usize) {
        let t = &v.term;
        let detail = self.geom().detail();
        self.term_box(out, detail, PANEL, LINE);
        let ti = t.tab.index();
        let Some(&ci) = v.map[ti].get(focus) else {
            return;
        };
        let col = &self.columns[ti];
        let (x, w) = (detail.0 + 10.0, detail.2 - 20.0);
        let mut y = detail.1 + 10.0;
        let pic = col.pics.get(ci).copied().flatten();
        y += self.term_picture(out, pic, (x, y, w, 190.0)) + 10.0;
        let it = &t.cat.list(t.tab)[focus];
        let status = if !t.allowed(t.tab, focus) {
            ("RESTRICTED ON THIS ISLAND", AMBER)
        } else if t.picked(t.tab, focus) {
            ("ISSUED", HI)
        } else if !t.affordable(t.tab, focus) {
            ("INSUFFICIENT FUNDS", RED)
        } else {
            ("AVAILABLE", MD)
        };
        self.term_text(out, x, y, PX, HI, &it.name.to_uppercase(), w);
        let price = format!("{} PTS", it.price);
        let pw = self.text_width(SMALL, &price);
        self.text(out, x + w - pw, y + 2.0, SMALL, MD, &price);
        self.text(out, x, y + 20.0, SMALL, status.1, status.0);
        y += 42.0;
        let us = self.me().is_some_and(|s| s.options.sys != 0);
        let texts = if us { &col.texts_us } else { &col.texts };
        let lines = texts.get(ci).map_or(&[][..], |v| &v[..]);
        for l in wrap(&self.hud, SMALL, w, lines) {
            if y > detail.1 + detail.3 - 18.0 {
                break;
            }
            self.text(out, x, y, SMALL, LO, &l);
            y += 15.0;
        }
    }

    fn draw_term_budget(&self, out: &mut Vec<Sprite>, t: &Terminal) {
        let g = self.geom();
        let (budget, meter) = (g.budget(), g.meter());
        let (time_r, tranq_r, observer_r, deploy_r) =
            (g.time(), g.tranq(), g.observer(), g.deploy());
        self.term_box(out, budget, PANEL, LINE);
        let (score, spent, left) = (t.score, t.spent(), t.remaining());
        let line = format!("ACCOUNT {score}   PERMITS -{spent}   BALANCE {left}");
        self.text(
            out,
            g.l + 12.0,
            486.0,
            PX,
            if left < 0 { RED } else { MD },
            &line,
        );
        // The budget meter: the share of the account spent.
        self.term_box(out, meter, [0.0, 0.0, 0.0, 0.4], LINE);
        let share = if score > 0 {
            (spent as f32 / score as f32).clamp(0.0, 1.0)
        } else if spent > 0 {
            1.0
        } else {
            0.0
        };
        let c = if left < 0 {
            RED
        } else if share > 0.85 {
            AMBER
        } else {
            MD
        };
        self.fill(
            out,
            (
                meter.0 + 2.0,
                meter.1 + 2.0,
                (meter.2 - 4.0) * share,
                meter.3 - 4.0,
            ),
            c,
        );
        let time = format!("TIME: {}  [T]", TIMES[t.time.min(2)]);
        let tc = if inside(self.cursor, time_r) { HI } else { MD };
        self.text(out, time_r.0 + 4.0, time_r.1 + 3.0, SMALL, tc, &time);
        let k = t.earnings();
        let earn = if (k - 1.0).abs() < 1e-4 {
            "EARNINGS x1.00".to_string()
        } else {
            format!("EARNINGS x{k:.2}")
        };
        self.text(
            out,
            g.earnings_x(),
            527.0,
            SMALL,
            if k < 1.0 { AMBER } else { MD },
            &earn,
        );
        for (r, label, on) in [
            (tranq_r, "TRANQ", t.tranq),
            (observer_r, "OBSERVER", t.observer),
        ] {
            let key = &label[..1];
            let text = format!(
                "{label}: {}  [{}]",
                if on { "ON" } else { "OFF" },
                if key == "T" { "Q" } else { "O" }
            );
            let c = if inside(self.cursor, r) {
                HI
            } else if on {
                AMBER
            } else {
                MD
            };
            self.text(out, r.0 + 4.0, r.1 + 3.0, SMALL, c, &text);
        }
        if let Some(n) = t.notice {
            self.text(
                out,
                g.l + 12.0,
                546.0,
                SMALL,
                AMBER,
                &format!("! {}", n.text()),
            );
        }
        let ready = t.deploy_problem().is_none();
        let hover = inside(self.cursor, deploy_r);
        let (fill, line, tc) = match (ready, hover) {
            (true, true) => (HI, HI, INV),
            (true, false) => (MD, HI, INV),
            (false, _) => (PANEL, LINE, FAINT),
        };
        self.term_box(out, deploy_r, fill, line);
        let label = "[F] DEPLOY";
        let x = deploy_r.0 + (deploy_r.2 - self.text_width(PX, label)) / 2.0;
        self.text(out, x, deploy_r.1 + 20.0, PX, tc, label);
    }
}
