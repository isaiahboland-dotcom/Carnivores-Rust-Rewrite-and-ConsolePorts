//! `c2-pc hunt <AREA.MAP>`: hunting an area (M3, in progress).
//!
//! The simulation lives in `c2_core::game::Hunt`; this file maps input,
//! drives fixed ticks and draws the world, animals, weapon and HUD.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use c2_core::area::TimeOfDay;
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{GameEvent, Hunt, HuntInput, HuntOptions, Outcome, WeaponInfo, Weapons};
use c2_core::player::{Controls, Player};
use c2_core::rng::MsRand;
use c2_formats::car::Car;
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::area_view::{find_file, AreaView};
use crate::audio::Audio;
use crate::dinos::{load_res, load_ship, load_species, DinoAssets};
use crate::effects_view::EffectsView;
use crate::gpu::Gpu;
use crate::hud::{load_tga, Hud, Sprite};
use crate::input::Input;
use crate::scene::Scene;
use crate::sounds::{get, SoundBank};
use crate::weapons::{load_weapons, WeaponAssets};
use c2_core::dino::{hunt, DinoEvent};
use c2_core::player::PlayerEvent;
use c2_core::ship::{ship_sound, ShipEvent};

/// Simulation step. The original runs on variable `TimeDt`; a fixed step keeps
/// the simulation deterministic (needed for logic parity later).
const TICK_MS: u32 = 10;

/// Weapons carried. The loadout menu doesn't exist yet: take them all.
const ALL_WEAPONS: u32 = u32::MAX;

pub struct HuntView {
    view: AreaView,
    hunt: Hunt,
    dinos: DinoAssets,
    weapons: WeaponAssets,
    weapon_infos: Vec<WeaponInfo>,
    weapon_anims: Vec<[i32; 4]>,
    hud: Hud,
    exit_img: Option<usize>,
    map_img: Option<usize>,
    call_imgs: Vec<Option<usize>>,
    /// Hunt targets (`TargetDino` bits by AI).
    targets: u32,
    /// Camera position before the latest tick, for interpolation.
    prev_cam: [f32; 3],
    acc_ms: f32,
    /// `RunMode` (toggled with Shift, as the original's run key).
    run: bool,
    /// Key presses seen this frame, applied on the next tick.
    pending: HuntInput,
    events: Vec<GameEvent>,
    message: Option<(String, f32)>,
    seed: u32,
    /// The game install (for the first-run save copy).
    game_dir: std::path::PathBuf,
    /// The hunt was left alive and saved: close the window.
    done: bool,
    audio: Audio,
    effects: EffectsView,
    sounds: SoundBank,
}

