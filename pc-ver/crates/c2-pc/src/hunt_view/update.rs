//! Input, fixed simulation ticks and what follows them (events, sounds,
//! the outcome, the camera).

use c2_formats::sav::KeyMap;
use c2_pad::Action as Pad;

use super::*;

impl HuntView {
    /// Controls from the player's key bindings (`ProcessControls`), and a
    /// gamepad's (`c2_pad`): the left stick walks (past a third of the way,
    /// as the keys do: the hunter's speed is the run toggle's), the right
    /// stick looks as the mouse does, curved for fine aim; `dt` seconds.
    pub(super) fn controls(&self, input: &Input, dt: f32) -> Controls {
        let o = &self.hunt.room.options;
        let k = &o.keys;
        let held = |vk| keys::held(input, vk);
        // The strafe key turns the turn keys into side-steps.
        let strafe = held(k.strafe);
        let mut mouse = if o.reverse_mouse != 0 {
            (input.mouse_delta.0, -input.mouse_delta.1)
        } else {
            input.mouse_delta
        };
        let pad = &input.pad;
        let [wx, wy] = pad.walk;
        let [lx, ly] = pad.look;
        mouse.0 += lx * lx.abs() * PAD_LOOK * dt;
        mouse.1 -= ly * ly.abs() * PAD_LOOK * dt;
        Controls {
            forward: held(k.forward) || wy > PAD_STEP,
            backward: held(k.backward) || wy < -PAD_STEP,
            strafe_left: held(k.strafe_left) || (strafe && held(k.left)) || wx < -PAD_STEP,
            strafe_right: held(k.strafe_right) || (strafe && held(k.right)) || wx > PAD_STEP,
            turn_left: !strafe && held(k.left),
            turn_right: !strafe && held(k.right),
            look_up: held(k.up),
            look_down: held(k.down),
            crouch: held(k.crouch) || self.pad_crouch,
            jump: held(k.jump) || pad.held(c2_pad::Action::Jump),
            run: self.run || self.touch.run,
            mouse,
            mouse_sens: o.mouse_sens as f32,
        }
    }

    pub(super) fn step(&mut self, input: &HuntInput) {
        let cars: Vec<Option<&Car>> = self
            .dinos
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        self.prev_cam = self.hunt.camera().0;
        self.hunt.opts.dynamic_night = self.view.area.cycle.map(|c| c.is_night());
        self.hunt.opts.night_vision_active = self.view.night_vision == Some(true);
        let before = self.hunt.real_time;
        self.hunt.tick(
            self.view.area(),
            &cars,
            input,
            TICK_MS as i32,
            &mut self.events,
        );
        if let Some(cycle) = &mut self.view.area.cycle {
            cycle.advance(self.hunt.real_time.wrapping_sub(before) as u32);
        }
    }

