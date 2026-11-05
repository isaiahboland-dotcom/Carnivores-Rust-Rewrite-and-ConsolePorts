//! The hunt's screen as the phone game's, for a game built from the player's
//! APK (`HUNTDAT/MOBILE`, see `menu::kit`): its binocular and scope masks
//! (half pictures, mirrored), the map's borders and dots, the compass,
//! the notification bar and the pop-up panel for
//! the pause and exit questions, in the game's lettering. A PC game folder
//! has no `MOBILE` and keeps the original's pictures.

use crate::gpu::Gpu;
use crate::hud::{Hud, Sprite};
use crate::menu::kit::{Kit, GOLD, LIGHT};
use std::path::Path;

pub struct HuntKit {
    kit: Kit,
    binoculars: Option<usize>,
    cross: Option<usize>,
    scope: Option<usize>,
    /// Left, right, upper, lower.
    borders: [Option<usize>; 4],
    me: Option<usize>,
    /// The weapons' pictures (the picker's), as the menu's.
    pictures: Vec<Option<usize>>,
    notify: Option<usize>,
    popup: Option<usize>,
    /// The kill notice's panel (a quarter) and the trophy plaque's.
    stats: Option<usize>,
    plaque: Option<usize>,
    /// The hexagons: the hunt menu's, the pickers' and the picked one's
    /// ring.
    menu_cell: Option<usize>,
    cell: Option<usize>,
    active: Option<usize>,
    /// The compass's face, and the area's painted map (grid and all).
    compass: Option<usize>,
    pub map: Option<usize>,
    /// A round button (our own drawing, tinted): night vision, flashlight.
    disc: usize,
}

/// The phone's yellow (the hunt's words, buttons, rings).
const YELLOW: [f32; 4] = [0.95, 0.86, 0.25, 1.0];

const BLACK: [f32; 4] = [0.04, 0.04, 0.04, 1.0];

impl HuntKit {
    /// `map`: the area's painted map (`AREA1_DAY`), when it has one.
    pub fn load(
        huntdat: &Path,
        hud: &mut Hud,
        gpu: &Gpu,
        weapons: usize,
        map: Option<&str>,
    ) -> Option<Self> {
        let kit = Kit::load(huntdat, hud, gpu)?;
        let disc = hud.add_image_filtered(gpu, DISC as u32, DISC as u32, &disc(), true);
        let compass = kit.picture("COMPAS.PNG", hud, gpu);
        let map = map.and_then(|m| kit.picture(&format!("MAPS/{m}.PNG"), hud, gpu));
        let mut img = |n: &str| kit.image(n, hud, gpu);
        // The kit's weapon pictures: the PC's six, the camera, the revolver
        // (the `_RES.TXT` the conversion writes has the revolver 7th).
        let number = |i: usize| if i < 6 { i + 1 } else { i + 2 };
        let pictures = (0..weapons)
            .map(|i| img(&format!("button_weapon_active_{}", number(i))))
            .collect();
        Some(Self {
            binoculars: img("binoculars_sight"),
            cross: img("binoculars_crosshair"),
            scope: img("optic_sight"),
            borders: [
                img("map_left_border"),
                img("map_right_border"),
                img("map_upper_border"),
                img("map_lower_border"),
            ],
            me: img("player_on_map"),
            pictures,
            notify: img("notification_bar"),
            popup: img("base_popup_background"),
            stats: img("game_stats_window"),
            plaque: img("trophy_info_window"),
            menu_cell: img("game_menu_cell"),
            cell: img("game_cell"),
            active: img("game_active_cell"),
            compass,
            map,
            disc,
            kit,
        })
    }

    fn size(hud: &Hud, id: Option<usize>) -> Option<(usize, f32, f32)> {
        let id = id?;
        hud.image(id).map(|im| (id, im.w as f32, im.h as f32))
    }

