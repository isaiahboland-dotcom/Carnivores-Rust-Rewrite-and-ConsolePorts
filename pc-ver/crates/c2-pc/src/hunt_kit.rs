//! The hunt's screen as the phone game's, for a game built from the player's
//! APK (`HUNTDAT/MOBILE`, see `menu::kit`): its binocular and scope masks
//! (half pictures, mirrored), the map's borders and dots, the round weapon,
//! binocular and call buttons, the notification bar and the pop-up panel for
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
    /// The round buttons: per weapon (by `_RES.TXT` index), binoculars,
    /// call.
    weapons: Vec<Option<usize>>,
    bin_button: Option<usize>,
    call_button: Option<usize>,
    notify: Option<usize>,
    popup: Option<usize>,
}

const BLACK: [f32; 4] = [0.04, 0.04, 0.04, 1.0];

impl HuntKit {
    pub fn load(huntdat: &Path, hud: &mut Hud, gpu: &Gpu, weapons: usize) -> Option<Self> {
        let kit = Kit::load(huntdat, hud, gpu)?;
        let mut img = |n: &str| kit.image(n, hud, gpu);
        // The kit's weapon pictures: the PC's six, the camera, the revolver
        // (the `_RES.TXT` the conversion writes has the revolver 7th).
        let weapons = (0..weapons)
            .map(|i| {
                img(&format!(
                    "game_weapon_button_{}",
                    if i < 6 { i + 1 } else { i + 2 }
                ))
            })
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
            weapons,
            bin_button: img("game_binoculars"),
            call_button: img("game_call"),
            notify: img("notification_bar"),
            popup: img("base_popup_background"),
            kit,
        })
    }

    fn size(hud: &Hud, id: Option<usize>) -> Option<(usize, f32, f32)> {
        let id = id?;
        hud.image(id).map(|im| (id, im.w as f32, im.h as f32))
    }

    /// A half mask drawn and mirrored across the screen's middle, as tall
    /// as the screen; black beyond it on a wider screen.
    fn mask(hud: &Hud, id: Option<usize>, screen: (u32, u32), out: &mut Vec<Sprite>) -> f32 {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let Some((id, w, h)) = Self::size(hud, id) else {
            return 1.0;
        };
        let k = sh / h;
        let (half, cx) = (w * k, sw / 2.0);
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

    /// Looking through the binoculars.
    pub fn binoculars(&self, hud: &Hud, screen: (u32, u32), out: &mut Vec<Sprite>) {
        let k = Self::mask(hud, self.binoculars, screen, out);
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
        Self::mask(hud, self.scope, screen, out);
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

    /// The round buttons down the right: binoculars, call, the weapon in
    /// hand (lit when in use).
    pub fn buttons(&self, hud: &Hud, screen: (u32, u32), st: Buttons, out: &mut Vec<Sprite>) {
        let px = screen.1 as f32 / 9.0;
        let gap = px * 0.15;
        let x = screen.0 as f32 - px - gap * 2.0;
        let mut y = screen.1 as f32 - px - screen.1 as f32 / 12.0;
        let weapon = st
            .weapon
            .and_then(|i| self.weapons.get(i).copied().flatten());
        if let (Some(f), Some(n), true) = (&self.kit.body, &st.ammo, weapon.is_some()) {
            let tpx = (px * 0.4).round();
            let at = (x - f.width(tpx, n) - gap, y + (px - tpx) / 2.0);
            let o = (1.0, 0.0, 0.0);
            f.sprites(
                out,
                o,
                (at.0 + 1.0, at.1 + 1.0),
                tpx,
                [0.0, 0.0, 0.0, 0.8],
                n,
            );
            f.sprites(out, o, at, tpx, LIGHT, n);
        }
        for (id, on) in [
            (weapon, st.weapon_out),
            (self.call_button, st.calls),
            (self.bin_button, st.binoculars),
        ] {
            let Some((id, _, _)) = Self::size(hud, id) else {
                continue;
            };
            let a = if on { 1.0 } else { 0.55 };
            out.push(Sprite {
                size: Some([px, px]),
                color: [a, a, a, a],
                ..Sprite::new(id, x, y, 1.0)
            });
            y -= px + gap;
        }
    }

    /// A message on the phone's notification bar, centred near the top.
    pub fn notify(&self, hud: &Hud, screen: (u32, u32), text: &str, out: &mut Vec<Sprite>) {
        let Some(f) = &self.kit.body else { return };
        let px = (screen.1 as f32 / 28.0).round();
        let tw = f.width(px, text);
        let (w, h) = (tw + px * 3.0, px * 2.2);
        let (x, y) = ((screen.0 as f32 - w) / 2.0, screen.1 as f32 * 0.08);
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
        match Self::size(hud, self.popup) {
            Some((id, _, _)) => {
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
            None => out.extend(hud.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.8])),
        }
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
}

/// What the round buttons show.
#[derive(Clone)]
pub struct Buttons {
    pub weapon: Option<usize>,
    pub weapon_out: bool,
    /// Rounds left, beside the weapon (the phone has no bullet pictures).
    pub ammo: Option<String>,
    pub binoculars: bool,
    pub calls: bool,
}
