//! The settings screen: three tabs (Game, Hunt, System), each a short
//! list of widgets laid out in the same two-column flow. The tab strip is
//! tapped, or turned with the D-Pad. A locked setting (the pack's instance
//! rules) is left out and the rest close up.

use c2_formats::rules::Lock;

use super::{Look, Menu, BACK, BAR_Y};
use crate::draw2d::{inside, Canvas};
use crate::gpu::Gpu;

/// The tabs, left to right.
pub(super) const TABS: [&str; 3] = ["Game", "Hunt", "System"];

/// The original's game options (`Options > Game`), 0..=255, kept in the
/// save: bars for aggressiveness, density and sensitivity.
const GAME_OPTS: [&str; 3] = ["Aggressive", "Density", "Sensitivity"];
const BAR_X: f32 = 120.0;
const BAR_W: f32 = 180.0;
const BAR_H: f32 = 18.0;

const TOP: f32 = 36.0;
const PITCH: f32 = 34.0;
const CELL_W: f32 = 140.0;
const CELL_H: f32 = 26.0;
const COL_X: [f32; 2] = [16.0, 164.0];

/// A tab's rectangle.
fn tab_rect(i: usize) -> [f32; 4] {
    [8.0 + 104.0 * i as f32, 4.0, 100.0, 24.0]
}

#[derive(Clone, Copy, PartialEq)]
enum Tog {
    Blood,
    DeathAnim,
    MapStyle,
    Teleport,
    Hints,
    Dhc,
    AllHuntable,
    Test,
    MultiAmmo,
    FreeObserver,
}

