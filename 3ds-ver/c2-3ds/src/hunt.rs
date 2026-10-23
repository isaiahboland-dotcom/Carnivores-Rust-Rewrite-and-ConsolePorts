//! Starting a hunt from a cooked pack and turning 3DS input into the
//! simulation's controls.

use std::collections::HashMap;

use anyhow::Result;
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{
    GameEvent, Hunt, HuntInput, HuntOptions, SurvivalKind, SurvivalOptions, Weapons,
};
use c2_core::load;
use c2_core::player::{Controls, Player};
use c2_core::rng::MsRand;
use c2_formats::car::Car;
use c2_formats::sav::Save;
use c2_vfs::Vfs;

use crate::rules::{Roam, Role, Rules};
use crate::world::DrawInfo;

/// Simulation step: 20 ms, half the PC's rate. Every animal costs about
/// 0.1 ms a step on an Old 3DS; the original updates once per frame with
/// any `TimeDt`, so the game plays the same, and the camera is
/// interpolated between steps.
pub const TICK_MS: u32 = 20;
/// Most ticks run in one frame (40 ms of game time).
pub const MAX_TICKS: u32 = 4;

/// The INFO.TXT `pack` this build reads: 4 = every area, animal model
/// (by file, `C2RS/M/`) and weapon; calls by menu slot; `cost.*` (linear)
/// and `heap.*` (main memory) lines; `dialect` for Modder's Edition rules.
pub const PACK_VERSION: u32 = 4;

fn kv(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// `C2RS/INFO.TXT`: what the cooker put in the pack.
#[derive(Debug, Clone)]
pub struct PackInfo {
    /// Pack layout version (`pack` in INFO.TXT).
    pub pack: u32,
    pub target: String,
    /// `AREA1`.. and `TROPHY`, as packed.
    pub areas: Vec<String>,
    pub view: u32,
    pub object_view: u32,
    /// Sample rate of the pack's sounds.
    pub sound_rate: u32,
    /// Linear memory a hunt may use, and what each piece takes (KiB):
    /// `hunt` (always), `AREAn`, `m:{model}`, `wpn{i}`.
    pub linear_kb: usize,
    pub costs: HashMap<String, usize>,
    /// Main memory: the budget, each area's loading peak with the hunter
    /// and ambient animals (`AREAn`), and each model and weapon.
    pub heap_kb: usize,
    pub heaps: HashMap<String, usize>,
    /// Which survivals are offered: `ours`, `mobile` or `both` (the mobile
    /// one only with its `C2RS/SURVIVE.TXT`, from the player's APK).
    pub survival: String,
}

impl PackInfo {
    pub fn parse(text: &str) -> Self {
        let m = kv(text);
        let get = |k: &str| m.get(k).cloned().unwrap_or_default();
        let num = |k: &str, d: u32| get(k).parse().unwrap_or(d);
        Self {
            pack: num("pack", 1),
            target: get("target"),
            areas: get("areas")
                .split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_string)
                .collect(),
            view: num("view", 42),
            object_view: num("object_view", 28),
            sound_rate: num("sound_rate", c2_core::sound::SAMPLE_RATE),
            linear_kb: num("linear_kb", 16 * 1024) as usize,
            costs: Self::prefixed(&m, "cost."),
            heap_kb: num("heap_kb", 34 * 1024) as usize,
            heaps: Self::prefixed(&m, "heap."),
            survival: get("survival"),
        }
    }

    fn prefixed(m: &HashMap<String, String>, prefix: &str) -> HashMap<String, usize> {
        m.iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(prefix)?.to_string(), v.parse().ok()?)))
            .collect()
    }

    pub fn cost(&self, piece: &str) -> usize {
        self.costs.get(piece).copied().unwrap_or(0)
    }

    pub fn heap(&self, piece: &str) -> usize {
        self.heaps.get(piece).copied().unwrap_or(0)
    }

    /// How to draw `area` (`C2RS/AREAn/INFO.TXT`).
    pub fn draw_info(&self, vfs: &Vfs, area: &str) -> DrawInfo {
        let m = kv(&String::from_utf8_lossy(
            &vfs.read(&format!("C2RS/{area}/INFO.TXT"))
                .unwrap_or_default(),
        ));
        let num = |k: &str, d: usize| m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        DrawInfo {
            area: area.to_string(),
            tile: num("terrain_tile", 64),
            cols: num("atlas_cols", 16),
            view: self.view,
            object_view: self.object_view,
            terrain_count: num("terrain_count", 1),
        }
    }
}

