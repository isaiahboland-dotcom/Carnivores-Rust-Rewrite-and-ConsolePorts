//! A hunt in progress: the hunter, the herd, weapons, wind and detection,
//! advanced in fixed ticks (`Hunt2.cpp: ProcessGame`).
//!
//! Sources: `ProcessShoot`, `HideWeapon` (Hunt2.cpp), the weapon state
//! machine in `DrawPostObjects`, `MakeShot`, `MakeNoise`, `CheckAfraid`,
//! `AnimateProcesses` (wind), `AddDeadBody`, `ProcessDemoMovement`.

use c2_formats::bits::Bits;
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

mod survival;
pub use survival::{
    dangerous, dangerous_ai, is_trex, modded, MobileWaves, Survival, SurvivalKind, SurvivalOptions,
    TREX_WAVE,
};

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
    /// The Modder's Edition's weapon keys, when the weapon has any.
    pub mee: Option<MeeWeapon>,
}

/// A dialect weapon's extras (`Resources.cpp: ReadWeaponLine` in the
/// Modder's Edition): flying bullets, recoil, firing modes, trackers.
#[derive(Debug, Clone, PartialEq)]
pub struct MeeWeapon {
    /// `land_veloc`: bullets fly `64 * veloc` units per 17 ms
    /// (`AddBullet`); 0 (not given) keeps the original's instant trace, as
    /// the engine's own bullets wouldn't move.
    pub veloc: f32,
    /// `land_fall`: what a bullet's fall speed grows by per 17 ms.
    pub fall: f32,
    /// `recoil`: the view kicks up by `recoil / 100` rad (`ProcessShoot`).
    pub recoil: f32,
    /// `shake`: the aim's sway starts at `shake * 4` when drawn.
    pub shake: Option<f32>,
    /// `canRun`: the hunter may run with it out.
    pub can_run: bool,
    /// `fullAuto`: fires while the trigger is held; else once per pull.
    pub full_auto: bool,
    /// `cannotMortal`: a vital hit only does its power.
    pub cannot_mortal: bool,
    /// `radar` (`radR`, `radG`, `radB`, `radTime`): an animal it hits shows
    /// on the map in this colour, for `radTime` ms (0: for good).
    pub radar: Option<([u8; 3], i32)>,
    /// `optic`: the scope's zoom (the original's is 3).
    pub zoom: f32,
    /// `cross`: cross-hairs on the model's last vertex, scoped or (without
    /// `optic`) always; not given: none.
    pub cross: bool,
    /// `unzoom`: no zoom or cross-hairs while drawing, hiding, reloading.
    pub unzoom: bool,
    /// Model animations for draw, fire, hide, reload (`getAnim`, `shtAnim`,
    /// `putAnim`, `rldAnimFull`); the original order when not given.
    pub anims: [usize; 4],
}

impl MeeWeapon {
    /// The keys that make a weapon a dialect one.
    const KEYS: &[&str] = &[
        "land_veloc",
        "land_fall",
        "recoil",
        "shake",
        "canrun",
        "fullauto",
        "cannotmortal",
        "radar",
        "getanim",
        "shtanim",
        "putanim",
        "rldanimfull",
        "cross",
        "unzoom",
    ];

    fn from_def(d: &WeaponDef) -> Option<Self> {
        let get = |k: &str| {
            d.extra
                .iter()
                .find(|e| e.key.eq_ignore_ascii_case(k))
                .map(|e| e.value.trim().to_string())
        };
        if !Self::KEYS.iter().any(|k| get(k).is_some()) {
            return None;
        }
        let num = |k: &str| get(k).map_or(0.0, |v| c2_formats::res_txt::c_atof(&v));
        let flag = |k: &str| get(k).is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");
        let anim = |k: &str, default: usize| {
            get(k)
                .map(|v| c2_formats::res_txt::c_atoi(&v))
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(default)
        };
        let colour = |k: &str| num(k).clamp(0.0, 255.0) as u8;
        Some(Self {
            veloc: num("land_veloc").max(0.0),
            fall: num("land_fall"),
            recoil: num("recoil"),
            shake: get("shake").map(|v| c2_formats::res_txt::c_atof(&v)),
            can_run: flag("canRun"),
            zoom: d.optic.unwrap_or(0.0),
            cross: flag("cross"),
            unzoom: flag("unzoom"),
            full_auto: flag("fullAuto"),
            cannot_mortal: flag("cannotMortal"),
            radar: flag("radar").then(|| {
                (
                    [colour("radR"), colour("radG"), colour("radB")],
                    num("radTime") as i32,
                )
            }),
            anims: [
                anim("getAnim", 0),
                anim("shtAnim", 1),
                anim("putAnim", 2),
                anim("rldAnimFull", 3),
            ],
        })
    }
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
            optic: d.optic.unwrap_or(0.0) != 0.0,
            fall: d.fall.unwrap_or(0) != 0,
            mee: MeeWeapon::from_def(d),
        }
        .read_optic(d)
    }

    /// The original reads `optic` as a whole number (`optic = 0.5`: none).
    fn read_optic(mut self, d: &WeaponDef) -> Self {
        if self.mee.is_none() {
            self.optic = d.optic.unwrap_or(0.0).trunc() != 0.0;
        }
        self
    }

    /// The model animation (and sound) for a `weapon_anim` slot.
    pub fn anim_of(&self, slot: usize) -> usize {
        self.mee
            .as_ref()
            .and_then(|m| m.anims.get(slot).copied())
            .unwrap_or(slot)
    }
}

/// Weapon `.CAR` animation and sound slots.
pub mod weapon_anim {
    pub const DRAW: usize = 0;
    pub const FIRE: usize = 1;
    pub const HIDE: usize = 2;
    pub const RELOAD: usize = 3;
}

/// A dialect round in flight (`TBullet`).
#[derive(Debug, Clone, PartialEq)]
pub struct Bullet {
    pub pos: [f32; 3],
    /// Units per 17 ms.
    pub vel: [f32; 3],
    pub weapon: usize,
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
    /// What `new` was given, for [`resupply`](Self::resupply).
    present: Bits,
    double_ammo: bool,
}

impl Weapons {
    /// `ReInitGame`: full ammo for every weapon in `present` (bit mask).
    pub fn new(
        info: Vec<WeaponInfo>,
        anim_ms: Vec<[i32; 4]>,
        present: Bits,
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
            present,
            double_ammo,
        };
        let mut first = None;
        for i in 0..n.min(Bits::LEN) {
            if present.has(i) {
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

    /// Every carried weapon back to the ammo it started with (not in the
    /// original: the 3DS "extra ammo" gear).
    pub fn resupply(&mut self) {
        for i in 0..self.info.len().min(Bits::LEN) {
            if self.present.has(i) {
                self.shots_left[i] = self.info[i].shots;
                self.ammo_mag[i] = i32::from(self.double_ammo);
            }
        }
    }

    /// Whether weapon `i` was taken on the hunt.
    pub fn carried(&self, i: usize) -> bool {
        self.present.has(i)
    }

    fn anim(&self, slot: usize) -> i32 {
        self.anim_ms.get(self.current).map_or(0, |a| a[slot])
    }

    /// The model animation and time to pose the view model with.
    pub fn pose(&self) -> Option<(usize, i32)> {
        match self.state {
            WeaponState::Hidden => None,
            s => {
                let slot = s as usize - 1;
                let anim = self
                    .info
                    .get(self.current)
                    .map_or(slot, |i| i.anim_of(slot));
                Some((anim, self.ftime))
            }
        }
    }

    /// The current weapon's dialect extras, if any.
    pub fn mee(&self) -> Option<&MeeWeapon> {
        self.info.get(self.current).and_then(|i| i.mee.as_ref())
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
        /// The hunter screams (a dialect kill type may say not to).
        scream: bool,
    },
    Ship(ShipEvent),
    /// The looping ambience changed: `zone` = RSC ambient index, `None` =
    /// underwater loop. `volume` is `AVolume` (0..=256).
    Ambient {
        zone: Option<usize>,
        volume: i32,
    },
    /// A one-off RSC random sound (`RandSound[sound]`) near the hunter.
    /// A hunting call: `call` is the target AI (10..), `variant` 0..=2 picks
    /// `CALL{call-9}_A/B/C.WAV`; `pos = None` is the hunter's own call.
    Call {
        call: i32,
        variant: usize,
        pos: Option<[f32; 3]>,
    },
    RandomSound {
        sound: usize,
        pos: [f32; 3],
        volume: i32,
    },
    /// A trophy was registered (slot in the trophy room).
    Trophy {
        slot: usize,
        score: i32,
    },
    /// A kill added `score` to the account (species `ctype`); also with the
    /// tranquilizer, where no trophy is kept.
    Scored {
        ctype: usize,
        score: i32,
        /// Distance of the kill, metres (the trophy's "range").
        range: f32,
        tranq: bool,
    },
    Message(String),
}

/// A kill this hunt (the frontends name its parts for the record).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntKill {
    pub ctype: usize,
    pub score: i32,
    /// Metres.
    pub range: f32,
    pub weapon: usize,
}

/// Hunt options (from the menu / save file).
#[derive(Debug, Clone, Copy)]
pub struct HuntOptions {
    /// Bit `ai` set for each species chosen as a target (`TargetDino`).
    pub targets: Bits,
    pub radar: bool,
    pub opt_agres: i32,
    pub opt_sens: i32,
    pub view_r: i32,
    /// Cells drawn (at least `view_r`; more with a longer PC render
    /// distance): how far shots carry and where animals out of sight are
    /// re-placed. Detection and aggression keep `view_r`.
    pub render_r: i32,
    pub camo: bool,
    pub scent: bool,
    pub tranq: bool,
    /// Observer mode (`-observ`): no weapons or calls, animals ignore you.
    pub observer: bool,
    /// The trophy room (`TrophyMode`): the area is `TROPHY.MAP` and the
    /// animals are the saved trophies, standing still.
    pub trophy: bool,
    /// Multiplies every kill's score (1 in the original; the 3DS's extra
    /// ammo gear lowers it).
    pub score_factor: f32,
    /// Multiplies the animals' running speed (1 in the original; a 3DS
    /// setting, 0.5..=1).
    pub dino_run_k: f32,
    /// Blood (setting; on in the original): off drops the spray of a hit
    /// and the wounded animals' trails.
    pub blood: bool,
    /// The death camera (setting; on in the original): off shows a black
    /// screen and the leave / restart prompt at once.
    pub death_anim: bool,
    /// `CheckAfraid` every this many ticks (1; the 3DS: 2, i.e. 25 times a
    /// second, as the original did once a frame at 25 fps). Its look
    /// traces are the costliest part of a tick on an Old 3DS.
    pub afraid_every: u32,
    /// Bit `10 + slot` per call slot whose kills score +10% (not in the
    /// original; the 3DS's roaming add-ons, hunted with no call).
    pub no_call_bonus: Bits,
    /// How far animals notice the hunter, by sight or smell, as a share of
    /// the original's distance (1; the 3DS's "3DS friendly" setting: less,
    /// to match its shorter view). Every animal, modded ones too.
    pub notice_k: f32,
    /// Stealth crouch (a 3DS setting): crouched, the hunter is half as easy
    /// to see and his shots carry half as far.
    pub stealth: bool,
}

impl Default for HuntOptions {
    fn default() -> Self {
        Self {
            targets: Bits::NONE,
            radar: false,
            opt_agres: 128,
            opt_sens: 128,
            view_r: 74,
            render_r: 74,
            camo: false,
            scent: false,
            tranq: false,
            observer: false,
            trophy: false,
            score_factor: 1.0,
            dino_run_k: 1.0,
            blood: true,
            death_anim: true,
            afraid_every: 1,
            no_call_bonus: Bits::NONE,
            notice_k: 1.0,
            stealth: false,
        }
    }
}

/// Call ids are `10 + menu slot` (the original's AI number); one past the
/// last the target set (`Bits`) can hold.
pub const MAX_CALL: i32 = Bits::LEN as i32;

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
    /// Binoculars key pressed.
    pub toggle_binoculars: bool,
    /// Binocular zoom keys held.
    pub zoom_in: bool,
    pub zoom_out: bool,
    /// Call key pressed (`kfCall`).
    pub call: bool,
    /// Change-call key pressed.
    pub change_call: bool,
    /// Map key pressed.
    pub toggle_map: bool,
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
    /// Dialect bullets in flight (`bullet[]`).
    pub bullets: Vec<Bullet>,
    /// The view's recoil still to come, yaw and pitch per 17 ms (`Recoil`).
    recoil: [f32; 2],
    /// The trigger was held last tick (`alreadyFired`).
    fire_held: bool,
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
    exit_time: i32,
    outcome: Option<Outcome>,
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
    amb_rnd: Vec<i32>,
    /// Random sounds allowed per zone (night drops flagged ones).
    amb_sounds: Vec<Vec<c2_formats::rsc::RandomSoundRef>>,
    current_ambient: Option<Option<usize>>,
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
    call_lock: i32,
    next_call: i32,
    /// A pending answer to a call: (time left, position, call).
    answer: Option<(i32, [f32; 3], i32)>,
    /// The trophy slot whose plaque is showing (`TrophyBody`), and for how
    /// much longer after a kill (`TrophyTime`, ms; 0 = while in range).
    pub trophy_body: Option<usize>,
    pub trophy_time: i32,
}