    /// A half mask drawn and mirrored across the screen's middle, as tall
    /// as the screen; black beyond it on a wider screen. `ends`: the
    /// picture is the mask's ends (the binoculars' goggles), set at the
    /// screen's sides with its inner edge stretched to the middle.
    fn mask(
        hud: &Hud,
        id: Option<usize>,
        screen: (u32, u32),
        ends: bool,
        out: &mut Vec<Sprite>,
    ) -> f32 {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let Some((id, w, h)) = Self::size(hud, id) else {
            return 1.0;
        };
        let k = sh / h;
        let cx = sw / 2.0;
        if ends {
            let end = (w * k).min(cx);
            let (e0, e1) = ((w - 2.0) / w, (w - 1.0) / w);
            for (x, ww, uv) in [
                (0.0, end, [0.0, 0.0, 1.0, 1.0]),
                (end - 0.5, cx - end + 1.0, [e0, 0.0, e1, 1.0]),
                (cx - 0.5, cx - end + 1.0, [e1, 0.0, e0, 1.0]),
                (sw - end, end, [1.0, 0.0, 0.0, 1.0]),
            ] {
                out.push(Sprite {
                    uv,
                    size: Some([ww, sh]),
                    ..Sprite::new(id, x, 0.0, 1.0)
                });
            }
            return k;
        }
        let half = w * k;
        for (x, uv) in [
            (cx - half, [0.0, 0.0, 1.0, 1.0]),
            (cx, [1.0, 0.0, 0.0, 1.0]),
        ] {
            out.push(Sprite {
                uv,
                size: Some([half, sh]),
                ..Sprite::new(id, x, 0.0, 1.0)
            });
        }
        // Wider screens: the mask's outer edge stretched to the sides.
        let side = (cx - half).max(0.0);
        if side > 0.0 {
            let (e0, e1) = (2.0 / w, 3.0 / w);
            for (x, uv) in [(0.0, [e0, 0.0, e1, 1.0]), (cx + half, [e1, 0.0, e0, 1.0])] {
                out.push(Sprite {
                    uv,
                    size: Some([side + 1.0, sh]),
                    ..Sprite::new(id, x - 0.5, 0.0, 1.0)
                });
            }
        }
        k
    }

    pub fn has_binocular_mask(&self) -> bool {
        self.binoculars.is_some()
    }

    /// Looking through the binoculars.
    pub fn binoculars(&self, hud: &Hud, screen: (u32, u32), out: &mut Vec<Sprite>) {
        let k = Self::mask(hud, self.binoculars, screen, true, out);
        if let Some((id, w, h)) = Self::size(hud, self.cross) {
            let (x, y) = (
                (screen.0 as f32 - w * k) / 2.0,
                (screen.1 as f32 - h * k) / 2.0,
            );
            out.push(Sprite::new(id, x, y, k));
        }
    }

    /// Looking through a scope.
    pub fn scope(&self, hud: &Hud, screen: (u32, u32), out: &mut Vec<Sprite>) {
        Self::mask(hud, self.scope, screen, false, out);
    }

    /// The map's frame around the `side`-pixel area picture at `x`, `y`.
    pub fn map_frame(&self, hud: &Hud, (x, y, side): (f32, f32, f32), out: &mut Vec<Sprite>) {
        let t = side / 30.0;
        let [l, r, u, d] = self.borders;
        let parts = [
            (l, x - t, y, t, side),
            (r, x + side, y, t, side),
            (u, x - t, y - 1.5 * t, side + 2.0 * t, 1.5 * t),
            (d, x - t, y + side, side + 2.0 * t, 1.5 * t),
        ];
        for (id, x, y, w, h) in parts {
            match id {
                Some(id) => out.push(Sprite {
                    size: Some([w, h]),
                    ..Sprite::new(id, x, y, 1.0)
                }),
                None => out.extend(hud.rect(x, y, w, h, BLACK)),
            }
        }
    }

    /// The hunter on the map: the phone's marker, `px` across, centred.
    pub fn map_me(&self, (x, y, px): (f32, f32, f32), out: &mut Vec<Sprite>) -> bool {
        let Some(id) = self.me else {
            return false;
        };
        out.push(Sprite {
            size: Some([px, px]),
            ..Sprite::new(id, x - px / 2.0, y - px / 2.0, 1.0)
        });
        true
    }