/// What the menu picked (the original menu passes these to the game on its
/// command line: `prj=`, `din=`, `wep=`, `dtm=`, `-double`, `-observ`).
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    /// `AREA1`.., or `TROPHY` for the trophy room.
    pub area: String,
    pub time: TimeOfDay,
    /// Bit 10 + menu slot per target (`TargetDino`; in the original the
    /// slot is the AI - 10, so the bit is the AI).
    pub targets: u128,
    /// Roaming add-ons picked (no menu slot of their own); their call
    /// slots are among the targets.
    pub roamers: Vec<Roam>,
    /// Target calls the pack has no sound for (add-ons without one): no
    /// button, and ZR doesn't call them. Filled when the hunt loads.
    pub silent_calls: Vec<i32>,
    /// Bit per carried weapon.
    pub weapons: u32,
    pub double_ammo: bool,
    pub observer: bool,
    /// Extra ammo gear (3DS only): refills to carry, and what it does to
    /// kill scores.
    pub resupplies: u32,
    pub score_factor: f32,
    /// Settings (3DS only): small ambient animals at most (0 = none), and
    /// the animals' running speed (0.5..=1).
    pub max_ambient: i32,
    pub dino_run_k: f32,
    /// Settings: blood, and the death camera.
    pub blood: bool,
    pub death_anim: bool,
    /// Hunt options: how far animals notice the hunter (1; "3DS friendly"
    /// less), stealth crouch (ZL toggles; crouched: gyro aim, half as
    /// visible and loud), and 4 random ambient species (modded games).
    pub notice_k: f32,
    pub stealth: bool,
    pub random_ambients: bool,
    /// The crouch button (setting).
    pub crouch_key: crate::settings::CrouchKey,
    /// A survival instead of a hunt (README M13).
    pub survival: Option<SurvivalKind>,
    /// Animals loaded for the survival only (bit 10 + slot, among the
    /// targets): hardcore's extras, custom's T-Rex. Not in the waves.
    pub survival_extra: u128,
    /// The mobile game's Survive mode, for a mobile survival.
    pub mobile_survival: Option<c2_formats::mobile_arcade::Survive>,
}

/// The most snowflakes on the 3DS (an area asks for up to 2000).
const SNOW_MAX: usize = 300;

/// The most animals in a survival wave on the 3DS.
pub const SURVIVAL_MAX: usize = 6;

impl Setup {
    pub fn trophy(&self) -> bool {
        self.area.eq_ignore_ascii_case("TROPHY")
    }

    /// Whether character `i` is loaded: the hunter, the ambient animals
    /// unless they are turned off, the targets and picked roamers; in the
    /// trophy room, every species (the trophies).
    pub fn keeps_char(&self, i: usize, role: Role) -> bool {
        self.trophy()
            || match role {
                Role::Hunter => true,
                Role::Ambient => self.max_ambient > 0,
                Role::Slot(k) => k < 118 && self.targets & (1 << (k + 10)) != 0,
                Role::Roamer => self.roamers.iter().any(|r| r.char == i),
                Role::Off => false,
            }
    }

    /// Whether call id `id` can be called: roamers' slots can't (their
    /// add-ons supply no call; they score +10% instead).
    pub fn callable(&self, id: i32) -> bool {
        !self.roamers.iter().any(|r| r.slot as i32 + 10 == id) && !self.silent_calls.contains(&id)
    }

    pub fn carried(&self) -> Vec<usize> {
        (0..32).filter(|i| self.weapons & (1 << i) != 0).collect()
    }
}