    /// A fresh hunt on the same area, reloading the saved trophy room.
    pub(super) fn restart(&mut self) {
        self.audio.stop_all();
        self.map_viewport = map_viewport::MapViewport::default();
        self.view.flashlight = self.setup.flashlight && Self::endless_night(&self.setup);
        if self.view.night_vision.is_some() {
            self.view.night_vision = Some(false);
        }
        if self.view.area.cycle.is_some() {
            self.view.area.cycle = Self::day_cycle(&self.setup);
        }
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let species = std::mem::take(&mut self.hunt.herd.species);
        let ship_ms = self.hunt.ship_anim_ms;
        self.hunt = Self::new_hunt(
            &self.view,
            species,
            &self.weapon_infos,
            &self.weapon_anims,
            self.seed,
            &self.setup,
            self.mee.as_ref(),
        );
        let muzzle = self.weapons.muzzle_ms();
        self.hunt.ship_anim_ms = ship_ms;
        if let Some(ms) = muzzle {
            self.hunt.weapons.muzzle_ms = ms;
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Logs what killed the hunter (an attacker the player never saw:
    /// a model that didn't load, past the draw distance, under water) and
    /// returns its name for the death message.
    pub(super) fn report_killer(&self) -> Option<String> {
        let h = &self.hunt;
        let c = h
            .death
            .and_then(|d| d.killer)
            .and_then(|i| h.herd.chars.get(i))?;
        let sp = h.herd.species.get(c.ctype)?;
        let (dx, dz) = (c.pos[0] - h.player.x, c.pos[2] - h.player.z);
        let dist = (dx * dx + dz * dz).sqrt();
        let drawn_r = self.view.radius as f32 * c2_core::CELL;
        let model = self.dinos.cars.get(c.ctype).and_then(Option::as_ref);
        let area = self.view.area();
        let water = area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2]);
        tracing::warn!(
            species = %sp.info.name,
            model = %sp.info.file,
            ai = sp.info.ai,
            dialect_ai = sp.mee.as_ref().map(|m| m.ai),
            kill_dist = sp.mee.as_ref().map(|m| m.kill_dist),
            scale = c.scale,
            distance = dist as i32,
            draw_distance = drawn_r as i32,
            model_loaded = model.is_some(),
            vertices = model.map_or(0, |(car, _)| car.mesh.vertices.len()),
            water_depth = water as i32,
            wave = c.wave,
            "hunter killed"
        );
        if model.is_none() || dist > drawn_r {
            tracing::warn!(species = %sp.info.name, "the killer was not drawn");
        }
        Some(sp.info.name.clone())
    }

    pub(super) fn handle_events(&mut self) {
        self.play_sounds();
        let killer = self
            .events
            .iter()
            .any(|e| matches!(e, GameEvent::HunterDied { .. }))
            .then(|| self.report_killer())
            .flatten();
        for e in self.events.drain(..) {
            match e {
                GameEvent::Message(m) => self.message = Some((m, 2.0)),
                GameEvent::HunterDied { kind, .. } => {
                    let how = ["eaten", "drowned", "fell", "killed"];
                    let how = how.get(kind as usize).unwrap_or(&"killed");
                    let m = match &killer {
                        Some(name) => format!("You were {how} by {name}."),
                        None => format!("You were {how}."),
                    };
                    self.message = Some((m, 6.0));
                }
                GameEvent::Scored {
                    ctype,
                    score,
                    range,
                    tranq,
                    night_bonus,
                } => {
                    let name = self
                        .hunt
                        .herd
                        .species
                        .get(ctype)
                        .map_or_else(String::new, |s| s.info.name.clone());
                    self.kill = Some((name, score, range, tranq, 6.0, night_bonus));
                }
                GameEvent::Killed { trophy: true, .. } => {
                    self.message = Some(("Trophy!".into(), 2.0));
                }
                _ => {}
            }
        }
    }

    /// Plays the sounds for this frame's events (`AddVoice*` call sites).
    pub(super) fn play_sounds(&mut self) {
        for c in c2_core::sound::commands(&self.events, &self.hunt) {
            match c {
                SoundCmd::Play { id, pos, volume } => {
                    self.audio.play(self.sounds.clip(id), pos, volume)
                }
                SoundCmd::Ambient { zone, volume } => {
                    let clip = match zone {
                        Some(z) => get(&self.sounds.ambients, z),
                        None => self.sounds.underwater.as_ref(),
                    };
                    self.audio.set_ambient(clip, volume);
                }
            }
        }
    }

    /// A frame: the picker's input while it's up; otherwise the keys and
    /// pad, the ticks they drive, then what came of them.
    pub(super) fn update_hunt(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        if let Some(row) = self.picker {
            self.update_picker(input, row);
            self.view.update(gpu, 0.0, &Input::default());
            return;
        }
        let light_tapped = self.update_lights(input);
        self.update_extras(input);
        // `KeyboardState[VK_CAPITAL] & 1`: Caps Lock's toggle hides them.
        if input.pressed(KeyCode::CapsLock) {
            self.gauges_hidden = !self.gauges_hidden;
        }
        let k = self.hunt.room.options.keys;
        self.weapon_flash = (self.weapon_flash - dt).max(0.0);
        if self.update_pick_keys(input, dt, &k) {
            return;
        }
        self.update_buttons(input, &k);
        let pad = &input.pad;
        let mut hin = HuntInput {
            controls: self.controls(input, dt),
            fire: keys::held(input, k.fire) || pad.held(Pad::Fire),
            // The Modder's Edition's weapons (keys of their own here: the
            // dialect shares Look up, Strafe and Turn right).
            reload: input.held(KeyCode::KeyR) || pad.held(Pad::Reload),
            rack: input.held(KeyCode::KeyF) || pad.held(Pad::Rack),
            switch_mode: input.held(KeyCode::KeyV) || pad.held(Pad::FireMode),
            zoom_in: input.held(KeyCode::Equal) || input.held(KeyCode::NumpadAdd),
            zoom_out: input.held(KeyCode::Minus) || input.held(KeyCode::NumpadSubtract),
            ..self.pending
        };
        // A virtual pointer works while the hunt owns the captured mouse.
        if light_tapped {
            hin.fire = false;
        }
        self.update_map_input(input, dt, &k, &mut hin);
        self.run_ticks(hin, dt);
        self.handle_events();
        self.age_notices(dt);
        self.check_outcome();
        self.follow_camera(gpu, dt, input);
    }

    /// The pick keys (the mouse only looks): a short press takes the next
    /// weapon or call, holding one opens its picker (true: just opened).
    fn update_pick_keys(&mut self, input: &Input, dt: f32, k: &KeyMap) -> bool {
        let can_pick = !self.hunt.paused
            && !self.hunt.show_exit()
            && self.hunt.death.is_none()
            && !self.hunt.opts.trophy;
        if can_pick && !self.hunt.opts.observer && input.pressed(self.pick_key()) {
            self.held_pick = Some((Pick::Weapon, 0.0));
        }
        if can_pick && keys::pressed(input, k.change_call) && !self.call_list().is_empty() {
            self.held_pick = Some((Pick::Call, 0.0));
        }
        if let Some((kind, t)) = self.held_pick {
            let t = t + dt;
            if !self.pick_held(input, kind) {
                self.held_pick = None;
                match kind {
                    Pick::Weapon => self.next_weapon(),
                    Pick::Call => self.pending.change_call = true,
                }
            } else if t >= HOLD_TO_PICK {
                self.held_pick = None;
                self.open_pick(kind, true);
                return true;
            } else {
                self.held_pick = Some((kind, t));
            }
        }
        false
    }

    /// The one-press keys and pad buttons: run, escape and the exit
    /// prompt, pause, crouch, the weapon, binoculars, map and calls.
    fn update_buttons(&mut self, input: &Input, k: &KeyMap) {
        let pressed = |vk| keys::pressed(input, vk);
        let pad = &input.pad;
        if pad.active() {
            self.pad_last = true;
        } else if input.last_pressed.is_some() || input.clicked || input.mouse_delta != (0.0, 0.0) {
            self.pad_last = false;
        }
        // With the exit prompt up, the pad's A is yes and B no.
        let prompt = self.hunt.show_exit();
        // The map open to teleport: the pad's D-pad and right stick move its
        // marker, ZR marks, A goes (not crouch, next call, jump).
        let map_pick = self.hunt.map_mode && self.hunt.can_teleport();
        if pressed(k.run) || (!prompt && pad.pressed(Pad::Run)) {
            self.run = !self.run;
        }
        if input.pressed(KeyCode::Escape) || pad.pressed(Pad::Exit) {
            self.hunt.press_escape();
        }
        if input.pressed(KeyCode::KeyY)
            || input.pressed(KeyCode::Enter)
            || (prompt && pad.pressed(Pad::Jump))
        {
            self.hunt.confirm_exit();
        }
        if prompt && pad.pressed(Pad::Run) {
            self.hunt.exit_prompt = false;
        }
        if pad.pressed(Pad::Crouch) && !map_pick {
            self.pad_crouch = !self.pad_crouch;
        }
        if pad.pressed(Pad::Pause) {
            self.hunt.toggle_pause();
        }
        // Pause; P too (laptops lack the key) unless P is bound to something.
        if input.pressed(KeyCode::Pause) || (!self.key_bound(80) && input.pressed(KeyCode::KeyP)) {
            self.hunt.toggle_pause();
        }
        if input.pressed(KeyCode::KeyN) {
            self.hunt.exit_prompt = false;
        }
        if self.hunt.opts.trophy {
            self.trophy_keys(input);
        } else if (input.pressed(KeyCode::KeyR) || pad.pressed(Pad::NextWeapon))
            && self.hunt.show_exit()
        {
            self.restart();
        }
        if pressed(k.show) || pad.pressed(Pad::Draw) {
            self.pending.toggle_weapon = true;
        }
        // Y: the next carried weapon (on the exit prompt: restart).
        if pad.pressed(Pad::NextWeapon) && !prompt {
            let carried = self.carried();
            let cur = self.hunt.weapons.target;
            let at = carried.iter().position(|&w| w == cur);
            let next = at.map_or(0, |i| (i + 1) % carried.len().max(1));
            if let Some(&w) = carried.get(next) {
                self.pending.select_weapon = Some(w);
            }
        }
        const DIGITS: [KeyCode; 6] = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
        ];
        if let Some(w) = DIGITS.iter().position(|&k| input.pressed(k)) {
            self.pending.select_weapon = Some(w);
        }
        if pressed(k.binoculars) || pad.pressed(Pad::Binoculars) {
            self.pending.toggle_binoculars = true;
        }
        if input.pressed(KeyCode::Tab) || pad.pressed(Pad::Map) {
            self.pending.toggle_map = true;
        }
        if pressed(k.call) || pad.pressed(Pad::Call) {
            self.pending.call = true;
        }
        if pad.pressed(Pad::NextCall) && !map_pick {
            self.pending.change_call = true;
        }
    }

