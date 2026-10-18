//! `c2-pc hunt <AREA.MAP>`: walk an area as the hunter (M3, in progress).

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use c2_core::area::TimeOfDay;
use c2_core::dino::{Ctx, DinoEvent, Herd, HunterView, PlaceOptions};
use c2_core::player::{Controls, Player, PlayerEvent};
use c2_core::rng::MsRand;
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::area_view::AreaView;
use crate::dinos::{load_species, DinoAssets};
use crate::gpu::Gpu;
use crate::input::Input;
use crate::scene::Scene;

/// Simulation step. The original runs on variable `TimeDt`; a fixed step keeps
/// the simulation deterministic (needed for logic parity later).
const TICK_MS: u32 = 10;

pub struct HuntView {
    view: AreaView,
    player: Player,
    /// Eye position before the latest tick, for interpolation.
    prev_eye: [f32; 3],
    acc_ms: f32,
    run: bool,
    events: Vec<PlayerEvent>,
    herd: Herd,
    dinos: DinoAssets,
    rng: MsRand,
    dino_events: Vec<DinoEvent>,
    /// `RealTime`, ms since the hunt started.
    real_time: i32,
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
        // PlaceHunter: a random landing spot (the original uses timeGetTime()).
        let seed = seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        });
        let area = view.area();
        let cell = area.landings[seed as usize % area.landings.len().max(1)];
        let player = Player::new(area, cell);
        let prev_eye = player.eye();

        let huntdat = map
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(std::path::Path::new("."));
        let (species, cars) = load_species(huntdat)?;
        let mut rng = MsRand::new(seed);
        let mut herd = Herd {
            species,
            chars: Vec::new(),
        };
        let opts = PlaceOptions {
            density: 128,
            opt_agres: 128,
            targets: 0,
        };
        herd.place(area, (player.x, player.z), &mut rng, opts);
        tracing::info!("placed {} characters", herd.chars.len());
        let dinos = DinoAssets::new(gpu, view.globals(), cars);
        Ok(Self {
            view,
            player,
            prev_eye,
            acc_ms: 0.0,
            run: false,
            events: Vec::new(),
            herd,
            dinos,
            rng,
            dino_events: Vec::new(),
            real_time: 0,
        })
    }

    fn controls(&self, input: &Input) -> Controls {
        let k = |a: KeyCode, b: KeyCode| input.held(a) || input.held(b);
        Controls {
            forward: k(KeyCode::KeyW, KeyCode::ArrowUp),
            backward: k(KeyCode::KeyS, KeyCode::ArrowDown),
            strafe_left: input.held(KeyCode::KeyA),
            strafe_right: input.held(KeyCode::KeyD),
            turn_left: input.held(KeyCode::ArrowLeft),
            turn_right: input.held(KeyCode::ArrowRight),
            crouch: k(KeyCode::KeyC, KeyCode::ControlLeft),
            jump: input.held(KeyCode::Space),
            run: self.run ^ input.held(KeyCode::ShiftLeft),
            mouse: input.mouse_delta,
            mouse_sens: 128.0,
        }
    }

    /// Debug/screenshot helper: simulate `ms` with no input, then stand the
    /// hunter `dist` units from the first animal with AI `ai_id`, facing it.
    pub fn spectate(&mut self, ms: u32, ai_id: i32, dist: f32, after_ms: u32) {
        let c = Controls::default();
        for _ in 0..ms / TICK_MS {
            self.tick(&c);
        }
        let Some(d) = self.herd.chars.iter().find(|ch| ch.ai == ai_id) else {
            tracing::warn!("no character with ai {ai_id}");
            return;
        };
        let (x, z) = (d.pos[0] + d.lookz * dist, d.pos[2] - d.lookx * dist);
        let p = &mut self.player;
        p.x = x;
        p.z = z;
        p.y = self.view.area().land_qh(x, z, f32::MAX);
        p.alpha = f32::atan2(d.pos[0] - x, -(d.pos[2] - z));
        p.beta = 0.1;
        self.prev_eye = p.eye();
        // Keep the world running with the camera fixed (to see which way
        // animals move).
        let (px, py, pz, pa, pb) = (p.x, p.y, p.z, p.alpha, p.beta);
        let idx = self
            .herd
            .chars
            .iter()
            .position(|ch| ch.ai == ai_id)
            .unwrap_or(0);
        let log = |h: &Herd| {
            let d = &h.chars[idx];
            tracing::debug!("watch #{idx}: pos ({:.0}, {:.0}) alpha {:.2} look ({:.2}, {:.2}) phase {} vspeed {:.2}", d.pos[0], d.pos[2], d.alpha, d.lookx, d.lookz, d.phase, d.vspeed);
        };
        log(&self.herd);
        tracing::debug!("camera at ({px:.0}, {pz:.0}) alpha {pa:.2}");
        for _ in 0..after_ms / TICK_MS {
            self.tick(&c);
            let p = &mut self.player;
            (p.x, p.y, p.z, p.alpha, p.beta) = (px, py, pz, pa, pb);
        }
        log(&self.herd);
        self.prev_eye = self.player.eye();
    }

    /// One fixed simulation step: hunter, then characters.
    fn tick(&mut self, c: &Controls) {
        self.prev_eye = self.player.eye();
        self.player
            .tick(self.view.area(), c, TICK_MS, &mut self.events);
        self.real_time += TICK_MS as i32;
        let p = &self.player;
        let mut ctx = Ctx {
            area: self.view.area(),
            rng: &mut self.rng,
            dt: TICK_MS as i32,
            real_time: self.real_time,
            hunter: HunterView {
                alpha: p.alpha,
                x: p.x,
                y: p.y,
                z: p.z,
                head_y: p.head_y,
                alive: true,
                underwater: p.underwater,
            },
            opt_agres: 128,
            tranq: false,
            view_r: 74,
            events: &mut self.dino_events,
        };
        self.herd.step(&mut ctx);
    }
}

