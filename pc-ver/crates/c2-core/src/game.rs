//! A hunt in progress: the hunter, the herd, weapons, wind and detection,
//! advanced in fixed ticks (`Hunt2.cpp: ProcessGame`).
//!
//! Sources: `ProcessShoot`, `HideWeapon` (Hunt2.cpp), the weapon state
//! machine in `DrawPostObjects`, `MakeShot`, `MakeNoise`, `CheckAfraid`,
//! `AnimateProcesses` (wind), `AddDeadBody`, `ProcessDemoMovement`.

use c2_formats::car::Car;
use c2_formats::res_txt::WeaponDef;

use crate::area::Area;
use crate::dino::{
    ai, delta_func, find_vector_alpha, hunt, Character, Ctx, DinoEvent, Herd, HunterView, PI,
};
use crate::player::{Controls, Player, PlayerEvent};
use crate::rng::MsRand;
use crate::ship::{Ship, ShipEvent};
use crate::trace::{trace_look, trace_shot, Hit};
use c2_formats::sav::{Save, TrophyItem};

/// `MAX_HEALTH`.
pub const MAX_HEALTH: i32 = 128_000;

/// `TWeapInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponInfo {
    pub name: String,
    pub file: String,
    pub pic: String,
    pub power: f32,
    pub prec: f32,
    pub loud: f32,
    pub rate: f32,
    pub shots: i32,
    /// Rounds per magazine (0 = no reload animation between shots).
    pub reload: i32,
    /// Extra pellets (`trace - 1`).
    pub trace_c: i32,
    pub optic: bool,
    /// Bullet drop (crossbow).
    pub fall: bool,
}

impl WeaponInfo {
    pub fn from_def(d: &WeaponDef) -> Self {
        Self {
            name: d.name.clone().unwrap_or_default(),
            file: d.file.clone().unwrap_or_default(),
            pic: d.pic.clone().unwrap_or_default(),
            power: d.power.unwrap_or(0.0),
            prec: d.prec.unwrap_or(0.0),
            loud: d.loud.unwrap_or(0.0),
            rate: d.rate.unwrap_or(0.0),
            shots: d.shots.unwrap_or(0),
            reload: d.reload.unwrap_or(0),
            trace_c: d.trace.unwrap_or(0) - 1,
            optic: d.optic.unwrap_or(0) != 0,
            fall: d.fall.unwrap_or(0) != 0,
        }
    }
}

/// Weapon `.CAR` animation and sound slots.
pub mod weapon_anim {
    pub const DRAW: usize = 0;
    pub const FIRE: usize = 1;
    pub const HIDE: usize = 2;
    pub const RELOAD: usize = 3;
}

/// `Weapon.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponState {
    Hidden = 0,
    Drawing = 1,
    /// Ready; `ftime > 0` while the fire animation plays.
    Ready = 2,
    Hiding = 3,
    Reloading = 4,
}

#[derive(Debug, Clone)]
pub struct Weapons {
    pub info: Vec<WeaponInfo>,
    /// `AniTime` of draw/fire/hide/reload per weapon (0 = missing).
    pub anim_ms: Vec<[i32; 4]>,
    pub current: usize,
    pub target: usize,
    pub state: WeaponState,
    pub ftime: i32,
    pub shakel: f32,
    pub shots_left: Vec<i32>,
    pub ammo_mag: Vec<i32>,
    /// Sniper scope view (`OPTICMODE`).
    pub optic_mode: bool,
}

impl Weapons {
    /// `ReInitGame`: full ammo for every weapon in `present` (bit mask).
    pub fn new(
        info: Vec<WeaponInfo>,
        anim_ms: Vec<[i32; 4]>,
        present: u32,
        double_ammo: bool,
    ) -> Self {
        let n = info.len();
        let mut w = Self {
            info,
            anim_ms,
            current: 0,
            target: 0,
            state: WeaponState::Hidden,
            ftime: 0,
            shakel: 0.0,
            shots_left: vec![0; n],
            ammo_mag: vec![0; n],
            optic_mode: false,
        };
        let mut first = None;
        for i in 0..n.min(32) {
            if present & (1 << i) != 0 {
                w.shots_left[i] = w.info[i].shots;
                if double_ammo {
                    w.ammo_mag[i] = 1;
                }
                first.get_or_insert(i);
            }
        }
        w.target = first.unwrap_or(0);
        w.current = w.target;
        w
    }

    fn anim(&self, slot: usize) -> i32 {
        self.anim_ms.get(self.current).map_or(0, |a| a[slot])
    }

    /// The animation slot and time to pose the view model with.
    pub fn pose(&self) -> Option<(usize, i32)> {
        match self.state {
            WeaponState::Hidden => None,
            s => Some((s as usize - 1, self.ftime)),
        }
    }

    /// Sway of the aim (`wpnDAlpha`, `wpnDBeta`).
    pub fn sway(&self, real_time: i32) -> (f32, f32) {
        let t = real_time as f32 / 300.0;
        (
            self.shakel * (t + PI / 2.0).sin() / 200.0,
            self.shakel * t.sin() / 400.0,
        )
    }
}