impl Tog {
    /// The instance rule that fixes this setting, if any.
    fn lock(self) -> Option<Lock> {
        match self {
            Tog::MapStyle => Some(Lock::MobileRadar),
            Tog::Teleport => Some(Lock::Teleport),
            Tog::AllHuntable => Some(Lock::AllHuntable),
            Tog::MultiAmmo => Some(Lock::MultiAmmo),
            Tog::FreeObserver => Some(Lock::AdvancedObserver),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Tog::Blood => "Blood",
            Tog::DeathAnim => "Death anim",
            Tog::MapStyle => "Mobile map",
            Tog::Teleport => "Teleport",
            Tog::Hints => "Hints",
            Tog::Dhc => "DHC terminal",
            Tog::AllHuntable => "All dinos",
            Tog::Test => "Test points",
            Tog::MultiAmmo => "Multi ammo",
            Tog::FreeObserver => "Adv observer",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Item {
    /// A game option bar (index in `GAME_OPTS`), a full row.
    Bar(usize),
    Crouch,
    Gyro,
    Toggle(Tog),
    Clear,
}

/// What each tab holds, in order.
fn tab_items(tab: usize) -> Vec<Item> {
    use Item::*;
    use Tog::*;
    match tab {
        0 => vec![
            Bar(0),
            Bar(1),
            Bar(2),
            Crouch,
            Gyro,
            Toggle(Blood),
            Toggle(DeathAnim),
        ],
        1 => vec![
            Toggle(MapStyle),
            Toggle(Teleport),
            Toggle(MultiAmmo),
            Toggle(FreeObserver),
            Toggle(Hints),
            Toggle(AllHuntable),
        ],
        _ => vec![Toggle(Dhc), Toggle(Test), Clear],
    }
}

/// The tab's widgets with their rectangles (a bar's is the bar itself).
fn layout(tab: usize) -> Vec<(Item, [f32; 4])> {
    let (mut y, mut col) = (TOP, 0);
    let mut out = Vec::new();
    for it in tab_items(tab) {
        if matches!(it, Item::Toggle(t) if t.lock().is_some_and(crate::instance_rules::locked)) {
            continue;
        }
        if let Item::Bar(_) = it {
            if col != 0 {
                (col, y) = (0, y + PITCH);
            }
            out.push((it, [BAR_X, y + 3.0, BAR_W, BAR_H]));
            y += PITCH;
        } else {
            out.push((it, [COL_X[col], y, CELL_W, CELL_H]));
            col += 1;
            if col == 2 {
                (col, y) = (0, y + PITCH);
            }
        }
    }
    out
}

impl Menu {
    /// A tap at (x, y) on the settings screen.
    pub(super) fn settings_tap(&mut self, x: f32, y: f32) {
        if let Some(t) = (0..TABS.len()).find(|&t| inside(tab_rect(t), x, y)) {
            self.settings_tab = t;
            self.clear_armed = false;
            return;
        }
        let Some((item, r)) = layout(self.settings_tab)
            .into_iter()
            .find(|&(_, r)| inside(r, x, y))
        else {
            self.clear_armed = false;
            return;
        };
        if item != Item::Clear {
            self.clear_armed = false;
        }
        match item {
            Item::Bar(i) => {
                let v = ((x - r[0]) / r[2] * 255.0).round().clamp(0.0, 255.0) as i32;
                let o = &mut self.save.options;
                match i {
                    0 => o.agres = v,
                    1 => o.dens = v,
                    _ => o.sens = v,
                }
            }
            Item::Clear => {
                if self.clear_armed {
                    let n = c2_core::profile::clear_unlocked(&mut self.save);
                    self.message = Some(match crate::save::store(&self.save) {
                        Ok(()) => format!("{n} trophies removed"),
                        Err(e) => format!("save failed: {e}"),
                    });
                }
                self.clear_armed = !self.clear_armed;
            }
            Item::Crouch => self.settings.crouch_key = self.settings.crouch_key.next(),
            Item::Gyro => {
                self.settings.gyro_pct = crate::settings::next_gyro(self.settings.gyro_pct);
                self.focus = Some("Stealth crouch: turning the 3DS aims".into());
            }
            Item::Toggle(t) => self.toggle_setting(t),
        }
    }

    fn toggle_setting(&mut self, t: Tog) {
        let s = &mut self.settings;
        let note = |on: bool, a: &str, b: &str| Some(if on { a } else { b }.to_string());
        match t {
            Tog::Blood => s.blood = !s.blood,
            Tog::DeathAnim => s.death_anim = !s.death_anim,
            Tog::MapStyle => s.map_mobile = !s.map_mobile,
            Tog::Teleport => {
                s.teleport = !s.teleport;
                self.focus = Some("Tap the map in a hunt; 5 s, then a minute".into());
            }
            Tog::Hints => s.hints = !s.hints,
            Tog::Dhc => {
                s.dhc_terminal = !s.dhc_terminal;
                self.focus = note(
                    s.dhc_terminal,
                    "DHC OS terminal: L/R pages, A picks",
                    "The hexagon menu",
                );
            }
            Tog::MultiAmmo => {
                s.multi_ammo = !s.multi_ammo;
                self.focus = Some("Tranq asks: half lethal/half darts, or all".into());
            }
            Tog::FreeObserver => {
                s.free_observer = !s.free_observer;
                self.focus = Some("Observe: teleport, fly, stop time".into());
            }
            Tog::Test => {
                s.test_points = !s.test_points;
                self.focus = note(
                    s.test_points,
                    "Test: unlimited points (save keeps its own)",
                    "Points: the hunter's score",
                );
            }
            Tog::AllHuntable => {
                s.all_huntable = !s.all_huntable;
                self.focus = note(
                    s.all_huntable,
                    "Add-ons that roam are in the list",
                    "Add-ons that roam: on their maps",
                );
            }
        }
    }

    fn setting_on(&self, t: Tog) -> bool {
        let s = &self.settings;
        match t {
            Tog::Blood => s.blood,
            Tog::DeathAnim => s.death_anim,
            Tog::MapStyle => s.map_mobile,
            Tog::Teleport => s.teleport,
            Tog::Hints => s.hints,
            Tog::Dhc => s.dhc_terminal,
            Tog::AllHuntable => s.all_huntable,
            Tog::Test => s.test_points,
            Tog::MultiAmmo => s.multi_ammo,
            Tog::FreeObserver => s.free_observer,
        }
    }

    /// The settings screen (bottom).
    pub(super) fn draw_settings(&self, c: &mut Canvas, gpu: &mut Gpu, look: &Look) {
        let tab = self.settings_tab;
        let items = layout(tab);
        let o = self.save.options;
        let values = [o.agres, o.dens, o.sens];
        c.rect(BACK, look.panel);
        for i in 0..TABS.len() {
            c.rect(tab_rect(i), if i == tab { look.on } else { look.panel });
        }
        for &(it, r) in &items {
            match it {
                Item::Bar(i) => {
                    c.rect(r, look.panel);
                    let f = values[i].clamp(0, 255) as f32 / 255.0;
                    c.rect([r[0], r[1], r[2] * f.max(0.02), r[3]], look.on);
                }
                Item::Toggle(t) => {
                    c.rect(r, if self.setting_on(t) { look.on } else { look.panel });
                }
                Item::Clear => c.rect(
                    r,
                    if self.clear_armed {
                        [140, 60, 30, 255]
                    } else {
                        look.panel
                    },
                ),
                _ => c.rect(r, look.panel),
            }
        }
        c.flush(gpu);
        for (i, name) in TABS.iter().enumerate() {
            let r = tab_rect(i);
            c.text_c(gpu, r[0] + r[2] / 2.0, r[1] + 8.0, 1.0, look.value, name);
        }
        let unlocked = self
            .save
            .bodies
            .iter()
            .filter(|b| b.ctype != 0 && !b.locked())
            .count();
        for &(it, r) in &items {
            let (cx, cy) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0 - 4.0);
            match it {
                Item::Bar(i) => {
                    c.text(gpu, 16.0, r[1] + 5.0, 1.0, look.label, GAME_OPTS[i]);
                    let pct = format!("{}%", values[i].clamp(0, 255) * 100 / 255);
                    c.text_c(gpu, cx, r[1] + 5.0, 1.0, look.value, &pct);
                }
                Item::Crouch => {
                    let t = format!("Crouch: {}", self.settings.crouch_key.name());
                    c.text_c(gpu, cx, cy, 1.0, look.value, &t);
                }
                Item::Gyro => {
                    let t = format!("Gyro aim: {}%", self.settings.gyro_pct);
                    c.text_c(gpu, cx, cy, 1.0, look.value, &t);
                }
                Item::Toggle(t) => {
                    let on = if self.setting_on(t) { "ON" } else { "OFF" };
                    c.text_c(gpu, cx, cy, 1.0, look.value, &format!("{}: {on}", t.name()));
                }
                Item::Clear => {
                    let t = if self.clear_armed {
                        format!("Again: remove {unlocked}")
                    } else {
                        "Clear trophies".into()
                    };
                    c.text_c(gpu, cx, cy, 1.0, look.value, &t);
                }
            }
        }
        c.text_c(
            gpu,
            BACK[0] + BACK[2] / 2.0,
            BAR_Y + 8.0,
            1.0,
            look.value,
            "Back",
        );
        c.text_c(gpu, 190.0, BAR_Y + 8.0, 1.0, look.label, "D-Pad: tabs");
    }

    /// What the tab's settings do (top).
    pub(super) fn draw_settings_top(&self, c: &mut Canvas, gpu: &mut Gpu, look: &Look) {
        let lines: &[(&str, bool)] = match self.settings_tab {
            0 => &[
                ("Game", true),
                ("Aggressive / Density / Sensitivity: as the", true),
                ("  original's game options (tap the bars)", false),
                ("", false),
                ("Crouch: ZL (New 3DS); Hold L (tap L jumps);", true),
                ("  D-Pad Up (walk, run, crouch in turn)", false),
                ("Gyro aim: how far turning the 3DS aims", true),
                ("  while crouched (stealth).", false),
                ("", false),
                ("Blood / Death animation", true),
                ("  Off: no blood sprays or trails; when", false),
                ("  you die, a black screen and the prompt.", false),
            ],
            1 => &[
                ("Hunt", true),
                ("Mobile map: the phone game's map look", true),
                ("Teleport: tap the map in a hunt", true),
                ("Multi ammo: lethal and tranquilizer rounds", true),
                ("Adv observer: teleport, fly, stop time", true),
                ("Hints: the controls shown in a hunt", true),
                ("All dinos: add-ons that roam are listed", true),
                ("", false),
                ("Run speed, ambient animals, 3DS friendly,", false),
                ("stealth and fleeing are set before each", false),
                ("hunt, on its options page.", false),
            ],
            _ => &[
                ("System", true),
                ("DHC: the DinoHunt Corp terminal look", true),
                ("  for the hunt setup.", false),
                ("Test points: unlimited points; the save", true),
                ("  keeps its own score.", false),
                ("Clear trophies: tap twice to remove every", true),
                ("  unlocked trophy.", false),
            ],
        };
        for (i, &(l, head)) in lines.iter().enumerate() {
            let col = match (i, head) {
                (0, _) => look.title,
                (_, true) => look.label,
                _ => look.value,
            };
            c.text(gpu, 12.0, 14.0 + 14.0 * i as f32, 1.0, col, l);
        }
    }
}
