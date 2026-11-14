//! The hardcore hunt ruleset on PC (`docs/HARDCORE-HUNT.md`): picking the
//! drop-off and pickup zones on the map before the hunt, calling the ship
//! (G), the knife's dodge (F with a strafe key), and their HUD.

use super::*;
use c2_core::game::hardcore::{Deploy, Pickup, ZONE_RADIUS};
use c2_formats::sav::KeyMap;
use c2_pad::Action as Pad;

/// Ammunition drops are marked on screen within this distance.
const BEACON_RANGE: f32 = 48.0 * c2_core::CELL;
const AMBER: [f32; 4] = [1.0, 0.85, 0.1, 1.0];
const GREEN: [f32; 4] = [0.3, 1.0, 0.4, 1.0];

impl HuntView {
    /// Once a frame, after the map's own input: the zones' picking while
    /// deploying; afterwards the ship's key and the dodge.
    pub(super) fn update_hardcore(&mut self, input: &Input, k: &KeyMap, hin: &mut HuntInput) {
        if self.hunt.hardcore.is_none() || self.hunt.show_exit() || self.hunt.paused {
            return;
        }
        if self.hunt.deploying().is_some() {
            self.hunt.map_mode = true;
            let [mx, mz] = self.map_viewport.map_point();
            let extent = c2_formats::map::MAP_SIZE as f32 * c2_core::CELL;
            let (lo, hi) = (c2_core::collide::WALK_MIN, c2_core::collide::WALK_MAX);
            let (x, z) = ((mx * extent).clamp(lo, hi), (mz * extent).clamp(lo, hi));
            self.map_cursor = Some((x, z));
            hin.controls.mouse = (0.0, 0.0);
            hin.fire = false;
            if keys::pressed(input, k.fire) || input.pad.pressed(Pad::Fire) {
                match self.hunt.pick_zone(self.view.area(), x, z) {
                    Ok(()) if self.hunt.deploying().is_none() => {
                        // The hunt starts where he was dropped.
                        self.hunt.map_mode = false;
                        self.map_cursor = None;
                        self.prev_cam = self.hunt.camera().0;
                        self.message = Some(("The hunt begins. G calls the ship".into(), 4.0));
                    }
                    Ok(()) => {}
                    Err(e) => self.message = Some((e, 3.0)),
                }
            }
            return;
        }
        if self.hunt.death.is_some() {
            return;
        }
        if input.pressed(KeyCode::KeyG) && !self.key_bound(71) {
            let mut ev = Vec::new();
            if let Err(e) = self.hunt.call_ship(&mut ev) {
                self.message = Some((e, 2.5));
            }
            self.events.extend(ev);
        }
        // F is the dialect weapons' rack; with the knife out there is
        // nothing to rack.
        if input.pressed(KeyCode::KeyF) && self.hunt.knife_ready() {
            let dir = if hin.controls.strafe_left { -1.0 } else { 1.0 };
            self.hunt.dodge(dir);
        }
    }

    /// The zones and drops on the open map, in map pixels from its corner
    /// (`ox`, `oy`), and the line under it while deploying.
    pub(super) fn hardcore_map(
        &self,
        (ox, oy): (f32, f32),
        scale: f32,
    ) -> (Vec<Sprite>, Option<String>) {
        let Some(h) = self.hunt.hardcore.as_deref() else {
            return (Vec::new(), None);
        };
        let mut out = Vec::new();
        let mut dot = |x: f32, z: f32, c: [f32; 4]| {
            out.extend(self.hud.rect(
                ox + x / 1024.0 * scale,
                oy + z / 1024.0 * scale,
                2.0 * scale,
                2.0 * scale,
                c,
            ));
        };
        if let Some((x, z)) = h.zone {
            for i in 0..20 {
                let a = i as f32 / 20.0 * std::f32::consts::TAU;
                let r = ZONE_RADIUS.max(3.0 * 1024.0);
                dot(x + a.cos() * r, z + a.sin() * r, GREEN);
            }
            dot(x, z, GREEN);
        }
        if h.rules.extraction.is_some() && h.deploy != Some(Deploy::DropOff) {
            let (x, z) = h.drop_off;
            for (dx, dz) in [
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
                (1.0, -1.0),
                (0.0, 0.0),
            ] {
                dot(x + dx * 2048.0, z + dz * 2048.0, [0.4, 0.7, 1.0, 1.0]);
            }
        }
        for d in self.hunt.ammo_drops() {
            dot(d.pos[0], d.pos[2], AMBER);
        }
        let gap = h.rules.extraction.unwrap_or_default().zone_gap as f32 * 4.0;
        let status = match h.deploy {
            Some(Deploy::DropOff) => Some("Hardcore hunt: click where to be dropped off".into()),
            Some(Deploy::Pickup) => Some(format!(
                "Click the pickup zone (at least {gap:.0}m from the drop-off)"
            )),
            None => None,
        };
        (out, status)
    }

