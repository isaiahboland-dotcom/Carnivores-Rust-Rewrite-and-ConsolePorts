//! Where things go on the menus' 800x600 canvas, and their colours.

/// The menus are drawn on an 800x600 canvas, scaled to fit the window.
pub(super) const CW: f32 = 800.0;
pub(super) const CH: f32 = 600.0;

pub(super) type Rect = (f32, f32, f32, f32);

pub(super) fn inside(p: Option<(f32, f32)>, r: Rect) -> bool {
    p.is_some_and(|(x, y)| x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3)
}

/// The hunt / survival switch in the setup screen's bottom bar (ours).
pub(super) const MODE_RECT: Rect = (150.0, 558.0, 240.0, 30.0);

/// The original's list rows: 16 px apart from the box's first row, y 382
/// (`MenuHunt[..].Rect.top + 16 * i`; clicks `(y - top) / 16`).
pub(super) const ROW_H: f32 = 16.0;

pub(super) const GAME_PANEL: Rect = (48.0, 76.0, 324.0, 190.0);
pub(super) const VIDEO_PANEL: Rect = (48.0, 346.0, 324.0, 200.0);
pub(super) const KEYS_PANEL: Rect = (432.0, 76.0, 320.0, 460.0);
pub(super) const OPT_ROW: f32 = 22.0;
pub(super) const SLIDER_X: f32 = 220.0;

/// Colours (RGBA).
pub(super) const TEXT: [f32; 4] = [0.62, 0.82, 0.5, 1.0];
pub(super) const HOVER: [f32; 4] = [0.9, 1.0, 0.75, 1.0];
pub(super) const PICKED: [f32; 4] = [1.0, 0.85, 0.3, 1.0];
pub(super) const DIM: [f32; 4] = [0.36, 0.42, 0.3, 1.0];
pub(super) const SAND: [f32; 4] = [0.9, 0.78, 0.6, 1.0];
pub(super) const BLUE: [f32; 4] = [0.55, 0.78, 0.95, 1.0];
pub(super) const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Main menu button (ours, under the original's): survival.
pub(super) const SURVIVAL_BUTTON: Rect = (330.0, 518.0, 260.0, 30.0);