pub struct Game {
    pub info: PackInfo,
    pub setup: Setup,
    pub draw: DrawInfo,
    pub area: Area,
    pub hunt: Hunt,
    pub cars: Vec<Option<Car>>,
    /// Per character, the name its model is cooked under (`C2RS/M/`).
    pub keys: Vec<String>,
    pub weapon_cars: Vec<Option<Car>>,
    pub events: Vec<GameEvent>,
    acc_ms: f32,
    /// Ticks run by the last `update`.
    pub ticks_last: u32,
    /// `RunMode`.
    pub run: bool,
    /// Stealth crouch: crouched (ZL toggles).
    pub crouched: bool,
    /// Crouch on L: how long it has been held (ms).
    l_ms: f32,
    /// Crouch on D-Pad Up: 0 walk, 1 run, 2 crouch.
    gait: u8,
    /// The trophy room's question waiting for A / B.
    pub ask: Option<TrophyAsk>,
    /// The gyroscope, on for a stealth hunt: raw-to-degrees/s.
    gyro: Option<f32>,
    /// Extra ammo refills left.
    pub resupplies: u32,
    pub prev_cam: [f32; 3],
}

/// What the trophy room asks before doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrophyAsk {
    Remove,
    Lock,
}

/// What the hunter does with the 3DS controls.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pad {
    /// Circle Pad, -1..1 (x right, y up).
    pub stick: (f32, f32),
    /// Look: C-Stick, touch drag (pixels this frame).
    pub look: (f32, f32),
    pub turn_left: bool,
    pub turn_right: bool,
    pub look_up: bool,
    pub look_down: bool,
    pub fire: bool,
    pub run: bool,
    pub jump: bool,
    pub crouch: bool,
    /// ZL pressed this frame (stealth crouch toggles).
    pub crouch_down: bool,
    /// L held, and released this frame (crouch on L: hold / tap).
    pub l_held: bool,
    pub l_up: bool,
    // Pressed this frame:
    pub toggle_weapon: bool,
    pub next_weapon: bool,
    pub binoculars: bool,
    pub call: bool,
    pub pause: bool,
    pub escape: bool,
    pub confirm: bool,
    pub cancel: bool,
    /// Y / X at the leave prompt (or once dead): a new hunt, same setup.
    pub restart: bool,
    /// Pressed this frame: L, R, X, Y (the trophy room's controls).
    pub l_down: bool,
    pub r_down: bool,
    pub x_down: bool,
    pub y_down: bool,
    /// From the bottom screen: a weapon (index), a call (AI).
    pub select_weapon: Option<usize>,
    pub select_call: Option<i32>,
    /// Use one extra ammo refill.
    pub resupply: bool,
}

/// Frees what the 3DS doesn't need once the GPU has its copies.
fn trim(area: &mut Area, cars: &mut [Option<Car>]) {
    area.rsc.textures = Vec::new();
    area.rsc.skies = Default::default();
    area.rsc.random_sounds = Vec::new();
    for a in &mut area.rsc.ambients {
        a.pcm = Vec::new();
    }
    for o in &mut area.rsc.objects {
        o.model.mesh.texture = Vec::new();
        o.sprite = Vec::new();
    }
    area.map.texture2 = Vec::new();
    for (i, l) in area.map.light.iter_mut().enumerate() {
        if i != area.time as usize {
            *l = Vec::new();
        }
    }
    for c in cars.iter_mut().flatten() {
        c.mesh.texture = Vec::new();
        for s in &mut c.sounds {
            s.pcm = Vec::new();
        }
    }
}

fn next_seed(s: u32) -> u32 {
    s.wrapping_mul(1_103_515_245).wrapping_add(12345)
}

