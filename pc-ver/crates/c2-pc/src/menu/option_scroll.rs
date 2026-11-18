//! Modifier viewport: drawing and hit testing use the same visible rows.
use super::*;

#[derive(Default)]
pub(super) struct Scroll {
    pub first: usize,
    remainder: f32,
    /// A finger (or the held button) dragging the list: where it was, in
    /// canvas units.
    grab: Option<f32>,
}
impl Scroll {
    fn wheel(&mut self, delta: f32, max: usize) {
        if !delta.is_finite() {
            return;
        }
        self.remainder += delta;
        let steps = self.remainder.trunc() as isize;
        self.remainder -= steps as f32;
        self.first = self
            .first
            .saturating_add_signed(steps.saturating_neg())
            .min(max);
        if (self.first == 0 && delta > 0.0) || (self.first == max && delta < 0.0) {
            self.remainder = 0.0;
        }
    }
}
/// `opts` without the rows whose setting the instance fixes
/// (`crate::instance_rules`), and without the Advanced DHC OS UI's while
/// the terminal is off (`pc`: the settings as they stand).
pub(super) fn shown(opts: &[Opt], pc: &PcSettings) -> Vec<Opt> {
    shown_in(opts, pc, crate::classic::on())
}

/// [`shown`] for a Play Vanilla session (`classic`) or not.
pub(super) fn shown_in(opts: &[Opt], pc: &PcSettings, classic: bool) -> Vec<Opt> {
    opts.iter()
        .copied()
        .filter(|o| match o {
            Opt::PcToggle(n, _) | Opt::PcSlider(n, _) => {
                !crate::instance_rules::hides(n)
                    && !(classic && crate::classic::hides(n))
                    && !(classic && is_dhc_row(n))
                    && !(*n == ADVANCED_DHC_UI && pc.dhc_terminal == 0)
            }
            _ => true,
        })
        .collect()
}

/// The board's scrolling panels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
    Game,
    Video,
}

impl Panel {
    fn rect(self) -> Rect {
        match self {
            Panel::Game => GAME_PANEL,
            Panel::Video => VIDEO_PANEL,
        }
    }

    fn opts(self, pc: &PcSettings) -> Vec<Opt> {
        match self {
            Panel::Game => shown(&GAME_OPTS, pc),
            Panel::Video => shown(&VIDEO_OPTS, pc),
        }
    }
}

/// A row's height on the board: the two labelled sliders take two lines.
fn row_height(o: &Opt) -> f32 {
    if matches!(
        o,
        Opt::PcSlider(
            "Total number of secret dinos on map" | "Survival starting budget",
            _
        )
    ) {
        38.0
    } else {
        OPT_ROW
    }
}

/// The rows to scroll past before the rest of `opts` fits `height`.
fn max_first(opts: &[Opt], height: f32) -> usize {
    let mut used = 0.0;
    let mut count = 0;
    for o in opts.iter().rev() {
        used += row_height(o);
        if used > height {
            break;
        }
        count += 1;
    }
    opts.len().saturating_sub(count)
}

pub(super) fn visible_rows(opts: &[Opt], panel: Rect, first: usize) -> Vec<(Opt, Rect)> {
    let mut rows = Vec::new();
    let mut y = panel.1;
    for o in opts.iter().skip(first) {
        let height = row_height(o);
        if y + height > panel.1 + panel.3 {
            break;
        }
        rows.push((*o, (panel.0, y, panel.2, height)));
        y += height;
    }
    rows
}

impl Menu {
    /// Where `which` scrolls from (`first`).
    pub(super) fn scroll_of(&self, which: Panel) -> &Scroll {
        match which {
            Panel::Game => &self.game_scroll,
            Panel::Video => &self.video_scroll,
        }
    }

    /// The panel, its up and down arrows and the farthest it scrolls. The
    /// phone-style screen scrolls its GAME tab only.
    fn scroll_layout(&self, mobile: bool, which: Panel) -> (Rect, Rect, Rect, usize) {
        let panel = if mobile {
            (40.0, 60.0, 720.0, 378.0)
        } else {
            which.rect()
        };
        let opts = which.opts(&self.pc);
        let max = if mobile {
            opts.len().saturating_sub(9)
        } else {
            max_first(&opts, panel.3)
        };
        let x = panel.0 + panel.2 + 2.0;
        (
            panel,
            (x, panel.1, 22.0, 24.0),
            (x, panel.1 + panel.3 - 24.0, 22.0, 24.0),
            max,
        )
    }

    /// Scrolls the option panels: the board's two, or the phone-style
    /// screen's GAME tab.
    pub(super) fn scroll_options(&mut self, input: &Input, mobile: bool) {
        if mobile {
            if self.opt_tab == 0 {
                self.scroll_panel(input, true, Panel::Game);
            }
            return;
        }
        self.scroll_panel(input, false, Panel::Game);
        self.scroll_panel(input, false, Panel::Video);
    }

