//! Hunt state and initialization.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{
    Bullet, DeathCam, HuntKill, HuntOptions, Outcome, Survival, Teleport, Weapons, MAX_CALL,
};
use crate::area::Area;
use crate::dino::{Herd, HunterView, PI};
use crate::player::Player;
use crate::rng::MsRand;
use crate::ship::Ship;
use c2_formats::sav::Save;

/// `MAX_HEALTH`.
pub const MAX_HEALTH: i32 = 128_000;

#[derive(Debug, Clone, Copy)]
pub struct Wind {
    pub alpha: f32,
    pub speed: f32,
}

impl Wind {
    pub fn nv(&self) -> [f32; 3] {
        [self.alpha.sin(), 0.0, -self.alpha.cos()]
    }
}

pub struct Hunt {
    pub player: Player,
    pub herd: Herd,
    pub weapons: Weapons,
    pub wind: Wind,
    pub rng: MsRand,
    pub opts: HuntOptions,
    /// `RealTime`, ms.
    pub real_time: i32,
    /// `Takt`: tick counter.
    pub takt: u32,
    /// `MyHealth`.
    pub health: i32,
    pub death: Option<DeathCam>,
    /// `ObjectsOnLook` (global in the original; persists between checks).
    pub(super) objects_on_look: i32,
    /// Dialect bullets in flight (`bullet[]`).
    pub bullets: Vec<Bullet>,
    /// The view's recoil still to come, yaw and pitch per 17 ms (`Recoil`).
    pub(super) recoil: [f32; 2],
    /// The trigger was held last tick (`alreadyFired`).
    pub(super) fire_held: bool,
    /// Shots fired / animals bagged this hunt (`TrophyRoom.Last`).
    pub shots_made: u32,
    pub successes: u32,
    pub ship: Ship,
    /// Length of the ship's landing animation (ms).
    pub ship_anim_ms: i32,
    /// The player's trophy room; trophies and score are added to it.
    pub room: Save,
    /// Local date and time for trophy records: (year, month, day, hour, minute).
    pub clock: (i32, i32, i32, i32, i32),
    /// The exit prompt is showing (`EXITMODE`).
    pub exit_prompt: bool,
    /// `PAUSE`: nothing moves while the hunter is alive.
    pub paused: bool,
    /// Countdown to leaving after confirming (`ExitTime`), ms.
    pub(super) exit_time: i32,
    pub(super) outcome: Option<Outcome>,
    /// The area's map number (1-based; 0 unknown), kept with trophies.
    pub area_no: i32,
    /// This hunt's kills (for the hunter's record).
    pub kills: Vec<HuntKill>,
    /// A survival (README M13) instead of a hunt.
    pub survival: Option<Survival>,
    /// Distance walked / time spent this hunt (`TrophyRoom.Last.path/time`).
    pub path: f32,
    pub time_s: f32,
    /// Countdown to each ambient zone's next random sound (`RndTime`).
    pub(super) amb_rnd: Vec<i32>,
    /// Random sounds allowed per zone (night drops flagged ones).
    pub(super) amb_sounds: Vec<Vec<c2_formats::rsc::RandomSoundRef>>,
    pub(super) current_ambient: Option<Option<usize>>,
    /// Looking through the binoculars (`BINMODE`) and their power (1.5..=3).
    pub bin_mode: bool,
    pub bin_power: f32,
    /// The map is showing (`MapMode`).
    pub map_mode: bool,
    /// Particles, blood trails and water rings.
    pub effects: crate::effects::Effects,
    /// Tick timings, when a clock is set (`prof.clock`).
    pub prof: crate::prof::Prof,
    /// Selected call (`TargetCall`, an AI number) and how long its icon shows.
    pub target_call: i32,
    pub call_icon_time: i32,
    pub(super) call_lock: i32,
    pub(super) next_call: i32,
    /// A pending answer to a call: (time left, position, call).
    pub(super) answer: Option<(i32, [f32; 3], i32)>,
    /// The trophy slot whose plaque is showing (`TrophyBody`), and for how
    /// much longer after a kill (`TrophyTime`, ms; 0 = while in range).
    pub trophy_body: Option<usize>,
    pub trophy_time: i32,
    /// A teleport under way or cooling down ([`Hunt::request_teleport`]).
    pub teleport: Teleport,
    /// Advanced observer: flying, and time stopped (animals, effects and
    /// the clock hold still; the camera moves).
    pub flying: bool,
    pub time_stopped: bool,
}

impl Hunt {
    pub fn new(
        area: &Area,
        player: Player,
        herd: Herd,
        weapons: Weapons,
        mut rng: MsRand,
        opts: HuntOptions,
        room: Save,
    ) -> Self {
        let wind = Wind {
            alpha: rng.r_rand(1024) as f32 * 2.0 * PI / 1024.0,
            speed: 10.0,
        };
        let ship = Ship::new(area, (player.x, player.z));
        let mut weapons = weapons;
        if opts.half_tranq && !opts.observer && !opts.trophy {
            weapons.split_tranq();
        }
        // LoadResources: random sounds per ambient zone; at night those with
        // flags set are removed; the first countdown is randomized.
        let night = area.time == crate::area::TimeOfDay::Night;
        let mut amb_rnd = Vec::new();
        let mut amb_sounds = Vec::new();
        for a in &area.rsc.ambients {
            let n = (a.random_count.max(0) as usize).min(a.random.len());
            let list: Vec<_> = a.random[..n]
                .iter()
                .copied()
                .filter(|r| !night || r.flags == 0)
                .collect();
            let freq = a.random[0].freq;
            amb_rnd.push(if n > 0 {
                (freq / 2 + rng.r_rand(freq)) * 1000
            } else {
                0
            });
            amb_sounds.push(list);
        }
        Self {
            ship,
            ship_anim_ms: 1,
            room,
            clock: (2026, 1, 1, 12, 0),
            exit_prompt: false,
            paused: false,
            exit_time: 0,
            outcome: None,
            area_no: 0,
            kills: Vec::new(),
            path: 0.0,
            time_s: 0.0,
            amb_rnd,
            amb_sounds,
            current_ambient: None,
            bin_mode: false,
            bin_power: 2.5,
            map_mode: false,
            effects: Default::default(),
            prof: Default::default(),
            target_call: (10..MAX_CALL)
                .find(|&a| opts.targets.has(a as usize))
                .unwrap_or(0),
            call_icon_time: 0,
            call_lock: 0,
            next_call: 0,
            answer: None,
            trophy_body: None,
            trophy_time: 0,
            teleport: Teleport::default(),
            flying: false,
            time_stopped: false,
            player,
            herd,
            weapons,
            wind,
            rng,
            opts,
            real_time: 0,
            takt: 0,
            health: MAX_HEALTH,
            death: None,
            survival: None,
            objects_on_look: 0,
            bullets: Vec::new(),
            recoil: [0.0; 2],
            fire_held: false,
            shots_made: 0,
            successes: 0,
        }
    }

    pub(super) fn hunter_view(&self) -> HunterView {
        let p = &self.player;
        HunterView {
            alpha: p.alpha,
            x: p.x,
            y: p.y,
            z: p.z,
            head_y: p.head_y,
            alive: self.health > 0,
            underwater: p.underwater,
        }
    }
}
