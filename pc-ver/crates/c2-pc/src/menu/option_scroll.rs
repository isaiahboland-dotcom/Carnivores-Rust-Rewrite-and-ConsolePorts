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
/// (`crate::instance_rules`).
pub(super) fn shown(opts: &[Opt]) -> Vec<Opt> {
    opts.iter()
        .copied()
        .filter(|o| match o {
            Opt::PcToggle(n, _) | Opt::PcSlider(n, _) => !crate::instance_rules::hides(n),
            _ => true,
        })
        .collect()
}

pub(super) fn visible_rows(opts: &[Opt], panel: Rect, first: usize) -> Vec<(Opt, Rect)> {
    let mut rows = Vec::new();
    let mut y = panel.1;
    for o in opts.iter().skip(first) {
        let height = if matches!(
            o,
            Opt::PcSlider(
                "Total number of secret dinos on map" | "Survival starting budget",
                _
            )
        ) {
            38.0
        } else {
            OPT_ROW
        };
        if y + height > panel.1 + panel.3 {
            break;
        }
        rows.push((*o, (panel.0, y, panel.2, height)));
        y += height;
    }
    rows
}

impl Menu {
    fn game_scroll_layout(&self, mobile: bool) -> (Rect, Rect, Rect, usize) {
        let panel = if mobile {
            (40.0, 60.0, 720.0, 378.0)
        } else {
            GAME_PANEL
        };
        let opts = shown(&GAME_OPTS);
        let max = if mobile {
            opts.len().saturating_sub(9)
        } else {
            let mut height = 0.0;
            let mut count = 0;
            for o in opts.iter().rev() {
                height += if matches!(
                    o,
                    Opt::PcSlider(
                        "Total number of secret dinos on map" | "Survival starting budget",
                        _
                    )
                ) {
                    38.0
                } else {
                    OPT_ROW
                };
                if height > panel.3 {
                    break;
                }
                count += 1;
            }
            opts.len().saturating_sub(count)
        };
        let x = panel.0 + panel.2 + 2.0;
        (
            panel,
            (x, panel.1, 22.0, 24.0),
            (x, panel.1 + panel.3 - 24.0, 22.0, 24.0),
            max,
        )
    }
    pub(super) fn scroll_game_options(&mut self, input: &Input, mobile: bool) {
        if mobile && self.opt_tab != 0 {
            return;
        }
        let (panel, up, down, max) = self.game_scroll_layout(mobile);
        let old = self.game_scroll.first;
        if inside(self.cursor, panel) {
            self.game_scroll.wheel(input.scroll, max);
        }
        // Touch: dragging the list a row's height moves it a row (not
        // while a slider is held).
        let y = self.cursor.map(|c| c.1);
        self.game_scroll.grab = match (self.game_scroll.grab, y) {
            (_, Some(y)) if mobile && input.mouse_down && self.drag.is_none() => {
                match self.game_scroll.grab {
                    Some(from) => {
                        let rows = ((y - from) / OPT_ROW).trunc();
                        self.game_scroll.wheel(rows, max);
                        Some(from + rows * OPT_ROW)
                    }
                    None => (input.clicked && inside(self.cursor, panel)).then_some(y),
                }
            }
            _ => None,
        };
        if input.clicked {
            if inside(self.cursor, up) {
                self.game_scroll.first = self.game_scroll.first.saturating_sub(1);
            }
            if inside(self.cursor, down) {
                self.game_scroll.first = (self.game_scroll.first + 1).min(max);
            }
        }
        self.game_scroll.first = self.game_scroll.first.min(max);
        if old != self.game_scroll.first {
            self.drag = None;
            self.clear_armed = false;
        }
    }
    pub(super) fn draw_game_scroll(&self, out: &mut Vec<Sprite>, mobile: bool) {
        if mobile && self.opt_tab != 0 {
            return;
        }
        let (_, up, down, max) = self.game_scroll_layout(mobile);
        if max == 0 {
            return;
        }
        for (r, label, enabled) in [
            (up, "^", self.game_scroll.first > 0),
            (down, "v", self.game_scroll.first < max),
        ] {
            self.text(out, r.0, r.1, 16.0, if enabled { SAND } else { DIM }, label);
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
        assert!(bottom
            .iter()
            .any(|(o, _)| matches!(o, Opt::PcToggle("Dynamic time", _))));
        assert!(matches!(bottom.last().unwrap().0, Opt::ClearTrophies));
        assert!(bottom
            .iter()
            .any(|(o, _)| matches!(o, Opt::PcToggle("Advanced observer mode", _))));
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
}
