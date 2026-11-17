//! Where the terminal's panels go. The design is 768 canvas units of
//! content between 16-unit margins; on a window wider than the 4:3 canvas
//! the content stretches to the window's edges (the centre stays at 400),
//! so the terminal fills a phone or a widescreen instead of sitting in a
//! letterboxed 800x600 box. Heights stay as designed.

use super::*;

/// The content's left edge and width, in canvas units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::menu) struct Geom {
    pub l: f32,
    pub w: f32,
}

/// Margin between the window's edge and the content.
const MARGIN: f32 = 16.0;
/// The designed content width (the original 800-unit canvas's).
#[cfg(test)]
pub(in crate::menu) const DESIGN_W: f32 = 768.0;
const GAP: f32 = 12.0;
const TAB_W: f32 = 126.0;

impl Geom {
    /// The geometry for a window `win_w` canvas units wide (never
    /// narrower than the canvas: a narrow window letterboxes instead).
    pub(in crate::menu) fn for_width(win_w: f32) -> Self {
        let win_w = win_w.max(CW);
        Geom {
            l: (CW - win_w) / 2.0 + MARGIN,
            w: win_w - 2.0 * MARGIN,
        }
    }

    pub(in crate::menu) fn right(self) -> f32 {
        self.l + self.w
    }

    /// The left panel's width on two-panel screens.
    fn left_w(self) -> f32 {
        (self.w - GAP) / 2.0 - 6.0
    }

    /// The right panel's left edge and width on two-panel screens.
    fn right_panel(self) -> (f32, f32) {
        let lw = self.left_w();
        (self.l + lw + GAP, self.w - lw - GAP)
    }

    /// The top bar's rule, and the status bar's.
    pub(in crate::menu) fn rule(self, y: f32) -> Rect {
        (self.l - 4.0, y, self.w + 8.0, 1.0)
    }

    pub(in crate::menu) fn back(self) -> Rect {
        (self.l - 4.0, 572.0, 120.0, 22.0)
    }

    pub(in crate::menu) fn tab(self, i: usize) -> Rect {
        (self.l + i as f32 * (TAB_W + 6.0), 44.0, TAB_W, 24.0)
    }

    /// The main menu's left edge for its entries' text, and a row.
    pub(in crate::menu) fn main_x(self) -> f32 {
        self.l + 40.0
    }

    pub(in crate::menu) fn main_row(self, i: usize) -> Rect {
        (
            self.main_x() - 8.0,
            120.0 + i as f32 * 32.0 - 6.0,
            320.0,
            28.0,
        )
    }

    /// The main menu's file panel, at the right.
    pub(in crate::menu) fn file_panel(self) -> Rect {
        (self.right() - 364.0, 112.0, 364.0, 240.0)
    }

    pub(in crate::menu) fn list(self) -> Rect {
        (self.l, 76.0, self.left_w(), 392.0)
    }

    pub(in crate::menu) fn detail(self) -> Rect {
        let (x, w) = self.right_panel();
        (x, 76.0, w, 392.0)
    }

    pub(in crate::menu) fn budget(self) -> Rect {
        (self.l, 476.0, self.w, 88.0)
    }

    pub(in crate::menu) fn meter(self) -> Rect {
        (self.l + 12.0, 506.0, (self.w - 208.0).min(560.0), 12.0)
    }

    pub(in crate::menu) fn time(self) -> Rect {
        (self.l + 8.0, 524.0, 180.0, 18.0)
    }

    pub(in crate::menu) fn earnings_x(self) -> f32 {
        self.l + 204.0
    }

    pub(in crate::menu) fn tranq(self) -> Rect {
        (self.l + 334.0, 524.0, 110.0, 18.0)
    }

    pub(in crate::menu) fn observer(self) -> Rect {
        (self.l + 454.0, 524.0, 130.0, 18.0)
    }

    pub(in crate::menu) fn deploy(self) -> Rect {
        (self.right() - 172.0, 494.0, 160.0, 56.0)
    }

    /// The specimen index's list (left) and the focused animal (right).
    pub(in crate::menu) fn index_list(self) -> Rect {
        (self.l, 76.0, self.left_w(), 484.0)
    }

    pub(in crate::menu) fn index_detail(self) -> Rect {
        let (x, w) = self.right_panel();
        (x, 76.0, w, 484.0)
    }

