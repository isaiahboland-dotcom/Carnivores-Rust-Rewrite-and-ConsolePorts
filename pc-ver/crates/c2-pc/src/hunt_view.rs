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
use crate::dinos::{load_res, load_ship, load_species, DinoAssets};
use crate::gpu::Gpu;
use crate::hud::{load_tga, Hud, Sprite};
use crate::input::Input;
use crate::scene::Scene;
use crate::weapons::{load_weapons, WeaponAssets};

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
}

impl HuntView {
    /// `seed`: fixed RNG seed (reproducible runs); `None` = from the clock.
    pub fn load(
        gpu: &Gpu,
        map: &std::path::Path,
        time: TimeOfDay,
        seed: Option<u32>,
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
        let dinos = DinoAssets::new(gpu, view.globals(), cars, load_ship(huntdat));
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
        );
        hunt.ship_anim_ms = dinos.ship_anim_ms();
        let prev_cam = hunt.camera().0;
        Ok(Self {
            view,
            hunt,
            dinos,
            weapons,
            weapon_infos,
            weapon_anims,
            hud,
            exit_img,
            prev_cam,
            acc_ms: 0.0,
            run: false,
            pending: HuntInput::default(),
            events: Vec::new(),
            message: None,
            seed,
            game_dir,
            done: false,
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
            targets: 0,
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
            HuntOptions::default(),
            room,
        );
        hunt.clock = local_clock();
        hunt
    }

    fn controls(&self, input: &Input) -> Controls {
        let k = |a: KeyCode, b: KeyCode| input.held(a) || input.held(b);
        Controls {
            forward: input.held(KeyCode::KeyW),
            backward: input.held(KeyCode::KeyS),
            strafe_left: input.held(KeyCode::KeyA),
            strafe_right: input.held(KeyCode::KeyD),
            turn_left: input.held(KeyCode::ArrowLeft),
            turn_right: input.held(KeyCode::ArrowRight),
            look_up: input.held(KeyCode::ArrowUp),
            look_down: input.held(KeyCode::ArrowDown),
            crouch: k(KeyCode::KeyX, KeyCode::KeyC),
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

    /// Draws the weapon right away (screenshots).
    pub fn draw_weapon(&mut self) {
        self.hunt.hide_weapon(&mut self.events);
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
        );
        self.hunt.ship_anim_ms = ship_ms;
        self.prev_cam = self.hunt.camera().0;
    }

    fn handle_events(&mut self) {
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
                _ => {} // sounds and effects come later
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
        let mut hin = HuntInput {
            controls: self.controls(input),
            fire: input.grabbed && input.mouse_down,
            ..self.pending
        };

        self.acc_ms = (self.acc_ms + dt * 1000.0).min(250.0);
        while self.acc_ms >= TICK_MS as f32 {
            self.step(&hin);
            // One-shot inputs apply to the first tick only.
            hin.controls.mouse = (0.0, 0.0);
            hin.toggle_weapon = false;
            hin.select_weapon = None;
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
        let (dinos, weapons, hunt) = (&mut self.dinos, &mut self.weapons, &self.hunt);
        let eye = self.view.cam.pos.to_array();
        let view = self.view.cam.view();
        let max = self.view.radius as f32 * c2_core::CELL;
        self.view
            .draw_with(gpu, pass, aspect, &mut |gpu, pass, globals| {
                dinos.draw(gpu, pass, globals, &hunt.herd, &hunt.ship, eye, max);
                weapons.draw(gpu, pass, globals, hunt, view, light);
            });

        // HUD at an integer scale of the original's 640x480 layout.
        let scale = (screen.1 as f32 / 480.0).floor().max(1.0);
        let mut sprites = self.weapons.hud_sprites(&self.hud, &self.hunt, scale);
        if self.hunt.show_exit() {
            if let Some((id, w, h)) = self
                .exit_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h)))
            {
                let x = (screen.0 as f32 - w as f32 * scale) / 2.0;
                let y = (screen.1 as f32 - h as f32 * scale) / 2.0;
                sprites.push(Sprite {
                    image: id,
                    x,
                    y,
                    scale,
                });
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
