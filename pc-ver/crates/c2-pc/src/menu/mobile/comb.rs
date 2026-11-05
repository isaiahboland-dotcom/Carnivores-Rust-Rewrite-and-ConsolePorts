//! The phone's honeycomb on the hunt setup's pages, and the animals' page
//! turned in twenties when a modded roster has more: two arrow buttons
//! beside the account, so the pictures keep the phone's size.

use super::*;

/// Animals a page shows at the phone's own size: four rows of five.
pub(super) const COMB_PAGE: usize = 20;
/// The arrows, either side of the account between back and next.
const ARROWS: [Rect; 2] = [(198.0, 505.0, 60.0, 52.0), (542.0, 505.0, 60.0, 52.0)];

/// The honeycomb: corner-up cells `w` x `h`, `sx` apart in a row and `sy`
/// between rows, every other row half a cell in; the first cell's top
/// left, and each row's length (the longer rows last, as the phone's).
pub(super) struct Comb {
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) sx: f32,
    pub(super) sy: f32,
    pub(super) x0: f32,
    pub(super) y0: f32,
    pub(super) rows: Vec<usize>,
}

impl Comb {
    /// `n` cells: the phone's rows (three; four of animals), more and
    /// smaller when they wouldn't fit across.
    pub(super) fn new(n: usize, page: usize) -> Self {
        let (gx, gy, gw, gh) = KIT_GRID;
        let mut r: usize = if page == 1 { 4 } else { 3 };
        loop {
            let h = (420.0 / (1.0 + 0.843 * (r - 1) as f32)).min(163.0);
            let (w, sy) = (h * 0.9, h * 0.843);
            let sx = w * 1.06;
            let long = n.div_ceil(r).max(1);
            if (long - 1) as f32 * sx + w + sx / 2.0 <= gw || r >= 12 {
                let (base, extra) = (n / r, n % r);
                let rows = (0..r).map(|i| base + usize::from(i >= r - extra)).collect();
                let tall = h + sy * (r - 1) as f32;
                return Self {
                    w,
                    h,
                    sx,
                    sy,
                    x0: gx,
                    y0: gy + ((gh - tall) / 2.0).max(0.0),
                    rows,
                };
            }
            r += 1;
        }
    }

    /// Cell `row`, `col`'s top left (beyond the rows too: the empty
    /// cells around).
    pub(super) fn at(&self, row: i32, col: i32) -> (f32, f32) {
        let odd = if row.rem_euclid(2) == 1 {
            self.sx / 2.0
        } else {
            0.0
        };
        (
            self.x0 + col as f32 * self.sx + odd,
            self.y0 + row as f32 * self.sy,
        )
    }

    pub(super) fn cells(&self) -> Vec<(f32, f32)> {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(r, &n)| (0..n).map(move |c| (r, c)))
            .map(|(r, c)| self.at(r as i32, c as i32))
            .collect()
    }
}

/// How many pages `n` animals fill.
fn pages(n: usize) -> usize {
    n.div_ceil(COMB_PAGE).max(1)
}

/// The part of `order` page `at` shows.
fn page_of(order: &[usize], at: usize) -> &[usize] {
    let start = at.min(pages(order.len()) - 1) * COMB_PAGE;
    &order[start..(start + COMB_PAGE).min(order.len())]
}

impl Menu {
    /// Whether the animals' page is turned in twenties.
    fn comb_paged(&self) -> bool {
        self.kit.is_some() && self.mobile_page == 1 && self.columns[1].order.len() > COMB_PAGE
    }

    /// The items the page's honeycomb shows, in order.
    pub(super) fn comb_items(&self) -> &[usize] {
        let order = &self.columns[self.mobile_col()].order;
        if self.comb_paged() {
            page_of(order, self.comb_page)
        } else {
            order
        }
    }

    /// The arrows, when the page turns: (back a page, on a page) and
    /// whether each can be pressed.
    fn comb_arrows(&self) -> Option<[(Rect, bool); 2]> {
        if !self.comb_paged() {
            return None;
        }
        let last = pages(self.columns[1].order.len()) - 1;
        let at = self.comb_page.min(last);
        Some([(ARROWS[0], at > 0), (ARROWS[1], at < last)])
    }

    /// Turns the page on a click on an arrow (or Page Up / Page Down).
    /// True when the input was used.
    pub(super) fn update_comb_arrows(&mut self, input: &Input) -> bool {
        let Some([(back, can_back), (on, can_on)]) = self.comb_arrows() else {
            return false;
        };
        let at = self.comb_page.min(pages(self.columns[1].order.len()) - 1);
        let click = |r: Rect| input.clicked && inside(self.cursor, r);
        if can_back && (click(back) || input.pressed(KeyCode::PageUp)) {
            self.comb_page = at - 1;
            return true;
        }
        if can_on && (click(on) || input.pressed(KeyCode::PageDown)) {
            self.comb_page = at + 1;
            return true;
        }
        click(back) || click(on)
    }

    /// The arrows on the bottom bar's planks, and the page under the account.
    pub(super) fn draw_comb_arrows(&self, out: &mut Vec<Sprite>, k: &super::super::kit::KitArt) {
        use super::super::kit::{PALE, STONE};
        let Some(arrows) = self.comb_arrows() else {
            return;
        };
        let full = [0.0, 0.0, 1.0, 1.0];
        for ((r, live), word) in arrows.into_iter().zip(["<", ">"]) {
            let tint = match (live, inside(self.cursor, r)) {
                (false, _) => [0.6, 0.6, 0.6, 0.7],
                (true, true) => [1.3, 1.3, 1.25, 1.0],
                _ => [1.0; 4],
            };
            if let Some(id) = k.plank {
                self.tinted(out, id, r, full, tint);
            }
            let color = if live { STONE } else { [0.45, 0.45, 0.43, 0.8] };
            let px = 30.0;
            let at = (r.0 + r.2 / 2.0, r.1 + (r.3 - px) / 2.0);
            self.kit_line(out, Face::Title, at, px, color, word, 0.5);
        }
        let n = pages(self.columns[1].order.len());
        let t = format!("{} / {n}", self.comb_page.min(n - 1) + 1);
        self.kit_line(out, Face::Body, (CW / 2.0, 545.0), 16.0, PALE, &t, 0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_holds_twenty_at_the_phone_size() {
        let full = Comb::new(COMB_PAGE, 1);
        let vanilla = Comb::new(18, 1);
        assert_eq!(full.rows.len(), 4, "{:?}", full.rows);
        assert_eq!(full.w, vanilla.w);
    }

    #[test]
    fn pages_split_in_twenties() {
        let order: Vec<usize> = (0..45).collect();
        assert_eq!(pages(order.len()), 3);
        assert_eq!(page_of(&order, 0), &order[..20]);
        assert_eq!(page_of(&order, 2), &order[40..]);
        // Past the end: the last page.
        assert_eq!(page_of(&order, 9), &order[40..]);
        assert_eq!(pages(0), 1);
    }
}