impl HuntView {
    /// `seed`: fixed RNG seed (reproducible runs); `None` = from the clock.
    pub fn load(
        gpu: &Gpu,
        map: &std::path::Path,
        time: TimeOfDay,
        seed: Option<u32>,
        targets: u32,
    ) -> Result<Self> {
        let mut view = AreaView::load(gpu, map, time)?;
        view.fly = false;
        let seed = seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        });
        let huntdat = map
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(std::path::Path::new("."));
        let res = load_res(huntdat)?;
        let (species, cars) = load_species(huntdat, &res)?;
        let mut hud = Hud::new(gpu);
        let (weapon_infos, weapon_anims, weapons) =
            load_weapons(gpu, view.globals(), &mut hud, huntdat, &res.weapons);
        let exit_img = find_file(huntdat, "MENU")
            .and_then(|m| find_file(&m, "EXIT.TGA"))
            .and_then(|p| load_tga(&p).ok())
            .map(|(w, h, rgba)| hud.add_image(gpu, w, h, &rgba));
        // The map: the frame with the area overview painted in at (11, 23).
        let map_img = find_file(huntdat, "MENU")
            .and_then(|m| find_file(&m, "MAPFRAME.TGA"))
            .and_then(|p| c2_formats::tga::parse(&std::fs::read(p).ok()?).ok())
            .map(|mut frame| {
                let px = Hunt::map_image(view.area());
                for y in 0..256 {
                    for x in 0..256 {
                        let (fx, fy) = (x + 11, y + 23);
                        if fx < frame.width && fy < frame.height {
                            frame.pixels[fy * frame.width + fx] = px[y * 256 + x];
                        }
                    }
                }
                let rgba = c2_core::color::image_to_rgba8(&frame.pixels, false);
                hud.add_image(gpu, frame.width as u32, frame.height as u32, &rgba)
            });
        // Call icons (`MENU/PICS/CALLn.TGA`), by AI 10..=19.
        let pics = find_file(huntdat, "MENU").and_then(|m| find_file(&m, "PICS"));
        let call_imgs: Vec<Option<usize>> = (1..=10)
            .map(|n| {
                let p = find_file(pics.as_deref()?, &format!("CALL{n}.TGA"))?;
                let (w, h, rgba) = load_tga(&p).ok()?;
                Some(hud.add_image(gpu, w, h, &rgba))
            })
            .collect();
        let dinos = DinoAssets::new(gpu, view.globals(), cars, load_ship(huntdat));
        let species_cars: Vec<Option<&Car>> = dinos
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        let sounds = SoundBank::load(
            huntdat,
            &species_cars,
            &weapons.cars(),
            dinos.ship_car(),
            &view.area().rsc,
        );
        let game_dir = huntdat
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let mut hunt = Self::new_hunt(
            &view,
            species,
            &weapon_infos,
            &weapon_anims,
            seed,
            &game_dir,
            targets,
        );
        hunt.ship_anim_ms = dinos.ship_anim_ms();
        let prev_cam = hunt.camera().0;
        let effects = EffectsView::new(gpu, view.globals(), huntdat);
        Ok(Self {
            view,
            hunt,
            dinos,
            weapons,
            weapon_infos,
            weapon_anims,
            hud,
            exit_img,
            map_img,
            call_imgs,
            targets,
            prev_cam,
            acc_ms: 0.0,
            run: false,
            pending: HuntInput::default(),
            events: Vec::new(),
            message: None,
            seed,
            game_dir,
            done: false,
            audio: Audio::new(),
            effects,
            sounds,
        })
    }

    /// `ReInitGame`: hunter at a random landing spot, a freshly placed herd.
    fn new_hunt(
        view: &AreaView,
        species: Vec<c2_core::dino::Species>,
        infos: &[WeaponInfo],
        anims: &[[i32; 4]],
        seed: u32,
        game_dir: &std::path::Path,
        targets: u32,
    ) -> Hunt {
        let area = view.area();
        let cell = area.landings[seed as usize % area.landings.len().max(1)];
        let player = Player::new(area, cell);
        let mut rng = MsRand::new(seed);
        let mut herd = Herd {
            species,
            chars: Vec::new(),
            demo_index: None,
        };
        let opts = PlaceOptions {
            density: 128,
            opt_agres: 128,
            targets,
        };
        herd.place(area, (player.x, player.z), &mut rng, opts);
        tracing::info!("placed {} characters", herd.chars.len());
        let weapons = Weapons::new(infos.to_vec(), anims.to_vec(), ALL_WEAPONS, false);
        let room = crate::save::load(game_dir);
        let mut hunt = Hunt::new(
            area,
            player,
            herd,
            weapons,
            rng,
            HuntOptions {
                targets,
                ..HuntOptions::default()
            },
            room,
        );
        hunt.clock = local_clock();
        hunt
    }

    fn controls(&self, input: &Input) -> Controls {
        Controls {
            forward: input.held(KeyCode::KeyW),
            backward: input.held(KeyCode::KeyS),
            strafe_left: input.held(KeyCode::KeyA),
            strafe_right: input.held(KeyCode::KeyD),
            turn_left: input.held(KeyCode::ArrowLeft),
            turn_right: input.held(KeyCode::ArrowRight),
            look_up: input.held(KeyCode::ArrowUp),
            look_down: input.held(KeyCode::ArrowDown),
            crouch: input.held(KeyCode::KeyX),
            jump: input.held(KeyCode::Space),
            run: self.run,
            mouse: input.mouse_delta,
            mouse_sens: 128.0,
        }
    }

    /// Debug/screenshot helper: simulate `ms` with no input, then stand the
    /// hunter `dist` units from the first animal with AI `ai_id`, facing it;
    /// `after_ms` more pass with the camera fixed.
    pub fn spectate(&mut self, ms: u32, ai_id: i32, dist: f32, after_ms: u32) {
        let idle = HuntInput::default();
        for _ in 0..ms / TICK_MS {
            self.step(&idle);
        }
        let Some(d) = self
            .hunt
            .herd
            .chars
            .iter()
            .find(|ch| ch.ai == ai_id)
            .cloned()
        else {
            tracing::warn!("no character with ai {ai_id}");
            return;
        };
        let (x, z) = (d.pos[0] + d.lookz * dist, d.pos[2] - d.lookx * dist);
        let area = self.view.area();
        let p = &mut self.hunt.player;
        p.x = x;
        p.z = z;
        p.y = area.land_qh(x, z, f32::MAX);
        p.alpha = f32::atan2(d.pos[0] - x, -(d.pos[2] - z));
        p.beta = 0.1;
        let keep = (p.x, p.y, p.z, p.alpha, p.beta);
        for _ in 0..after_ms / TICK_MS {
            self.step(&idle);
            let p = &mut self.hunt.player;
            (p.x, p.y, p.z, p.alpha, p.beta) = keep;
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Debug/screenshot helper: bag the first animal with AI `ai_id`, run
    /// `ms`, then look at the ship from `dist` away.
    pub fn watch_ship(&mut self, ai_id: i32, ms: u32, dist: f32) {
        if let Some(i) = self
            .hunt
            .herd
            .chars
            .iter()
            .position(|c| c.ai == ai_id && c.health > 0)
        {
            self.hunt.debug_bag(self.view.area(), i, &mut self.events);
        }
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        let s = self.hunt.ship.pos;
        tracing::info!(
            "ship state {} at {:?} alpha {:.2}",
            self.hunt.ship.state,
            s,
            self.hunt.ship.alpha
        );
        let p = &mut self.hunt.player;
        p.x = s[0] + dist;
        p.z = s[2];
        p.y = s[1] - 400.0;
        p.alpha = -std::f32::consts::FRAC_PI_2;
        p.beta = 0.0;
        self.prev_cam = self.hunt.camera().0;
    }

    /// Fires once, then runs `ms` (screenshots).
    pub fn fire_and_wait(&mut self, ms: u32) {
        self.step(&HuntInput {
            fire: true,
            ..Default::default()
        });
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Opens the map right away (screenshots).
    pub fn open_map(&mut self) {
        let input = HuntInput {
            toggle_map: true,
            ..Default::default()
        };
        self.step(&input);
    }

    /// Raises the binoculars right away (screenshots).
    pub fn raise_binoculars(&mut self) {
        let input = HuntInput {
            toggle_binoculars: true,
            ..Default::default()
        };
        self.step(&input);
    }

    /// Draws weapon `w` right away (screenshots).
    pub fn draw_weapon(&mut self, w: usize) {
        let input = HuntInput {
            select_weapon: Some(w),
            ..Default::default()
        };
        self.step(&input);
        for _ in 0..200 {
            self.step(&HuntInput::default());
        }
        self.prev_cam = self.hunt.camera().0;
    }

    fn step(&mut self, input: &HuntInput) {
        let cars: Vec<Option<&Car>> = self
            .dinos
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        self.prev_cam = self.hunt.camera().0;
        self.hunt.tick(
            self.view.area(),
            &cars,
            input,
            TICK_MS as i32,
            &mut self.events,
        );
    }

    /// A fresh hunt on the same area, reloading the saved trophy room.
    fn restart(&mut self) {
        self.audio.stop_all();
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let species = std::mem::take(&mut self.hunt.herd.species);
        let ship_ms = self.hunt.ship_anim_ms;
        self.hunt = Self::new_hunt(
            &self.view,
            species,
            &self.weapon_infos,
            &self.weapon_anims,
            self.seed,
            &self.game_dir,
            self.targets,
        );
        self.hunt.ship_anim_ms = ship_ms;
        self.prev_cam = self.hunt.camera().0;
    }

    fn handle_events(&mut self) {
        self.play_sounds();
        for e in self.events.drain(..) {
            match e {
                GameEvent::Message(m) => self.message = Some((m, 2.0)),
                GameEvent::HunterDied { kind } => {
                    let how = ["eaten", "drowned", "fell", "killed"];
                    let m = format!("You were {}.", how.get(kind as usize).unwrap_or(&"killed"));
                    self.message = Some((m, 6.0));
                }
                GameEvent::Killed { trophy: true, .. } => {
                    self.message = Some(("Trophy!".into(), 2.0));
                }
                _ => {}
            }
        }
    }

    /// Plays the sounds for this frame's events (`AddVoice*` call sites).
    fn play_sounds(&mut self) {
        let snd = &self.sounds;
        let a = &mut self.audio;
        let h = &self.hunt;
        let r3 = (h.real_time % 3) as usize;
        for e in &self.events {
            match e {
                GameEvent::WeaponSound { weapon, slot } => a.play(
                    snd.weapons.get(*weapon).and_then(|w| get(w, *slot)),
                    None,
                    256,
                ),
                GameEvent::Dino(DinoEvent::Sound { index, sound, pos }) => {
                    let ct = h.herd.chars.get(*index).map_or(usize::MAX, |c| c.ctype);
                    a.play(
                        snd.species.get(ct).and_then(|s| get(s, *sound)),
                        Some(*pos),
                        256,
                    );
                }
                GameEvent::Player(PlayerEvent::Step) => {
                    let v = 24 + (h.player.vspeed * 50.0) as i32;
                    a.play(get(&snd.steps, r3), None, v);
                }
                GameEvent::Player(PlayerEvent::StepWater) => {
                    let v = 64 + (h.player.vspeed * 30.0) as i32;
                    a.play(get(&snd.steps_water, r3), None, v);
                }
                GameEvent::Player(PlayerEvent::Land) => a.play(get(&snd.steps, r3), None, 64),
                GameEvent::HunterDied { kind } if *kind != hunt::BREATH => {
                    a.play(get(&snd.screams, (h.real_time % 4) as usize), None, 256)
                }
                GameEvent::Ship(ShipEvent::Sound { slot, at }) => {
                    a.play(get(&snd.ship, *slot), *at, 256)
                }
                GameEvent::RandomSound { sound, pos, volume } => {
                    a.play(get(&snd.random, *sound), Some(*pos), *volume)
                }
                GameEvent::Call { call, variant, pos } => {
                    let clip = snd
                        .calls
                        .get((*call - 10).max(0) as usize)
                        .and_then(|c| get(c, *variant));
                    a.play(clip, *pos, 256);
                }
                GameEvent::Ambient { zone, volume } => {
                    let clip = match zone {
                        Some(z) => get(&snd.ambients, *z),
                        None => snd.underwater.as_ref(),
                    };
                    a.set_ambient(clip, *volume);
                }
                _ => {}
            }
        }
    }
}

impl Scene for HuntView {
    fn title(&self) -> String {
        let h = &self.hunt;
        let p = &h.player;
        let w = &h.weapons;
        let weapon = match w.pose() {
            Some(_) => format!(
                " — {} ({} left)",
                w.info[w.current].name, w.shots_left[w.current]
            ),
            None => String::new(),
        };
        let msg = self
            .message
            .as_ref()
            .map(|(m, _)| format!(" — {m}"))
            .unwrap_or_default();
        format!(
            "c2rs hunt {} — {:.0} fps — {} animals, {} bagged{}{}{}{}",
            self.view.name(),
            self.view.fps,
            h.herd
                .chars
                .iter()
                .filter(|c| c.health > 0 && c.ai >= 10)
                .count(),
            h.successes,
            weapon,
            if self.run { " — RUN" } else { "" },
            if p.underwater {
                " — UNDERWATER"
            } else if p.swim {
                " — SWIMMING"
            } else {
                ""
            },
            msg,
        )
    }

    fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        if input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight) {
            self.run = !self.run;
        }
        if input.pressed(KeyCode::Escape) {
            self.hunt.press_escape();
        }
        if input.pressed(KeyCode::KeyY) || input.pressed(KeyCode::Enter) {
            self.hunt.confirm_exit();
        }
        if input.pressed(KeyCode::KeyR) && self.hunt.show_exit() {
            self.restart();
        }
        if input.pressed(KeyCode::KeyE) {
            self.pending.toggle_weapon = true;
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
        if input.pressed(KeyCode::KeyB) {
            self.pending.toggle_binoculars = true;
        }
        if input.pressed(KeyCode::Tab) {
            self.pending.toggle_map = true;
        }
        if input.pressed(KeyCode::AltLeft) || input.pressed(KeyCode::AltRight) {
            self.pending.call = true;
        }
        if input.pressed(KeyCode::KeyC) {
            self.pending.change_call = true;
        }
        let mut hin = HuntInput {
            controls: self.controls(input),
            fire: input.grabbed && input.mouse_down,
            zoom_in: input.held(KeyCode::Equal) || input.held(KeyCode::NumpadAdd),
            zoom_out: input.held(KeyCode::Minus) || input.held(KeyCode::NumpadSubtract),
            ..self.pending
        };

        self.acc_ms = (self.acc_ms + dt * 1000.0).min(250.0);
        while self.acc_ms >= TICK_MS as f32 {
            self.step(&hin);
            // One-shot inputs apply to the first tick only.
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
        self.handle_events();
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }

        match self.hunt.outcome() {
            Some(Outcome::Exited) if !self.done => {
                if let Err(e) = crate::save::store(&self.hunt.room) {
                    tracing::error!("{e:#}");
                }
                self.done = true;
            }
            // No menu yet: after dying, start a new hunt on the same area
            // (this hunt's trophies are discarded, as in the original).
            Some(Outcome::Died) => self.restart(),
            _ => {}
        }

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

    fn clear_color(&self) -> [f64; 3] {
        self.view.clear_color()
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        // Weapon light: 96 + ground light / 4 (`wpnlight`).
        let p = &self.hunt.player;
        let cell = c2_formats::map::MAP_SIZE * (p.z / 256.0) as usize + (p.x / 256.0) as usize;
        let ground = self
            .view
            .area()
            .light_map()
            .get(cell)
            .copied()
            .unwrap_or(128);
        let light = (96.0 + f32::from(ground) / 4.0) / 255.0;
        let (dinos, weapons, hunt, effects) = (
            &mut self.dinos,
            &mut self.weapons,
            &self.hunt,
            &mut self.effects,
        );
        let eye = self.view.cam.pos.to_array();
        let view = self.view.cam.view();
        let max = self.view.radius as f32 * c2_core::CELL;
        self.view
            .draw_with(gpu, pass, aspect, &mut |gpu, pass, globals| {
                dinos.draw(gpu, pass, globals, &hunt.herd, &hunt.ship, eye, max);
                effects.draw(gpu, pass, globals, &hunt.effects);
                weapons.draw(gpu, pass, globals, hunt, view, light, aspect);
            });

        // HUD at an integer scale of the original's 640x480 layout.
        let scale = (screen.1 as f32 / 480.0).floor().max(1.0);
        let mut sprites = self.weapons.hud_sprites(&self.hud, &self.hunt, scale);
        // Binoculars: what's in the cross-hairs (`Render_LifeInfo`).
        if let Some(li) = self.hunt.life_info(self.view.area()) {
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
                sprites.extend(
                    self.hud
                        .text(x, y + 16.0 * scale * i as f32, scale, green, l),
                );
            }
        }
        // Scope cross-hairs at the view model's aim point (`DrawOpticCross`).
        if self.hunt.zoom() > 1.0 && !self.hunt.bin_mode {
            if let Some(p) = self.weapons.aim_point {
                let far = self.view.radius as f32 * c2_core::CELL;
                let vp = crate::camera::projection(aspect, far * 1.5, self.hunt.zoom()) * view;
                let c = vp * glam::Vec4::new(p[0], p[1], p[2], 1.0);
                if c.w > 0.0 {
                    let sx = (c.x / c.w + 1.0) * 0.5 * screen.0 as f32;
                    let sy = (1.0 - c.y / c.w) * 0.5 * screen.1 as f32;
                    let (cx, cy) = (screen.0 as f32 / 2.0, screen.1 as f32 / 2.0);
                    if (cx - sx).abs() <= cx && (cy - sy).abs() <= cy / 2.0 {
                        let w = screen.0 as f32 / 12.0;
                        let col = [0.0, 0.0, 0.0625, 0.5];
                        sprites.extend(self.hud.rect(sx - w, sy, 2.0 * w, scale, col));
                        sprites.extend(self.hud.rect(sx, sy - w, scale, 2.0 * w, col));
                    }
                }
            }
        }
        // Map (`DrawHMap`): view range circle, the hunter, radar blips.
        if self.hunt.map_mode {
            if let Some((id, mw, mh)) = self
                .map_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
            {
                let (cx, cy) = (screen.0 as f32 / 2.0, screen.1 as f32 / 2.0);
                let (fx, fy) = (cx - mw * scale / 2.0, cy - (mh / 2.0 + 6.0) * scale);
                sprites.push(Sprite::new(id, fx, fy, scale));
                let (ox, oy) = (fx + 11.0 * scale, fy + 23.0 * scale);
                let p = &self.hunt.player;
                // CCX = (x / 512) * 2; one map pixel per 4 cells.
                let ccx = ((p.x as i32 / 512) * 2) as f32 / 4.0;
                let ccy = ((p.z as i32 / 512) * 2) as f32 / 4.0;
                let dot = |x: f32, y: f32, c: [f32; 4]| {
                    self.hud
                        .rect(ox + x * scale, oy + y * scale, 2.0 * scale, 2.0 * scale, c)
                };
                let r = self.hunt.opts.view_r as f32 / 4.0;
                for i in 0..48 {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    let (px, py) = (ccx + a.cos() * r, ccy + a.sin() * r);
                    sprites.extend(self.hud.rect(
                        ox + (px + 1.0) * scale,
                        oy + (py + 1.0) * scale,
                        scale,
                        scale,
                        [0.0, 0.12, 0.0, 1.0],
                    ));
                    sprites.extend(self.hud.rect(
                        ox + px * scale,
                        oy + py * scale,
                        scale,
                        scale,
                        [0.0, 0.56, 0.0, 1.0],
                    ));
                }
                sprites.extend(dot(ccx + 1.0, ccy + 1.0, [0.25, 0.0, 0.0, 1.0]));
                sprites.extend(dot(ccx, ccy, [0.97, 0.0, 0.0, 1.0]));
                if self.hunt.opts.radar {
                    for c in &self.hunt.herd.chars {
                        if c.ai >= 10 && c.health > 0 && self.targets & (1 << c.ai) != 0 {
                            sprites.extend(dot(
                                c.pos[0] / 1024.0,
                                c.pos[2] / 1024.0,
                                [0.0, 0.97, 0.0, 1.0],
                            ));
                        }
                    }
                }
            }
        }
        // The selected call's icon (top right) after pressing change-call.
        if self.hunt.call_icon_time > 0 {
            let i = (self.hunt.target_call - 10).max(0) as usize;
            if let Some((id, w)) = self
                .call_imgs
                .get(i)
                .copied()
                .flatten()
                .and_then(|id| self.hud.image(id).map(|im| (id, im.w as f32)))
            {
                sprites.push(Sprite::new(
                    id,
                    screen.0 as f32 - (10.0 + w) * scale,
                    7.0 * scale,
                    scale,
                ));
            }
        }
        if self.hunt.show_exit() {
            if let Some((id, w, h)) = self
                .exit_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h)))
            {
                let x = (screen.0 as f32 - w as f32 * scale) / 2.0;
                let y = (screen.1 as f32 - h as f32 * scale) / 2.0;
                sprites.push(Sprite::new(id, x, y, scale));
            }
        }
        self.hud.draw(gpu, pass, screen, &sprites);
    }

    fn grab_cursor(&self) -> bool {
        true
    }

    fn handles_escape(&self) -> bool {
        true
    }

    fn wants_exit(&self) -> bool {
        self.done
    }
}

/// Local-ish date and time for trophy records (UTC; no timezone database).
fn local_clock() -> (i32, i32, i32, i32, i32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y as i32,
        m as i32,
        d as i32,
        (rem / 3600) as i32,
        (rem % 3600 / 60) as i32,
    )
}