    fn scroll_panel(&mut self, input: &Input, mobile: bool, which: Panel) {
        let (panel, up, down, max) = self.scroll_layout(mobile, which);
        let mut scroll = std::mem::take(match which {
            Panel::Game => &mut self.game_scroll,
            Panel::Video => &mut self.video_scroll,
        });
        let old = scroll.first;
        if inside(self.cursor, panel) {
            scroll.wheel(input.scroll, max);
        }
        // Touch: dragging the list a row's height moves it a row (not
        // while a slider is held).
        let y = self.cursor.map(|c| c.1);
        scroll.grab = match (scroll.grab, y) {
            (_, Some(y)) if mobile && input.mouse_down && self.drag.is_none() => {
                match scroll.grab {
                    Some(from) => {
                        let rows = ((y - from) / OPT_ROW).trunc();
                        scroll.wheel(rows, max);
                        Some(from + rows * OPT_ROW)
                    }
                    None => (input.clicked && inside(self.cursor, panel)).then_some(y),
                }
            }
            _ => None,
        };
        if input.clicked {
            if inside(self.cursor, up) {
                scroll.first = scroll.first.saturating_sub(1);
            }
            if inside(self.cursor, down) {
                scroll.first = (scroll.first + 1).min(max);
            }
        }
        scroll.first = scroll.first.min(max);
        if old != scroll.first {
            self.drag = None;
            self.clear_armed = false;
        }
        *match which {
            Panel::Game => &mut self.game_scroll,
            Panel::Video => &mut self.video_scroll,
        } = scroll;
    }
    pub(super) fn draw_option_scroll(&self, out: &mut Vec<Sprite>, mobile: bool) {
        if mobile && self.opt_tab != 0 {
            return;
        }
        let panels: &[Panel] = if mobile {
            &[Panel::Game]
        } else {
            &[Panel::Game, Panel::Video]
        };
        for &which in panels {
            let (_, up, down, max) = self.scroll_layout(mobile, which);
            if max == 0 {
                continue;
            }
            let first = self.scroll_of(which).first;
            for (r, label, enabled) in [(up, "^", first > 0), (down, "v", first < max)] {
                self.text(out, r.0, r.1, 16.0, if enabled { SAND } else { DIM }, label);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modifier_rows_stay_inside_panel_and_bottom_options_are_reachable() {
        let top = visible_rows(&GAME_OPTS, GAME_PANEL, 0);
        assert!(!top.iter().any(|(o, _)| matches!(o, Opt::ClearTrophies)));
        let last_start = (0..GAME_OPTS.len())
            .find(|&first| {
                visible_rows(&GAME_OPTS, GAME_PANEL, first).len() == GAME_OPTS.len() - first
            })
            .unwrap();
        let bottom = visible_rows(&GAME_OPTS, GAME_PANEL, last_start);
        // Every row shows on some scroll position.
        for o in &GAME_OPTS {
            let seen = (0..=last_start).any(|f| {
                visible_rows(&GAME_OPTS, GAME_PANEL, f)
                    .iter()
                    .any(|(r, _)| {
                        std::mem::discriminant(r) == std::mem::discriminant(o)
                            && opt_name(r) == opt_name(o)
                    })
            });
            assert!(seen, "{:?} never shows", opt_name(o));
        }
        assert!(matches!(bottom.last().unwrap().0, Opt::ClearTrophies));
        assert!(bottom
            .iter()
            .any(|(o, _)| matches!(o, Opt::PcToggle("Pause weapon / call pickers", _))));
        for first in 0..=last_start {
            let rows = visible_rows(&GAME_OPTS, GAME_PANEL, first);
            for (_, r) in &rows {
                assert!(r.1 >= GAME_PANEL.1 && r.1 + r.3 <= GAME_PANEL.1 + GAME_PANEL.3);
            }
            for pair in rows.windows(2) {
                assert!(pair[0].1 .1 + pair[0].1 .3 <= pair[1].1 .1);
            }
        }
    }
    // The Video panel holds nine rows; its options past the ninth were
    // out of reach on the board.
    #[test]
    fn every_video_option_is_reachable_on_the_board() {
        let max = max_first(&VIDEO_OPTS, VIDEO_PANEL.3);
        assert!(max > 0);
        let bottom = visible_rows(&VIDEO_OPTS, VIDEO_PANEL, max);
        assert_eq!(bottom.len(), VIDEO_OPTS.len() - max);
        assert!(bottom
            .iter()
            .any(|(o, _)| matches!(o, Opt::PcToggle("Animated waves", _))));
        for first in 0..=max {
            for (_, r) in visible_rows(&VIDEO_OPTS, VIDEO_PANEL, first) {
                assert!(r.1 >= VIDEO_PANEL.1 && r.1 + r.3 <= VIDEO_PANEL.1 + VIDEO_PANEL.3);
            }
        }
    }

    // A heading is never the last row shown with its options scrolled off.
    #[test]
    fn modern_graphics_heading_sits_above_its_options() {
        let at = VIDEO_OPTS
            .iter()
            .position(|o| matches!(o, Opt::Header("Modern graphics")));
        let waves = VIDEO_OPTS
            .iter()
            .position(|o| matches!(o, Opt::PcToggle("Animated waves", _)));
        assert!(at.is_some() && at.map(|i| i + 1) == waves);
    }

    #[test]
    fn trackpad_accumulates_and_bounds_do_not_store_overscroll() {
        let mut s = Scroll::default();
        s.wheel(-0.4, 3);
        s.wheel(-0.4, 3);
        assert_eq!(s.first, 0);
        s.wheel(-0.4, 3);
        assert_eq!(s.first, 1);
        s.wheel(-50.0, 3);
        assert_eq!(s.first, 3);
        s.wheel(1.0, 3);
        assert_eq!(s.first, 2);
        s.wheel(50.0, 3);
        assert_eq!(s.first, 0);
        s.wheel(-1.0, 3);
        assert_eq!(s.first, 1);
    }

    fn opt_name(o: &Opt) -> &'static str {
        match o {
            Opt::Slider(n, _)
            | Opt::Toggle(n, _, _)
            | Opt::PcSlider(n, _)
            | Opt::PcToggle(n, _)
            | Opt::Header(n) => n,
            Opt::ClearTrophies => "clear trophies",
        }
    }
}
