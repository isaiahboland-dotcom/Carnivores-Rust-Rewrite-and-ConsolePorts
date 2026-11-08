//! The hunt's frame: the world, then the HUD's layers in order.

use super::*;

impl HuntView {
    /// The world pass (unless blacked out), then the HUD sprites, later
    /// layers over earlier ones.
    pub(super) fn draw_frame(
        &mut self,
        gpu: &Gpu,
        pass: &mut wgpu::RenderPass<'_>,
        screen: (u32, u32),
    ) {
        self.screen = screen;
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        if !self.blacked_out() {
            self.draw_world(gpu, pass, aspect);
        }

        // HUD at an integer scale of the original's 640x480 layout.
        let scale = (screen.1 as f32 / 480.0).floor().max(1.0);
        let mut sprites = Vec::new();
        self.draw_optics(screen, &mut sprites);
        let ammo_start = sprites.len();
        sprites.extend(self.weapons.hud_sprites(&self.hud, &self.hunt, scale));
        let ammo_end = self.ammo_bottom(&sprites[ammo_start..]);
        self.draw_kill_notice(screen, scale, &mut sprites);
        self.draw_survival(&mut sprites, ammo_end, screen, scale);
        self.draw_teleport_countdown(screen, scale, &mut sprites);
        self.draw_life_info(screen, scale, &mut sprites);
        self.draw_scope_cross(screen, scale, aspect, &mut sprites);
        self.draw_map(screen, scale, &mut sprites);
        self.draw_call_icon(screen, scale, &mut sprites);
        self.draw_trophy_banner(screen, scale, &mut sprites);
        self.draw_plaque(screen, scale, &mut sprites);
        self.draw_kit_hud(screen, &mut sprites);
        self.draw_picker(screen, scale, &mut sprites);
        self.draw_trophy_prompt(screen, scale, &mut sprites);
        self.draw_prompts(screen, scale, &mut sprites);
        self.draw_touch(&mut sprites);
        self.draw_lights(&mut sprites, scale);
        self.hud.draw(gpu, pass, screen, &sprites);
    }

    /// The area with the animals, effects, rounds in flight, the gauges and
    /// the weapon in hand.
    fn draw_world(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, aspect: f32) {
        // Weapon light: 96 + ground light / 4 (`wpnlight`).
        let p = &self.hunt.player;
        let cell = c2_formats::map::MAP_SIZE * (p.z / 256.0) as usize + (p.x / 256.0) as usize;
        let ground = self.view.area().light_at(cell);
        let light = (96.0 + ground / 4.0) / 255.0;
        let (dinos, weapons, hunt, effects) = (
            &mut self.dinos,
            &mut self.weapons,
            &self.hunt,
            &mut self.effects,
        );
        let shown = gauges::Gauges::shown(hunt, self.gauges_hidden);
        let mut gauges = self.gauges.as_mut().filter(|_| shown);
        let eye = self.view.cam.pos.to_array();
        let view = self.view.cam.view();
        let max = self.view.radius as f32 * c2_core::CELL;
        self.view
            .draw_with(gpu, pass, aspect, &mut |gpu, pass, globals, frustum| {
                let seen = crate::dinos::Seen { eye, max, frustum };
                dinos.draw(gpu, pass, globals, &hunt.herd, &hunt.ship, seen);
                effects.draw(gpu, pass, globals, &hunt.effects);
                weapons.draw_rounds(gpu, pass, globals, hunt);
                if let Some(g) = gauges.as_mut() {
                    g.draw(gpu, pass, globals, hunt, view, aspect);
                }
                weapons.draw(gpu, pass, globals, hunt, view, light, aspect);
            });
    }

    /// The phone kit's binocular or scope mask, alive only.
    fn draw_optics(&self, screen: (u32, u32), out: &mut Vec<Sprite>) {
        let Some(k) = &self.kit else {
            return;
        };
        if self.hunt.death.is_none() {
            if self.hunt.bin_mode {
                k.binoculars(&self.hud, screen, out);
            } else if self.hunt.weapons.optic_mode && self.hunt.zoom() > 1.0 {
                k.scope(&self.hud, screen, out);
            }
        }
    }

