//! The options screen in the terminal's look: the same rows, sliders and
//! key bindings as the original screen (`options.rs` handles them), split
//! over four tabs (GAME, VIDEO, CONTROLS, DHC OS) instead of three panels
//! over the original's board. GAME, VIDEO and DHC OS are one full-width
//! column with the values on the right; CONTROLS has the keys left and the
//! mouse right. The DHC OS rows have a tab of their own so GAME fits above
//! the status bar.

use super::draw::PX;
use super::geom::Geom;
use super::palette::*;
use super::*;

/// The tabs' titles, in order (`Menu::term_opt_tab`).
const TABS: [&str; 4] = ["GAME", "VIDEO", "CONTROLS", "DHC OS"];
/// The tab keys, `1`..`4`.
const TAB_KEYS: [KeyCode; 4] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
];

/// A row's height (the original's key rows' 20, so GAME fits one column).
const ROW: f32 = 20.0;

/// A row's height: the two labelled sliders take two lines.
fn row_height(o: &Opt) -> f32 {
    match o {
        Opt::PcSlider("Total number of secret dinos on map" | "Survival starting budget", _) => {
            38.0
        }
        _ => ROW,
    }
}

/// Stacks `opts` down column `col` from its top.
fn stack(opts: &[Opt], col: Rect) -> Vec<(Opt, Rect)> {
    let mut y = col.1;
    opts.iter()
        .map(|o| {
            let h = row_height(o);
            let r = (col.0, y, col.2, h);
            y += h;
            (*o, r)
        })
        .collect()
}

/// Key-binding rows: an index into `ACTIONS` and its rectangle.
type KeyRows = Vec<(usize, Rect)>;

/// The GAME rows (`dhc`: false) or the DHC OS tab's (true), in a Play
/// Vanilla session (`classic`) or not.
fn game_rows(pc: &PcSettings, dhc: bool, classic: bool) -> Vec<Opt> {
    option_scroll::shown_in(&GAME_OPTS, pc, classic)
        .into_iter()
        .filter(|o| match o {
            Opt::PcToggle(n, _) | Opt::PcSlider(n, _) => is_dhc_row(n) == dhc,
            _ => !dhc,
        })
        .collect()
}

/// Tab `tab`'s option rows and key-binding rows (index into `ACTIONS`),
/// with their rectangles (`pc`: the settings, which decide the rows shown).
pub(in crate::menu) fn term_layout(
    g: Geom,
    tab: usize,
    pc: &PcSettings,
) -> (Vec<(Opt, Rect)>, KeyRows) {
    term_layout_in(g, tab, pc, crate::classic::on())
}

/// [`term_layout`] for a Play Vanilla session (`classic`) or not.
fn term_layout_in(
    g: Geom,
    tab: usize,
    pc: &PcSettings,
    classic: bool,
) -> (Vec<(Opt, Rect)>, KeyRows) {
    match tab {
        0 => (
            stack(&game_rows(pc, false, classic), g.opt_wide()),
            Vec::new(),
        ),
        1 => (
            stack(
                &option_scroll::shown_in(&VIDEO_OPTS, pc, classic),
                g.opt_wide(),
            ),
            Vec::new(),
        ),
        3 => (
            stack(&game_rows(pc, true, classic), g.opt_wide()),
            Vec::new(),
        ),
        _ => {
            let cols = g.opt_cols();
            let c = cols[0];
            let keys = (0..ACTIONS.len())
                .map(|i| (i, (c.0, c.1 + i as f32 * ROW, c.2, ROW)))
                .collect();
            (stack(&MOUSE_OPTS, cols[1]), keys)
        }
    }
}

impl Menu {
    /// Switches tabs by a click on one, `1`..`3`, or Tab (Shift+Tab
    /// back). True when the input was taken.
    pub(in crate::menu) fn term_options_tabs(&mut self, input: &Input) -> bool {
        let n = TABS.len();
        let g = self.geom();
        let clicked = (0..n).find(|&i| input.clicked && inside(self.cursor, g.tab(i)));
        let key = TAB_KEYS.iter().position(|&k| input.pressed(k));
        let cycle = input.pressed(KeyCode::Tab).then(|| {
            let back = input.held(KeyCode::ShiftLeft) || input.held(KeyCode::ShiftRight);
            (self.term_opt_tab + if back { n - 1 } else { 1 }) % n
        });
        let Some(t) = clicked.or(key).or(cycle) else {
            return false;
        };
        self.term_opt_tab = t;
        self.clear_armed = false;
        self.drag = None;
        true
    }