    /// The open map: its gestures (they neither turn the camera nor fire),
    /// and teleporting's marker, mark and go.
    fn update_map_input(&mut self, input: &Input, dt: f32, k: &KeyMap, hin: &mut HuntInput) {
        let pad = &input.pad;
        if self.hunt.map_mode && !self.hunt.show_exit() {
            self.map_viewport
                .update(input, dt, self.setup.pc.map_zoom != 0);
            hin.controls.mouse = (0.0, 0.0);
            hin.fire = false;
        }
        if !(self.hunt.map_mode && self.hunt.can_teleport()) {
            self.map_cursor = None;
            self.teleport_pick = None;
            return;
        }
        let [mx, mz] = self.map_viewport.map_point();
        let extent = c2_formats::map::MAP_SIZE as f32 * c2_core::CELL;
        let (lo, hi) = (c2_core::collide::WALK_MIN, c2_core::collide::WALK_MAX);
        let (x, z) = ((mx * extent).clamp(lo, hi), (mz * extent).clamp(lo, hi));
        self.map_cursor = Some((x, z));
        hin.controls.mouse = (0.0, 0.0);
        hin.controls.jump = keys::held(input, k.jump);
        hin.controls.crouch = keys::held(input, k.crouch);
        hin.fire = false;
        if keys::pressed(input, k.fire) || pad.pressed(Pad::Fire) {
            self.teleport_pick = Some((x, z));
        }
        let go = input.pressed(KeyCode::Enter) || pad.pressed(Pad::Jump);
        if let (Some((px, pz)), true) = (self.teleport_pick, go && !self.hunt.show_exit()) {
            let msg = match self.hunt.request_teleport(self.view.area(), px, pz) {
                Ok(()) => "Teleporting...".to_string(),
                Err(e) => e,
            };
            self.message = Some((msg, 2.0));
            self.teleport_pick = None;
        }
    }

