//! Hunt map rendering; overlays use the same viewport as picking.
use super::*;

impl HuntView {
    pub(super) fn draw_map(&mut self, screen: (u32, u32), scale: f32, sprites: &mut Vec<Sprite>) {
        // Map (`DrawHMap`): view range circle, the hunter, radar blips.
        if self.hunt.map_mode {
            // The phone's map: on its panel, filling most of the screen's
            // height.
            let kit_map = self.kit.as_ref().map(|k| {
                let area = crate::save::area_title(&self.setup.game_dir, self.hunt.area_no);
                k.map_panel(&self.hud, screen, &area, sprites)
            });
            let scale = match kit_map {
                Some((_, _, side)) => side / 256.0,
                None => scale,
            };
            if let Some((id, mw, mh)) = self
                .map_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
            {
                let (cx, cy) = (screen.0 as f32 / 2.0, screen.1 as f32 / 2.0);
                let (fx, fy) = match kit_map {
                    Some((x, y, _)) => (x, y),
                    None => (cx - mw * scale / 2.0, cy - (mh / 2.0 + 6.0) * scale),
                };
                let s = 256.0 * scale / mw.max(1.0);
                let s = if kit_map.is_some() { s } else { scale };
                sprites.push(Sprite::new(id, fx, fy, s));
                let (ox, oy) = (fx + self.map_at.0 * scale, fy + self.map_at.1 * scale);
                if let (Some(k), None) = (&self.kit, kit_map) {
                    k.map_frame(&self.hud, (ox, oy, 256.0 * scale), sprites);
                }
                self.map_viewport.rect = Some((ox, oy, 256.0 * scale));
                let origin = self.map_viewport.origin;
                let span = self.map_viewport.span;
                sprites.push(Sprite {
                    uv: [
                        (self.map_at.0 + origin[0] * 256.0) / mw,
                        (self.map_at.1 + origin[1] * 256.0) / mh,
                        (self.map_at.0 + (origin[0] + span) * 256.0) / mw,
                        (self.map_at.1 + (origin[1] + span) * 256.0) / mh,
                    ],
                    size: Some([256.0 * scale; 2]),
                    ..Sprite::new(id, ox, oy, 1.0)
                });
                let markers_start = sprites.len();
                let mut decorations = Vec::new();
                let p = &self.hunt.player;
                // CCX = (x / 512) * 2; one map pixel per 4 cells.
                let ccx = ((p.x as i32 / 512) * 2) as f32 / 4.0;
                let ccy = ((p.z as i32 / 512) * 2) as f32 / 4.0;
                let dot = |x: f32, y: f32, c: [f32; 4]| {
                    self.hud
                        .rect(ox + x * scale, oy + y * scale, 2.0 * scale, 2.0 * scale, c)
                };
                let r = self.hunt.opts.view_r as f32 / 4.0;
                let yellow = [1.0, 0.85, 0.1, 1.0];
                if self.map_mobile && kit_map.is_none() {
                    // The mobile game's: a yellow frame, and a yellow cone
                    // for what the hunter sees (77 degrees, as the
                    // original's camera; out to the view range).
                    let (fw, fh) = (256.0 * scale, 256.0 * scale);
                    for (x, y, w, h) in [
                        (
                            ox - 2.0 * scale,
                            oy - 2.0 * scale,
                            fw + 4.0 * scale,
                            2.0 * scale,
                        ),
                        (ox - 2.0 * scale, oy + fh, fw + 4.0 * scale, 2.0 * scale),
                        (ox - 2.0 * scale, oy, 2.0 * scale, fh),
                        (ox + fw, oy, 2.0 * scale, fh),
                    ] {
                        decorations.extend(self.hud.rect(x, y, w, h, yellow));
                    }
                    sprites.extend(sector(
                        &self.hud,
                        (ccx, ccy),
                        r,
                        (p.alpha, 0.8f32.atan()),
                        (ox, oy),
                        scale,
                        [1.0, 0.85, 0.1, 0.45],
                    ));
                }
                // The view range circle (yellow in the mobile look).
                let (ring, shade) = if kit_map.is_some() {
                    // The phone's: a pale circle.
                    ([0.95, 0.92, 0.88, 0.8], [0.0, 0.0, 0.0, 0.3])
                } else if self.map_mobile {
                    (yellow, [0.3, 0.25, 0.0, 1.0])
                } else {
                    ([0.0, 0.56, 0.0, 1.0], [0.0, 0.12, 0.0, 1.0])
                };
                // A line in the mobile look, the original's dots otherwise.
                let n = if self.map_mobile { 128 } else { 48 };
                for i in 0..n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    let (px, py) = (ccx + a.cos() * r, ccy + a.sin() * r);
                    sprites.extend(self.hud.rect(
                        ox + (px + 1.0) * scale,
                        oy + (py + 1.0) * scale,
                        scale,
                        scale,
                        shade,
                    ));
                    sprites.extend(self.hud.rect(
                        ox + px * scale,
                        oy + py * scale,
                        scale,
                        scale,
                        ring,
                    ));
                }
                let marker = (
                    ox + (ccx + 1.0) * scale,
                    oy + (ccy + 1.0) * scale,
                    5.0 * scale,
                );
                if !self.kit.as_ref().is_some_and(|k| k.map_me(marker, sprites)) {
                    sprites.extend(dot(ccx + 1.0, ccy + 1.0, [0.25, 0.0, 0.0, 1.0]));
                    let me = if self.map_mobile {
                        yellow
                    } else {
                        [0.97, 0.0, 0.0, 1.0]
                    };
                    sprites.extend(dot(ccx, ccy, me));
                }
                // Teleporting: the marker, and when it can go.
                if let Some((x, z)) = self.map_cursor {
                    let (mx, my) = (x / 1024.0, z / 1024.0);
                    for (dx, dy) in [(-3.0, 0.0), (3.0, 0.0), (0.0, -3.0), (0.0, 3.0), (0.0, 0.0)] {
                        sprites.extend(dot(mx + dx, my + dy, [1.0, 1.0, 1.0, 1.0]));
                    }
                    // The spot picked: a circle.
                    if let Some((x, z)) = self.teleport_pick {
                        let (cx, cy) = (x / 1024.0, z / 1024.0);
                        for i in 0..24 {
                            let a = i as f32 / 24.0 * std::f32::consts::TAU;
                            sprites.extend(dot(
                                cx + a.cos() * 4.0,
                                cy + a.sin() * 4.0,
                                [1.0, 0.85, 0.1, 1.0],
                            ));
                        }
                    }
                    let t = &self.hunt.teleport;
                    let status = match (t.pending, t.cooldown, self.teleport_pick) {
                        (Some((_, ms)), _, _) => format!("Teleporting in {}s", (ms + 999) / 1000),
                        (None, ms, _) if ms > 0 => {
                            format!("Teleport ready in {}s", (ms + 999) / 1000)
                        }
                        (None, _, Some(_)) => "Enter: teleport to the circle".into(),
                        _ => "Click a spot to teleport to".into(),
                    };
                    let px = 8.0 * (screen.1 as f32 / 480.0).max(1.0).round();
                    decorations.extend(self.hud.label(
                        ox,
                        oy + 262.0 * scale,
                        px,
                        [1.0; 4],
                        &status,
                    ));
                }
                // Where he looks: a short line of dots, as on the 3DS map.
                if self.map_heading {
                    let (s, c) = p.alpha.sin_cos();
                    for i in 1..4 {
                        let d = i as f32 * 1.5;
                        let (x, y) = (ccx + s * d, ccy - c * d);
                        sprites.extend(self.hud.rect(
                            ox + (x + 0.5) * scale,
                            oy + (y + 0.5) * scale,
                            scale,
                            scale,
                            [0.97, 0.31, 0.31, 1.0],
                        ));
                    }
                }
                // Radar: the picked animals green, other huntables that
                // spawned anyway blue.
                if self.hunt.opts.radar {
                    for c in &self.hunt.herd.chars {
                        if c.health <= 0 {
                            continue;
                        }
                        let color = if self.hunt.is_target(c) && kit_map.is_some() {
                            yellow
                        } else if self.hunt.is_target(c) {
                            [0.0, 0.97, 0.0, 1.0]
                        } else if self.hunt.is_huntable(c) {
                            [0.2, 0.55, 1.0, 1.0]
                        } else {
                            continue;
                        };
                        sprites.extend(dot(c.pos[0] / 1024.0, c.pos[2] / 1024.0, color));
                    }
                }
                // Hit by a tracker: in its colour, radar or not.
                for c in &self.hunt.herd.chars {
                    if let Some((rgb, _)) = c.tracked {
                        let color = [rgb[0], rgb[1], rgb[2]].map(|v| f32::from(v) / 255.0);
                        sprites.extend(dot(
                            c.pos[0] / 1024.0,
                            c.pos[2] / 1024.0,
                            [color[0], color[1], color[2], 1.0],
                        ));
                    }
                }
                let markers = sprites.split_off(markers_start);
                sprites.extend(
                    markers
                        .into_iter()
                        .filter_map(|s| self.map_viewport.project(s, &self.hud)),
                );
                sprites.extend(decorations);
                if self.setup.pc.map_zoom != 0 || self.hunt.can_teleport() {
                    self.map_viewport.pointer(&self.hud, sprites);
                }
            }
        }
    }
}