impl Scene for HuntView {
    fn title(&self) -> String {
        let p = &self.player;
        format!(
            "c2rs hunt {} — {:.0} fps — {} animals — WASD move, mouse look, Shift run (R toggles), C crouch, Space jump/swim{}{}",
            self.view.name(),
            self.view.fps,
            self.herd.chars.len(),
            if self.run { " — RUN" } else { "" },
            if p.underwater {
                " — UNDERWATER"
            } else if p.swim {
                " — SWIMMING"
            } else {
                ""
            },
        )
    }

    fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        if input.pressed(KeyCode::KeyR) {
            self.run = !self.run;
        }
        let mut c = self.controls(input);
        self.acc_ms += dt * 1000.0;
        // Don't try to catch up after a long stall (window drag, breakpoint).
        self.acc_ms = self.acc_ms.min(250.0);
        while self.acc_ms >= TICK_MS as f32 {
            self.tick(&c);
            c.mouse = (0.0, 0.0); // mouse motion applies once
            self.acc_ms -= TICK_MS as f32;
        }
        self.events.clear(); // sounds come later
        self.dino_events.clear();
        let t = self.acc_ms / TICK_MS as f32;
        let eye = Vec3::from(self.prev_eye).lerp(Vec3::from(self.player.eye()), t);
        self.view.cam.pos = eye;
        // Original yaw/pitch conventions are mirrored relative to the fly cam.
        self.view.cam.yaw = -self.player.alpha;
        self.view.cam.pitch = -self.player.beta;
        self.view.update(gpu, dt, input);
    }

    fn clear_color(&self) -> [f64; 3] {
        self.view.clear_color()
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, aspect: f32) {
        let (herd, dinos) = (&self.herd, &mut self.dinos);
        let eye = self.view.cam.pos.to_array();
        let max = self.view.radius as f32 * c2_core::CELL;
        self.view
            .draw_with(gpu, pass, aspect, &mut |gpu, pass, globals| {
                dinos.draw(gpu, pass, globals, herd, eye, max)
            });
    }

    fn grab_cursor(&self) -> bool {
        true
    }
}
