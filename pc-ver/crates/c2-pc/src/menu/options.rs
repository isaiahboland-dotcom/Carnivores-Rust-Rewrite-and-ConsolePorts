//! The options screen: game, video, mouse and key rows.

use super::*;

/// Options panel rows.
#[derive(Debug, Clone, Copy)]
pub(super) enum Opt {
    Slider(&'static str, fn(&mut Save) -> &mut i32),
    Toggle(&'static str, [&'static str; 2], fn(&mut Save) -> &mut i32),
    /// A slider for a PC-only setting (`settings::PcSettings`).
    PcSlider(&'static str, fn(&mut PcSettings) -> &mut i32),
    /// An on/off PC-only setting.
    PcToggle(&'static str, fn(&mut PcSettings) -> &mut i32),
    /// Removes every trophy not locked (asked first: a second click).
    ClearTrophies,
}

pub(super) const GAME_OPTS: [Opt; 15] = [
    Opt::Slider("Aggressiveness", |s| &mut s.options.agres),
    Opt::Slider("Density", |s| &mut s.options.dens),
    Opt::Slider("Sensitivity", |s| &mut s.options.sens),
    Opt::Toggle("Measurement", ["Metric", "US"], |s| &mut s.options.sys),
    Opt::PcToggle("Blood", |p| &mut p.blood),
    Opt::PcToggle("Death animation", |p| &mut p.death_anim),
    Opt::PcToggle("All dinos huntable", |p| &mut p.all_huntable),
    Opt::PcSlider("Total number of secret dinos on map", |p| {
        &mut p.secret_dinos
    }),
    Opt::PcToggle("Dynamic time", |p| &mut p.dynamic_time),
    Opt::PcToggle("Map puts the gun away", |p| &mut p.map_holsters),
    Opt::PcToggle("Multi ammo", |p| &mut p.multi_ammo),
    Opt::PcToggle("Advanced observer mode", |p| &mut p.advanced_observer),
    Opt::PcSlider("Survival starting budget", |p| &mut p.survival_budget),
    Opt::PcToggle("Enable hardcore survival mode", |p| {
        &mut p.hardcore_survival
    }),
    Opt::ClearTrophies,
];

/// Experimental on Android: phones vary far more than PCs.
pub(super) const HIGH_END_VIEW: &str = if cfg!(target_os = "android") {
    "High-end view (experimental)"
} else {
    "High-end view"
};

/// The Android-only Resolution row (the phone scales the frame up).
pub const RESOLUTION: &str = "Resolution";

#[cfg(not(target_os = "android"))]
pub(super) const VIDEO_OPTS: [Opt; 9] = [
    Opt::Slider("View range", |s| &mut s.options.view_r),
    Opt::PcSlider("Render distance", |p| &mut p.render),
    Opt::Slider("Brightness", |s| &mut s.options.brightness),
    Opt::PcSlider("Foliage distance", |p| &mut p.model_distance),
    Opt::PcToggle("Mobile radar", |p| &mut p.map_mobile),
    Opt::PcToggle("Map: where you look", |p| &mut p.map_heading),
    Opt::PcToggle("Map zoom / pan", |p| &mut p.map_zoom),
    Opt::PcToggle("Teleport (map)", |p| &mut p.teleport),
    Opt::PcToggle(HIGH_END_VIEW, |p| &mut p.high_end_view),
];

#[cfg(target_os = "android")]
pub(super) const VIDEO_OPTS: [Opt; 10] = [
    Opt::Slider("View range", |s| &mut s.options.view_r),
    Opt::PcSlider("Render distance", |p| &mut p.render),
    Opt::Slider("Brightness", |s| &mut s.options.brightness),
    Opt::PcSlider("Foliage distance", |p| &mut p.model_distance),
    Opt::PcToggle("Mobile radar", |p| &mut p.map_mobile),
    Opt::PcToggle("Map: where you look", |p| &mut p.map_heading),
    Opt::PcToggle("Map zoom / pan", |p| &mut p.map_zoom),
    Opt::PcToggle("Teleport (map)", |p| &mut p.teleport),
    Opt::PcToggle(HIGH_END_VIEW, |p| &mut p.high_end_view),
    Opt::PcSlider(RESOLUTION, |p| &mut p.render_scale),
];

pub(super) const MOUSE_OPTS: [Opt; 3] = [
    Opt::Slider("Mouse sensitivity", |s| &mut s.options.mouse_sens),
    Opt::Toggle("Reverse mouse", ["Off", "On"], |s| {
        &mut s.options.reverse_mouse
    }),
    Opt::PcToggle("Button hints", |p| &mut p.hints),
];

/// Bindable actions, in `KeyMap` order.
/// Picks one binding out of the key map.
pub(super) type KeyField = fn(&mut KeyMap) -> &mut i32;

pub(super) const ACTIONS: [(&str, KeyField); 17] = [
    ("Forward", |k| &mut k.forward),
    ("Backward", |k| &mut k.backward),
    ("Look up", |k| &mut k.up),
    ("Look down", |k| &mut k.down),
    ("Turn left", |k| &mut k.left),
    ("Turn right", |k| &mut k.right),
    ("Fire", |k| &mut k.fire),
    ("Get weapon", |k| &mut k.show),
    ("Step left", |k| &mut k.strafe_left),
    ("Step right", |k| &mut k.strafe_right),
    ("Strafe", |k| &mut k.strafe),
    ("Jump", |k| &mut k.jump),
    ("Run", |k| &mut k.run),
    ("Crouch", |k| &mut k.crouch),
    ("Call", |k| &mut k.call),
    ("Change call", |k| &mut k.change_call),
    ("Binoculars", |k| &mut k.binoculars),
];

impl Menu {
    /// The option rows of each panel with their rectangles.
    pub(super) fn option_rows(&self) -> Vec<(Opt, Rect)> {
        let mut out = Vec::new();
        let (game, video) = (
            option_scroll::shown(&GAME_OPTS),
            option_scroll::shown(&VIDEO_OPTS),
        );
        for (panel, opts) in [(GAME_PANEL, &game[..]), (VIDEO_PANEL, &video[..])] {
            let first = if panel == GAME_PANEL {
                self.game_scroll.first
            } else {
                0
            };
            out.extend(option_scroll::visible_rows(opts, panel, first));
        }
        let y0 = KEYS_PANEL.1 + ACTIONS.len() as f32 * 20.0 + 12.0;
        for (i, o) in MOUSE_OPTS.iter().enumerate() {
            out.push((
                *o,
                (KEYS_PANEL.0, y0 + i as f32 * OPT_ROW, KEYS_PANEL.2, OPT_ROW),
            ));
        }
        out
    }

    pub(super) fn slider_value(&self, r: Rect) -> Option<i32> {
        let (x, _) = self.cursor?;
        let x0 = r.0 + r.2 - 128.0 - 8.0;
        Some((((x - x0 - 4.0) / 120.0) * 255.0).clamp(0.0, 255.0) as i32)
    }

    pub(super) fn update_options(&mut self, input: &Input, region: u8) {
        if let Some(a) = self.binding {
            let vk = if input.clicked && input.cursor.is_some() {
                Some(keys::VK_LBUTTON)
            } else if input.right_clicked {
                Some(keys::VK_RBUTTON)
            } else {
                input
                    .last_pressed
                    .filter(|&k| k != KeyCode::Escape)
                    .and_then(keys::vk_of)
            };
            if input.pressed(KeyCode::Escape) {
                self.binding = None;
            } else if let Some(vk) = vk {
                if let Some(s) = self.me_mut() {
                    *(ACTIONS[a].1)(&mut s.options.keys) = vk;
                }
                self.binding = None;
            }
            return;
        }
        if self.kit.is_some() {
            self.update_kit_options(input);
            return;
        }
        if input.pressed(KeyCode::Escape) || (input.clicked && region == 4) {
            self.store_me();
            self.pc.store();
            self.screen = Screen::Main;
            return;
        }
        if !input.mouse_down {
            self.drag = None;
        }
        self.scroll_game_options(input, false);
        let rows = self.option_rows();
        if input.clicked {
            let on_clear = rows
                .iter()
                .any(|(o, r)| matches!(o, Opt::ClearTrophies) && inside(self.cursor, *r));
            if on_clear {
                if self.clear_armed {
                    if let Some(s) = self.me_mut() {
                        c2_core::profile::clear_unlocked(s);
                    }
                    self.store_me();
                }
                self.clear_armed = !self.clear_armed;
                return;
            }
            self.clear_armed = false;
            for (i, _) in ACTIONS.iter().enumerate() {
                let r = (
                    KEYS_PANEL.0,
                    KEYS_PANEL.1 + i as f32 * 20.0,
                    KEYS_PANEL.2,
                    20.0,
                );
                if inside(self.cursor, r) {
                    self.binding = Some(i);
                    return;
                }
            }
            for (o, r) in &rows {
                if !inside(self.cursor, *r) {
                    continue;
                }
                match o {
                    Opt::Slider(..) | Opt::PcSlider(..) => self.drag = Some(*o),
                    Opt::Toggle(_, _, f) => {
                        if let Some(s) = self.me_mut() {
                            let v = f(s);
                            *v = i32::from(*v == 0);
                        }
                    }
                    Opt::PcToggle(_, f) => {
                        let v = f(&mut self.pc);
                        *v = i32::from(*v == 0);
                        self.offer_animals();
                    }
                    Opt::ClearTrophies => {}
                }
            }
        }
        if let Some(Opt::PcSlider(name, f)) = self.drag {
            let r = rows.iter().find_map(|(o, r)| match o {
                Opt::PcSlider(n, _) if *n == name => Some(*r),
                _ => None,
            });
            if let Some(v) = r.and_then(|r| self.slider_value(r)) {
                *f(&mut self.pc) = if name == "Survival starting budget" {
                    v * 10
                } else {
                    v
                };
            }
        }
        if let Some(Opt::Slider(name, f)) = self.drag {
            let r = rows.iter().find_map(|(o, r)| match o {
                Opt::Slider(n, _) if *n == name => Some(*r),
                _ => None,
            });
            if let Some(v) = r.and_then(|r| self.slider_value(r)) {
                if let Some(s) = self.me_mut() {
                    *f(s) = v;
                }
            }
        }
    }

    pub(super) fn draw_options(&self, out: &mut Vec<Sprite>, region: u8) {
        self.board(out, &self.options, &[region]);
        if self.kit.is_some() {
            self.draw_kit_options(out);
            return;
        }
        let Some(s) = self.me() else { return };
        let mut copy = s.clone();
        self.draw_game_scroll(out, false);
        for (o, r) in self.option_rows() {
            let hover = inside(self.cursor, r);
            let color = if hover { SAND } else { [0.85, 0.7, 0.5, 1.0] };
            match o {
                Opt::Slider(name, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = *f(&mut copy);
                    let x0 = r.0 + r.2 - 128.0 - 8.0;
                    self.image_native(out, self.slider, x0, r.1 + 4.0);
                    let kx = x0 + 4.0 + v.clamp(0, 255) as f32 * 120.0 / 255.0 - 4.0;
                    self.image_native(out, self.knob, kx, r.1 + 5.0);
                }
                Opt::PcSlider(name, f) => {
                    let budget = name == "Survival starting budget";
                    let secret = budget || name == "Total number of secret dinos on map";
                    self.text(
                        out,
                        r.0,
                        r.1 + 4.0,
                        if secret { 10.0 } else { 12.0 },
                        color,
                        name,
                    );
                    let v = *f(&mut self.pc.clone());
                    let slider_y = r.1 + if secret { 20.0 } else { 4.0 };
                    if secret {
                        let value = if budget && v == 0 {
                            "Automatic".into()
                        } else if v == 0 {
                            "Off (selected dinos only)".into()
                        } else {
                            v.to_string()
                        };
                        self.text(out, r.0, slider_y, 10.0, SAND, &value);
                    }
                    let x0 = r.0 + r.2 - 128.0 - 8.0;
                    self.image_native(out, self.slider, x0, slider_y);
                    let v = if budget { v / 10 } else { v };
                    let kx = x0 + 4.0 + v.clamp(0, 255) as f32 * 120.0 / 255.0 - 4.0;
                    self.image_native(out, self.knob, kx, slider_y + 1.0);
                }
                Opt::Toggle(name, labels, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = labels[usize::from(*f(&mut copy) != 0)];
                    self.text(out, r.0 + SLIDER_X, r.1 + 4.0, 12.0, SAND, v);
                }
                Opt::PcToggle(name, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = ["Off", "On"][usize::from(*f(&mut self.pc.clone()) != 0)];
                    self.text(out, r.0 + SLIDER_X, r.1 + 4.0, 12.0, SAND, v);
                }
                Opt::ClearTrophies => {
                    let n = copy
                        .bodies
                        .iter()
                        .filter(|b| b.ctype != 0 && !b.locked())
                        .count();
                    let t = if self.clear_armed {
                        format!("Click again: remove {n}")
                    } else {
                        "Remove unlocked trophies".into()
                    };
                    let c = if self.clear_armed { PICKED } else { color };
                    self.text(out, r.0, r.1 + 4.0, 12.0, c, &t);
                }
            }
        }
        for (i, (name, f)) in ACTIONS.iter().enumerate() {
            let y = KEYS_PANEL.1 + i as f32 * 20.0;
            let r = (KEYS_PANEL.0, y, KEYS_PANEL.2, 20.0);
            let hover = inside(self.cursor, r);
            let color = if hover { SAND } else { [0.85, 0.7, 0.5, 1.0] };
            self.text(out, r.0, y + 4.0, 12.0, color, name);
            let key = if self.binding == Some(i) {
                "press a key...".to_string()
            } else {
                keys::name(*f(&mut copy.options.keys))
            };
            let kc = if self.binding == Some(i) {
                PICKED
            } else {
                SAND
            };
            self.text(out, r.0 + 170.0, y + 4.0, 12.0, kc, &key);
        }
    }
}