/// Things the frontend should present (sounds, effects, messages).
#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    /// Weapon sound slot (`weapon_anim::*`) of the current weapon.
    WeaponSound {
        weapon: usize,
        slot: usize,
    },
    Player(PlayerEvent),
    Dino(DinoEvent),
    /// A bullet impact (`AddElements`).
    Impact {
        hit: Hit,
        pos: [f32; 3],
    },
    /// An animal died; `trophy` = huntable species (`AI >= 10`).
    Killed {
        index: usize,
        trophy: bool,
    },
    HunterDied {
        kind: i32,
    },
    Ship(ShipEvent),
    /// A trophy was registered (slot in the trophy room).
    /// The looping ambience changed: `zone` = RSC ambient index, `None` =
    /// underwater loop. `volume` is `AVolume` (0..=256).
    Ambient {
        zone: Option<usize>,
        volume: i32,
    },
    /// A one-off RSC random sound (`RandSound[sound]`) near the hunter.
    RandomSound {
        sound: usize,
        pos: [f32; 3],
        volume: i32,
    },
    Trophy {
        slot: usize,
        score: i32,
    },
    Message(String),
}

/// Hunt options (from the menu / save file).
#[derive(Debug, Clone, Copy)]
pub struct HuntOptions {
    /// Bit `ai` set for each species chosen as a target (`TargetDino`).
    pub targets: u32,
    pub radar: bool,
    pub opt_agres: i32,
    pub opt_sens: i32,
    pub view_r: i32,
    pub camo: bool,
    pub scent: bool,
    pub tranq: bool,
}

impl Default for HuntOptions {
    fn default() -> Self {
        Self {
            targets: 0,
            radar: false,
            opt_agres: 128,
            opt_sens: 128,
            view_r: 74,
            camo: false,
            scent: false,
            tranq: false,
        }
    }
}

/// One tick of player input.
#[derive(Debug, Clone, Copy, Default)]
pub struct HuntInput {
    pub controls: Controls,
    /// Fire held.
    pub fire: bool,
    /// Show/hide weapon pressed.
    pub toggle_weapon: bool,
    /// Weapon number key pressed (0-based).
    pub select_weapon: Option<usize>,
}

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

/// The death camera (`DemoPoint`).
#[derive(Debug, Clone, Copy)]
pub struct DeathCam {
    /// Character index of the hunter's body.
    pub body: usize,
    /// 1 = flying in, then ms since it started circling (+2).
    pub time: i32,
    pub pos: [f32; 3],
    pub alpha: f32,
    pub beta: f32,
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
    objects_on_look: i32,
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
    /// Countdown to leaving after confirming (`ExitTime`), ms.
    exit_time: i32,
    outcome: Option<Outcome>,
    /// Distance walked / time spent this hunt (`TrophyRoom.Last.path/time`).
    pub path: f32,
    pub time_s: f32,
    /// Countdown to each ambient zone's next random sound (`RndTime`).
    amb_rnd: Vec<i32>,
    /// Random sounds allowed per zone (night drops flagged ones).
    amb_sounds: Vec<Vec<c2_formats::rsc::RandomSoundRef>>,
    current_ambient: Option<Option<usize>>,
}