    /// The frame, title, tabs and panel the option rows are drawn into.
    pub(in crate::menu) fn draw_term_options_frame(&self, out: &mut Vec<Sprite>) {
        self.term_frame(out);
        let g = self.geom();
        let title = "SYSTEM OPTIONS";
        let w = self.text_width(PX, title);
        self.text(out, g.right() - w, 50.0, PX, HI, title);
        for (i, name) in TABS.iter().enumerate() {
            let r = g.tab(i);
            let label = format!("{} {name}", i + 1);
            let on = i == self.term_opt_tab;
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
        self.term_box(out, g.opt_panel(), PANEL, LINE);
        let hints = if self.pad_used {
            "[B] BACK   [L]/[R] SWITCH TAB   D-PAD MOVE   [A] CHANGE"
        } else {
            "[ESC] BACK   1-4 / TAB: SWITCH TAB   CLICK A ROW TO CHANGE IT"
        };
        self.term_status(out, hints, "");
    }

    /// A slider at `x0`, `y` (128 wide, as the original's picture) set to
    /// `v` (0..=255).
    pub(in crate::menu) fn term_slider(&self, out: &mut Vec<Sprite>, x0: f32, y: f32, v: i32) {
        self.term_box(out, (x0, y + 1.0, 128.0, 10.0), [0.0, 0.0, 0.0, 0.4], LINE);
        let w = 124.0 * v.clamp(0, 255) as f32 / 255.0;
        self.fill(out, (x0 + 2.0, y + 3.0, w, 6.0), MD);
    }

    /// The status bar's Back was clicked.
    pub(in crate::menu) fn term_back_clicked(&self, input: &Input) -> bool {
        self.terminal_style() && input.clicked && inside(self.cursor, self.geom().back())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(o: &Opt) -> &'static str {
        match o {
            Opt::Slider(n, _)
            | Opt::Toggle(n, _, _)
            | Opt::PcSlider(n, _)
            | Opt::PcToggle(n, _)
            | Opt::Header(n) => n,
            Opt::ClearTrophies => "clear trophies",
        }
    }

    /// Every option and key shows on exactly one tab, inside the panel,
    /// with no rows overlapping and none past the status bar.
    #[test]
    fn tabs_show_every_row_once_inside_the_panel() {
        for win in [800.0, 1600.0, 2400.0] {
            check_layout(Geom::for_width(win));
        }
    }

    /// DHC OS on (the terminal's look), so every row shows.
    fn dhc_on() -> PcSettings {
        PcSettings {
            dhc_terminal: 1,
            ..PcSettings::default()
        }
    }

    fn check_layout(g: Geom) {
        let pc = dhc_on();
        let panel = g.opt_panel();
        let mut names = Vec::new();
        let mut keys = Vec::new();
        for tab in 0..TABS.len() {
            let (rows, key_rows) = term_layout_in(g, tab, &pc, false);
            let mut rects: Vec<Rect> = rows.iter().map(|(_, r)| *r).collect();
            rects.extend(key_rows.iter().map(|(_, r)| *r));
            for r in &rects {
                assert!(r.0 >= panel.0 && r.0 + r.2 <= panel.0 + panel.2);
                assert!(r.1 >= panel.1 && r.1 + r.3 <= panel.1 + panel.3);
                assert!(r.1 + r.3 <= 568.0, "a row under the status bar");
            }
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    let apart = a.0 + a.2 <= b.0
                        || b.0 + b.2 <= a.0
                        || a.1 + a.3 <= b.1
                        || b.1 + b.3 <= a.1;
                    assert!(apart, "rows overlap on tab {tab}: {a:?} {b:?}");
                }
            }
            names.extend(rows.iter().map(|(o, _)| name(o)));
            keys.extend(key_rows.iter().map(|(i, _)| *i));
        }
        let want: Vec<&str> = game_rows(&pc, false, false)
            .iter()
            .chain(&option_scroll::shown_in(&VIDEO_OPTS, &pc, false))
            .chain(&MOUSE_OPTS)
            .chain(&game_rows(&pc, true, false))
            .map(name)
            .collect();
        assert_eq!(names, want);
        let mut every: Vec<&str> = option_scroll::shown_in(&GAME_OPTS, &pc, false)
            .iter()
            .chain(&option_scroll::shown_in(&VIDEO_OPTS, &pc, false))
            .chain(&MOUSE_OPTS)
            .map(name)
            .collect();
        let mut got = names.clone();
        every.sort_unstable();
        got.sort_unstable();
        assert_eq!(got, every, "each row on exactly one tab");
        assert_eq!(keys, (0..ACTIONS.len()).collect::<Vec<_>>());
    }

    /// The Advanced DHC OS UI's row is on the DHC OS tab while the
    /// terminal is on, and nowhere while it is off.
    #[test]
    fn the_advanced_row_shows_only_with_dhc_os() {
        let g = Geom::for_width(800.0);
        let has = |pc: &PcSettings, tab: usize| {
            term_layout_in(g, tab, pc, false)
                .0
                .iter()
                .any(|(o, _)| name(o) == ADVANCED_DHC_UI)
        };
        let on = dhc_on();
        assert!(has(&on, 3) && !has(&on, 0));
        let off = PcSettings::default();
        assert!((0..TABS.len()).all(|t| !has(&off, t)));
        assert!(!option_scroll::shown_in(&GAME_OPTS, &off, false)
            .iter()
            .any(|o| name(o) == ADVANCED_DHC_UI));
        // Play Vanilla shows no DHC OS rows at all.
        assert!(term_layout_in(g, 3, &on, true).0.is_empty());
    }
}
