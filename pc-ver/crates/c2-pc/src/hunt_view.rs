//! `c2-pc hunt <AREA.MAP>`: walk an area as the hunter (M3, in progress).

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use c2_core::area::TimeOfDay;
use c2_core::player::{Controls, Player, PlayerEvent};
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::area_view::AreaView;
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
}

impl HuntView {
    pub fn load(gpu: &Gpu, map: &std::path::Path, time: TimeOfDay) -> Result<Self> {
        let mut view = AreaView::load(gpu, map, time)?;
        view.fly = false;
        // PlaceHunter: a random landing spot (the original uses timeGetTime()).
        let area = view.area();
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let cell = area.landings[seed as usize % area.landings.len().max(1)];
        let player = Player::new(area, cell);
        let prev_eye = player.eye();
        Ok(Self {
            view,
            player,
            prev_eye,
            acc_ms: 0.0,
            run: false,
            events: Vec::new(),
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
}

impl Scene for HuntView {
    fn title(&self) -> String {
        let p = &self.player;
        format!(
            "c2rs hunt {} — {:.0} fps — WASD move, mouse look, Shift run (R toggles), C crouch, Space jump/swim{}{}",
            self.view.name(),
            self.view.fps,
            if self.run { " — RUN" } else { "" },
            if p.underwater { " — UNDERWATER" } else if p.swim { " — SWIMMING" } else { "" },
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
            self.prev_eye = self.player.eye();
            self.player
                .tick(self.view.area(), &c, TICK_MS, &mut self.events);
            c.mouse = (0.0, 0.0); // mouse motion applies once
            self.acc_ms -= TICK_MS as f32;
        }
        self.events.clear(); // sounds come later
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
        self.view.draw(gpu, pass, aspect);
    }

    fn grab_cursor(&self) -> bool {
        true
    }
}