    /// A file's picture or 3D specimen and its description (left), and
    /// its facts (right).
    pub(in crate::menu) fn file_pic(self) -> Rect {
        (self.l, 76.0, self.left_w(), 260.0)
    }

    pub(in crate::menu) fn file_text(self) -> Rect {
        (self.l, 344.0, self.left_w(), 216.0)
    }

    pub(in crate::menu) fn file_facts(self) -> Rect {
        let (x, w) = self.right_panel();
        (x, 76.0, w, 484.0)
    }

    /// Hunter records (left) and the ID panel (right).
    pub(in crate::menu) fn records(self) -> Rect {
        (self.l, 76.0, self.left_w(), 420.0)
    }

    pub(in crate::menu) fn id_panel(self) -> Rect {
        let (x, w) = self.right_panel();
        (x, 76.0, w, 420.0)
    }

    pub(in crate::menu) fn id_field(self) -> Rect {
        let p = self.id_panel();
        (p.0 + 12.0, 118.0, p.2 - 24.0, 30.0)
    }

    pub(in crate::menu) fn login(self) -> Rect {
        let p = self.id_panel();
        (p.0 + 12.0, 440.0, 176.0, 44.0)
    }

    pub(in crate::menu) fn delete(self) -> Rect {
        let p = self.id_panel();
        (p.0 + p.2 - 188.0, 440.0, 176.0, 44.0)
    }

    /// The options panel under the tabs, its one wide column, and the
    /// CONTROLS tab's two.
    pub(in crate::menu) fn opt_panel(self) -> Rect {
        (self.l, 76.0, self.w, 484.0)
    }

    pub(in crate::menu) fn opt_wide(self) -> Rect {
        (self.l + 20.0, 88.0, self.w - 40.0, 464.0)
    }

    pub(in crate::menu) fn opt_cols(self) -> [Rect; 2] {
        let w = (self.w - 40.0 - 48.0) / 2.0;
        [
            (self.l + 20.0, 88.0, w, 464.0),
            (self.l + 20.0 + w + 48.0, 88.0, w, 464.0),
        ]
    }
}

impl Menu {
    /// The terminal's geometry for the window as last drawn.
    pub(in crate::menu) fn geom(&self) -> Geom {
        let (s, _, _) = self.canvas();
        let win_w = if self.screen_size.0 == 0 {
            CW
        } else {
            self.screen_size.0 as f32 / s
        };
        Geom::for_width(win_w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 4:3 canvas gets the original layout exactly.
    #[test]
    fn design_width_is_the_original_layout() {
        let g = Geom::for_width(CW);
        assert_eq!((g.l, g.w), (16.0, DESIGN_W));
        assert_eq!(g.list(), (16.0, 76.0, 372.0, 392.0));
        assert_eq!(g.detail(), (400.0, 76.0, 384.0, 392.0));
        assert_eq!(g.deploy(), (612.0, 494.0, 160.0, 56.0));
        assert_eq!(g.file_panel(), (420.0, 112.0, 364.0, 240.0));
        assert_eq!(g.opt_cols()[1], (424.0, 88.0, 340.0, 464.0));
        assert_eq!(g.delete(), (596.0, 440.0, 176.0, 44.0));
        // The field manual's panels sit side by side above the status bar.
        let (pic, text, facts) = (g.file_pic(), g.file_text(), g.file_facts());
        assert!(pic.1 + pic.3 < text.1 && text.1 + text.3 <= 568.0);
        assert!(pic.0 + pic.2 < facts.0 && facts.1 + facts.3 <= 568.0);
        assert_eq!(g.index_detail(), facts);
    }

    /// A wide window is filled edge to edge, mirrored about the centre.
    #[test]
    fn wide_window_fills_the_width() {
        let g = Geom::for_width(1600.0);
        assert_eq!(g.l, -400.0 + 16.0);
        assert_eq!(g.l + g.w / 2.0, CW / 2.0);
        let (l, d) = (g.list(), g.detail());
        assert!(l.0 + l.2 < d.0 && d.0 + d.2 <= g.right());
        assert!(g.deploy().0 + g.deploy().2 <= g.right() + 4.0);
    }

    /// A window narrower than the canvas keeps the design layout.
    #[test]
    fn narrow_window_keeps_the_design() {
        assert_eq!(Geom::for_width(500.0), Geom::for_width(CW));
    }
}