    /// Compact map compass and ammunition count for keyboard/controller play.
    pub fn buttons(&self, hud: &Hud, screen: (u32, u32), st: Buttons, out: &mut Vec<Sprite>) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        if let (Some(f), Some(n)) = (&self.kit.body, &st.ammo) {
            let px = (sh * 0.035).round();
            Self::outlined(f, out, (sw - f.width(px, n) - 12.0, 12.0), px, LIGHT, n);
        }
        // The compass: its face seen from above the hunter, the needle
        // where he looks.
        let (rw, rh) = (sh * 0.16, sh * 0.095);
        let (cx, cy) = (12.0 + rw / 2.0, 12.0 + sh * 0.03 + rh / 2.0);
        if let Some((id, _, _)) = Self::size(hud, self.compass) {
            out.push(Sprite {
                uv: [0.0, 0.0, 1.0, 0.86],
                size: Some([rw, rh]),
                ..Sprite::new(id, cx - rw / 2.0, cy - rh / 2.0, 1.0)
            });
            let (s, c) = st.heading.sin_cos();
            let dot = (sh * 0.008).max(2.0);
            for (grow, color) in [(1.6, [0.05, 0.1, 0.2, 1.0]), (1.0, [0.2, 0.85, 1.0, 1.0])] {
                for i in 0..10 {
                    let t = i as f32 / 9.0 * 0.42;
                    let (x, y) = (cx + s * rw * t, cy - c * rh * t);
                    let r = dot * grow * (1.0 - t);
                    out.extend(hud.rect(x - r / 2.0, y - r / 2.0, r, r, color));
                }
            }
        }
        if let Some(f) = &self.kit.heading {
            let px = (sh * 0.025).round();
            let t = "AREA MAP";
            Self::outlined(f, out, (cx - f.width(px, t) / 2.0, 12.0), px, YELLOW, t);
            // The menu tab (Esc).
            let (tw, th) = (sh * 0.15, sh * 0.046);
            out.extend(hud.rect((sw - tw) / 2.0, 0.0, tw, th, [0.12, 0.14, 0.17, 0.6]));
            let t = "menu";
            let px = (th * 0.62).round();
            Self::outlined(
                f,
                out,
                ((sw - f.width(px, t)) / 2.0, (th - px) / 2.0),
                px,
                YELLOW,
                t,
            );
        }
    }

    /// A round light button at (centre x, y, diameter): yellow when on, dim
    /// when off, grey while the other light is on. Its word sits in the
    /// middle.
    pub fn light_button(
        &self,
        hud: &Hud,
        (x, y, d): (f32, f32, f32),
        word: &str,
        on: bool,
        other_on: bool,
        out: &mut Vec<Sprite>,
    ) {
        let tint = if on {
            YELLOW
        } else if other_on {
            [0.42, 0.42, 0.4, 0.6]
        } else {
            [YELLOW[0], YELLOW[1], YELLOW[2], 0.55]
        };
        if hud.image(self.disc).is_some() {
            out.push(Sprite {
                size: Some([d, d]),
                color: tint,
                ..Sprite::new(self.disc, x - d / 2.0, y - d / 2.0, 1.0)
            });
        }
        let ink = [BLACK[0], BLACK[1], BLACK[2], if on { 1.0 } else { 0.75 }];
        if let Some(f) = self.kit.body.as_ref().or(self.kit.heading.as_ref()) {
            let px = (d * 0.36).round();
            let w = f.width(px, word);
            f.sprites(
                out,
                (1.0, 0.0, 0.0),
                (x - w / 2.0, y - px * 0.55),
                px,
                ink,
                word,
            );
        } else {
            let px = d * 0.3;
            let w = hud.label_width(px, word);
            out.extend(hud.label(x - w / 2.0, y - px / 2.0, px, ink, word));
        }
    }

    /// A kit sprite as a HUD image.
    pub fn image(&self, name: &str, hud: &mut Hud, gpu: &Gpu) -> Option<usize> {
        self.kit.image(name, hud, gpu)
    }

    /// Weapon `i`'s picture (`_RES.TXT` order).
    pub fn weapon_picture(&self, i: usize) -> Option<usize> {
        self.pictures.get(i).copied().flatten()
    }

    /// Words with a dark edge, as the phone's yellow ones.
    fn outlined(
        f: &crate::menu::kit::KitFont,
        out: &mut Vec<Sprite>,
        (x, y): (f32, f32),
        px: f32,
        color: [f32; 4],
        text: &str,
    ) {
        let o = (1.0, 0.0, 0.0);
        let dark = [0.1, 0.08, 0.0, color[3]];
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)] {
            f.sprites(out, o, (x + dx, y + dy), px, dark, text);
        }
        f.sprites(out, o, (x, y), px, color, text);
    }

    /// Hexagons in rows near the top (the weapon and call pickers): each
    /// picture, the picked one ringed.
    pub fn hexes(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        items: &[(Option<usize>, bool)],
        out: &mut Vec<Sprite>,
    ) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let n = items.len();
        if n == 0 {
            return;
        }
        let (w, h) = (sh * 0.223, sh * 0.25);
        let (sx, sy) = (sh * 0.247, sh * 0.212);
        let per = n.div_ceil(2).max(2).min(n);
        let rows = n.div_ceil(per);
        let span = (per - 1) as f32 * sx + w + if rows > 1 { sx / 2.0 } else { 0.0 };
        let x0 = (sw - span) / 2.0;
        for (i, &(pic, on)) in items.iter().enumerate() {
            let (r, c) = (i / per, i % per);
            let x = x0 + c as f32 * sx + if r % 2 == 1 { sx / 2.0 } else { 0.0 };
            let y = sh * 0.098 + r as f32 * sy;
            if let Some((id, _, _)) = Self::size(hud, self.cell) {
                out.push(Sprite {
                    size: Some([w, h]),
                    color: [1.0, 1.0, 1.0, 0.35],
                    ..Sprite::new(id, x, y, 1.0)
                });
            }
            if let Some((id, iw, ih)) = Self::size(hud, pic) {
                let k = (w * 0.86 / iw).min(h * 0.8 / ih);
                let (dw, dh) = (iw * k, ih * k);
                out.push(Sprite {
                    size: Some([dw, dh]),
                    ..Sprite::new(id, x + (w - dw) / 2.0, y + (h - dh) / 2.0, 1.0)
                });
            }
            if on {
                if let Some((id, _, _)) = Self::size(hud, self.active) {
                    out.push(Sprite {
                        size: Some([w, h]),
                        ..Sprite::new(id, x, y, 1.0)
                    });
                }
            }
        }
    }

    /// The hunt's menu (Esc): hexagons along the top, each a choice and
    /// its key.
    pub fn menu(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        items: &[(&str, &str)],
        out: &mut Vec<Sprite>,
    ) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let (w, h) = (sh * 0.2, sh * 0.174);
        // The phone's places: three along the top, the way out between and
        // under them.
        let places = [(-0.174, 0.056), (0.17, 0.056), (0.0, 0.168), (0.514, 0.056)];
        for (&(label, key), (dx, y)) in items.iter().zip(places) {
            let (x, y) = (sw / 2.0 + dx * sh - w / 2.0, y * sh - h / 2.0);
            if let Some((id, _, _)) = Self::size(hud, self.menu_cell) {
                out.push(Sprite {
                    size: Some([w, h]),
                    color: [1.0, 1.0, 1.0, 0.45],
                    ..Sprite::new(id, x, y, 1.0)
                });
            }
            let Some(f) = &self.kit.heading else { continue };
            let px = (sh * 0.034).round();
            let ty = y + (h - px) / 2.0 + if y < 0.0 { -y / 2.0 } else { 0.0 };
            Self::outlined(
                f,
                out,
                (x + (w - f.width(px, label)) / 2.0, ty),
                px,
                YELLOW,
                label,
            );
            if let Some(b) = &self.kit.body {
                let kp = (px * 0.6).round();
                let at = (x + (w - b.width(kp, key)) / 2.0, ty + px * 1.1);
                b.sprites(out, (1.0, 0.0, 0.0), at, kp, LIGHT, key);
            }
        }
    }

    /// The map's panel: dark, "AREA MAP" and the area's name over the map,
    /// RELOCATE under it when the hunter can move (Enter); the map's
    /// square (x, y, side).
    pub fn map_panel(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        area: &str,
        out: &mut Vec<Sprite>,
    ) -> (f32, f32, f32) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let p = sh * 0.8;
        let (px0, py0) = ((sw - p) / 2.0, sh * 0.03);
        match Self::size(hud, self.plaque) {
            Some((id, _, _)) => out.push(Sprite {
                size: Some([p, p]),
                color: [1.0, 1.0, 1.0, 0.7],
                ..Sprite::new(id, px0, py0, 1.0)
            }),
            None => out.extend(hud.rect(px0, py0, p, p, [0.0, 0.0, 0.0, 0.7])),
        }
        let side = sh * 0.74;
        let (mx, my) = ((sw - side) / 2.0, py0 + sh * 0.043);
        if let Some(f) = &self.kit.heading {
            let px = (sh * 0.03).round();
            let y = py0 + sh * 0.01;
            Self::outlined(f, out, (mx + sh * 0.01, y), px, YELLOW, "AREA MAP");
            let name = area.to_uppercase();
            let x = mx + side - sh * 0.01 - f.width(px, &name);
            Self::outlined(f, out, (x, y), px, YELLOW, &name);
        }
        (mx, my, side)
    }

    /// A message on the phone's notification bar, centred near the top.
    pub fn notify(&self, hud: &Hud, screen: (u32, u32), text: &str, out: &mut Vec<Sprite>) {
        self.notify_at(hud, screen, text, screen.1 as f32 * 0.08, out);
    }

    /// The notification bar with its top at `y`.
    pub fn notify_at(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        text: &str,
        y: f32,
        out: &mut Vec<Sprite>,
    ) {
        let Some(f) = &self.kit.body else { return };
        let px = (screen.1 as f32 / 28.0).round();
        let tw = f.width(px, text);
        let (w, h) = (tw + px * 3.0, px * 2.2);
        let x = (screen.0 as f32 - w) / 2.0;
        match Self::size(hud, self.notify) {
            Some((id, _, _)) => out.push(Sprite {
                size: Some([w, h]),
                color: [1.0, 1.0, 1.0, 0.9],
                ..Sprite::new(id, x, y, 1.0)
            }),
            None => out.extend(hud.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.7])),
        }
        let o = (1.0, 0.0, 0.0);
        f.sprites(
            out,
            o,
            ((screen.0 as f32 - tw) / 2.0, y + (h - px) / 2.0),
            px,
            LIGHT,
            text,
        );
    }

    /// The pop-up panel with a heading (pause, leave the hunt); returns its
    /// bottom.
    pub fn popup(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        heading: &str,
        out: &mut Vec<Sprite>,
    ) -> f32 {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let (w, h) = (sh * 0.7, sh * 0.7 * 176.0 / 280.0);
        let (x, y) = ((sw - w) / 2.0, (sh - h) / 2.0);
        // The picture is the panel's top left quarter.
        Self::quarters(hud, self.popup, (x, y, w, h), [0.0, 0.0, 0.0, 0.8], out);
        if let Some(f) = self.kit.title.as_ref().or(self.kit.body.as_ref()) {
            let px = (sh / 16.0).round();
            let tw = f.width(px, heading);
            let o = (1.0, 0.0, 0.0);
            let at = ((sw - tw) / 2.0, y + (h - px) / 2.0);
            f.sprites(
                out,
                o,
                (at.0 + 2.0, at.1 + 2.0),
                px,
                [0.0, 0.0, 0.0, 0.8],
                heading,
            );
            f.sprites(out, o, at, px, GOLD, heading);
        }
        y + h
    }

    /// A panel whose picture is its top left quarter, mirrored into the
    /// other three; `fill` without the picture.
    fn quarters(
        hud: &Hud,
        id: Option<usize>,
        (x, y, w, h): (f32, f32, f32, f32),
        fill: [f32; 4],
        out: &mut Vec<Sprite>,
    ) {
        let Some((id, _, _)) = Self::size(hud, id) else {
            out.extend(hud.rect(x, y, w, h, fill));
            return;
        };
        let (hw, hh) = (w / 2.0, h / 2.0);
        for (dx, dy, uv) in [
            (0.0, 0.0, [0.0, 0.0, 1.0, 1.0]),
            (hw, 0.0, [1.0, 0.0, 0.0, 1.0]),
            (0.0, hh, [0.0, 1.0, 1.0, 0.0]),
            (hw, hh, [1.0, 1.0, 0.0, 0.0]),
        ] {
            out.push(Sprite {
                uv,
                size: Some([hw, hh]),
                ..Sprite::new(id, x + dx, y + dy, 1.0)
            });
        }
    }

    /// The last kill, on the phone's stats panel left of the buttons:
    /// lines of text, each with its colour.
    pub fn kill_notice(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        lines: &[(String, [f32; 4])],
        out: &mut Vec<Sprite>,
    ) {
        let Some(f) = &self.kit.body else { return };
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let px = (sh / 48.0).round().max(10.0);
        let pad = px * 1.2;
        let tw = lines
            .iter()
            .map(|(l, _)| f.width(px, l))
            .fold(0.0, f32::max);
        let (w, h) = (tw + pad * 2.0, px * 1.3 * lines.len() as f32 + pad * 1.6);
        // Clear of the round buttons' column (`buttons`).
        let button = sh / 9.0;
        let (x, y) = (sw - button * 1.3 - w - button * 0.3, sh - h - sh / 12.0);
        Self::quarters(hud, self.stats, (x, y, w, h), [0.0, 0.0, 0.0, 0.7], out);
        for (i, (l, c)) in lines.iter().enumerate() {
            let ly = y + pad * 0.8 + px * 1.3 * i as f32;
            f.sprites(out, (1.0, 0.0, 0.0), (x + pad, ly), px, *c, l);
        }
    }

    /// A trophy's record on the phone's plaque, along the bottom (to the
    /// right in the trophy room): lines of label and value.
    pub fn plaque(
        &self,
        hud: &Hud,
        screen: (u32, u32),
        right: bool,
        lines: &[Vec<(&str, String)>],
        out: &mut Vec<Sprite>,
    ) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let w = if right {
            (sh * 0.75).min(sw * 0.9)
        } else {
            (sh * 0.42).min(sw * 0.45)
        };
        let h = w * 176.0 / 336.0;
        let m = sh / 40.0;
        let x = if right { sw - w - m } else { (sw - w) / 2.0 };
        let y = sh - h - m;
        match Self::size(hud, self.plaque) {
            Some((id, _, _)) => out.push(Sprite {
                size: Some([w, h]),
                color: [1.0, 1.0, 1.0, 0.8],
                ..Sprite::new(id, x, y, 1.0)
            }),
            None => out.extend(hud.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.8])),
        }
        let Some(f) = &self.kit.body else { return };
        let n = lines.len().max(1) as f32;
        let step = h * 0.8 / n;
        let px = (step * 0.7).round();
        let o = (1.0, 0.0, 0.0);
        for (i, parts) in lines.iter().enumerate() {
            let ly = y + h * 0.1 + step * i as f32 + (step - px) / 2.0;
            let mut lx = x + w * 0.06;
            for (label, value) in parts {
                for (t, c) in [(*label, LIGHT), (value.as_str(), GOLD)] {
                    f.sprites(out, o, (lx, ly), px, c, t);
                    lx += f.width(px, t);
                }
            }
        }
    }
}

/// Desktop hunt status.
#[derive(Clone)]
pub struct Buttons {
    /// Rounds left (the APK has no bullet pictures).
    pub ammo: Option<String>,
    /// Where the hunter looks (radians, 0 north), for the compass.
    pub heading: f32,
}

/// The light buttons' disc picture's side.
const DISC: usize = 64;

/// A white disc with a soft edge and a darker rim, to be tinted.
fn disc() -> Vec<u8> {
    let mut px = vec![0u8; DISC * DISC * 4];
    let r = DISC as f32 / 2.0;
    for y in 0..DISC {
        for x in 0..DISC {
            let d = (x as f32 + 0.5 - r).hypot(y as f32 + 0.5 - r);
            let alpha = (r - d).clamp(0.0, 1.0);
            let shade = if d > r - 4.0 { 150 } else { 255 };
            let i = (y * DISC + x) * 4;
            px[i..i + 4].copy_from_slice(&[shade, shade, shade, (alpha * 255.0) as u8]);
        }
    }
    px
}
