//! PC-only settings the original save file has no room for, kept in
//! `c2rs.cfg` in the data folder: the render distance, fog, blood and the
//! death camera.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// The last loaded or stored resolution slider, for the frame loop.
static RENDER_SCALE: AtomicU32 = AtomicU32::new(255);

/// The resolution scale the settings last loaded or stored ask for (see
/// [`PcSettings::resolution_scale`]).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn resolution_scale() -> f32 {
    scale_of(RENDER_SCALE.load(Ordering::Relaxed) as i32)
}

fn scale_of(slider: i32) -> f32 {
    0.5 + 0.5 * slider.clamp(0, 255) as f32 / 255.0
}

/// The farthest render distance, in cells (the original's longest view is
/// 104; the map is 1024 across).
pub const MAX_RENDER_CELLS: i32 = 384;

/// The farthest render distance with high-end view on: the whole map from
/// anywhere on it.
pub const HIGH_END_RENDER_CELLS: i32 = 1024;

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
    /// Model distance slider, 0..=255: how far objects (trees, bushes,
    /// rocks) are drawn as models before their far sprite takes over.
    /// 0 = the original's 24 cells (`ctViewRM`), 255 = models out to the
    /// render distance, no sprites.
    pub model_distance: i32,
    /// The map shows which way the hunter looks (1 = on; not in the
    /// original, the 3DS map's tick).
    pub map_heading: i32,
    /// The map as the mobile game draws it: a yellow frame and a yellow
    /// cone for what the hunter sees (1 = on).
    pub map_mobile: i32,
    /// Opt-in mouse-wheel map zoom and right-drag panning.
    pub map_zoom: i32,
    /// Teleporting by the map (1 = on): open it, move the marker with the
    /// mouse, click; 5 s, then a minute's cooldown.
    pub teleport: i32,
    /// Button hints in the hunt (1 = on): map, binoculars, switch gun,
    /// jump, crouch, for the pad or the keys, whichever was used last. The
    /// exit and pause prompts show theirs either way.
    pub hints: i32,
    /// Every huntable animal in the hunt menu (1 = on): a dialect game's
    /// roaming add-ons too, which their makers left out of the menu to be
    /// come across on their own maps; off, they only roam there.
    pub all_huntable: i32,
    /// Unselected huntable animals per map; zero disables them.
    pub secret_dinos: i32,
    /// Opt-in one-hour visual day/night cycle.
    pub dynamic_time: i32,
    /// The flashlight's beam last picked in a hunt (B toggles it there):
    /// 0 wide (the original), 1 narrow, with twice the reach.
    pub narrow_flashlight: i32,
    /// Multi ammo (1 = on): with the tranquilizer picked, the hunt menu asks
    /// Half tranq (rounds split between lethal and darts, swapped in the
    /// hunt) or Full tranq (darts only).
    pub multi_ammo: i32,
    /// That answer: 1 Half tranq, 0 Full tranq.
    pub half_tranq: i32,
    /// Advanced observer mode (1 = on): observing, unlimited instant
    /// teleports, flying and stopping time.
    pub advanced_observer: i32,
    /// Starting survival points; zero derives the budget from menu prices.
    pub survival_budget: i32,
    /// Hardcore survival offered beside Classic and Modded (1 = on; off by
    /// default, not in the original): endless night, flashlight, radar.
    pub hardcore_survival: i32,
    /// "Map puts the gun away" (1 = on, the default; not in the original,
    /// which opens the map only with the weapon put away).
    pub map_holsters: i32,
    /// High-end view (1 = on; off by default, not in the original; on
    /// Android, experimental): render distance up to
    /// [`HIGH_END_RENDER_CELLS`] with the far ground in coarser levels, and
    /// objects fading between model and sprite instead of switching.
    pub high_end_view: i32,
    /// Resolution slider, 0..=255 (Android only, not in the original): the
    /// frame is drawn at [`PcSettings::resolution_scale`] of the screen's
    /// pixels and the phone scales it up. 255 = every pixel.
    pub render_scale: i32,
}