    /// The last kill: a box in the bottom right.
    fn draw_kill_notice(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let Some((name, score, range, tranq, _, night_bonus)) = &self.kill else {
            return;
        };
        let us = self.hunt.room.options.sys != 0;
        let dist = if us {
            format!("Range: {:.0} ft", range / 0.3)
        } else {
            format!("Range: {range:.0} m")
        };
        let lines = [
            name.clone(),
            if *night_bonus != 0 {
                format!("+{score} points (night +{night_bonus}%)")
            } else {
                format!("+{score} points")
            },
            dist,
            if *tranq {
                "Tranquilized (+25%, no trophy)".to_string()
            } else {
                "Killed".to_string()
            },
        ];
        if let Some(k) = &self.kit {
            let lines: Vec<_> = lines
                .into_iter()
                .enumerate()
                .map(|(i, l)| (l, if i == 1 { GOLD } else { LIGHT }))
                .collect();
            k.kill_notice(&self.hud, screen, &lines, out);
            return;
        }
        let px = 8.0 * scale;
        let w = lines
            .iter()
            .map(|l| self.hud.label_width(px, l))
            .fold(0.0, f32::max)
            + 16.0 * scale;
        let h = (lines.len() as f32 * 12.0 + 12.0) * scale;
        let (x, y) = (
            screen.0 as f32 - w - 12.0 * scale,
            screen.1 as f32 - h - 12.0 * scale,
        );
        out.extend(self.hud.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.7]));
        for (i, l) in lines.iter().enumerate() {
            let c = if i == 1 {
                [0.95, 0.85, 0.2, 1.0]
            } else {
                [0.9, 0.9, 0.9, 1.0]
            };
            let ly = y + (6.0 + 12.0 * i as f32) * scale;
            out.extend(self.hud.label(x + 8.0 * scale, ly, px, c, l));
        }
    }

    /// A teleport under way: its countdown, centre top.
    fn draw_teleport_countdown(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        if let Some((_, ms)) = self.hunt.teleport.pending {
            let px = 16.0 * scale.max(1.0).round();
            let t = format!("Teleporting in {}", (ms + 999) / 1000);
            let w = self.hud.label_width(px, &t);
            let (x, y) = ((screen.0 as f32 - w) / 2.0, screen.1 as f32 * 0.15);
            out.extend(self.hud.label(x, y, px, [1.0, 0.85, 0.1, 1.0], &t));
        }
    }

    /// Binoculars: what's in the cross-hairs (`Render_LifeInfo`).
    fn draw_life_info(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        let Some(li) = self.hunt.life_info(self.view.area()) else {
            return;
        };
        let green = [0.0, 0.69, 0.0, 1.0];
        let x = screen.0 as f32 / 2.0 + screen.0 as f32 / 64.0;
        let y = screen.1 as f32 / 2.0 + screen.1 as f32 / 6.8;
        let imperial = self.hunt.room.options.sys != 0;
        let lines = if imperial {
            [
                li.name.clone(),
                format!("Weight: {:.2}t", li.weight / 0.907),
                format!("Distance: {}ft", (li.distance * 3.0 / 64.0) as i32),
            ]
        } else {
            [
                li.name.clone(),
                format!("Weight: {:.2}T", li.weight),
                format!("Distance: {}m", (li.distance * 3.0 / 64.0) as i32 / 3),
            ]
        };
        for (i, l) in lines.iter().enumerate() {
            out.extend(
                self.hud
                    .text(x, y + 16.0 * scale * i as f32, scale, green, l),
            );
        }
    }

    /// Scope cross-hairs at the view model's aim point (`DrawOpticCross`).
    fn draw_scope_cross(&self, screen: (u32, u32), scale: f32, aspect: f32, out: &mut Vec<Sprite>) {
        if !self.hunt.scope_cross() {
            return;
        }
        let Some(p) = self.weapons.aim_point else {
            return;
        };
        let far = self.view.radius as f32 * c2_core::CELL;
        let view = self.view.cam.view();
        let vp = crate::camera::projection(aspect, far * 1.5, self.hunt.zoom()) * view;
        let c = vp * glam::Vec4::new(p[0], p[1], p[2], 1.0);
        if c.w > 0.0 {
            let sx = (c.x / c.w + 1.0) * 0.5 * screen.0 as f32;
            let sy = (1.0 - c.y / c.w) * 0.5 * screen.1 as f32;
            let (cx, cy) = (screen.0 as f32 / 2.0, screen.1 as f32 / 2.0);
            if (cx - sx).abs() <= cx && (cy - sy).abs() <= cy / 2.0 {
                let w = screen.0 as f32 / 12.0;
                let col = [0.0, 0.0, 0.0625, 0.5];
                out.extend(self.hud.rect(sx - w, sy, 2.0 * w, scale, col));
                out.extend(self.hud.rect(sx, sy - w, scale, 2.0 * w, col));
            }
        }
    }

    /// The selected call's icon (top right) after pressing change-call.
    fn draw_call_icon(&self, screen: (u32, u32), scale: f32, out: &mut Vec<Sprite>) {
        if self.hunt.call_icon_time <= 0 {
            return;
        }
        let i = (self.hunt.target_call - 10).max(0) as usize;
        if let Some((id, w)) = self
            .call_imgs
            .get(i)
            .copied()
            .flatten()
            .and_then(|id| self.hud.image(id).map(|im| (id, im.w as f32)))
        {
            out.push(Sprite::new(
                id,
                screen.0 as f32 - (10.0 + w) * scale,
                7.0 * scale,
                scale,
            ));
        }
    }

    /// The phone kit's hunt screen: its buttons (with the ammo count and
    /// heading), the hexagon of a call or weapon just picked, and a notice.
    fn draw_kit_hud(&self, screen: (u32, u32), out: &mut Vec<Sprite>) {
        let Some(k) = &self.kit else {
            return;
        };
        let h = &self.hunt;
        let quiet = h.paused || h.show_exit() || h.map_mode || self.picker.is_some();
        if h.death.is_none() && !h.opts.trophy && !quiet {
            let st = Buttons {
                ammo: h.weapons.pose().map(|_| self.kit_ammo()),
                heading: h.player.alpha,
            };
            k.buttons(&self.hud, screen, st, out);
        }
        // The call just picked: its animal in a hexagon, as the
        // phone's (its name when there's no picture).
        let pic = (h.call_icon_time > 0)
            .then(|| {
                let i = (h.target_call - 10).max(0) as usize;
                self.call_pics.get(i).copied().flatten()
            })
            .flatten();
        if pic.is_some() && !quiet {
            k.hexes(&self.hud, screen, &[(pic, true)], out);
        } else if self.weapon_flash > 0.0 && !quiet {
            let w = k.weapon_picture(h.weapons.target);
            k.hexes(&self.hud, screen, &[(w, true)], out);
        }
        let call = (h.call_icon_time > 0 && pic.is_none())
            .then(|| self.call_name())
            .flatten()
            .map(|n| format!("Call: {n}"));
        let note = match (&self.message, &h.survival) {
            (Some((m, _)), None) => Some(m.as_str()),
            _ => call.as_deref(),
        };
        if let Some(m) = note {
            k.notify(&self.hud, screen, m, out);
        }
    }

    /// The kit's ammo count for the weapon in hand.
    fn kit_ammo(&self) -> String {
        let w = &self.hunt.weapons;
        let cur = w.current;
        let left = w.shots_left[cur];
        if w.multi_ammo() {
            let (l, t) = w.kinds(cur);
            format!("{l}L / {t}T")
        } else if w.info[cur].chambers() {
            // The dialect's: chambered / the reserve.
            format!("{} / {left}", w.chambered[cur])
        } else {
            match w.ammo_mag.get(cur) {
                Some(&m) if m > 0 => format!("{} / {left}", w.info[cur].shots),
                _ => left.to_string(),
            }
        }
    }
}