/// What the binoculars show about the animal in the cross-hairs.
#[derive(Debug, Clone, PartialEq)]
pub struct LifeInfo {
    pub index: usize,
    pub name: String,
    /// Tonnes (`Mass * scale^2`).
    pub weight: f32,
    /// World units.
    pub distance: f32,
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
        // The main loop skips processing while paused, unless dead.
        if self.paused && self.health > 0 {
            return;
        }
        self.prof.start();
        self.real_time += dt;
        self.takt = self.takt.wrapping_add(1);
        // ProcessSyncro: health recovers 4 per ms while alive.
        if self.health > 0 {
            self.health = (self.health + dt * 4).min(MAX_HEALTH);
        }

        if let Some(d) = self.death {
            self.death_cam(area, d, dt);
        } else {
            if self.opts.trophy {
                let sp = &self.herd.species;
                self.player.obstacles = self
                    .herd
                    .chars
                    .iter()
                    .map(|c| {
                        [
                            c.pos[0],
                            c.pos[2],
                            sp.get(c.ctype).map_or(0.0, |s| s.info.radius),
                        ]
                    })
                    .collect();
            }
            let mut pev = Vec::new();
            // Hunt2.cpp: running only with the weapon away (the dialect's
            // `canRun` weapons too).
            let mut controls = input.controls;
            controls.run &= self.weapons.state == WeaponState::Hidden
                || self.weapons.mee().is_some_and(|m| m.can_run);
            self.player.tick(area, &controls, dt as u32, &mut pev);
            self.apply_recoil(dt);
            self.prof.mark(0);
            self.player_effects(area, &pev);
            out.extend(pev.into_iter().map(GameEvent::Player));
            if self.player.underwater && self.health > 0 {
                // Drowning.
                self.health -= dt * 12;
                if self.health <= 0 {
                    self.kill_hunter(area, None, hunt::BREATH, true, out);
                }
            }
            self.weapon_input(area, cars, input, out);
            self.ambience(area, dt, out);
            self.binoculars(input, dt);
            self.calls(input, dt, out);
            if input.toggle_map
                && !self.opts.trophy
                && self.health > 0
                && !self.bin_mode
                && self.weapons.state == WeaponState::Hidden
            {
                self.map_mode = !self.map_mode;
            }
            if self.weapons.state != WeaponState::Hidden || self.bin_mode {
                self.map_mode = false;
            }
            if !self.opts.trophy {
                self.path += dt as f32 * self.player.vspeed / 128.0;
                self.time_s += dt as f32 / 1000.0;
            }
        }
        self.weapon_step(dt, out);
        self.animate_bullets(area, cars, dt, out);
        self.track_step(dt);
        self.prof.mark(1);

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
            render_r: self.opts.render_r.max(self.opts.view_r),
            run_k: self.opts.dino_run_k,
            events: &mut dev,
        };
        if !self.opts.trophy {
            self.herd.step(&mut ctx);
        }
        self.prof.mark(2);
        for e in dev {
            match e {
                DinoEvent::KillHunter {
                    index,
                    kind,
                    scream,
                } => self.kill_hunter(area, Some(index), kind, scream, out),
                DinoEvent::BloodTrail { pos } if self.opts.blood => {
                    self.effects.blood_trail(area, &mut self.rng, pos);
                }
                DinoEvent::BloodTrail { .. } => {}
                DinoEvent::Splash { pos } => {
                    // AnimateDimorDead hitting the water: five rings.
                    for sc in [2.0, 2.5, 3.0, 3.5, 3.0] {
                        let (x, z) = (
                            pos[0] + self.rng.si_rand(128) as f32,
                            pos[2] + self.rng.si_rand(128) as f32,
                        );
                        self.effects.ring(area, x, z, sc);
                    }
                }
                DinoEvent::Spray { pos } => {
                    let a = self.camera().1;
                    for _ in 0..3 {
                        let p = [
                            pos[0] + self.rng.si_rand(128) as f32,
                            pos[1],
                            pos[2] + self.rng.si_rand(128) as f32,
                        ];
                        self.effects.add(
                            area,
                            &mut self.rng,
                            p,
                            crate::effects::Part::Bubble,
                            10,
                            a,
                        );
                    }
                }
                _ => {}
            }
            out.push(GameEvent::Dino(e));
        }
        self.prof.mark(3);
        // AnimateProcesses: now and then, bubbles rise in deep water ahead.
        if self.takt & 63 == 0 {
            let (cam, a, _) = self.camera();
            let al = a + self.rng.si_rand(60) as f32 * PI / 180.0;
            let l = (1024 + self.rng.r_rand(3120)) as f32;
            let (x, z) = (cam[0] + al.sin() * l, cam[2] - al.cos() * l);
            if area.land_up_h(x, z) > area.land_height(x, z) + 256.0 {
                let n = 6 + self.rng.r_rand(6);
                self.effects.add(
                    area,
                    &mut self.rng,
                    [x, area.land_height(x, z), z],
                    crate::effects::Part::Bubble,
                    n,
                    a,
                );
            }
        }
        self.effects.step(area, dt, self.takt);
        if let Some(s) = &mut self.effects.snow {
            let wind = (self.wind.nv(), self.wind.speed);
            let at = (self.player.x, self.player.z);
            s.step(
                area,
                &mut self.rng,
                at,
                wind,
                self.player.alpha,
                self.real_time,
                dt,
            );
        }
        self.prof.mark(4);

        // AnimateProcesses: wind drifts every other frame.
        if self.takt & 1 != 0 {
            self.wind.alpha += self.rng.si_rand(16) as f32 / 4096.0;
            self.wind.speed += self.rng.si_rand(400) as f32 / 6400.0;
        }
        self.wind.speed = self.wind.speed.clamp(4.0, 18.0);
        if self.takt.is_multiple_of(self.opts.afraid_every.max(1)) {
            self.check_afraid(area);
        }
        self.prof.mark(5);
        if self.opts.trophy {
            self.process_trophy();
        } else if self.trophy_time > 0 {
            self.trophy_time -= dt;
            if self.trophy_time <= 0 {
                self.trophy_time = 0;
                self.trophy_body = None;
            }
        }
        self.exit_step(dt);
        self.survival_step(area, dt, out);
        if (self.opts.observer || self.opts.trophy) && self.health > 0 {
            self.health = MAX_HEALTH;
        }

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
        self.prof.mark(6);
        self.prof.end();
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
        if self.survival.is_some() {
            // No ship, trophy or score in a survival; the kill is recorded.
            let p = &self.player;
            let range =
                ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2))
                    .sqrt()
                    / 64.0;
            self.kills.push(HuntKill {
                ctype: c.ctype,
                score: 0,
                range,
                weapon: self.weapons.current,
            });
            return;
        }
        let water = area.land_up_h(c.pos[0], c.pos[2]) - area.land_height(c.pos[0], c.pos[2]);
        if water < 100.0 && !self.opts.tranq {
            self.ship.tasks.push_back(index);
            let slot = crate::ship::ship_sound::CALL;
            out.push(GameEvent::Ship(ShipEvent::Sound { slot, at: None }));
        }
        // The first free stand; in a dialect game the first free spot made
        // for this animal (none: no trophy, as `AddShipTask` there).
        let spots = &self.herd.trophy_spots;
        let t = if spots.is_empty() {
            Some(
                self.room
                    .bodies
                    .iter()
                    .take(23)
                    .position(|b| b.ctype == 0)
                    .unwrap_or(23),
            )
        } else {
            // A spot made for it, else any free one (add-ons written
            // without `tropinfo` would never have a trophy).
            let free = |t: &usize| self.room.bodies[*t].ctype == 0;
            let n = spots.len().min(self.room.bodies.len());
            (0..n)
                .filter(free)
                .find(|&t| spots[t].chars.contains(&c.ctype))
                .or_else(|| (0..n).find(free))
        };
        let base = self
            .herd
            .species
            .get(c.ctype)
            .map_or(0.0, |s| s.info.base_score);
        let mut score = base;
        if self.successes > 1 {
            score *= 1.0 + self.successes as f32 / 10.0;
        }
        // The original halves a kill that isn't a target; the Modder's
        // Edition doesn't (`SubmitDinoScore`: the line is commented out).
        let dialect = self
            .herd
            .species
            .get(c.ctype)
            .is_some_and(|s| s.tuning.mee_ai.is_some());
        if !dialect && !self.is_target(c) {
            score /= 2.0;
        }
        let no_call = self
            .call_bit(c)
            .is_some_and(|b| self.opts.no_call_bonus.has(b as usize));
        // Night and no-call: +10% each (not in the original; owner's requests).
        for (on, k) in [
            (self.opts.tranq, 1.25),
            (area.time == crate::area::TimeOfDay::Night, 1.10),
            (no_call, 1.10),
            (self.opts.radar, 0.70),
            (self.opts.scent, 0.80),
            (self.opts.camo, 0.85),
        ] {
            if on {
                score *= k;
            }
        }
        score *= self.opts.score_factor;
        self.room.score += score as i32;
        let p = &self.player;
        let range =
            ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2)).sqrt()
                / 64.0;
        out.push(GameEvent::Scored {
            ctype: c.ctype,
            score: score as i32,
            range,
            tranq: self.opts.tranq,
        });
        self.kills.push(HuntKill {
            ctype: c.ctype,
            score: score as i32,
            range,
            weapon: self.weapons.current,
        });
        if let (false, Some(t)) = (self.opts.tranq, t) {
            let (y, mo, d, h, mi) = self.clock;
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
            self.room.bodies[t].set_area(self.area_no);
            self.trophy_time = 20 * 1000;
            self.trophy_body = Some(t);
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
        // A dialect weapon fires once per pull unless `fullAuto`; the
        // original's fire whenever ready while the trigger is held.
        let once = self.weapons.mee().is_some_and(|m| !m.full_auto);
        if input.fire && !(once && self.fire_held) {
            self.process_shoot(area, cars, out);
        }
        self.fire_held = input.fire;
    }

    /// `HideWeapon`: draw if hidden, put away if ready.
    pub fn hide_weapon(&mut self, out: &mut Vec<GameEvent>) {
        let w = &mut self.weapons;
        if self.player.underwater && w.state == WeaponState::Hidden {
            return;
        }
        if self.opts.observer || self.opts.trophy {
            return;
        }
        if w.state == WeaponState::Hidden {
            if w.shots_left.get(w.current).copied().unwrap_or(0) == 0 {
                return;
            }
            w.optic_mode = w.info.get(w.current).is_some_and(|i| i.optic);
            self.bin_mode = false;
            out.push(GameEvent::WeaponSound {
                weapon: w.current,
                slot: weapon_anim::DRAW,
            });
            w.ftime = 0;
            w.state = WeaponState::Drawing;
            w.shakel = w.mee().and_then(|m| m.shake).map_or(0.2, |s| s * 4.0);
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
        let reach = 256.0 * self.opts.render_r.max(self.opts.view_r) as f32;
        if let Some(m) = &info.mee {
            // ProcessShoot: Recoil.y = -recoil / 100; x by up to half that.
            let r = m.recoil / 100.0;
            self.recoil[1] = -r;
            self.recoil[0] += (self.rng.r_rand(8) - 4) as f32 / 8.0 * r;
        }
        let flying = info.mee.as_ref().filter(|m| m.veloc > 0.0).map(|m| m.veloc);
        for _ in 0..=info.trace_c.max(0) {
            let ra = self.rng.si_rand(128) as f32 * 0.0001 * (2.0 - info.prec);
            let rb = self.rng.si_rand(128) as f32 * 0.0001 * (2.0 - info.prec);
            let (sa, ca) = (alpha + da + ra).sin_cos();
            let (sb, cb) = (beta + db + rb).sin_cos();
            let nv = [sa * cb, -sb, -ca * cb];
            if let Some(v) = flying {
                // AddBullet: 64 * veloc units per 17 ms.
                self.bullets.push(Bullet {
                    pos: origin,
                    vel: nv.map(|n| n * 64.0 * v),
                    weapon: cur,
                });
                continue;
            }
            let to = [
                origin[0] + nv[0] * reach,
                origin[1] + nv[1] * reach,
                origin[2] + nv[2] * reach,
            ];
            self.make_shot(area, cars, &info, origin, to, out);
        }
        // Stealth crouch: crouched, a shot carries half as far.
        let hush = if self.opts.stealth && self.player.head_y <= 180.0 {
            0.5
        } else {
            1.0
        };
        self.make_noise(
            [self.player.x, self.player.y, self.player.z],
            self.opts.view_r as f32 * 200.0 * info.loud * hush,
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
        let (chars, species) = (&self.herd.chars, &self.herd.species);
        let res = if !info.fall {
            trace_shot(area, chars, species, cars, a, b)
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
                r = trace_shot(area, chars, species, cars, from, to);
                if r.is_some() {
                    break;
                }
                from = to;
            }
            r
        };
        let Some((hit, pos)) = res else { return };
        self.apply_hit(area, info, hit, pos, out);
    }

    /// A round's hit (`MakeShot` / `AnimateBullet`): impact effects, then
    /// damage to an animal.
    fn apply_hit(
        &mut self,
        area: &Area,
        info: &WeaponInfo,
        hit: Hit,
        pos: [f32; 3],
        out: &mut Vec<GameEvent>,
    ) {
        out.push(GameEvent::Impact { hit, pos });
        // AddElements per surface (MakeShot).
        let a = self.camera().1;
        let pw = info.power as i32;
        match hit {
            Hit::Ground | Hit::Model => self.effects.add(
                area,
                &mut self.rng,
                pos,
                crate::effects::Part::Ground,
                6 + pw * 4,
                a,
            ),
            Hit::Water => {
                self.effects.add(
                    area,
                    &mut self.rng,
                    pos,
                    crate::effects::Part::Water,
                    4 + pw * 3,
                    a,
                );
                self.effects.ring(area, pos[0], pos[2], 1.2);
                self.effects.ring(area, pos[0], pos[2], 1.2);
            }
            Hit::Character { .. } if self.opts.blood => self.effects.add(
                area,
                &mut self.rng,
                pos,
                crate::effects::Part::Blood,
                4 + pw * 4,
                a,
            ),
            Hit::Character { .. } => {}
        }
        let Hit::Character { index, mortal } = hit else {
            return;
        };
        let c = &mut self.herd.chars[index];
        if c.health == 0 {
            return;
        }
        // The dialect's `registerDamage` also marks the animal hurt and
        // aware of the hunter.
        c.damaged = true;
        if let Some((rgb, ms)) = info.mee.as_ref().and_then(|m| m.radar) {
            c.tracked = Some((rgb, (ms > 0).then_some(ms)));
        }
        let mortal = mortal && !info.mee.as_ref().is_some_and(|m| m.cannot_mortal);
        if mortal {
            c.health = 0;
        } else {
            c.health -= info.power as i32;
        }
        c.health = c.health.max(0);
        let mee = self
            .herd
            .species
            .get(c.ctype)
            .is_some_and(|s| s.mee.is_some());
        if c.health == 0 {
            let trophy = c.ai >= 10;
            let ct = c.ctype;
            out.push(GameEvent::Killed { index, trophy });
            if trophy {
                self.successes += 1;
                self.add_ship_task(area, index, out);
            }
            if self.herd.chars[index].wave > 0 {
                // A wave animal isn't replaced.
            } else if mee {
                let p = (self.player.x, self.player.z);
                let agres = self.opts.opt_agres;
                self.herd
                    .add_secondary_in(area, index, p, &mut self.rng, agres);
            } else if !trophy {
                self.add_secondary_one(area, ct);
            }
        } else {
            c.aware_hunter = true;
            c.afraid_time = 60 * 1000;
            if c.ai != ai::TREX || c.state == 0 {
                c.state = 2;
            }
            if mee || c.ai != ai::BRACH {
                c.blood_ttime += 90000;
            }
        }
        let c = &mut self.herd.chars[index];
        if c.ai == ai::TREX {
            c.state = if c.state != 0 { 5 } else { 1 };
        }
    }

    /// `AnimateBullets`: each flying round moves its way for this tick
    /// (scaled from the engine's 17 ms step), tracing what it passes; it
    /// falls by its weapon's `land_fall` a step, and is gone on a hit or
    /// past the view range.
    fn animate_bullets(
        &mut self,
        area: &Area,
        cars: &[Option<&Car>],
        dt: i32,
        out: &mut Vec<GameEvent>,
    ) {
        if self.bullets.is_empty() {
            return;
        }
        let k = dt as f32 / 17.0;
        let reach = 256.0 * self.opts.render_r.max(self.opts.view_r) as f32;
        let bullets = std::mem::take(&mut self.bullets);
        let mut kept = Vec::with_capacity(bullets.len());
        for mut b in bullets {
            let Some(info) = self.weapons.info.get(b.weapon).cloned() else {
                continue;
            };
            let to = [
                b.pos[0] + b.vel[0] * k,
                b.pos[1] + b.vel[1] * k,
                b.pos[2] + b.vel[2] * k,
            ];
            let res = trace_shot(area, &self.herd.chars, &self.herd.species, cars, b.pos, to);
            if let Some((hit, pos)) = res {
                self.apply_hit(area, &info, hit, pos, out);
                continue;
            }
            let (dx, dz) = (self.player.x - to[0], self.player.z - to[2]);
            if dx * dx + dz * dz > reach * reach {
                continue;
            }
            b.pos = to;
            b.vel[1] -= info.mee.as_ref().map_or(0.0, |m| m.fall) * k;
            kept.push(b);
        }
        kept.append(&mut self.bullets);
        self.bullets = kept;
    }

    /// Trackers run out (`RTime -= TimeDt`).
    fn track_step(&mut self, dt: i32) {
        for c in &mut self.herd.chars {
            if let Some((_, Some(ms))) = &mut c.tracked {
                *ms -= dt;
                if *ms <= 0 {
                    c.tracked = None;
                }
            }
        }
    }

    /// The recoil moves the view each step and eases off, 0.005 rad per
    /// 17 ms (`Recoil` in the dialect's camera code).
    fn apply_recoil(&mut self, dt: i32) {
        if self.recoil == [0.0; 2] {
            return;
        }
        let k = dt as f32 / 17.0;
        let p = &mut self.player;
        p.alpha += self.recoil[0] * k;
        p.beta = (p.beta + self.recoil[1] * k).clamp(-1.26, 1.46);
        let ease = 0.005 * k;
        self.recoil[1] = (self.recoil[1] + ease).min(0.0);
        let x = self.recoil[0];
        self.recoil[0] = if x.abs() <= ease {
            0.0
        } else {
            x - ease * x.signum()
        };
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

    /// `MakeNoise`: startle everything within `range`. Dialect species hear
    /// `2 * hearK` times as far, only start an alarm if calm, and remember
    /// the shot (`heardShot`).
    pub fn make_noise(&mut self, pos: [f32; 3], range: f32) {
        for c in &mut self.herd.chars {
            if c.health == 0 {
                continue;
            }
            let l = ((c.pos[0] - pos[0]).powi(2)
                + (c.pos[1] - pos[1]).powi(2)
                + (c.pos[2] - pos[2]).powi(2))
            .sqrt();
            let sp = self.herd.species.get(c.ctype);
            if let Some(sp) = sp.filter(|s| s.mee.is_some()) {
                if l > range * sp.info.hear_k * 2.0 {
                    continue;
                }
                if c.ai == ai::TREX {
                    if c.state == 0 {
                        c.state = 2;
                        c.aware_hunter = true;
                        c.heard_shot = true;
                    }
                    continue;
                }
                // Fish and Mosasaurus do not hear shots (`Aquatic`).
                if crate::dino::mee::is_fish(c.ai) {
                    continue;
                }
                c.afraid_time = ((10.0 + (range - l) / 256.0) as i32) * 1024;
                if c.state == 0 {
                    c.state = 2;
                }
                c.no_find_cnt = 0;
                c.aware_hunter = true;
                c.heard_shot = true;
                continue;
            }
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
        if self.health <= 0 || self.opts.trophy || self.opts.observer || self.player.underwater {
            return;
        }
        let p = &self.player;
        let ppos = [p.x, p.y, p.z];
        let head_y = p.head_y;
        let wlook = self.wind.nv();
        let kmask = if self.opts.camo { 1.5 } else { 1.0 };
        let kscent = if self.opts.scent { 1.5 } else { 1.0 };
        // Crouched (`kstand`'s test) with stealth crouch on.
        let stealthy = self.opts.stealth && head_y <= 180.0;
        for i in 0..self.herd.chars.len() {
            let c = &self.herd.chars[i];
            let Some(sp) = self.herd.species.get(c.ctype) else {
                continue;
            };
            // The dialect keeps checking alarmed animals (renewing
            // `AfraidTime`), and asks the AI's table which ones look.
            let mee = sp.mee.is_some();
            let skip = if mee {
                !crate::dino::mee::ai_info(c.ai).sniffer
                    || (c.ai == ai::TREX && (c.afraid_time != 0 || c.state == 1))
            } else {
                c.ai < 10 || c.afraid_time != 0 || c.state == 1
            };
            if c.health == 0 || skip {
                continue;
            }
            let mut rlook = [ppos[0] - c.pos[0], ppos[1] - c.pos[1], ppos[2] - c.pos[2]];
            let rl = (rlook[0] * rlook[0] + rlook[1] * rlook[1] + rlook[2] * rlook[2]).sqrt();
            let mut kr = rl / 256.0 / (32.0 + self.opts.view_r as f32 / 2.0);
            if rl > 0.0 {
                rlook = [rlook[0] / rl, rlook[1] / rl, rlook[2] / rl];
            }
            kr *= 2.5 / (1.5 + self.opts.opt_sens as f32 / 128.0);
            kr /= self.opts.notice_k.clamp(0.1, 1.0);
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
            if stealthy {
                k_look *= 2.0;
            }

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
                if !mee || c.state == 0 {
                    c.state = 2;
                }
                c.aware_hunter = true;
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
    fn kill_hunter(
        &mut self,
        area: &Area,
        by: Option<usize>,
        kind: i32,
        scream: bool,
        out: &mut Vec<GameEvent>,
    ) {
        if self.death.is_some() {
            return;
        }
        self.health = 0;
        self.bin_mode = false;
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
            let mee = self.herd.species.get(c.ctype).and_then(|s| s.mee.as_ref());
            let pl = match (mee, c.ai) {
                // The dialect: the kill type's `hunterOffset`.
                (Some(m), _) => m.kill(c.kill_type).map_or(0.0, |k| k.offset),
                (None, ai::SPINO) => 200.0,
                (None, ai::CERAT) => 320.0,
                (None, ai::TREX) => 0.0,
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
            // The dialect does not carry the body (`elevate` is disabled).
            let mee = self
                .herd
                .species
                .get(c.ctype)
                .is_some_and(|s| s.mee.is_some());
            if c.ai == ai::TREX && !mee {
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
        out.push(GameEvent::HunterDied { kind, scream });
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
        self.exit_prompt
            || self
                .death
                .is_some_and(|d| d.time > 6_000 || !self.opts.death_anim)
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

    /// `MakeCall`, `ChangeCall` and the delayed answer (`AnimateProcesses`).
    fn calls(&mut self, input: &HuntInput, dt: i32, out: &mut Vec<GameEvent>) {
        self.call_lock = (self.call_lock - dt).max(0);
        self.call_icon_time = (self.call_icon_time - dt).max(0);
        if let Some((t, pos, call)) = &mut self.answer {
            *t -= dt;
            if *t <= 0 {
                let variant = (self.rng.r_rand(128) % 3) as usize;
                out.push(GameEvent::Call {
                    call: *call,
                    variant,
                    pos: Some(*pos),
                });
                self.answer = None;
            }
        }
        let targets = self.opts.targets;
        if input.change_call && !targets.is_empty() {
            if self.call_icon_time > 0 {
                for _ in 0..MAX_CALL {
                    self.target_call += 1;
                    if self.target_call >= MAX_CALL {
                        self.target_call = 10;
                    }
                    if usize::try_from(self.target_call).is_ok_and(|c| targets.has(c)) {
                        break;
                    }
                }
            }
            self.call_icon_time = 2048;
        }
        if !input.call
            || targets.is_empty()
            || self.player.underwater
            || self.opts.observer
            || self.opts.trophy
            || self.call_lock > 0
        {
            return;
        }
        self.call_lock = 1024 * 3;
        self.next_call = (self.next_call + (self.real_time % 2) + 1) % 3;
        out.push(GameEvent::Call {
            call: self.target_call,
            variant: self.next_call as usize,
            pos: None,
        });

        let danger = self
            .herd
            .species
            .iter()
            .find(|s| s.mee.is_none() && s.info.ai == self.target_call)
            .is_some_and(|s| s.info.danger_call);
        let p = [self.player.x, self.player.y, self.player.z];
        let mut best: Option<(usize, f32)> = None;
        let range = self.opts.view_r as f32 * 400.0;
        let slot = self.target_call - 10;
        for i in 0..self.herd.chars.len() {
            if danger && self.herd.chars[i].ai < 10 {
                let t = (10 + self.rng.r_rand(5)) * 1024;
                let c = &mut self.herd.chars[i];
                c.state = 2;
                c.afraid_time = t;
            }
            let c = &self.herd.chars[i];
            let d =
                ((p[0] - c.pos[0]).powi(2) + (p[1] - c.pos[1]).powi(2) + (p[2] - c.pos[2]).powi(2))
                    .sqrt();
            // The dialect's species: calls are by menu slot. A species
            // answers its `callNo` slot's call, flees the ones it fears
            // (`fearCall`), and hears `2 * hearK` times as far.
            let mee = self
                .herd
                .species
                .get(c.ctype)
                .and_then(|s| s.mee.as_ref().map(|m| (m, s.info.hear_k)));
            let (answers, hears) = match mee {
                Some((m, hear_k)) => {
                    let hears = d < range * hear_k * 2.0;
                    let flier = matches!(
                        m.ai,
                        ai::DIMOR | ai::PTERA | crate::dino::mee::mee_ai::BRACH
                    );
                    if c.health != 0 && hears && !flier && m.fear_calls.contains(&slot) {
                        let t = (10 + self.rng.r_rand(5)) * 1024;
                        let c = &mut self.herd.chars[i];
                        c.state = 2;
                        c.afraid_time = t;
                    }
                    (m.call_no == slot, hears)
                }
                None => (c.ai == self.target_call, d < range),
            };
            let c = &self.herd.chars[i];
            if !answers || c.health == 0 || c.afraid_time != 0 || c.state != 0 {
                continue;
            }
            if hears {
                if self.rng.r_rand(128) > 32 && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
                let (tx, tz) = (
                    p[0] + self.rng.si_rand(1800) as f32,
                    p[2] + self.rng.si_rand(1800) as f32,
                );
                let c = &mut self.herd.chars[i];
                c.tgx = tx;
                c.tgz = tz;
            }
        }
        if let Some((i, _)) = best {
            // The answer comes from a third of the way toward the animal.
            let c = self.herd.chars[i].pos;
            let pos = [
                p[0] + (c[0] - p[0]) / 3.0,
                p[1] + (c[1] - p[1]) / 3.0,
                p[2] + (c[2] - p[2]) / 3.0,
            ];
            self.answer = Some((2000 + self.rng.r_rand(2000), pos, self.target_call));
        }
    }

    /// `GenerateMapImage`: the 256x256 overview (1 pixel per 4 cells) made
    /// from each cell's texture at the 32x32 mip level (16x16 for water).
    pub fn map_image(area: &Area) -> Vec<u16> {
        let mip = |src: &[u16], l: usize| -> Vec<u16> {
            // GenerateMipMap: average 2x2 per 5-bit channel, rounded.
            let mut d = vec![0u16; l * l];
            for y in 0..l {
                for x in 0..l {
                    let at = |dx: usize, dy: usize| {
                        u32::from(
                            src.get((y * 2 + dy) * l * 2 + x * 2 + dx)
                                .copied()
                                .unwrap_or(0),
                        )
                    };
                    let px = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                    let ch = |sh: u32| {
                        ((px.iter().map(|p| (p >> sh) & 31).sum::<u32>() + 2) >> 2) as u16
                    };
                    d[y * l + x] = (ch(10) << 10) | (ch(5) << 5) | ch(0);
                }
            }
            d
        };
        let mips: Vec<(Vec<u16>, Vec<u16>)> = area
            .rsc
            .textures
            .iter()
            .map(|t| {
                let b = mip(t, 64);
                let c = mip(&b, 32);
                let d = mip(&c, 16);
                (c, d)
            })
            .collect();
        let mut out = vec![0u16; 256 * 256];
        for y in 0..256 {
            for x in 0..256 {
                let i = (y << 2) * c2_formats::map::MAP_SIZE + (x << 2);
                let px = if area.map.flags[i] & crate::area::cell_flags::WATER != 0 {
                    let t = area
                        .rsc
                        .waters
                        .get(usize::from(area.map.water[i]))
                        .map_or(0, |w| w.texture_index.max(0) as usize);
                    mips.get(t).map_or(0, |(_, d)| d[(y & 15) * 16 + (x & 15)])
                } else {
                    let t = usize::from(area.map.texture1[i]);
                    mips.get(t).map_or(0, |(c, _)| c[(y & 31) * 32 + (x & 31)])
                };
                out[y * 256 + x] = px;
            }
        }
        out
    }

    /// Ripples from the hunter: wading, diving in or out, swimming.
    fn player_effects(&mut self, area: &Area, events: &[PlayerEvent]) {
        let [cx, _, cz] = self.player.eye();
        for e in events {
            match e {
                PlayerEvent::StepWater => self.effects.ring(area, cx, cz, 1.2),
                PlayerEvent::WaterIn | PlayerEvent::WaterOut => {
                    self.effects.ring(area, cx, cz, 2.0)
                }
                _ => {}
            }
        }
        if self.player.swim && self.takt & 31 == 0 {
            self.effects.ring(area, cx, cz, 1.5);
        }
    }

    /// `ToggleBinocular` and the +/- power keys.
    fn binoculars(&mut self, input: &HuntInput, dt: i32) {
        if input.toggle_binoculars
            && self.weapons.state == WeaponState::Hidden
            && !self.player.underwater
            && self.health > 0
        {
            self.bin_mode = !self.bin_mode;
        }
        if self.player.underwater {
            self.bin_mode = false;
        }
        if self.bin_mode {
            let k = self.bin_power * dt as f32 / 4000.0;
            if input.zoom_in {
                self.bin_power += k;
            }
            if input.zoom_out {
                self.bin_power -= k;
            }
            self.bin_power = self.bin_power.clamp(1.5, 3.0);
        }
    }

    /// Field-of-view magnification: binoculars, or the sniper scope while a
    /// scoped weapon is out.
    pub fn zoom(&self) -> f32 {
        if self.bin_mode {
            self.bin_power
        } else if self.scoped() {
            match self
                .weapons
                .info
                .get(self.weapons.current)
                .and_then(|i| i.mee.as_ref())
            {
                Some(m) => m.zoom.max(0.1),
                None => 3.0,
            }
        } else {
            1.0
        }
    }

    /// Looking through the current weapon's scope (zoomed): optic mode,
    /// the weapon out, and (an `unzoom` dialect weapon) ready.
    fn scoped(&self) -> bool {
        let w = &self.weapons;
        let unzoomed = w
            .info
            .get(w.current)
            .and_then(|i| i.mee.as_ref())
            .is_some_and(|m| m.unzoom && w.state != WeaponState::Ready);
        w.optic_mode && w.state != WeaponState::Hidden && self.death.is_none() && !unzoomed
    }

    /// Cross-hairs on the view model's aim point (`DrawOpticCross`): the
    /// original's when scoped; a dialect weapon's only with `cross` (scoped,
    /// or out without a scope), not while it `unzoom`s.
    pub fn scope_cross(&self) -> bool {
        let w = &self.weapons;
        if self.bin_mode || self.death.is_some() || w.state == WeaponState::Hidden {
            return false;
        }
        match w.info.get(w.current).map(|i| (i, i.mee.as_ref())) {
            Some((i, Some(m))) => {
                m.cross
                    && (!i.optic || w.optic_mode)
                    && (!m.unzoom || w.state == WeaponState::Ready)
            }
            Some((_, None)) => self.scoped(),
            None => false,
        }
    }

    /// `ScanLifeForms`: the nearest visible animal close to the view centre.
    /// Whether `c` is one of the hunted species: in Modder's Edition data,
    /// marked `radar = TRUE` for this hunt, else picked in the menu by its
    /// slot (`callNo`); in the original's, picked by AI (`TargetDino`).
    pub fn is_target(&self, c: &crate::dino::Character) -> bool {
        let Some(sp) = self.herd.species.get(c.ctype) else {
            return false;
        };
        sp.tuning.radar.unwrap_or_else(|| {
            self.call_bit(c)
                .is_some_and(|b| self.opts.targets.has(b as usize))
        })
    }

    /// The target bit (call id) `c` is picked by: `10 + callNo` in Modder's
    /// Edition data, its AI in the original's. `None` for a dialect ambient,
    /// flyer or fish: its `callNo` is 0 when not given, which would make it
    /// the first menu animal (green on the radar when that one is hunted).
    pub fn call_bit(&self, c: &crate::dino::Character) -> Option<i32> {
        let sp = self.herd.species.get(c.ctype)?;
        match &sp.mee {
            Some(m) if (10..=29).contains(&m.ai) => Some(10 + m.call_no),
            Some(_) => None,
            None => Some(c.ai),
        }
    }

    /// A huntable animal (a menu animal, picked or not), not an ambient,
    /// flyer or fish.
    pub fn is_huntable(&self, c: &crate::dino::Character) -> bool {
        match self.herd.species.get(c.ctype).and_then(|s| s.mee.as_ref()) {
            Some(m) => (10..=29).contains(&m.ai),
            None => c.ai >= 10,
        }
    }

    pub fn life_info(&self, area: &Area) -> Option<LifeInfo> {
        if !self.bin_mode {
            return None;
        }
        let (cam, alpha, beta) = self.camera();
        let (sa, ca) = alpha.sin_cos();
        let (sb, cb) = beta.sin_cos();
        let view_r = self.opts.view_r as f32 * 256.0;
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in self.herd.chars.iter().enumerate() {
            if c.health == 0 {
                continue;
            }
            // RotateVector: into camera space.
            let (x, y, z) = (c.pos[0] - cam[0], c.pos[1] - cam[1], c.pos[2] - cam[2]);
            let rx = x * ca + z * sa;
            let vz = z * ca - x * sa;
            let ry = y * cb - vz * sb;
            let rz = vz * cb + y * sb;
            if rz > -512.0 {
                continue;
            }
            let d = (rx * rx + ry * ry + rz * rz).sqrt();
            if d > view_r || (rx.abs() + ry.abs()) / d > 0.15 {
                continue;
            }
            if best.is_some_and(|(_, bd)| d >= bd) {
                continue;
            }
            if trace_look(area, [c.pos[0], c.pos[1] + 220.0, c.pos[2]], cam).0 {
                continue;
            }
            best = Some((i, d));
        }
        let (i, _) = best?;
        let c = &self.herd.chars[i];
        let info = &self.herd.species.get(c.ctype)?.info;
        let p = &self.player;
        let distance =
            ((c.pos[0] - p.x).powi(2) + (c.pos[1] - p.y).powi(2) + (c.pos[2] - p.z).powi(2)).sqrt();
        Some(LifeInfo {
            index: i,
            name: info.name.clone(),
            weight: info.mass * c.scale * c.scale,
            distance,
        })
    }

    /// Escape: toggle the exit prompt; the trophy room is left at once.
    pub fn press_escape(&mut self) {
        if self.opts.trophy {
            self.outcome = Some(Outcome::Exited);
            return;
        }
        if self.paused {
            self.paused = false;
            return;
        }
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

    /// The pause key: toggle `PAUSE`, closing the exit prompt.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.exit_prompt = false;
    }

    /// `ProcessTrophy`: the plaque of the trophy the hunter stands in front
    /// of (2.5 cells ahead of its nose, within 148 units).
    fn process_trophy(&mut self) {
        let p = [self.player.x, self.player.y, self.player.z];
        self.trophy_body = None;
        for c in &self.herd.chars {
            let x = c.pos[0] + c.lookx * 256.0 * 2.5;
            let z = c.pos[2] + c.lookz * 256.0 * 2.5;
            let d = ((x - p[0]).powi(2) + (c.pos[1] - p[1]).powi(2) + (z - p[2]).powi(2)).sqrt();
            if d < 148.0 {
                self.trophy_body = Some(c.state as usize);
            }
        }
    }

    /// `RemoveCurrentTrophy` (R in the trophy room): delete the trophy whose
    /// plaque is showing.
    pub fn remove_current_trophy(&mut self) {
        if !self.opts.trophy {
            return;
        }
        let Some(slot) = self.trophy_body else {
            return;
        };
        if self
            .room
            .bodies
            .get(slot)
            .is_none_or(|b| b.ctype == 0 || b.locked())
        {
            return;
        }
        self.room.bodies[slot].ctype = 0;
        if let Some(i) = self
            .herd
            .chars
            .iter()
            .position(|c| c.state as usize == slot)
        {
            self.herd.chars.remove(i);
        }
        self.trophy_time = 0;
        self.trophy_body = None;
    }

    /// Locks or unlocks the trophy whose plaque is showing (locked ones
    /// can't be removed). Whether it is now locked.
    pub fn toggle_lock_current(&mut self) -> Option<bool> {
        let slot = self.trophy_body.filter(|_| self.opts.trophy)?;
        let b = self.room.bodies.get_mut(slot).filter(|b| b.ctype != 0)?;
        b.set_locked(!b.locked());
        Some(b.locked())
    }

    /// Moves the trophy whose plaque is showing to the next free stand
    /// before it (`-1`) or after it (`1`); its stuffed animal goes there.
    pub fn move_current_trophy(&mut self, area: &Area, dir: i32) -> bool {
        let Some(slot) = self.trophy_body.filter(|_| self.opts.trophy) else {
            return false;
        };
        let n = if self.herd.trophy_spots.is_empty() {
            self.room.bodies.len().min(area.landings.len())
        } else {
            self.herd.trophy_spots.len().min(self.room.bodies.len())
        };
        let mut t = slot as i32;
        let to = loop {
            t += dir;
            if t < 0 || t >= n as i32 {
                return false;
            }
            if self.room.bodies[t as usize].ctype == 0 {
                break t as usize;
            }
        };
        self.room.bodies.swap(slot, to);
        let room = self.room.clone();
        self.herd.place_trophies(area, &room, &mut self.rng);
        self.trophy_body = Some(to);
        true
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
            trailing: Vec::new(),
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
            base_score: 1.0,
            smell_k: 1.0,
            hear_k: 1.0,
            look_k: 1.0,
            sh_delta: 0.0,
            scale0: 1000,
            scale_a: 0,
            danger_call: false,
        }
    }

    #[test]
    fn dialect_scopes_zoom_by_their_optic_and_cross_only_when_asked() {
        let def = |optic: f32, keys: &[(&str, &str)]| WeaponDef {
            optic: Some(optic),
            extra: keys
                .iter()
                .map(|&(k, v)| c2_formats::res_txt::Entry {
                    key: k.into(),
                    value: v.into(),
                })
                .collect(),
            ..Default::default()
        };
        let (_, mut hunt, _) = setup(false);
        let mut scoped = |d: &WeaponDef, state: WeaponState| {
            hunt.weapons.info[0] = WeaponInfo::from_def(d);
            hunt.weapons.optic_mode = hunt.weapons.info[0].optic;
            hunt.weapons.state = state;
            (hunt.zoom(), hunt.scope_cross())
        };
        // The original: 3x and the cross, whatever `optic` says.
        assert_eq!(scoped(&def(1.0, &[]), WeaponState::Ready), (3.0, true));
        assert_eq!(scoped(&def(0.5, &[]), WeaponState::Ready), (1.0, false));
        // The dialect: its own zoom, no cross unless asked.
        let rifle = def(1.1, &[("recoil", "0")]);
        assert_eq!(scoped(&rifle, WeaponState::Ready), (1.1, false));
        let sniper = def(3.0, &[("cross", "TRUE"), ("unzoom", "TRUE")]);
        assert_eq!(scoped(&sniper, WeaponState::Ready), (3.0, true));
        assert_eq!(scoped(&sniper, WeaponState::Reloading), (1.0, false));
        // A cross without a scope: drawn at the hip.
        assert_eq!(
            scoped(&def(0.0, &[("cross", "TRUE")]), WeaponState::Ready),
            (1.0, true)
        );
    }

    #[test]
    fn resupply_refills_carried_weapons_only() {
        let mut w = Weapons::new(
            vec![rifle(), rifle()],
            vec![[100; 4]; 2],
            Bits::bit(0),
            true,
        );
        w.shots_left = vec![0, 0];
        w.ammo_mag = vec![0, 0];
        w.resupply();
        assert_eq!(w.shots_left, vec![14, 0]);
        assert_eq!(w.ammo_mag, vec![1, 0]);
    }

    #[test]
    fn weapons_past_slot_32_get_ammo() {
        let n = 50;
        let mut w = Weapons::new(
            vec![rifle(); n],
            vec![[100; 4]; n],
            Bits::bit(40) | Bits::bit(2),
            false,
        );
        assert_eq!(w.shots_left[40], 14);
        assert!(w.carried(40) && w.carried(2) && !w.carried(3));
        w.shots_left[40] = 0;
        w.resupply();
        assert_eq!(w.shots_left[40], 14);
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
            mee: None,
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
            ..Default::default()
        };
        let mut c = Character {
            ctype: 1,
            alpha: PI / 2.0,
            pos: [player.x, 640.0, player.z - 2000.0],
            ..Default::default()
        };
        c.reset(&herd.species[1], &mut rng, 128);
        herd.chars.push(c);
        let weapons = Weapons::new(
            vec![rifle()],
            vec![[100, 100, 100, 100]],
            Bits::bit(0),
            false,
        );
        let opts = HuntOptions::default();
        let hunt = Hunt::new(&area, player, herd, weapons, rng, opts, blank_save());
        (area, hunt, car)
    }

    #[test]
    fn no_death_camera_prompts_at_once() {
        let (_, mut hunt, _) = setup(false);
        hunt.death = Some(DeathCam {
            body: 0,
            time: 1,
            pos: [0.0; 3],
            alpha: 0.0,
            beta: 0.0,
        });
        assert!(!hunt.show_exit());
        hunt.opts.death_anim = false;
        assert!(hunt.show_exit());
    }

    #[test]
    fn no_blood_leaves_no_trail() {
        let (area, mut hunt, _) = setup(false);
        hunt.opts.blood = false;
        hunt.herd.chars[0].health = 3;
        hunt.herd.chars[0].blood_ttime = 90_000;
        let mut out = Vec::new();
        for _ in 0..500 {
            hunt.tick(&area, &[], &HuntInput::default(), 10, &mut out);
        }
        assert!(hunt.effects.blood.is_empty());
        hunt.opts.blood = true;
        for _ in 0..500 {
            hunt.tick(&area, &[], &HuntInput::default(), 10, &mut out);
        }
        assert!(!hunt.effects.blood.is_empty());
    }

    /// The farthest an animal ahead notices the hunter, in steps of 500.
    fn notice_range(notice_k: f32, stealth: bool, crouched: bool) -> f32 {
        notice_range_by(notice_k, stealth, crouched, 1.0)
    }

    /// [`notice_range`] for an animal with `smell` (0: sight only).
    fn notice_range_by(notice_k: f32, stealth: bool, crouched: bool, smell: f32) -> f32 {
        let mut last = 0.0;
        for k in 1..80 {
            let d = k as f32 * 500.0;
            let (area, mut hunt, _) = setup(false);
            (hunt.opts.notice_k, hunt.opts.stealth) = (notice_k, stealth);
            if crouched {
                hunt.player.head_y = 110.0;
            }
            hunt.wind.speed = 0.0;
            hunt.herd.species[1].info.smell_k = smell.max(1e-4);
            let c = &mut hunt.herd.chars[0];
            c.pos = [hunt.player.x, c.pos[1], hunt.player.z - d];
            (c.lookx, c.lookz) = (0.0, 1.0);
            hunt.check_afraid(&area);
            if hunt.herd.chars[0].afraid_time != 0 {
                last = d;
            }
        }
        last
    }

    #[test]
    fn survival_sends_growing_waves_and_awards_no_trophies() {
        let (area, mut hunt, car) = setup(false);
        hunt.herd.species.push(Species::new(info(ai::VELO), &car));
        assert!(dangerous(&hunt.herd.species[2]) && !dangerous(&hunt.herd.species[1]));
        hunt.start_survival(
            &area,
            SurvivalOptions {
                roster: vec![2],
                max_wave: 3,
                ..Default::default()
            },
        );
        // The plant-eater stays; the first wave comes after a rest.
        assert_eq!(hunt.herd.chars.len(), 1);
        let cars = [None, Some(&car), Some(&car)];
        let mut out = Vec::new();
        let wave = |hunt: &Hunt, n: u32| hunt.herd.chars.iter().filter(|c| c.wave == n).count();
        let run = |hunt: &mut Hunt, ms: i32, out: &mut Vec<GameEvent>| {
            for _ in 0..ms / 10 {
                hunt.tick(&area, &cars, &HuntInput::default(), 10, out);
            }
        };
        run(&mut hunt, 3100, &mut out);
        assert_eq!((hunt.wave(), wave(&hunt, 1)), (1, 3));
        assert!(out
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "Wave 1")));
        for c in hunt.herd.chars.iter().filter(|c| c.wave == 1) {
            assert!(c.aware_hunter && c.state != 0);
            let d = (c.pos[0] - hunt.player.x).hypot(c.pos[2] - hunt.player.z);
            assert!(d >= 20.0 * 256.0, "{d}");
        }
        // Killed: logged, but no score, trophy or ship.
        while let Some(i) = hunt
            .herd
            .chars
            .iter()
            .position(|c| c.wave == 1 && c.health > 0)
        {
            hunt.debug_bag(&area, i, &mut out);
        }
        assert_eq!((hunt.kills.len(), hunt.room.score), (3, 0));
        assert!(hunt.ship.tasks.is_empty() && hunt.room.bodies.iter().all(|b| b.ctype == 0));
        // The next wave (capped at 3) replaces the fallen; health is back.
        hunt.health = 1000;
        run(&mut hunt, 4100, &mut out);
        assert_eq!((hunt.wave(), wave(&hunt, 1), wave(&hunt, 2)), (2, 0, 3));
        assert_eq!(hunt.health, MAX_HEALTH);
    }

    #[test]
    fn modded_survival_waves_buy_the_dearest_first() {
        let w = super::survival::mee_waves(&[7, 8, 9]);
        assert_eq!(w.len(), 26);
        // 2, 4, 8, 16 points; costs 1, 3, 9.
        assert_eq!(w[0], [7, 7]);
        assert_eq!(w[1], [8, 7]);
        assert_eq!(w[2], [8, 8, 7, 7]);
        assert_eq!(w[3], [9, 8, 8, 7]);
        let spot = c2_formats::mee::SurvivalSpot {
            hunter: (345, 245),
            angle: 90.0,
            spawn: [318, 336, 170, 180],
        };
        let o = SurvivalOptions::mee(&[7, 8, 9], &spot, 6).expect("roster");
        let m = o.mobile.expect("waves");
        assert!(o.unlimited);
        assert_eq!(m.hunter, [345.0 * 256.0 + 128.0, 245.0 * 256.0 + 128.0]);
        assert!((m.spawn[0] - (327.0 * 256.0 + 128.0)).abs() < 1.0);
        assert!((m.hunter_alpha.unwrap_or(0.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(SurvivalOptions::mee(&[], &spot, 6).is_none());
    }

    #[test]
    fn mobile_survival_on_ice_age_takes_stand_ins_round_the_hunter() {
        let (area, mut hunt, car) = setup(false);
        for (name, a) in [("Smilodon", ai::ALLO), ("Wooly rhinoceros", ai::CHASM)] {
            let mut i = info(a);
            i.name = name.into();
            hunt.herd.species.push(Species::new(i, &car));
        }
        let sv = c2_formats::mobile_arcade::Survive {
            area: 5,
            hunter: [100_000.0, 100_000.0],
            spawn: [100_000.0, 110_000.0, 500.0, 500.0],
            first_ms: 500,
            rest_ms: 2000,
            waves: vec![vec!["Allosaurus".into(), "Chasmosaurus".into()]],
            after: vec!["Allosaurus".into()],
            stand_ins: false,
        };
        let start = (hunt.player.x, hunt.player.z);
        let o = SurvivalOptions::mobile(&hunt, &sv, 8).expect("stand-ins");
        let m = o.mobile.clone().unwrap_or_default();
        assert!(m.stand_ins);
        assert_eq!(m.waves, vec![vec![2, 3]]);
        hunt.start_survival(&area, o);
        // Not moved to Mount Ravan's spot.
        assert_eq!((hunt.player.x, hunt.player.z), start);
        let cars = [None, Some(&car), Some(&car), Some(&car)];
        let mut out = Vec::new();
        for _ in 0..60 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
        }
        let wave: Vec<&Character> = hunt.herd.chars.iter().filter(|c| c.wave == 1).collect();
        assert_eq!(wave.len(), 2);
        // Round the hunter, not in the box.
        assert!(wave.iter().all(|c| (c.pos[2] - 110_000.0).abs() > 500.0));
    }

    #[test]
    fn mobile_survival_sends_its_waves_from_its_box() {
        let (area, mut hunt, car) = setup(false);
        for (name, a) in [("Chasmosaurus", ai::CHASM), ("Allosaurus", ai::ALLO)] {
            let mut i = info(a);
            i.name = name.into();
            hunt.herd.species.push(Species::new(i, &car));
        }
        let sv = c2_formats::mobile_arcade::Survive {
            area: 5,
            hunter: [100_000.0, 100_000.0],
            spawn: [100_000.0, 110_000.0, 500.0, 500.0],
            first_ms: 500,
            rest_ms: 2000,
            waves: vec![vec!["Chasmosaurus".into()], vec!["Allosaurus".into(); 2]],
            after: vec!["Chasmosaurus".into(); 3],
            stand_ins: false,
        };
        let missing = c2_formats::mobile_arcade::Survive {
            after: vec!["Utahraptor".into()],
            ..sv.clone()
        };
        assert!(SurvivalOptions::mobile(&hunt, &missing, 8).is_none());
        let o = SurvivalOptions::mobile(&hunt, &sv, 8).unwrap();
        hunt.start_survival(&area, o);
        assert!((hunt.player.x - 100_000.0).abs() < 256.0);
        let cars = [None, Some(&car), Some(&car), Some(&car)];
        let mut out = Vec::new();
        for _ in 0..60 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
        }
        let wave: Vec<&Character> = hunt.herd.chars.iter().filter(|c| c.wave == 1).collect();
        assert_eq!(wave.len(), 1);
        let c = wave[0];
        assert_eq!(c.ctype, 2);
        assert!((c.pos[2] - 110_000.0).abs() <= 500.0 && (c.pos[0] - 100_000.0).abs() <= 500.0);
        // 39 cells off, the Chasmosaurus still comes (not away), by a
        // way 8 cells ahead.
        let (lx, lz) = c.lure.expect("a lure while far off");
        assert!(
            (lz - c.pos[2]).abs() <= 9.0 * 256.0 && lz < c.pos[2],
            "{lz} {}",
            c.pos[2]
        );
        assert!((c.tgz - lz).abs() < 1.0 && (c.tgx - lx).abs() < 1.0);
        // Then the next wave in turn, 2 s after this one falls.
        hunt.debug_bag(
            &area,
            hunt.herd.chars.iter().position(|c| c.wave == 1).unwrap(),
            &mut out,
        );
        for _ in 0..210 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
        }
        let w2: Vec<usize> = hunt
            .herd
            .chars
            .iter()
            .filter(|c| c.wave == 2)
            .map(|c| c.ctype)
            .collect();
        assert_eq!(w2, [3, 3]);
    }

    #[test]
    fn survival_waves_start_by_the_hunter_not_up_a_mountain() {
        let (mut area, mut hunt, car) = setup(false);
        // The box's west half a plateau far above the hunter.
        for z in 0..1024 {
            for x in 0..388 {
                area.map.height[z * 1024 + x] = 250;
            }
        }
        let mut i = info(ai::CHASM);
        i.name = "Chasmosaurus".into();
        hunt.herd.species.push(Species::new(i, &car));
        let sv = c2_formats::mobile_arcade::Survive {
            area: 5,
            hunter: [100_000.0, 100_000.0],
            spawn: [100_000.0, 106_000.0, 2000.0, 500.0],
            first_ms: 500,
            rest_ms: 2000,
            waves: vec![vec!["Chasmosaurus".into(); 6]],
            after: vec![],
            stand_ins: false,
        };
        hunt.start_survival(&area, SurvivalOptions::mobile(&hunt, &sv, 8).unwrap());
        let cars = [None, Some(&car), Some(&car)];
        let mut out = Vec::new();
        for _ in 0..60 {
            hunt.tick(&area, &cars, &HuntInput::default(), 10, &mut out);
        }
        let wave: Vec<&Character> = hunt.herd.chars.iter().filter(|c| c.wave == 1).collect();
        assert_eq!(wave.len(), 6);
        assert!(
            wave.iter().all(|c| c.pos[0] >= 388.0 * 256.0),
            "none on the plateau"
        );
    }

    #[test]
    fn survival_picks_waves_mods_and_the_t_rex() {
        let (_, mut hunt, car) = setup(false);
        for (name, a) in [
            ("Allosaurus", ai::ALLO),
            ("Utahraptor", ai::VELO),
            ("T-Rex", ai::TREX),
        ] {
            let mut i = info(a);
            i.name = name.into();
            hunt.herd.species.push(Species::new(i, &car));
        }
        // Nothing hunting picked: every hunter but the T-Rex; the mod's
        // animal is the hardcore extra.
        let o = SurvivalOptions::pick(&hunt, SurvivalKind::Hardcore, 5);
        assert_eq!((o.roster, o.hardcore, o.trex), (vec![2, 3], vec![3], None));
        // The allosaur picked: it alone comes; custom brings the T-Rex.
        hunt.opts.targets = Bits::bit(ai::ALLO as usize);
        let o = SurvivalOptions::pick(&hunt, SurvivalKind::Custom, 5);
        assert_eq!((o.roster, o.trex, o.unlimited), (vec![2], Some(4), true));
    }

    #[test]
    fn friendly_and_stealth_shorten_noticing() {
        let normal = notice_range(1.0, false, false);
        assert!(normal > 0.0);
        let friendly = notice_range(0.6, false, false);
        assert!(
            friendly < normal && friendly > 0.0,
            "{friendly} vs {normal}"
        );
        // Crouched, stealth halves how visible the hunter is (sight; smell
        // still finds him).
        let crouched = notice_range_by(1.0, false, true, 0.0);
        assert!(crouched > 0.0);
        assert!(notice_range_by(1.0, true, true, 0.0) < crouched);
        // Standing, stealth changes nothing.
        assert_eq!(
            notice_range_by(1.0, true, false, 0.0),
            notice_range_by(1.0, false, false, 0.0)
        );
    }

    #[test]
    fn non_targets_score_half_only_in_the_original() {
        let score = |dialect: bool, targets: Bits, bonus: Bits| {
            let (area, mut hunt, _) = setup(false);
            hunt.herd.species[1].info.base_score = 100.0;
            if dialect {
                hunt.herd.species[1].tuning.mee_ai = Some(ai::STEGO);
            }
            (hunt.opts.targets, hunt.opts.no_call_bonus) = (targets, bonus);
            hunt.add_ship_task(&area, 0, &mut Vec::new());
            hunt.room.score
        };
        let stego = Bits::bit(ai::STEGO as usize);
        let none = Bits::NONE;
        assert_eq!(score(false, stego, none), 100);
        assert_eq!(score(false, none, none), 50);
        assert_eq!(score(true, none, none), 100);
        assert_eq!(score(true, stego, stego), 110);
    }

    #[test]
    fn dialect_calls_go_by_menu_slot() {
        let (area, mut hunt, car) = setup(false);
        let cars = [None, Some(&car)];
        let mut m = crate::dino::mee::MeeInfo::new(ai::STEGO);
        (m.call_no, m.fear_calls) = (3, vec![5]);
        hunt.herd.species[1].mee = Some(Box::new(m));
        // Can't see or smell the hunter: only calls move it.
        hunt.herd.species[1].info.look_k = 0.0;
        hunt.herd.species[1].info.smell_k = 0.0;
        hunt.opts.targets = Bits::bit(13) | Bits::bit(15);
        let call = HuntInput {
            call: true,
            ..Default::default()
        };
        let mut out = Vec::new();
        // Slot 3's call: it comes over (heads near the hunter).
        hunt.target_call = 13;
        hunt.tick(&area, &cars, &call, 10, &mut out);
        let c = &hunt.herd.chars[0];
        assert!((c.tgx - hunt.player.x).abs() <= 1800.0 && (c.tgz - hunt.player.z).abs() <= 1800.0);
        assert_eq!(c.state, 0);
        assert!(out
            .iter()
            .any(|e| matches!(e, GameEvent::Call { call: 13, .. })));
        // Slot 5's call: it fears that one.
        hunt.call_lock = 0;
        hunt.target_call = 15;
        hunt.tick(&area, &cars, &call, 10, &mut out);
        assert_ne!(hunt.herd.chars[0].state, 0);
    }

    #[test]
    fn dialect_ambients_without_a_call_are_not_the_first_menu_animal() {
        let (_, mut hunt, _) = setup(false);
        hunt.opts.targets = Bits::bit(10);
        let c = hunt.herd.chars[0].clone();
        // `callNo` left out: 0, as the engine reads it.
        hunt.herd.species[1].mee = Some(Box::new(crate::dino::mee::MeeInfo::new(ai::STEGO)));
        assert!(hunt.is_target(&c));
        hunt.herd.species[1].mee = Some(Box::new(crate::dino::mee::MeeInfo::new(2)));
        assert_eq!(hunt.call_bit(&c), None);
        assert!(!hunt.is_target(&c));
        // Unless its data puts it on the radar.
        hunt.herd.species[1].tuning.radar = Some(true);
        assert!(hunt.is_target(&c));
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
            h.apply_recoil(10);
            h.weapon_step(10, out);
            h.animate_bullets(area, cars, 10, out);
            h.track_step(10);
        }
    }

    #[test]
    fn dialect_weapons_fly_kick_track_and_fire_once_per_pull() {
        let res = c2_formats::res_txt::ResTxt::parse(
            "weapons {\n{\n name = 'Rifle'\n land_power = 4\n land_veloc = 23.0\n land_prec = 1.8\n \
             shots = 7\n recoil = 4\n shake = 0.05\n radar = TRUE\n radR = 255\n radG = 0\n \
             radB = 0\n radTime = 1000\n land_fall = 0.6\n}\n}\n.\n",
        )
        .unwrap_or_default();
        let block = crate::mee::vanilla_keys_for_test(&res.sections[0].blocks[0]);
        let info = WeaponInfo::from_def(&WeaponDef::from_block(&block));
        let m = info.mee.clone().expect("dialect keys");
        assert_eq!((m.veloc, m.fall, m.recoil), (23.0, 0.6, 4.0));
        assert_eq!(m.radar, Some(([255, 0, 0], 1000)));
        assert!(!info.fall, "land_fall isn't the crossbow flag");

        let (area, mut h, car) = setup(false);
        h.weapons = Weapons::new(vec![info], vec![[100, 100, 100, 100]], Bits::bit(0), false);
        let cars = [Some(&car), Some(&car)];
        let mut ev = Vec::new();
        let toggle = HuntInput {
            toggle_weapon: true,
            ..Default::default()
        };
        weapon_ticks(&mut h, &area, &cars, toggle, 1, &mut ev);
        assert!((h.weapons.shakel - 0.2).abs() < 0.01, "shake * 4");
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 12, &mut ev);
        let fire = HuntInput {
            fire: true,
            ..Default::default()
        };
        let beta = h.player.beta;
        let health = h.herd.chars[0].health;
        weapon_ticks(&mut h, &area, &cars, fire, 1, &mut ev);
        assert_eq!(h.bullets.len(), 1, "a round in flight, not a trace");
        // 2000 units off at 1472 a 17 ms step: there within a few ticks.
        weapon_ticks(&mut h, &area, &cars, fire, 4, &mut ev);
        assert!(h.bullets.is_empty());
        assert!(h.herd.chars[0].health < health, "hit");
        assert!(h.herd.chars[0].tracked.is_some(), "tracked");
        assert!(h.player.beta < beta, "the recoil lifts the aim");
        // Held past the fire animation: no second shot until released.
        weapon_ticks(&mut h, &area, &cars, fire, 20, &mut ev);
        assert_eq!(h.weapons.shots_left[0], 6);
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 1, &mut ev);
        weapon_ticks(&mut h, &area, &cars, fire, 1, &mut ev);
        assert_eq!(h.weapons.shots_left[0], 5);
        // The tracker runs out after its second.
        weapon_ticks(&mut h, &area, &cars, HuntInput::default(), 120, &mut ev);
        assert!(h.herd.chars[0].tracked.is_none());
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
    fn dialect_trophies_take_a_spot_made_for_them() {
        use c2_formats::mee::TrophySpot;
        let spot = |c: usize| TrophySpot {
            chars: vec![c],
            ..Default::default()
        };
        let (area, mut h, _) = setup(false);
        // The animal (type 1) has the second spot.
        h.herd.trophy_spots = vec![spot(0), spot(1)];
        h.add_ship_task(&area, 0, &mut Vec::new());
        assert_eq!((h.room.bodies[0].ctype, h.room.bodies[1].ctype), (0, 1));
        // None made for it: any free spot.
        let (area, mut h, _) = setup(false);
        h.herd.trophy_spots = vec![spot(0)];
        h.add_ship_task(&area, 0, &mut Vec::new());
        assert_eq!(h.room.bodies[0].ctype, 1);
        // All taken: no trophy.
        let (area, mut h, _) = setup(false);
        h.herd.trophy_spots = vec![spot(0)];
        h.room.bodies[0].ctype = 5;
        let mut ev = Vec::new();
        h.add_ship_task(&area, 0, &mut ev);
        assert!(!ev.iter().any(|e| matches!(e, GameEvent::Trophy { .. })));
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
        assert!(ev.iter().any(|e| matches!(
            e,
            GameEvent::Scored {
                ctype: 1,
                score: 0,
                ..
            }
        )));
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

    #[test]
    fn trophy_room_shows_plaques_and_removes_trophies() {
        let mut area = Area::flat(10);
        area.set_trophy_mode();
        let car = target_car(true);
        let mut room = blank_save();
        room.bodies[0] = TrophyItem {
            ctype: 1,
            scale: 1.0,
            ..Default::default()
        };
        room.bodies[7] = TrophyItem {
            ctype: 1,
            scale: 0.8,
            ..Default::default()
        };
        let mut rng = MsRand::new(5);
        let mut herd = Herd {
            species: vec![
                Species::new(info(0), &car),
                Species::new(info(ai::STEGO), &car),
            ],
            chars: vec![],
            demo_index: None,
            ..Default::default()
        };
        herd.place_trophies(&area, &room, &mut rng);
        assert_eq!(herd.chars.len(), 2);
        assert_eq!((herd.chars[1].state, herd.chars[1].scale), (7, 0.8));
        // Slot 0 is cell (69, 66), facing +z (look = (cos a, sin a)); stand
        // 2.5 cells ahead of it.
        let mut player = Player::new(&area, (69, 68));
        player.z = 66.0 * 256.0 + 128.0 + 640.0;
        let weapons = Weapons::new(vec![rifle()], vec![[100; 4]], Bits::NONE, false);
        let opts = HuntOptions {
            trophy: true,
            ..Default::default()
        };
        let mut h = Hunt::new(&area, player, herd, weapons, rng, opts, room);
        let mut ev = Vec::new();
        h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
        assert_eq!(h.trophy_body, Some(0));
        // Locked, it stays; unlocked, R removes it.
        assert_eq!(h.toggle_lock_current(), Some(true));
        h.remove_current_trophy();
        assert_eq!(h.room.bodies[0].ctype, 1);
        assert_eq!(h.toggle_lock_current(), Some(false));
        // Moved to the next free stand after it (slot 1), and back.
        assert!(h.move_current_trophy(&area, 1));
        assert_eq!((h.room.bodies[0].ctype, h.room.bodies[1].ctype), (0, 1));
        assert_eq!(h.trophy_body, Some(1));
        assert!(h.herd.chars.iter().any(|c| c.state == 1));
        assert!(h.move_current_trophy(&area, -1));
        assert!(!h.move_current_trophy(&area, -1));
        h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
        assert_eq!(h.trophy_body, Some(0));
        h.remove_current_trophy();
        assert_eq!(h.room.bodies[0].ctype, 0);
        assert_eq!(h.herd.chars.len(), 1);
        h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
        assert_eq!(h.trophy_body, None);
        // No weapons, no map; Escape leaves without touching the stats.
        h.tick(
            &area,
            &[],
            &HuntInput {
                toggle_weapon: true,
                toggle_map: true,
                ..Default::default()
            },
            10,
            &mut ev,
        );
        assert_eq!(h.weapons.state, WeaponState::Hidden);
        assert!(!h.map_mode);
        h.press_escape();
        assert_eq!(h.outcome(), Some(Outcome::Exited));
        assert_eq!(h.room.total.time, 0.0);
    }

    #[test]
    fn pause_freezes_and_health_recovers() {
        let (area, mut h, _car) = setup(true);
        let mut ev = Vec::new();
        h.health = MAX_HEALTH - 100;
        h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
        assert_eq!(h.health, MAX_HEALTH - 60);
        h.toggle_pause();
        let (t, pos) = (h.real_time, h.herd.chars[0].pos);
        h.tick(&area, &[], &HuntInput::default(), 10, &mut ev);
        assert_eq!((h.real_time, h.health), (t, MAX_HEALTH - 60));
        assert_eq!(h.herd.chars[0].pos, pos);
        h.press_escape();
        assert!(!h.paused && !h.exit_prompt);
    }
}