impl Default for PcSettings {
    fn default() -> Self {
        Self {
            render: 0,
            fog: 0,
            blood: 1,
            death_anim: 1,
            model_distance: 0,
            map_heading: 0,
            map_mobile: 0,
            map_zoom: 0,
            teleport: 0,
            hints: 0,
            all_huntable: 0,
            secret_dinos: 10,
            dynamic_time: 0,
            narrow_flashlight: 0,
            multi_ammo: 0,
            half_tranq: 1,
            advanced_observer: 0,
            survival_budget: 0,
            hardcore_survival: 0,
            map_holsters: 1,
            high_end_view: 0,
            render_scale: 255,
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
        let s = Self::default();
        let s = std::fs::read_to_string(path()).map_or(s, |text| Self::parse(&text));
        s.publish();
        s
    }

    /// Makes the settings the frame loop reads ([`resolution_scale`]) these.
    fn publish(&self) {
        RENDER_SCALE.store(self.render_scale as u32, Ordering::Relaxed);
    }

    /// The share of the screen's pixels the frame is drawn at, per side:
    /// 50% to 100%.
    pub fn resolution_scale(&self) -> f32 {
        scale_of(self.render_scale)
    }

    fn parse(text: &str) -> Self {
        let mut s = Self::default();
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
                "model_distance" => s.model_distance = v.clamp(0, 255),
                // The old on/off switch: off meant models all the way.
                "far_sprites" if v == 0 => s.model_distance = 255,
                "map_heading" => s.map_heading = i32::from(v != 0),
                "map_mobile" => s.map_mobile = i32::from(v != 0),
                "map_zoom" => s.map_zoom = i32::from(v != 0),
                "teleport" => s.teleport = i32::from(v != 0),
                "button_hints" => s.hints = i32::from(v != 0),
                "narrow_flashlight" => s.narrow_flashlight = i32::from(v != 0),
                "multi_ammo" => s.multi_ammo = i32::from(v != 0),
                "half_tranq" => s.half_tranq = i32::from(v != 0),
                "advanced_observer" => s.advanced_observer = i32::from(v != 0),
                "dynamic_time" => s.dynamic_time = i32::from(v != 0),
                "survival_budget" => s.survival_budget = v.max(0),
                "hardcore_survival" => s.hardcore_survival = i32::from(v != 0),
                "map_holsters" => s.map_holsters = i32::from(v != 0),
                "high_end_view" => s.high_end_view = i32::from(v != 0),
                "render_scale" => s.render_scale = v.clamp(0, 255),
                "secret_dinos" => s.secret_dinos = v.clamp(0, 255),
                "all_huntable" => s.all_huntable = i32::from(v != 0),
                _ => {}
            }
        }
        s
    }

    pub fn store(&self) {
        self.publish();
        let text = format!(
            "render_distance = {}\nfog = {}\nblood = {}\ndeath_animation = {}\nmodel_distance = {}\n\
             map_heading = {}\nmap_mobile = {}\nmap_zoom = {}\nteleport = {}\nbutton_hints = {}\nall_huntable = {}\nsecret_dinos = {}\ndynamic_time = {}\nnarrow_flashlight = {}\nmulti_ammo = {}\nhalf_tranq = {}\nadvanced_observer = {}\nsurvival_budget = {}\nhardcore_survival = {}\nmap_holsters = {}\nhigh_end_view = {}\nrender_scale = {}\n",
            self.render,
            self.fog,
            self.blood,
            self.death_anim,
            self.model_distance,
            self.map_heading,
            self.map_mobile,
            self.map_zoom,
            self.teleport,
            self.hints,
            self.all_huntable,
            self.secret_dinos,
            self.dynamic_time,
            self.narrow_flashlight,
            self.multi_ammo,
            self.half_tranq,
            self.advanced_observer,
            self.survival_budget,
            self.hardcore_survival,
            self.map_holsters,
            self.high_end_view,
            self.render_scale
        );
        if let Some(dir) = path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path(), text) {
            tracing::warn!("saving settings: {e}");
        }
    }

    /// Cells out to which objects are models, given the cells drawn
    /// (`render`); `None`: models all the way, no sprites.
    pub fn model_cells(&self, render: i32) -> Option<f32> {
        if self.model_distance >= 255 {
            return None;
        }
        let base = c2_core::model::SPRITE_CELLS;
        let span = (render as f32 - base).max(0.0);
        Some(base + span * self.model_distance as f32 / 255.0)
    }

    /// Cells to draw, given the original view range `view` (cells).
    pub fn render_cells(&self, view: i32) -> i32 {
        if self.fog != 0 {
            return view;
        }
        let max = if self.high_end_view != 0 {
            HIGH_END_RENDER_CELLS
        } else {
            MAX_RENDER_CELLS
        };
        view + (max - view).max(0) * self.render / 255
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survival_budget_defaults_to_auto_and_accepts_point_overrides() {
        assert_eq!(PcSettings::parse("").survival_budget, 0);
        assert_eq!(
            PcSettings::parse("survival_budget = -20").survival_budget,
            0
        );
        assert_eq!(
            PcSettings::parse("survival_budget = 5000").survival_budget,
            5000
        );
    }

    #[test]
    fn hardcore_survival_is_opt_in() {
        assert_eq!(PcSettings::default().hardcore_survival, 0);
        assert_eq!(PcSettings::parse("blood = 0\n").hardcore_survival, 0);
        assert_eq!(
            PcSettings::parse("hardcore_survival = 1\n").hardcore_survival,
            1
        );
    }

    #[test]
    fn dynamic_time_is_opt_in_for_existing_settings() {
        assert_eq!(PcSettings::parse("blood = 0\n").dynamic_time, 0);
        assert_eq!(PcSettings::parse("dynamic_time = 1\n").dynamic_time, 1);
        assert_eq!(PcSettings::parse("dynamic_time = 0\n").dynamic_time, 0);
    }

    #[test]
    fn map_zoom_is_opt_in_for_existing_settings() {
        assert_eq!(PcSettings::default().map_zoom, 0);
        assert_eq!(PcSettings::parse("map_mobile = 1\n").map_zoom, 0);
        assert_eq!(PcSettings::parse("map_zoom = 1\n").map_zoom, 1);
        assert_eq!(PcSettings::parse("map_zoom = 0\n").map_zoom, 0);
    }

    #[test]
    fn narrow_flashlight_is_opt_in() {
        assert_eq!(PcSettings::parse("dynamic_time = 1\n").narrow_flashlight, 0);
        assert_eq!(
            PcSettings::parse("narrow_flashlight = 1\n").narrow_flashlight,
            1
        );
        assert_eq!(
            PcSettings::parse("narrow_flashlight = 0\n").narrow_flashlight,
            0
        );
    }

    #[test]
    fn multi_ammo_and_advanced_observer_are_opt_in_and_round_trip() {
        let d = PcSettings::parse("");
        assert_eq!((d.multi_ammo, d.half_tranq, d.advanced_observer), (0, 1, 0));
        let s = PcSettings::parse("multi_ammo = 1\nhalf_tranq = 0\nadvanced_observer = 1\n");
        assert_eq!((s.multi_ammo, s.half_tranq, s.advanced_observer), (1, 0, 1));
    }

    #[test]
    fn model_distance_defaults_to_the_original_and_reads_the_old_switch() {
        let d = PcSettings::default();
        assert_eq!(d.model_cells(74), Some(c2_core::model::SPRITE_CELLS));
        assert_eq!(PcSettings::parse("far_sprites = 1\n").model_distance, 0);
        assert_eq!(PcSettings::parse("far_sprites = 0\n").model_cells(74), None);
        let s = PcSettings::parse("model_distance = 255\n");
        assert_eq!(s.model_cells(74), None);
        let half = PcSettings {
            model_distance: 128,
            ..d
        };
        let c = half.model_cells(384).unwrap_or(0.0);
        assert!(c > 200.0 && c < 210.0, "{c}");
    }

    #[test]
    fn resolution_defaults_to_every_pixel_and_halves_at_most() {
        let d = PcSettings::parse("blood = 1\n");
        assert_eq!(d.render_scale, 255);
        assert_eq!(d.resolution_scale(), 1.0);
        let low = PcSettings::parse("render_scale = 0\n");
        assert_eq!(low.resolution_scale(), 0.5);
    }

    #[test]
    fn fog_keeps_the_original_view() {
        let mut s = PcSettings::default();
        assert_eq!(s.render_cells(74), 74);
        s.render = 255;
        assert_eq!(s.render_cells(74), MAX_RENDER_CELLS);
        s.fog = 1;
        assert_eq!(s.render_cells(74), 74);
    }

    #[test]
    fn high_end_view_is_opt_in_and_reaches_the_whole_map() {
        let mut s = PcSettings::parse("render_distance = 255\n");
        assert_eq!(s.high_end_view, 0);
        assert_eq!(s.render_cells(74), MAX_RENDER_CELLS);
        s = PcSettings::parse("render_distance = 255\nhigh_end_view = 1\n");
        assert_eq!(s.render_cells(74), HIGH_END_RENDER_CELLS);
        s.render = 0;
        assert_eq!(s.render_cells(74), 74);
    }
}