/// `ReInitGame`: the hunter at a random landing spot (or the trophy room's
/// door) and the herd placed (or the trophies), as `c2-pc`'s `new_hunt`.
fn new_hunt(
    area: &Area,
    setup: &Setup,
    species: &[c2_core::dino::Species],
    tables: Option<&c2_formats::mee::Tables>,
    weapon_info: &[(c2_core::game::WeaponInfo, [i32; 4])],
    ship_ms: i32,
    seed: u32,
    room: Save,
) -> Hunt {
    let o = room.options;
    let trophy = setup.trophy();
    let mut rng = MsRand::new(seed);
    let player = if trophy {
        Player::new(area, (76, 70))
    } else {
        let cell = area
            .landings
            .get(seed as usize % area.landings.len().max(1))
            .copied()
            .unwrap_or((512, 512));
        Player::new(area, cell)
    };
    let mut herd = Herd {
        species: species.to_vec(),
        chars: Vec::new(),
        demo_index: None,
        ..Default::default()
    };
    // Dialect games: the trophy room's spots (where trophies stand, and
    // which a kill takes).
    if let Some(t) = tables {
        herd.trophy_spots = t.trophies.clone();
    }
    if trophy {
        herd.place_trophies(area, &room, &mut rng);
    } else if let Some(t) = tables {
        herd.place_mee(area, (player.x, player.z), &mut rng, t, o.dens);
    } else {
        herd.place(
            area,
            (player.x, player.z),
            &mut rng,
            PlaceOptions {
                density: o.dens,
                opt_agres: o.agres,
                targets: setup.targets,
                max_ambient: Some(setup.max_ambient),
            },
        );
    }
    let carried = if trophy || setup.observer {
        0
    } else {
        setup.weapons
    };
    let (infos, anims): (Vec<_>, Vec<_>) = weapon_info.iter().cloned().unzip();
    let weapons = Weapons::new(infos, anims, u128::from(carried), setup.double_ammo);
    let opts = HuntOptions {
        targets: if setup.observer { 0 } else { setup.targets },
        radar: o.radar != 0,
        opt_agres: o.agres,
        opt_sens: o.sens,
        // ctViewR at the lowest view-range option (42 cells); 60 in the
        // trophy room.
        view_r: if trophy { 60 } else { 42 },
        render_r: 0,
        camo: o.camo != 0,
        scent: o.scent != 0,
        tranq: o.tranq != 0,
        observer: setup.observer,
        trophy,
        score_factor: setup.score_factor,
        dino_run_k: setup.dino_run_k,
        blood: setup.blood,
        death_anim: setup.death_anim,
        afraid_every: 2,
        notice_k: setup.notice_k,
        stealth: setup.stealth,
        no_call_bonus: setup
            .roamers
            .iter()
            .filter(|r| r.slot < 118)
            .fold(0, |m, r| m | 1 << (r.slot + 10)),
    };
    let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
    hunt.ship_anim_ms = ship_ms;
    // The area's snow (Ice Age), thinner than on PC.
    hunt.effects.snow = tables
        .and_then(|t| t.snow)
        .filter(|_| !setup.trophy())
        .map(|s| c2_core::effects::Snowfall::new(s, SNOW_MAX));
    let mobile = setup
        .mobile_survival
        .as_ref()
        .filter(|_| setup.survival == Some(SurvivalKind::Mobile))
        .and_then(|m| SurvivalOptions::mobile(&hunt, m, SURVIVAL_MAX));
    if let Some(o) = mobile {
        hunt.start_survival(area, o);
    } else if let Some(kind) = setup.survival {
        let kind = if kind == SurvivalKind::Mobile {
            SurvivalKind::Normal
        } else {
            kind
        };
        let mut o = SurvivalOptions::pick(&hunt, kind, SURVIVAL_MAX);
        // The menu's extras come only as extras.
        let extra = |&i: &usize| {
            let c = c2_core::dino::Character {
                ctype: i,
                ..Default::default()
            };
            hunt.call_bit(&c).is_some_and(|b| {
                setup.survival_extra & 1u128.checked_shl(b as u32).unwrap_or(0) != 0
            })
        };
        let extras: Vec<usize> = (0..species.len()).filter(extra).collect();
        o.roster.retain(|i| !extras.contains(i));
        if kind == SurvivalKind::Hardcore && !extras.is_empty() {
            o.hardcore = extras;
        }
        hunt.start_survival(area, o);
    }
    hunt
}

