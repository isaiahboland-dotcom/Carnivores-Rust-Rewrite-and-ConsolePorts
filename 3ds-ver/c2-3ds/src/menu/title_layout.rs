//! The title page's buttons and where they sit on the bottom screen.
//!
//! One function places them for the classic draw, the DHC OS terminal draw
//! and the tap handling, so a tap always resolves against what was drawn.
//! Pure (no GPU, no pack), so `c2-cook` runs its tests on the host.

/// A title page button, in the order it is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleButton {
    Play,
    Survival,
    Trophies,
    Settings,
    Quit,
    Profile,
}

/// The buttons stand in y 0..`REGION_H`; the message line below stays put.
const REGION_H: f32 = 216.0;
const BUTTON_H: f32 = 30.0;
const PITCH: f32 = 36.0;
/// One column: its x and width.
const SINGLE: (f32, f32) = (60.0, 200.0);
/// Two columns: the left one's x, the right one's x, and their width.
const DOUBLE: (f32, f32, f32) = (12.0, 164.0, 144.0);

/// How many buttons fit in one column of the region.
pub fn column_capacity() -> usize {
    ((REGION_H + (PITCH - BUTTON_H)) / PITCH) as usize
}

/// The buttons shown: survival only when one is offered, the rest always.
pub fn title_buttons(survival: bool) -> Vec<TitleButton> {
    use TitleButton::*;
    [Play, Survival, Trophies, Settings, Quit, Profile]
        .into_iter()
        .filter(|&b| b != Survival || survival)
        .collect()
}

/// The top of a column of `n` buttons centred in the region.
fn column_top(n: usize) -> f32 {
    let stack = n as f32 * PITCH - (PITCH - BUTTON_H);
    ((REGION_H - stack) / 2.0).floor()
}

/// `buttons` with their rectangles (`[x, y, w, h]`): one centred column,
/// or two (the left one holding the odd one out) when they do not fit.
pub fn layout(buttons: &[TitleButton]) -> Vec<(TitleButton, [f32; 4])> {
    let n = buttons.len();
    let column = |list: &[TitleButton], x: f32, w: f32| {
        let top = column_top(list.len());
        list.iter()
            .enumerate()
            .map(move |(i, &b)| (b, [x, top + i as f32 * PITCH, w, BUTTON_H]))
            .collect::<Vec<_>>()
    };
    if n <= column_capacity() {
        return column(buttons, SINGLE.0, SINGLE.1);
    }
    let (left, right) = buttons.split_at(n.div_ceil(2));
    let mut out = column(left, DOUBLE.0, DOUBLE.2);
    out.extend(column(right, DOUBLE.1, DOUBLE.2));
    out
}

/// The title page as shown: [`title_buttons`] placed by [`layout`].
pub fn title_layout(survival: bool) -> Vec<(TitleButton, [f32; 4])> {
    layout(&title_buttons(survival))
}

/// The button under a tap at (x, y), if any.
pub fn hit(placed: &[(TitleButton, [f32; 4])], x: f32, y: f32) -> Option<TitleButton> {
    placed
        .iter()
        .find(|(_, r)| x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3])
        .map(|&(b, _)| b)
}

#[cfg(test)]
mod tests {
    use super::TitleButton::*;
    use super::*;

    fn ys(placed: &[(TitleButton, [f32; 4])]) -> Vec<f32> {
        placed.iter().map(|(_, r)| r[1]).collect()
    }

    #[test]
    fn six_buttons_centred_in_order() {
        let p = title_layout(true);
        let order: Vec<_> = p.iter().map(|&(b, _)| b).collect();
        assert_eq!(order, [Play, Survival, Trophies, Settings, Quit, Profile]);
        assert_eq!(ys(&p), [3.0, 39.0, 75.0, 111.0, 147.0, 183.0]);
        assert!(p
            .iter()
            .all(|(_, r)| r[0] == 60.0 && r[2] == 200.0 && r[3] == 30.0));
        // The last button ends above the message line (y 224).
        assert!(p[5].1[1] + p[5].1[3] <= 216.0);
    }

    #[test]
    fn without_survival_five_close_up() {
        let p = title_layout(false);
        assert!(p.iter().all(|&(b, _)| b != Survival));
        assert_eq!(ys(&p), [21.0, 57.0, 93.0, 129.0, 165.0]);
    }

    #[test]
    fn capacity_is_six() {
        assert_eq!(column_capacity(), 6);
    }

    #[test]
    fn seven_split_four_and_three() {
        let seven = [Play, Survival, Trophies, Settings, Quit, Profile, Play];
        let p = layout(&seven);
        let (left, right): (Vec<_>, Vec<_>) = p.iter().partition(|(_, r)| r[0] == 12.0);
        assert_eq!((left.len(), right.len()), (4, 3));
        assert!(right.iter().all(|(_, r)| r[0] == 164.0 && r[2] == 144.0));
        // Each column centred on the same pitch.
        assert_eq!(left[0].1[1], 39.0);
        assert_eq!(right[0].1[1], 57.0);
        assert_eq!(right[1].1[1] - right[0].1[1], 36.0);
    }

    #[test]
    fn taps_hit_what_is_drawn() {
        for survival in [true, false] {
            let p = title_layout(survival);
            for &(b, r) in &p {
                assert_eq!(hit(&p, r[0] + 1.0, r[1] + 1.0), Some(b));
                assert_eq!(hit(&p, r[0] + r[2] - 1.0, r[1] + r[3] - 1.0), Some(b));
            }
            // The gap between two buttons and the margins hit nothing.
            assert_eq!(hit(&p, 160.0, p[0].1[1] + 32.0), None);
            assert_eq!(hit(&p, 20.0, p[0].1[1] + 5.0), None);
        }
        // Without survival, the second button is the trophy room.
        let p = title_layout(false);
        assert_eq!(hit(&p, 160.0, 60.0), Some(Trophies));
    }
}