/// How a hunt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Left alive: the trophy room should be saved.
    Exited,
    /// Died (or quit while dead): this hunt's trophies are discarded.
    Died,
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
            exit_time: 0,
            outcome: None,
            path: 0.0,
            time_s: 0.0,
            amb_rnd,
            amb_sounds,
            current_ambient: None,
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
            objects_on_look: 0,
            shots_made: 0,
            successes: 0,
        }
    }

    fn hunter_view(&self) -> HunterView {
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

    /// Camera position and angles for rendering.
    pub fn camera(&self) -> ([f32; 3], f32, f32) {
        match &self.death {
            Some(d) => (d.pos, d.alpha, d.beta),
            None => {
                let (a, b) = self.player.camera_angles();
                (self.player.eye(), a, b)
            }
        }
    }

    /// Advances one tick of `dt` ms.
    pub fn tick(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        input: &HuntInput,
        dt: i32,
        out: &mut Vec<GameEvent>,
    ) {
        self.real_time += dt;
        self.takt = self.takt.wrapping_add(1);

        if let Some(d) = self.death {
            self.death_cam(area, d, dt);
        } else {
            let mut pev = Vec::new();
            self.player.tick(area, &input.controls, dt as u32, &mut pev);
            out.extend(pev.into_iter().map(GameEvent::Player));
            if self.player.underwater && self.health > 0 {
                // Drowning.
                self.health -= dt * 12;
                if self.health <= 0 {
                    self.kill_hunter(area, None, hunt::BREATH, out);
                }
            }
            self.weapon_input(area, cars, input, out);
            self.ambience(area, dt, out);
            self.path += dt as f32 * self.player.vspeed / 128.0;
            self.time_s += dt as f32 / 1000.0;
        }
        self.weapon_step(dt, out);

        let mut dev = Vec::new();
        let hv = self.hunter_view();
        let mut ctx = Ctx {
            area,
            rng: &mut self.rng,
            dt,
            real_time: self.real_time,
            hunter: hv,
            opt_agres: self.opts.opt_agres,
            tranq: self.opts.tranq,
            view_r: self.opts.view_r,
            events: &mut dev,
        };
        self.herd.step(&mut ctx);
        for e in dev {
            if let DinoEvent::KillHunter { index, kind } = e {
                self.kill_hunter(area, Some(index), kind, out);
            }
            out.push(GameEvent::Dino(e));
        }

        // AnimateProcesses: wind drifts every other frame.
        if self.takt & 1 != 0 {
            self.wind.alpha += self.rng.si_rand(16) as f32 / 4096.0;
            self.wind.speed += self.rng.si_rand(400) as f32 / 6400.0;
        }
        self.wind.speed = self.wind.speed.clamp(4.0, 18.0);
        self.check_afraid(area);
        self.exit_step(dt);

        let mut sev = Vec::new();
        let player = [self.player.x, self.player.y, self.player.z];
        let (view_r, rt, anim) = (self.opts.view_r, self.real_time, self.ship_anim_ms);
        self.ship
            .step(area, &mut self.herd, player, view_r, rt, dt, anim, &mut sev);
        for e in sev {
            if let ShipEvent::Removed { index } = e {
                self.remove_character(index);
            }
            out.push(GameEvent::Ship(e));
        }
    }

    /// `RemoveCharacter`, fixing up every index that refers past it.
    fn remove_character(&mut self, index: usize) {
        if index >= self.herd.chars.len() {
            return;
        }
        self.herd.chars.remove(index);
        self.ship.character_removed(index);
        if let Some(d) = &mut self.herd.demo_index {
            if *d > index {
                *d -= 1;
            }
        }
        if let Some(d) = &mut self.death {
            if d.body > index {
                d.body -= 1;
            }
        }
    }

    /// `AddShipTask`: call the ship for a trophy and record it.
    fn add_ship_task(&mut self, area: &Area, index: usize, out: &mut Vec<GameEvent>) {
        let Some(c) = self.herd.chars.get(index) else {
            return;
        };
        let water = area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2]);
        if water < 100.0 && !self.opts.tranq {
            self.ship.tasks.push_back(index);
            let slot = crate::ship::ship_sound::CALL;
            out.push(GameEvent::Ship(ShipEvent::Sound { slot, at: None }));
        }
        let t = self
            .room
            .bodies
            .iter()
            .take(23)
            .position(|b| b.ctype == 0)
            .unwrap_or(23);
        let base = self
            .herd
            .species
            .get(c.ctype)
            .map_or(0, |s| s.info.base_score);
        let mut score = base as f32;
        if self.successes > 1 {
            score *= 1.0 + self.successes as f32 / 10.0;
        }
        if self.opts.targets & 1u32.checked_shl(c.ai as u32).unwrap_or(0) == 0 {
            score /= 2.0;
        }
        for (on, k) in [
            (self.opts.tranq, 1.25),
            (self.opts.radar, 0.70),
            (self.opts.scent, 0.80),
            (self.opts.camo, 0.85),
        ] {
            if on {
                score *= k;
            }
        }
        self.room.score += score as i32;
        if !self.opts.tranq {
            let (y, mo, d, h, mi) = self.clock;
            let p = &self.player;
            let range =
                ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2))
                    .sqrt()
                    / 64.0;
            self.room.bodies[t] = TrophyItem {
                ctype: c.ctype as i32,
                scale: c.scale,
                weapon: self.weapons.current as i32,
                score: score as i32,
                phase: self.real_time & 3,
                time: (h << 10) + mi,
                date: (y << 20) + (mo << 10) + d,
                range,
                ..Default::default()
            };
            out.push(GameEvent::Trophy {
                slot: t,
                score: score as i32,
            });
        }
    }

    // ------------------------------------------------------------ weapons

    fn weapon_input(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        input: &HuntInput,
        out: &mut Vec<GameEvent>,
    ) {
        if let Some(w) = input.select_weapon {
            if self.weapons.ftime == 0 {
                if self.weapons.shots_left.get(w).copied().unwrap_or(0) == 0 {
                    out.push(GameEvent::Message("No weapon".into()));
                } else {
                    self.weapons.target = w;
                    if self.weapons.state == WeaponState::Hidden {
                        self.weapons.current = w;
                    }
                    self.hide_weapon(out);
                }
            }
        }
        if input.toggle_weapon {
            self.hide_weapon(out);
        }
        if input.fire {
            self.process_shoot(area, cars, out);
        }
    }

    /// `HideWeapon`: draw if hidden, put away if ready.
    pub fn hide_weapon(&mut self, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        if self.player.underwater && w.state == WeaponState::Hidden {
            return;
        }
        if w.state == WeaponState::Hidden {
            if w.shots_left.get(w.current).copied().unwrap_or(0) == 0 {
                return;
            }
            w.optic_mode = w.info.get(w.current).is_some_and(|i| i.optic);
            out.push(GameEvent::WeaponSound {
                weapon: w.current,
                slot: weapon_anim::DRAW,
            });
            w.ftime = 0;
            w.state = WeaponState::Drawing;
            w.shakel = 0.2;
            return;
        }
        if w.state != WeaponState::Ready || w.ftime != 0 {
            return;
        }
        out.push(GameEvent::WeaponSound {
            weapon: w.current,
            slot: weapon_anim::HIDE,
        });
        w.state = WeaponState::Hiding;
        w.ftime = 0;
        w.optic_mode = false;
    }

    /// Weapon animation state machine (from `DrawPostObjects`).
    fn weapon_step(&mut self, dt: i32, out: &mut Vec<GameEvent>) {
        if self.weapons.state == WeaponState::Hidden {
            return;
        }
        let w = &mut self.weapons;
        w.shakel = (w.shakel + dt as f32 / 10000.0).min(4.0);
        let cur = w.current;
        match w.state {
            WeaponState::Drawing => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::DRAW) {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                }
            }
            WeaponState::Reloading => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::RELOAD) {
                    w.ftime = 0;
                    w.state = WeaponState::Ready;
                }
            }
            WeaponState::Ready if w.ftime > 0 => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::FIRE) {
                    w.ftime = 0;
                    let info = &w.info[cur];
                    let left = w.shots_left[cur];
                    if info.reload != 0
                        && left % info.reload == 0
                        && (left > 0 || w.ammo_mag[cur] > 0)
                    {
                        w.state = WeaponState::Reloading;
                        w.ftime = 1;
                        out.push(GameEvent::WeaponSound {
                            weapon: cur,
                            slot: weapon_anim::RELOAD,
                        });
                    }
                    if w.shots_left[cur] == 0 && w.ammo_mag[cur] > 0 {
                        w.ammo_mag[cur] -= 1;
                        w.shots_left[cur] = w.info[cur].shots;
                        if w.anim(weapon_anim::RELOAD) != 0 {
                            w.state = WeaponState::Reloading;
                            w.ftime = 1;
                            out.push(GameEvent::WeaponSound {
                                weapon: cur,
                                slot: weapon_anim::RELOAD,
                            });
                        }
                    }
                }
            }
            WeaponState::Hiding => {
                w.ftime += dt;
                if w.ftime >= w.anim(weapon_anim::HIDE) {
                    w.ftime = 0;
                    w.state = WeaponState::Hidden;
                    if w.current != w.target {
                        w.current = w.target;
                        self.hide_weapon(out);
                    }
                    return;
                }
            }
            _ => {}
        }
        // Out of ammo: put it away and pick another weapon that has some.
        let w = &mut self.weapons;
        if w.shots_left.get(w.current).copied().unwrap_or(0) == 0 {
            if let Some(next) = w.shots_left.iter().position(|&s| s > 0) {
                w.target = next;
            }
            self.hide_weapon(out);
        }
    }

    /// `ProcessShoot`.
    fn process_shoot(&mut self, area: &Area, cars: &[Option<&Car>], out: &mut Vec<GameEvent>) {
        let cur = self.weapons.current;
        if self.weapons.shots_left.get(cur).copied().unwrap_or(0) == 0 {
            return;
        }
        if self.player.underwater {
            self.hide_weapon(out);
            return;
        }
        if self.weapons.state != WeaponState::Ready || self.weapons.ftime != 0 {
            return;
        }
        self.weapons.ftime = 1;
        self.player.head_back_r = 64.0;
        out.push(GameEvent::WeaponSound {
            weapon: cur,
            slot: weapon_anim::FIRE,
        });
        self.shots_made += 1;

        let info = self.weapons.info[cur].clone();
        let (da, db) = self.weapons.sway(self.real_time);
        let p = &self.player;
        let origin = [p.x, p.y + p.head_y, p.z];
        let (alpha, beta) = (p.alpha, p.beta);
        let reach = 256.0 * self.opts.view_r as f32;
        for _ in 0..=info.trace_c.max(0) {
            let ra = self.rng.si_rand(128) as f32 * 0.0001 * (2.0 - info.prec);
            let rb = self.rng.si_rand(128) as f32 * 0.0001 * (2.0 - info.prec);
            let (sa, ca) = (alpha + da + ra).sin_cos();
            let (sb, cb) = (beta + db + rb).sin_cos();
            let nv = [sa * cb, -sb, -ca * cb];
            let to = [
                origin[0] + nv[0] * reach,
                origin[1] + nv[1] * reach,
                origin[2] + nv[2] * reach,
            ];
            self.make_shot(area, cars, &info, origin, to, out);
        }
        self.make_noise(
            [self.player.x, self.player.y, self.player.z],
            self.opts.view_r as f32 * 200.0 * info.loud,
        );
        self.weapons.shots_left[cur] -= 1;
    }

    /// `MakeShot`: trace one round (in three falling segments for the
    /// crossbow) and apply damage.
    fn make_shot(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        info: &WeaponInfo,
        a: [f32; 3],
        b: [f32; 3],
        out: &mut Vec<GameEvent>,
    ) {
        let chars = &self.herd.chars;
        let res = if !info.fall {
            trace_shot(area, chars, cars, a, b)
        } else {
            let dy = 40.0 * self.opts.view_r as f32 / 36.0;
            let dl = [
                (b[0] - a[0]) / 3.0,
                (b[1] - a[1]) / 3.0,
                (b[2] - a[2]) / 3.0,
            ];
            let mut from = a;
            let mut r = None;
            for drop in [dy / 2.0, dy * 3.0, dy * 5.0] {
                let to = [from[0] + dl[0], from[1] + dl[1] - drop, from[2] + dl[2]];
                r = trace_shot(area, chars, cars, from, to);
                if r.is_some() {
                    break;
                }
                from = to;
            }
            r
        };
        let Some((hit, pos)) = res else { return };
        out.push(GameEvent::Impact { hit, pos });
        let Hit::Character { index, mortal } = hit else {
            return;
        };
        let c = &mut self.herd.chars[index];
        if c.health == 0 {
            return;
        }
        if mortal {
            c.health = 0;
        } else {
            c.health -= info.power as i32;
        }
        c.health = c.health.max(0);
        if c.health == 0 {
            let trophy = c.ai >= 10;
            let ct = c.ctype;
            out.push(GameEvent::Killed { index, trophy });
            if trophy {
                self.successes += 1;
                self.add_ship_task(area, index, out);
            } else {
                self.add_secondary_one(area, ct);
            }
        } else {
            c.afraid_time = 60 * 1000;
            if c.ai != ai::TREX || c.state == 0 {
                c.state = 2;
            }
            if c.ai != ai::BRACH {
                c.blood_ttime += 90000;
            }
        }
        let c = &mut self.herd.chars[index];
        if c.ai == ai::TREX {
            c.state = if c.state != 0 { 5 } else { 1 };
        }
    }

    /// `Characters_AddSecondaryOne`: replace a killed ambient animal.
    fn add_secondary_one(&mut self, area: &Area, ctype: usize) {
        if self.herd.chars.len() > 64 {
            return;
        }
        let (px, pz) = (self.player.x, self.player.z);
        for _ in 0..128 {
            let mut p = [
                px + self.rng.si_rand(20040) as f32,
                0.0,
                pz + self.rng.si_rand(20040) as f32,
            ];
            p[1] = area.land_height(p[0], p[2]);
            if crate::dino::check_place_collision_p(area, &mut p) {
                continue;
            }
            if (p[0] - px).abs() + (p[2] - pz).abs() < 256.0 * 40.0 {
                continue;
            }
            let mut c = Character {
                ctype,
                pos: p,
                tgx: p[0],
                tgz: p[2],
                ..Default::default()
            };
            if let Some(sp) = self.herd.species.get(ctype) {
                c.reset(sp, &mut self.rng, self.opts.opt_agres);
            }
            self.herd.chars.push(c);
            return;
        }
    }

    /// `MakeNoise`: startle everything within `range`.
    pub fn make_noise(&mut self, pos: [f32; 3], range: f32) {
        for c in &mut self.herd.chars {
            if c.health == 0 {
                continue;
            }
            let l = ((c.pos[0] - pos[0]).powi(2)
                + (c.pos[1] - pos[1]).powi(2)
                + (c.pos[2] - pos[2]).powi(2))
            .sqrt();
            if l > range {
                continue;
            }
            if c.ai == ai::TREX {
                if c.state == 0 {
                    c.state = 2;
                }
            } else {
                c.afraid_time = ((10.0 + (range - l) / 256.0) as i32) * 1024;
                c.state = 2;
                c.no_find_cnt = 0;
            }
        }
    }

    // ------------------------------------------------------------ detection

    /// `CheckAfraid`: can each big animal see or smell the hunter?
    fn check_afraid(&mut self, area: &Area) {
        if self.health <= 0 || self.player.underwater {
            return;
        }
        let p = &self.player;
        let ppos = [p.x, p.y, p.z];
        let head_y = p.head_y;
        let wlook = self.wind.nv();
        let kmask = if self.opts.camo { 1.5 } else { 1.0 };
        let kscent = if self.opts.scent { 1.5 } else { 1.0 };
        for i in 0..self.herd.chars.len() {
            let c = &self.herd.chars[i];
            if c.health == 0 || c.ai < 10 || c.afraid_time != 0 || c.state == 1 {
                continue;
            }
            let Some(sp) = self.herd.species.get(c.ctype) else {
                continue;
            };
            let mut rlook = [ppos[0] - c.pos[0], ppos[1] - c.pos[1], ppos[2] - c.pos[2]];
            let rl = (rlook[0] * rlook[0] + rlook[1] * rlook[1] + rlook[2] * rlook[2]).sqrt();
            let mut kr = rl / 256.0 / (32.0 + self.opts.view_r as f32 / 2.0);
            if rl > 0.0 {
                rlook = [rlook[0] / rl, rlook[1] / rl, rlook[2] / rl];
            }
            kr *= 2.5 / (1.5 + self.opts.opt_sens as f32 / 128.0);
            if kr > 3.0 {
                continue;
            }
            let kwind = (wlook[0] * rlook[0] + wlook[2] * rlook[2]) * self.wind.speed / 10.0;
            let klook = -(c.lookx * rlook[0] + c.lookz * rlook[2]);
            let kstand = if head_y > 180.0 { 0.7 } else { 1.2 };

            let mut k_look = kr * ((klook + 3.0) / 3.0) * kstand * kmask;
            if klook > 0.3 {
                k_look *= 2.0;
            }
            if klook > 0.8 {
                k_look *= 2.0;
            }
            k_look /= sp.info.look_k.max(1e-3);
            let eye = [c.pos[0], c.pos[1] + 220.0, c.pos[2]];
            if k_look < 1.0 {
                let (blocked, n) =
                    trace_look(area, eye, [ppos[0], ppos[1] + head_y / 2.0, ppos[2]]);
                self.objects_on_look = n;
                if blocked {
                    k_look *= 1.3;
                }
            }
            if k_look < 1.0 {
                let (blocked, n) = trace_look(area, eye, [ppos[0], ppos[1] + head_y, ppos[2]]);
                self.objects_on_look = n;
                if blocked {
                    k_look = 2.0;
                }
            }
            k_look *= 1.0 + self.objects_on_look as f32 / 6.0;

            let mut k_smell = kr * ((kwind + 2.0) / 2.0) * ((klook + 3.0) / 3.0) * kscent;
            if kwind > 0.0 {
                k_smell *= 2.0;
            }
            k_smell /= sp.info.smell_k.max(1e-3);

            let mut k_res = k_look.min(k_smell);
            if k_res < 1.0 {
                k_res = k_res.min(kr);
                let c = &mut self.herd.chars[i];
                c.afraid_time = (1.0 / (k_res + 0.1) * 10.0 * 1000.0) as i32;
                c.state = 2;
                if c.ai == ai::TREX && k_look > k_smell {
                    c.state = 3;
                }
                c.no_find_cnt = 0;
            }
        }
    }

    // ------------------------------------------------------------ death

    /// `AddDeadBody`: the hunter dies; a body is added and the camera
    /// switches to the death cam.
    fn kill_hunter(&mut self, area: &Area, by: Option<usize>, kind: i32, out: &mut Vec<GameEvent>) {
        if self.death.is_some() {
            return;
        }
        self.health = 0;
        self.weapons.optic_mode = false;
        let (cam, alpha, beta) = self.camera();
        let mut body = Character {
            ctype: 0,
            alpha,
            ..Default::default()
        };
        if let Some(sp) = self.herd.species.first() {
            body.reset(sp, &mut self.rng, self.opts.opt_agres);
        }
        body.health = 0;
        body.pos = [self.player.x, self.player.y, self.player.z];
        let killer = by.and_then(|i| self.herd.chars.get(i).map(|c| (i, c.clone())));
        if let Some((_, c)) = &killer {
            let pl = match c.ai {
                ai::SPINO => 200.0,
                ai::CERAT => 320.0,
                ai::TREX => 0.0,
                _ => 170.0,
            };
            let (x, z) = (
                c.pos[0] + c.lookx * pl * c.scale,
                c.pos[2] + c.lookz * pl * c.scale,
            );
            body.pos = [x, area.land_qh(x, z, self.player.y), z];
        }
        body.phase = kind.max(0) as usize;
        body.prev_phase = body.phase;
        self.herd.chars.push(body);
        let index = self.herd.chars.len() - 1;
        self.herd.demo_index = Some(index);
        if let Some((k, c)) = killer {
            if c.ai == ai::TREX {
                // The T-Rex carries the body: same size and heading, and the
                // camera watches the T-Rex.
                let b = &mut self.herd.chars[index];
                b.scale = c.scale;
                b.alpha = c.alpha;
                self.herd.demo_index = Some(k);
            }
        }
        self.death = Some(DeathCam {
            body: index,
            time: 1,
            pos: cam,
            alpha,
            beta,
        });
        out.push(GameEvent::HunterDied { kind });
    }

    /// `ProcessDemoMovement`: fly to the body, then circle it.
    fn death_cam(&mut self, area: &Area, mut d: DeathCam, dt: i32) {
        let dtf = dt as f32;
        let Some(watched) = self.herd.demo_index.and_then(|i| self.herd.chars.get(i)) else {
            return;
        };
        let trex = watched.ai == ai::TREX;
        let mut target = watched.pos;
        target[1] += if trex { 256.0 + 512.0 } else { 256.0 };
        let base = if trex { 1424.0 } else { 824.0 };
        let l = ((target[0] - d.pos[0]).powi(2) + (target[2] - d.pos[2]).powi(2)).sqrt();
        if d.time == 1 && l < base {
            d.time = 2;
        }
        let mut nv = [
            target[0] - d.pos[0],
            target[1] - d.pos[1],
            target[2] - d.pos[2],
        ];
        let nl = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2])
            .sqrt()
            .max(1e-6);
        nv = [nv[0] / nl, nv[1] / nl, nv[2] / nl];
        if d.time == 1 {
            delta_func(&mut d.pos[0], target[0], nv[0].abs() * dtf * 3.0);
            delta_func(&mut d.pos[2], target[2], nv[2].abs() * dtf * 3.0);
        } else {
            d.time += dt;
            d.alpha += dtf / 1224.0;
            let (sa, ca) = d.alpha.sin_cos();
            delta_func(&mut d.pos[0], target[0] - sa * base, dtf);
            delta_func(&mut d.pos[2], target[2] + ca * base, dtf);
        }
        let horiz = ((target[0] - d.pos[0]).powi(2) + (target[2] - d.pos[2]).powi(2)).sqrt();
        let mut b = find_vector_alpha(horiz, target[1] - d.pos[1] - 400.0);
        if b > PI {
            b -= 2.0 * PI;
        }
        delta_func(&mut d.beta, -b, dtf / 4000.0);
        let h = area.land_qh(d.pos[0], d.pos[2], d.pos[1]);
        delta_func(&mut d.pos[1], h + 128.0, dtf / 8.0);
        if d.pos[1] < h + 80.0 {
            d.pos[1] = h + 80.0;
        }
        self.death = Some(d);
    }

    /// How the hunt ended, once it has (death: 12 s after dying).
    pub fn outcome(&self) -> Option<Outcome> {
        if self.death.is_some_and(|d| d.time > 12_000) {
            return Some(Outcome::Died);
        }
        self.outcome
    }

    /// Show the exit prompt (`EXITMODE`, or 6 s after death).
    pub fn show_exit(&self) -> bool {
        self.exit_prompt || self.death.is_some_and(|d| d.time > 6_000)
    }

    /// Debug helper (screenshots, tools): kill character `index` outright as
    /// if by a vital shot, scoring it and calling the ship.
    pub fn debug_bag(&mut self, area: &Area, index: usize, out: &mut Vec<GameEvent>) {
        let Some(c) = self.herd.chars.get_mut(index) else {
            return;
        };
        if c.health == 0 {
            return;
        }
        c.health = 0;
        let trophy = c.ai >= 10;
        out.push(GameEvent::Killed { index, trophy });
        if trophy {
            self.successes += 1;
            self.add_ship_task(area, index, out);
        }
    }

    /// Ambient loop by zone and random wildlife sounds (`ProcessControls`).
    fn ambience(&mut self, area: &Area, dt: i32, out: &mut Vec<GameEvent>) {
        let [cx, cy, cz] = self.player.eye();
        let (ax, az) = (
            ((cx as i32) >> 9).clamp(0, 511) as usize,
            ((cz as i32) >> 9).clamp(0, 511) as usize,
        );
        let zone = usize::from(area.map.ambient.get(az * 512 + ax).copied().unwrap_or(0));
        let want = if self.player.underwater {
            None
        } else {
            Some(zone)
        };
        if self.current_ambient != Some(want) {
            self.current_ambient = Some(want);
            let volume = match want {
                None => 240,
                Some(z) => area.rsc.ambients.get(z).map_or(0, |a| a.volume),
            };
            out.push(GameEvent::Ambient { zone: want, volume });
        }
        let Some(z) = want else { return };
        let (Some(t), Some(list), Some(amb)) = (
            self.amb_rnd.get_mut(z),
            self.amb_sounds.get(z),
            area.rsc.ambients.get(z),
        ) else {
            return;
        };
        if list.is_empty() {
            return;
        }
        *t -= dt;
        if *t > 0 {
            return;
        }
        let freq = amb.random[0].freq;
        *t = (freq / 2 + self.rng.r_rand(freq)) * 1000;
        let rr = self.rng.rand() as usize % list.len();
        let r = list[rr];
        let pos = [
            cx + self.rng.si_rand(4096) as f32,
            cy + self.rng.si_rand(256) as f32,
            cz + self.rng.si_rand(4096) as f32,
        ];
        out.push(GameEvent::RandomSound {
            sound: r.number.max(0) as usize,
            pos,
            volume: r.volume,
        });
    }

    /// Escape: toggle the exit prompt.
    pub fn press_escape(&mut self) {
        if self.exit_time == 0 {
            self.exit_prompt = !self.exit_prompt;
        }
    }

    /// Y / Enter at the prompt: leave (in 4 s if alive).
    pub fn confirm_exit(&mut self) {
        if self.show_exit() && self.exit_time == 0 {
            self.exit_time = if self.health > 0 { 4000 } else { 1 };
            self.exit_prompt = false;
        }
    }

    /// Counts down a confirmed exit; on leaving alive, folds this hunt into
    /// the totals and updates the rank (`AnimateProcesses`, `SaveTrophy`).
    fn exit_step(&mut self, dt: i32) {
        if self.exit_time == 0 || self.outcome.is_some() {
            return;
        }
        self.exit_time -= dt;
        if self.exit_time > 0 {
            return;
        }
        if self.health > 0 {
            let r = &mut self.room;
            r.last = c2_formats::sav::Stats {
                shots_made: self.shots_made as i32,
                success: self.successes as i32,
                path: self.path,
                time: self.time_s,
            };
            r.total.shots_made += r.last.shots_made;
            r.total.success += r.last.success;
            r.total.path += r.last.path;
            r.total.time += r.last.time;
            r.rank = if r.score >= 300 {
                2
            } else if r.score >= 100 {
                1
            } else {
                0
            };
            self.outcome = Some(Outcome::Exited);
        } else {
            self.outcome = Some(Outcome::Died);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dino::{DinoInfo, Species};
    use c2_formats::car::Animation;
    use c2_formats::model::{face_flags, Face, Mesh, Vertex};

    /// A "dinosaur" that is one big upright triangle (800 wide, 800 tall).
    fn target_car(mortal: bool) -> Car {
        let tri = [-3200i16, 0, 0, 3200, 0, 0, 0, 6400, 0];
        let data: Vec<i16> = tri.iter().chain(tri.iter()).copied().collect();
        Car {
            name_raw: [0; 32],
            mesh: Mesh {
                vertices: vec![
                    Vertex {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        owner: 0,
                        hide: 0
                    };
                    3
                ],
                faces: vec![Face {
                    v: [0, 1, 2],
                    tu: [0; 3],
                    tv: [0; 3],
                    flags: if mortal { face_flags::MORTAL } else { 0 },
                    dmask: 0,
                    distant: 0,
                    next: 0,
                    group: 0,
                    reserved: [0; 12],
                }],
                texture: vec![],
            },
            animations: (0..8)
                .map(|_| Animation {
                    name_raw: [0; 32],
                    kps: 2,
                    frames: 2,
                    data: data.clone(),
                })
                .collect(),
            sounds: vec![],
            anim_sounds: None,
        }
    }

    fn info(ai_id: i32) -> DinoInfo {
        DinoInfo {
            name: "t".into(),
            file: String::new(),
            ai: ai_id,
            health0: 5,
            mass: 1.0,
            length: 1.0,
            radius: 256.0,
            base_score: 1,
            smell_k: 1.0,
            hear_k: 1.0,
            look_k: 1.0,
            sh_delta: 0.0,
            scale0: 1000,
            scale_a: 0,
            danger_call: false,
        }
    }

    fn rifle() -> WeaponInfo {
        WeaponInfo {
            name: "Rifle".into(),
            file: String::new(),
            pic: String::new(),
            power: 4.0,
            prec: 1.8,
            loud: 1.5,
            rate: 1.0,
            shots: 14,
            reload: 0,
            trace_c: 0,
            optic: false,
            fall: false,
        }
    }

    fn blank_save() -> Save {
        Save {
            player_name_raw: [0; 128],
            reg_number: 0,
            score: 0,
            rank: 0,
            last: Default::default(),
            total: Default::default(),
            bodies: [TrophyItem::default(); 24],
            options: Default::default(),
        }
    }

    /// Hunter at the map centre looking -z; a Stegosaurus-AI target 2000 ahead.
    fn setup(mortal: bool) -> (Area, Hunt, Car) {
        let area = Area::flat(10);
        let car = target_car(mortal);
        let player = Player::new(&area, (500, 500));
        let mut rng = MsRand::new(5);
        let species = vec![
            Species::new(info(0), &car),
            Species::new(info(ai::STEGO), &car),
        ];
        let mut herd = Herd {
            species,
            chars: vec![],
            demo_index: None,
        };
        let mut c = Character {
            ctype: 1,
            alpha: PI / 2.0,
            pos: [player.x, 640.0, player.z - 2000.0],
            ..Default::default()
        };
        c.reset(&herd.species[1], &mut rng, 128);
        herd.chars.push(c);
        let weapons = Weapons::new(vec![rifle()], vec![[100, 100, 100, 100]], 1, false);
        let opts = HuntOptions::default();
        let hunt = Hunt::new(&area, player, herd, weapons, rng, opts, blank_save());
        (area, hunt, car)
    }

    /// Ticks without stepping the herd (keeps the target still).
    fn weapon_ticks(
        h: &mut Hunt,
        area: &Area,
        cars: &[Option<&Car>],
        input: HuntInput,
        n: usize,
        out: &mut Vec<GameEvent>,
    ) {
        for _ in 0..n {
            h.weapon_input(area, cars, &input, out);
            h.weapon_step(10, out);
        }
    }

    #[test]
    fn draw_fire_and_ammo() {
        let (area, mut h, car) = setup(false);
        let cars = [Some(&car), Some(&car)];
        let mut ev = Vec::new();
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                toggle_weapon: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        assert_eq!(h.weapons.state, WeaponState::Drawing);
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
        assert_eq!(h.weapons.state, WeaponState::Ready);
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                fire: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        assert_eq!(h.weapons.shots_left[0], 13);
        assert!(ev.contains(&GameEvent::WeaponSound {
            weapon: 0,
            slot: weapon_anim::FIRE
        }));
        // Held fire doesn't re-fire until the fire animation finishes.
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                fire: true,
                ..Default::default()
            },
            5,
            &mut ev,
        );
        assert_eq!(h.weapons.shots_left[0], 13);
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                fire: true,
                ..Default::default()
            },
            12,
            &mut ev,
        );
        assert_eq!(h.weapons.shots_left[0], 12);
    }

    #[test]
    fn vital_hit_kills_and_noise_alarms() {
        let (area, mut h, car) = setup(true);
        let cars = [Some(&car), Some(&car)];
        let mut ev = Vec::new();
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                toggle_weapon: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                fire: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        assert!(
            ev.iter().any(|e| matches!(
                e,
                GameEvent::Impact {
                    hit: Hit::Character {
                        index: 0,
                        mortal: true
                    },
                    ..
                }
            )),
            "{ev:?}"
        );
        assert_eq!(h.herd.chars[0].health, 0);
        assert!(ev.contains(&GameEvent::Killed {
            index: 0,
            trophy: true
        }));
        assert_eq!(h.successes, 1);
    }

    #[test]
    fn body_hit_wounds_and_alarms() {
        let (area, mut h, car) = setup(false);
        let cars = [Some(&car), Some(&car)];
        let mut ev = Vec::new();
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                toggle_weapon: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                fire: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        let c = &h.herd.chars[0];
        assert_eq!(c.health, 1, "5 - power 4");
        assert_eq!(c.state, 2, "alarmed");
        assert!(c.afraid_time > 0 && c.blood_ttime > 0);
    }

    #[test]
    fn trophy_is_recorded_and_the_ship_takes_the_body() {
        let (area, mut h, car) = setup(true);
        let cars = [Some(&car), Some(&car)];
        h.ship_anim_ms = 2000;
        let mut ev = Vec::new();
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                toggle_weapon: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
        weapon_ticks(
            &mut h,
            &area,
            &cars,
            HuntInput {
                fire: true,
                ..Default::default()
            },
            1,
            &mut ev,
        );
        assert_eq!(h.ship.tasks.len(), 1);
        // Base score 1, halved for a non-target species.
        assert!(
            ev.contains(&GameEvent::Trophy { slot: 0, score: 0 }),
            "{ev:?}"
        );
        assert_eq!(h.room.bodies[0].ctype, 1);
        let n = h.herd.chars.len();
        let mut removed = false;
        for _ in 0..12_000 {
            ev.clear();
            h.tick(&area, &cars, &HuntInput::default(), 10, &mut ev);
            if ev
                .iter()
                .any(|e| matches!(e, GameEvent::Ship(ShipEvent::Removed { .. })))
            {
                removed = true;
                break;
            }
        }
        assert!(removed, "ship state {} at {:?}", h.ship.state, h.ship.pos);
        assert_eq!(h.herd.chars.len(), n - 1);
    }
}