impl Game {
    /// Loads what `setup` needs: its area, the hunter, ambient and target
    /// animals, the carried weapons.
    pub fn load(
        vfs: &Vfs,
        info: PackInfo,
        setup: Setup,
        room: Save,
        seed: u32,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Self> {
        // The area first, while the heap is empty: its map is the biggest
        // file, and is briefly in memory twice (file and parsed).
        // `c2-cook`'s `load_peak` repeats these steps; keep them in step.
        let mut setup = setup;
        setup.silent_calls = (10..128)
            .filter(|&ai| setup.targets & (1u128 << ai) != 0)
            .filter(|ai| vfs.resolve(&format!("C2RS/SND/K{ai}_0.PCM")).is_none())
            .collect();
        progress(&format!("area {}", setup.area));
        let area = load::area_with(vfs, &setup.area, setup.time, true)?;
        progress("rules (_RES.TXT)");
        let rules =
            Rules::load(vfs)?.hunt_with(&setup.area, setup.time, setup.targets, &setup.roamers);
        progress("animals");
        // The trophy room needs only the species on display (every one a
        // pack with add-ons has won't fit in an Old 3DS).
        let trophies: Vec<usize> = room
            .bodies
            .iter()
            .filter_map(|b| usize::try_from(b.ctype).ok().filter(|&c| c != 0))
            .collect();
        // Random ambients: 4 ambient species this hunt (modded games add
        // many; the rest aren't loaded, so they don't spawn).
        let ambients = setup.random_ambients.then(|| {
            let all: Vec<usize> = (0..rules.roles.len())
                .filter(|&i| rules.roles[i] == Role::Ambient)
                .collect();
            pick(&all, 4, seed)
        });
        let (mut species, cars) = load::species_with(vfs, &rules.res, &|i| {
            if setup.trophy() {
                return trophies.contains(&i) || rules.roles.get(i) == Some(&Role::Hunter);
            }
            if let Some(a) = &ambients {
                if rules.roles.get(i) == Some(&Role::Ambient) {
                    return setup.max_ambient > 0 && a.contains(&i);
                }
            }
            rules.roles.get(i).is_some_and(|&r| setup.keeps_char(i, r))
        });
        if let Some(t) = &rules.tables {
            c2_core::mee::tune(&mut species, t);
        }
        let res = &rules.res;
        progress("weapons");
        let carried = setup.carried();
        let weapons = load::weapons_with(vfs, res, &|i| carried.contains(&i));
        let mut weapon_cars = Vec::new();
        let mut weapon_info = Vec::new();
        for w in weapons {
            weapon_info.push((w.info, w.anims));
            weapon_cars.push(w.car);
        }
        let ship_ms = load::ship_anim_ms(vfs);
        let seed = next_seed(seed);
        let mut hunt = new_hunt(
            &area,
            &setup,
            &species,
            rules.tables.as_ref(),
            &weapon_info,
            ship_ms,
            seed,
            room,
        );
        hunt.area_no = c2_formats::mee::area_of(&setup.area).map_or(0, |n| n as i32);
        hunt.clock = clock();
        // Start on a call that sounds.
        if !setup.callable(hunt.target_call) {
            if let Some(a) =
                (10..128).find(|&a| setup.targets & (1u128 << a) != 0 && setup.callable(a))
            {
                hunt.target_call = a;
            }
        }
        let keys = rules.keys;
        let resupplies = setup.resupplies;
        let setup_stealth = setup.stealth;
        Ok(Self {
            prev_cam: hunt.camera().0,
            draw: info.draw_info(vfs, &setup.area),
            info,
            setup,
            area,
            hunt,
            cars,
            keys,
            weapon_cars,
            events: Vec::new(),
            acc_ms: 0.0,
            ticks_last: 0,
            run: false,
            crouched: false,
            l_ms: 0.0,
            gait: 0,
            ask: None,
            gyro: gyro_on(setup_stealth),
            resupplies,
        })
    }

    /// This hunt's kills into the hunter's record.
    pub fn record_kills(&mut self, vfs: &Vfs) -> std::io::Result<()> {
        use c2_core::record::{Kill, Mode};
        let kills = std::mem::take(&mut self.hunt.kills);
        let wave = self.hunt.wave();
        if kills.is_empty() && wave == 0 {
            return Ok(());
        }
        let mode = if self.hunt.survival.is_some() {
            Mode::Survival
        } else {
            Mode::Hunt
        };
        let n = self.hunt.area_no;
        let area = crate::menu::title(vfs, &format!("AREA{n}.TXT"), format!("Area {n}"));
        let (y, mo, d, _, _) = self.hunt.clock;
        let mut r = crate::save::load_record();
        for k in kills {
            r.kills.push(Kill {
                mode,
                animal: self
                    .hunt
                    .herd
                    .species
                    .get(k.ctype)
                    .map_or_else(String::new, |s| s.info.name.clone()),
                score: k.score,
                range: k.range,
                weapon: self
                    .hunt
                    .weapons
                    .info
                    .get(k.weapon)
                    .map_or_else(String::new, |w| w.name.clone()),
                area: area.clone(),
                date: (y << 20) + (mo << 10) + d,
            });
        }
        r.best_wave = r.best_wave.max(wave);
        crate::save::store_record(&r)
    }

    /// Frees load-time data; call once the GPU copies exist.
    pub fn trim(&mut self) {
        trim(&mut self.area, &mut self.cars);
        for c in self.weapon_cars.iter_mut().flatten() {
            c.mesh.texture = Vec::new();
        }
    }

    /// Runs the simulation for `dt` seconds of real time.
    /// The trophy room's buttons: L / R move the trophy shown to the next
    /// free stand that way, X removes it, Y locks or unlocks it (both
    /// asked: A yes, B no). They don't do their hunt jobs here. A message
    /// to show, if any.
    pub fn trophy_buttons(&mut self, pad: &mut Pad) -> Option<String> {
        if !self.hunt.opts.trophy {
            return None;
        }
        let mut msg = None;
        let asked = self.ask.is_some();
        if let Some(a) = self.ask {
            if pad.confirm {
                match a {
                    TrophyAsk::Remove => self.hunt.remove_current_trophy(),
                    TrophyAsk::Lock => {
                        let _ = self.hunt.toggle_lock_current();
                    }
                }
                self.ask = None;
            } else if pad.cancel {
                self.ask = None;
            }
        } else if let Some(slot) = self.hunt.trophy_body {
            let locked = self.hunt.room.bodies.get(slot).is_some_and(|b| b.locked());
            for (on, dir) in [(pad.l_down, -1), (pad.r_down, 1)] {
                if on && !self.hunt.move_current_trophy(&self.area, dir) {
                    msg = Some("No free stand that way".into());
                }
            }
            if pad.x_down {
                if locked {
                    msg = Some("Locked: Y to unlock".into());
                } else {
                    self.ask = Some(TrophyAsk::Remove);
                }
            }
            if pad.y_down {
                self.ask = Some(TrophyAsk::Lock);
            }
        }
        // Not their hunt jobs.
        (pad.fire, pad.jump, pad.restart) = (false, false, false);
        (pad.look_up, pad.turn_left) = (false, false);
        // A / B answered the question (else they answer the leave prompt).
        if asked {
            (pad.confirm, pad.cancel) = (false, false);
        }
        msg
    }

    pub fn update(&mut self, pad: &Pad, dt: f32) {
        use crate::settings::CrouchKey;
        let key = self.setup.crouch_key;
        if pad.run {
            if key == CrouchKey::DpadUp {
                self.gait = (self.gait + 1) % 3;
                self.run = self.gait == 1;
            } else {
                self.run = !self.run;
            }
        }
        // Crouch on L: held a quarter second it crouches (or toggles the
        // stealth crouch), a tap jumps.
        let mut l_jump = false;
        let mut l_crouch = false;
        if key == CrouchKey::L {
            if pad.l_held {
                let was = self.l_ms;
                self.l_ms += dt * 1000.0;
                if was < 250.0 && self.l_ms >= 250.0 && self.setup.stealth {
                    self.crouched = !self.crouched;
                }
                l_crouch = self.l_ms >= 250.0;
            } else {
                l_jump = pad.l_up && self.l_ms < 250.0;
                self.l_ms = 0.0;
            }
        }
        if pad.pause {
            self.hunt.toggle_pause();
        }
        if pad.escape {
            self.hunt.press_escape();
        }
        if self.hunt.exit_prompt {
            if pad.confirm {
                self.hunt.confirm_exit();
            }
            if pad.cancel {
                self.hunt.exit_prompt = false;
            }
        }
        if self.setup.stealth && key == CrouchKey::Zl && pad.crouch_down {
            self.crouched = !self.crouched;
        }
        let crouch = match key {
            CrouchKey::DpadUp => self.gait == 2,
            _ if self.setup.stealth => self.crouched,
            CrouchKey::Zl => pad.crouch,
            CrouchKey::L => l_crouch,
        };
        let jump = match key {
            CrouchKey::L => l_jump,
            _ => pad.jump,
        };
        // Crouched in stealth: aim by turning the console.
        let mut look = pad.look;
        if let (Some(k), true) = (self.gyro, crouch) {
            let mut r = ctru_sys::angularRate { x: 0, z: 0, y: 0 };
            unsafe { ctru_sys::hidGyroRead(&mut r) };
            // Degrees turned this frame, as look units (0.0025 rad each,
            // as the core turns them).
            let units =
                |raw: i16| f32::from(raw) * k * dt * (std::f32::consts::PI / 180.0) / 0.0025;
            look.0 -= units(r.z);
            look.1 -= units(r.y);
        }
        let (sx, sy) = pad.stick;
        let controls = Controls {
            forward: sy > 0.35,
            backward: sy < -0.35,
            strafe_left: sx < -0.35,
            strafe_right: sx > 0.35,
            turn_left: pad.turn_left,
            turn_right: pad.turn_right,
            look_up: pad.look_up,
            look_down: pad.look_down,
            crouch,
            jump,
            run: self.run,
            mouse: look,
            mouse_sens: f32::from(128u8),
        };
        if let Some(ai) = pad.select_call {
            self.hunt.target_call = ai;
        }
        if pad.resupply && self.resupplies > 0 && self.hunt.death.is_none() {
            self.hunt.weapons.resupply();
            self.resupplies -= 1;
            self.events.push(GameEvent::Message("Ammo refilled".into()));
        }
        let select = if let Some(w) = pad.select_weapon {
            Some(w)
        } else if pad.next_weapon {
            let w = &self.hunt.weapons;
            let carried = self.setup.carried();
            let cur = carried.iter().position(|&i| i == w.current);
            let next = cur.map_or(0, |c| (c + 1) % carried.len().max(1));
            carried.get(next).copied()
        } else {
            None
        };
        let mut input = HuntInput {
            controls,
            fire: pad.fire,
            toggle_weapon: pad.toggle_weapon,
            select_weapon: select,
            toggle_binoculars: pad.binoculars,
            call: pad.call && self.setup.callable(self.hunt.target_call),
            ..Default::default()
        };
        // At most `MAX_TICKS` per frame: past that the game slows down
        // instead of spending ever longer catching up (a slow tick would
        // otherwise make the next frame need more of them).
        self.acc_ms = (self.acc_ms + dt * 1000.0).min((MAX_TICKS * TICK_MS) as f32);
        self.ticks_last = 0;
        let cars: Vec<Option<&Car>> = self.cars.iter().map(Option::as_ref).collect();
        while self.acc_ms >= TICK_MS as f32 {
            self.prev_cam = self.hunt.camera().0;
            self.hunt
                .tick(&self.area, &cars, &input, TICK_MS as i32, &mut self.events);
            input.controls.mouse = (0.0, 0.0);
            input.toggle_weapon = false;
            input.select_weapon = None;
            input.toggle_binoculars = false;
            input.call = false;
            self.acc_ms -= TICK_MS as f32;
            self.ticks_last += 1;
        }
    }

    /// Fraction of a tick since the last one (for camera interpolation).
    pub fn alpha(&self) -> f32 {
        self.acc_ms / TICK_MS as f32
    }
}

/// `n` of `from`, picked by `seed` (all of them if there are no more).
pub(crate) fn pick(from: &[usize], n: usize, seed: u32) -> Vec<usize> {
    let mut left = from.to_vec();
    let mut x = seed | 1;
    let mut out = Vec::new();
    while out.len() < n && !left.is_empty() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.push(left.swap_remove(x as usize % left.len()));
    }
    out
}

/// Turns the gyroscope on for a stealth hunt: its raw-to-degrees/s factor.
fn gyro_on(stealth: bool) -> Option<f32> {
    if !stealth || unsafe { ctru_sys::HIDUSER_EnableGyroscope() } < 0 {
        return None;
    }
    let mut k = 0.0f32;
    let ok = unsafe { ctru_sys::HIDUSER_GetGyroscopeRawToDpsCoefficient(&mut k) } >= 0;
    Some(if ok && k > 0.0 { k } else { 1.0 / 14.375 })
}

impl Drop for Game {
    fn drop(&mut self) {
        if self.gyro.is_some() {
            let _ = unsafe { ctru_sys::HIDUSER_DisableGyroscope() };
        }
    }
}

/// The date and time (`Hunt::clock`: year, month, day, hour, minute) from
/// the console's clock (local time).
fn clock() -> (i32, i32, i32, i32, i32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970 to a civil date (H. Hinnant's algorithm).
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
        (rem / 60 % 60) as i32,
    )
}