    /// The ship's status, the knife, the drops' beacons and the forfeit
    /// warning on the exit prompt.
    pub(super) fn draw_hardcore(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let Some(h) = self.hunt.hardcore.as_deref() else {
            return;
        };
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let px = 8.0 * scale.max(1.0);
        let centred = |out: &mut Vec<Sprite>, y: f32, px: f32, c: [f32; 4], t: &str| {
            let w = self.hud.label_width(px, t);
            out.extend(self.hud.label((sw - w) / 2.0, y, px, c, t));
        };
        if self.hunt.show_exit() && self.hunt.exit_forfeits() {
            let t = "Leaving without the ship forfeits this hunt: trophies and points are lost";
            centred(out, sh * 0.25, px, [1.0, 0.4, 0.3, 1.0], t);
        }
        if self.hunt.death.is_some() || h.deploy.is_some() {
            return;
        }
        let secs = |ms: i32| (ms + 999) / 1000;
        let ship = match h.pickup {
            Pickup::Idle => None,
            Pickup::Arriving(ms) => Some((format!("Ship arrives in {}s", secs(ms)), AMBER)),
            Pickup::Hovering(ms) => Some((
                format!("Ship at the pickup zone: leaves in {}s", secs(ms)),
                GREEN,
            )),
            Pickup::Cooldown(ms) => {
                Some((format!("Ship ready in {}s", secs(ms)), [0.8, 0.8, 0.8, 1.0]))
            }
        };
        if let Some((t, c)) = ship {
            centred(out, sh * 0.08, px * 1.5, c, &t);
        }
        if !self.hunt.map_mode {
            self.draw_beacons(screen, px, out);
            self.draw_knife(screen, scale, px, out);
        }
    }

    /// A marker and its distance over each drop in view and near enough.
    fn draw_beacons(&self, screen: (u32, u32), px: f32, out: &mut Vec<Sprite>) {
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let far = self.view.radius as f32 * c2_core::CELL * 1.5;
        let vp =
            crate::camera::projection(sw / sh.max(1.0), far, self.view.zoom) * self.view.cam.view();
        let p = &self.hunt.player;
        for d in self.hunt.ammo_drops() {
            let dist = ((d.pos[0] - p.x).powi(2) + (d.pos[2] - p.z).powi(2)).sqrt();
            if dist > BEACON_RANGE {
                continue;
            }
            let clip = vp * glam::Vec4::new(d.pos[0], d.pos[1] + 96.0, d.pos[2], 1.0);
            if clip.w <= 1.0 {
                continue;
            }
            let (x, y) = (
                (clip.x / clip.w * 0.5 + 0.5) * sw,
                (0.5 - clip.y / clip.w * 0.5) * sh,
            );
            let s = (px * 1.5 * (2048.0 / dist.max(256.0)).clamp(0.6, 3.0)).round();
            out.extend(
                self.hud
                    .rect(x - s / 2.0, y - s / 2.0, s, s, [0.25, 0.2, 0.05, 0.9]),
            );
            out.extend(self.hud.rect(
                x - s / 2.0 + 1.0,
                y - s / 2.0 + 1.0,
                s - 2.0,
                s - 2.0,
                AMBER,
            ));
            let t = format!("AMMO {:.0}m", dist / 64.0);
            let w = self.hud.label_width(px, &t);
            out.extend(
                self.hud
                    .label(x - w / 2.0, y - s / 2.0 - px - 2.0, px, AMBER, &t),
            );
        }
    }

    /// The knife in hand: a blade drawn in code (no game asset), thrust
    /// forward during a stab; the dodge's state beside it.
    fn draw_knife(&self, screen: (u32, u32), scale: f32, px: f32, out: &mut Vec<Sprite>) {
        let Some(k) = self.hunt.knife() else {
            return;
        };
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        let swing = k.swing_ms as f32 / c2_core::game::hardcore::SWING_MS as f32;
        // Out and back over the stab.
        let thrust = (swing * std::f32::consts::PI).sin() * 60.0 * scale;
        let (bx, by) = (sw * 0.62 - thrust, sh - 20.0 * scale - thrust * 1.4);
        let steel = [0.78, 0.8, 0.84, 1.0];
        for i in 0..28 {
            let t = i as f32;
            // The blade narrows to its point, up and to the left.
            let w = (10.0 - t * 0.33).max(1.5) * scale;
            out.extend(self.hud.rect(
                bx - t * 1.6 * scale,
                by - t * 3.0 * scale,
                w,
                3.2 * scale,
                steel,
            ));
        }
        let grip = [0.2, 0.14, 0.08, 1.0];
        out.extend(self.hud.rect(
            bx - 6.0 * scale,
            by + 2.0 * scale,
            22.0 * scale,
            4.0 * scale,
            grip,
        ));
        for i in 0..8 {
            let t = i as f32;
            out.extend(self.hud.rect(
                bx + (2.0 + t * 1.6) * scale,
                by + (6.0 + t * 3.0) * scale,
                10.0 * scale,
                3.2 * scale,
                grip,
            ));
        }
        let dodge = self
            .hunt
            .hardcore_rules()
            .is_some_and(|r| r.knife_dodge)
            .then(|| match k.dodge_cooldown {
                0 => "F: dodge".to_string(),
                ms => format!("Dodge in {}s", (ms + 999) / 1000),
            });
        let t = match dodge {
            Some(d) => format!("KNIFE   {d}"),
            None => "KNIFE".into(),
        };
        let w = self.hud.label_width(px, &t);
        out.extend(
            self.hud
                .label(sw - w - 12.0 * scale, sh - 14.0 * scale, px, AMBER, &t),
        );
    }
}
