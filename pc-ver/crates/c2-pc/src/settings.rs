//! PC-only settings the original save file has no room for, kept in
//! `c2rs.cfg` in the data folder: the render distance and fog.

use std::path::PathBuf;

/// The farthest render distance, in cells (the original's longest view is
/// 104; the map is 1024 across).
pub const MAX_RENDER_CELLS: i32 = 384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PcSettings {
    /// Render distance slider, 0..=255: 0 = the original view range, 255 =
    /// [`MAX_RENDER_CELLS`].
    pub render: i32,
    /// Fog: hunt at the original view range and fog, whatever `render`
    /// says (0 = off, 1 = on).
    pub fog: i32,
}

fn path() -> PathBuf {
    crate::save::data_dir().join("c2rs.cfg")
}

impl PcSettings {
    /// The saved settings, or the defaults (nothing changed from the
    /// original) if there are none or they can't be read.
    pub fn load() -> Self {
        let mut s = Self::default();
        let Ok(text) = std::fs::read_to_string(path()) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v: i32 = v.trim().parse().unwrap_or(0);
            match k.trim() {
                "render_distance" => s.render = v.clamp(0, 255),
                "fog" => s.fog = i32::from(v != 0),
                _ => {}
            }
        }
        s
    }

    pub fn store(&self) {
        let text = format!("render_distance = {}\nfog = {}\n", self.render, self.fog);
        if let Some(dir) = path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path(), text) {
            tracing::warn!("saving settings: {e}");
        }
    }

    /// Cells to draw, given the original view range `view` (cells).
    pub fn render_cells(&self, view: i32) -> i32 {
        if self.fog != 0 {
            return view;
        }
        view + (MAX_RENDER_CELLS - view).max(0) * self.render / 255
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_keeps_the_original_view() {
        let mut s = PcSettings::default();
        assert_eq!(s.render_cells(74), 74);
        s.render = 255;
        assert_eq!(s.render_cells(74), MAX_RENDER_CELLS);
        s.fog = 1;
        assert_eq!(s.render_cells(74), 74);
    }
}