    /// The fixed ticks this frame's time owes; one-shot inputs apply to
    /// the first only.
    fn run_ticks(&mut self, mut hin: HuntInput, dt: f32) {
        self.acc_ms = (self.acc_ms + dt * 1000.0).min(250.0);
        while self.acc_ms >= TICK_MS as f32 {
            self.step(&hin);
            hin.controls.mouse = (0.0, 0.0);
            hin.toggle_weapon = false;
            hin.select_weapon = None;
            hin.toggle_binoculars = false;
            hin.toggle_map = false;
            hin.call = false;
            hin.change_call = false;
            self.pending = HuntInput::default();
            self.acc_ms -= TICK_MS as f32;
        }
    }

    /// The message and kill notice count down and go.
    fn age_notices(&mut self, dt: f32) {
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        if let Some(k) = &mut self.kill {
            k.4 -= dt;
            if k.4 <= 0.0 {
                self.kill = None;
            }
        }
        // The original's trophy plaque says it already: one notice a kill.
        if self.hunt.trophy_body.is_some() && !self.hunt.opts.trophy {
            self.kill = None;
        }
    }

    /// The hunt left alive (saved) or died.
    fn check_outcome(&mut self) {
        match self.hunt.outcome() {
            Some(Outcome::Exited) if !self.done => {
                if let Err(e) = crate::save::store(&self.hunt.room) {
                    tracing::error!("{e:#}");
                }
                self.record_kills();
                self.audio.stop_all();
                self.done = true;
            }
            // Dying discards this hunt's trophies (the save isn't written).
            // From the menu, go back to it; otherwise start over. A
            // survival ends this way: its kills and wave are recorded.
            Some(Outcome::Died) if !self.done => {
                self.record_kills();
                if self.setup.from_menu {
                    self.audio.stop_all();
                    self.done = true;
                } else {
                    self.restart();
                }
            }
            _ => {}
        }
    }

    /// The camera between the last two ticks, the listener and the ship's
    /// engine with it.
    fn follow_camera(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        let (pos, alpha, beta) = self.hunt.camera();
        let t = self.acc_ms / TICK_MS as f32;
        self.view.cam.pos = Vec3::from(self.prev_cam).lerp(Vec3::from(pos), t);
        // Original yaw/pitch conventions are mirrored relative to the fly cam.
        self.view.cam.yaw = -alpha;
        self.view.cam.pitch = -beta;
        self.view.zoom = self.hunt.zoom();
        self.audio.set_camera(pos, alpha);
        let ship = &self.hunt.ship;
        let engine = (ship.state != -1).then_some(ship.pos);
        self.audio
            .set_engine(get(&self.sounds.ship, ship_sound::ENGINE), engine);
        self.view.update(gpu, dt, input);
    }
}