/// A filled pie slice in map pixels (from the map's corner `origin`,
/// `scale` screen pixels each): centre `c`, radius `r`, facing `alpha`
/// (the hunter's heading), `half` either side; as one-pixel runs.
fn sector(
    hud: &Hud,
    c: (f32, f32),
    r: f32,
    (alpha, half): (f32, f32),
    origin: (f32, f32),
    scale: f32,
    color: [f32; 4],
) -> Vec<Sprite> {
    use std::f32::consts::{PI, TAU};
    let mut out = Vec::new();
    let inside = |x: f32, y: f32| {
        let (dx, dy) = (x - c.0, y - c.1);
        if dx * dx + dy * dy > r * r {
            return false;
        }
        // Heading `alpha`: forward is (sin, -cos) on the map.
        let a = (dx.atan2(-dy) - alpha + PI).rem_euclid(TAU) - PI;
        a.abs() <= half
    };
    let n = r.ceil() as i32 + 1;
    for j in -n..=n {
        let y = (c.1 + j as f32).floor();
        let mut run: Option<f32> = None;
        for i in -n..=n + 1 {
            let x = (c.0 + i as f32).floor();
            let on = i <= n && inside(x + 0.5, y + 0.5);
            match (on, run) {
                (true, None) => run = Some(x),
                (false, Some(x0)) => {
                    out.extend(hud.rect(
                        origin.0 + x0 * scale,
                        origin.1 + y * scale,
                        (x - x0) * scale,
                        scale,
                        color,
                    ));
                    run = None;
                }
                _ => {}
            }
        }
    }
    out
}
