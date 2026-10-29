//! PC-only settings the original save file has no room for, kept in
//! `c2rs.cfg` in the data folder: the render distance, fog, blood and the
//! death camera.

use std::path::PathBuf;

/// The farthest render distance, in cells (the original's longest view is
/// 104; the map is 1024 across).
pub const MAX_RENDER_CELLS: i32 = 384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcSettings {
    /// Render distance slider, 0..=255: 0 = the original view range, 255 =
    /// [`MAX_RENDER_CELLS`].
    pub render: i32,
    /// Fog: hunt at the original view range and fog, whatever `render`
    /// says (0 = off, 1 = on).
    pub fog: i32,
    /// Blood sprays and trails (1 = on, as in the original).
    pub blood: i32,
    /// The death camera (1 = on); off: a black screen and the prompt.
    pub death_anim: i32,
    /// Far objects as their sprites (1 = on, as the original); off: as
    /// models all the way.
    pub far_sprites: i32,
    /// The map shows which way the hunter looks (1 = on; not in the
    /// original, the 3DS map's tick).
    pub map_heading: i32,
    /// The map as the mobile game draws it: a yellow frame and a yellow
    /// cone for what the hunter sees (1 = on).
    pub map_mobile: i32,
    /// Teleporting by the map (1 = on): open it, move the marker with the
    /// mouse, click; 5 s, then a minute's cooldown.
    pub teleport: i32,
    /// Button hints in the hunt (1 = on): map, binoculars, switch gun,
    /// jump, crouch, for the pad or the keys, whichever was used last. The
    /// exit and pause prompts show theirs either way.
    pub hints: i32,
}

impl Default for PcSettings {
    fn default() -> Self {
        Self {
            render: 0,
            fog: 0,
            blood: 1,
            death_anim: 1,
            far_sprites: 1,
            map_heading: 0,
            map_mobile: 0,
            teleport: 0,
            hints: 0,
        }
    }
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
                "blood" => s.blood = i32::from(v != 0),
                "death_animation" => s.death_anim = i32::from(v != 0),
                "far_sprites" => s.far_sprites = i32::from(v != 0),
                "map_heading" => s.map_heading = i32::from(v != 0),
                "map_mobile" => s.map_mobile = i32::from(v != 0),
                "teleport" => s.teleport = i32::from(v != 0),
                "button_hints" => s.hints = i32::from(v != 0),
                _ => {}
            }
        }
        s
    }

    pub fn store(&self) {
        let text = format!(
            "render_distance = {}\nfog = {}\nblood = {}\ndeath_animation = {}\nfar_sprites = {}\n\
             map_heading = {}\nmap_mobile = {}\nteleport = {}\nbutton_hints = {}\n",
            self.render,
            self.fog,
            self.blood,
            self.death_anim,
            self.far_sprites,
            self.map_heading,
            self.map_mobile,
            self.teleport,
            self.hints
        );
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
