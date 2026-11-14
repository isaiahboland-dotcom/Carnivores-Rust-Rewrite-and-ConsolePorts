//! The options screen in the terminal's look: the same rows, sliders and
//! key bindings as the original screen (`options.rs` handles them), split
//! over three tabs (GAME, VIDEO, CONTROLS) instead of three panels over
//! the original's board. GAME and VIDEO are one full-width column with the
//! values on the right; CONTROLS has the keys left and the mouse right.

use super::draw::{tab_rect, BACK_RECT, PX};
use super::palette::*;
use super::*;

/// The tabs' titles, in order (`Menu::term_opt_tab`).
const TABS: [&str; 3] = ["GAME", "VIDEO", "CONTROLS"];
/// The tab keys, `1`..`3`.
const TAB_KEYS: [KeyCode; 3] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3];

/// The panel under the tabs; GAME and VIDEO's column; CONTROLS' two.
const PANEL_RECT: Rect = (16.0, 76.0, 768.0, 484.0);
const WIDE: Rect = (36.0, 88.0, 728.0, 464.0);
const COLS: [Rect; 2] = [(36.0, 88.0, 340.0, 464.0), (424.0, 88.0, 340.0, 464.0)];
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

/// Tab `tab`'s option rows and key-binding rows (index into `ACTIONS`),
/// with their rectangles.
pub(in crate::menu) fn term_layout(tab: usize) -> (Vec<(Opt, Rect)>, KeyRows) {
    match tab {
        0 => (stack(&option_scroll::shown(&GAME_OPTS), WIDE), Vec::new()),
        1 => (stack(&option_scroll::shown(&VIDEO_OPTS), WIDE), Vec::new()),
        _ => {
            let c = COLS[0];
            let keys = (0..ACTIONS.len())
                .map(|i| (i, (c.0, c.1 + i as f32 * ROW, c.2, ROW)))
                .collect();
            (stack(&MOUSE_OPTS, COLS[1]), keys)
        }
    }
}

impl Menu {
    /// Switches tabs by a click on one, `1`..`3`, or Tab (Shift+Tab
    /// back). True when the input was taken.
    pub(in crate::menu) fn term_options_tabs(&mut self, input: &Input) -> bool {
        let n = TABS.len();
        let clicked = (0..n).find(|&i| input.clicked && inside(self.cursor, tab_rect(i)));
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
        let title = "SYSTEM OPTIONS";
        let w = self.text_width(PX, title);
        self.text(out, 784.0 - w, 50.0, PX, HI, title);
        for (i, name) in TABS.iter().enumerate() {
            let r = tab_rect(i);
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
        self.term_box(out, PANEL_RECT, PANEL, LINE);
        self.term_status(
            out,
            "[ESC] BACK   1-3 / TAB: SWITCH TAB   CLICK A ROW TO CHANGE IT",
            "",
        );
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
        self.terminal_style() && input.clicked && inside(self.cursor, BACK_RECT)
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
            | Opt::PcToggle(n, _) => n,
            Opt::ClearTrophies => "clear trophies",
        }
    }

    /// Every option and key shows on exactly one tab, inside the panel,
    /// with no rows overlapping and none past the status bar.
    #[test]
    fn tabs_show_every_row_once_inside_the_panel() {
        let mut names = Vec::new();
        let mut keys = Vec::new();
        for tab in 0..TABS.len() {
            let (rows, key_rows) = term_layout(tab);
            let mut rects: Vec<Rect> = rows.iter().map(|(_, r)| *r).collect();
            rects.extend(key_rows.iter().map(|(_, r)| *r));
            for r in &rects {
                assert!(r.0 >= PANEL_RECT.0 && r.0 + r.2 <= PANEL_RECT.0 + PANEL_RECT.2);
                assert!(r.1 >= PANEL_RECT.1 && r.1 + r.3 <= PANEL_RECT.1 + PANEL_RECT.3);
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
        let want: Vec<&str> = option_scroll::shown(&GAME_OPTS)
            .iter()
            .chain(&option_scroll::shown(&VIDEO_OPTS))
            .chain(&MOUSE_OPTS)
            .map(name)
            .collect();
        assert_eq!(names, want);
        assert_eq!(keys, (0..ACTIONS.len()).collect::<Vec<_>>());
    }
}
