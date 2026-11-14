//! The DHC OS terminal's colours on the 3DS (the PC terminal's scheme,
//! without its CRT effects), for the menu's terminal pages, the settings
//! screen and the hunt's exit prompt when the terminal is on.

use crate::draw2d::{Canvas, Rgba};

pub const BG: Rgba = [1, 6, 3, 255];
pub const PANEL: Rgba = [5, 24, 13, 255];
pub const LINE: Rgba = [18, 64, 31, 255];
pub const HI: Rgba = [147, 255, 190, 255];
pub const MD: Rgba = [84, 217, 138, 255];
pub const LO: Rgba = [43, 148, 87, 255];
pub const FAINT: Rgba = [23, 100, 58, 255];
pub const AMBER: Rgba = [255, 194, 71, 255];
pub const RED: Rgba = [255, 95, 95, 255];
pub const INV: Rgba = [2, 20, 10, 255];

/// A one-pixel frame around `r`, filled.
pub fn frame(c: &mut Canvas, r: [f32; 4], fill: Rgba, line: Rgba) {
    c.rect(r, line);
    c.rect([r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0], fill);
}
