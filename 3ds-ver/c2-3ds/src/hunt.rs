//! Starting a hunt from a cooked pack and turning 3DS input into the
//! simulation's controls.

use anyhow::{bail, Context, Result};
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{GameEvent, Hunt, HuntInput, HuntOptions, Weapons};
use c2_core::load;
use c2_core::player::{Controls, Player};
use c2_core::rng::MsRand;
use c2_formats::car::Car;
use c2_vfs::Vfs;

use crate::world::DrawInfo;

/// Simulation step, as on PC.
pub const TICK_MS: u32 = 10;

/// The INFO.TXT `pack` this build reads: 2 = RSC and CARs without textures
/// or sounds, `terrain_count` given.
const PACK_VERSION: u32 = 2;

/// `C2RS/INFO.TXT`: what the cooker put in the pack.
#[derive(Debug, Clone)]
pub struct PackInfo {
    /// Pack layout version (`pack` in INFO.TXT).
    pub pack: u32,
    pub target: String,
    pub area: String,
    pub time: TimeOfDay,
    /// `_RES.TXT` character indices of the huntable animals.
    pub dinos: Vec<usize>,
    /// `_RES.TXT` weapon indices.
    pub weapons: Vec<usize>,
    pub draw: DrawInfo,
    /// Sample rate of the pack's sounds.
    pub sound_rate: u32,
}

impl PackInfo {
    pub fn parse(text: &str) -> Self {
        let get = |k: &str| {
            text.lines()
                .filter_map(|l| l.split_once('='))
                .find(|(a, _)| a.trim() == k)
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_default()
        };
        let list = |k: &str| -> Vec<usize> {
            get(k)
                .split(',')
                .filter_map(|v| v.trim().parse().ok())
                .collect()
        };
        let num = |k: &str, d: u32| get(k).parse().unwrap_or(d);
        Self {
            pack: num("pack", 1),
            target: get("target"),
            area: match get("area") {
                a if a.is_empty() => "AREA1".into(),
                a => a,
            },
            time: match get("time").as_str() {
                "dusk" => TimeOfDay::Dusk,
                "night" => TimeOfDay::Night,
                _ => TimeOfDay::Day,
            },
            dinos: list("dinos"),
            sound_rate: num("sound_rate", c2_core::sound::SAMPLE_RATE),
            weapons: list("weapons"),
            draw: DrawInfo {
                tile: num("terrain_tile", 64) as usize,
                cols: num("atlas_cols", 16) as usize,
                view: num("view", 42),
                object_view: num("object_view", 28),
                terrain_count: num("terrain_count", 1) as usize,
            },
        }
    }
}

pub struct Game {
    pub info: PackInfo,
    pub area: Area,
    pub hunt: Hunt,
    pub cars: Vec<Option<Car>>,
    pub weapon_cars: Vec<Option<Car>>,
    pub events: Vec<GameEvent>,
    acc_ms: f32,
    /// `RunMode`.
    pub run: bool,
    seed: u32,
    pub species: Vec<c2_core::dino::Species>,
    weapon_info: Vec<(c2_core::game::WeaponInfo, [i32; 4])>,
    ship_ms: i32,
    pub prev_cam: [f32; 3],
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
    // Pressed this frame:
    pub toggle_weapon: bool,
    pub next_weapon: bool,
    pub binoculars: bool,
    pub call: bool,
    pub pause: bool,
    pub escape: bool,
    pub confirm: bool,
    pub cancel: bool,
    /// From the bottom screen: a weapon (index), a call (AI).
    pub select_weapon: Option<usize>,
    pub select_call: Option<i32>,
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

/// The hunter at a landing spot, the herd placed, the pack's weapons carried.
fn new_hunt(
    area: &Area,
    info: &PackInfo,
    species: &[c2_core::dino::Species],
    weapon_info: &[(c2_core::game::WeaponInfo, [i32; 4])],
    ship_ms: i32,
    seed: u32,
) -> Hunt {
    let mut rng = MsRand::new(seed);
    let cell = area
        .landings
        .get(seed as usize % area.landings.len().max(1))
        .copied()
        .unwrap_or((512, 512));
    let player = Player::new(area, cell);
    let room = c2_core::profile::new_player("Hunter", 0, 0);
    let o = room.options;
    let targets = info
        .dinos
        .iter()
        .filter_map(|&i| species.get(i))
        .filter(|s| (0..32).contains(&s.info.ai))
        .fold(0u32, |m, s| m | 1 << s.info.ai);
    let mut herd = Herd {
        species: species.to_vec(),
        chars: Vec::new(),
        demo_index: None,
    };
    herd.place(
        area,
        (player.x, player.z),
        &mut rng,
        PlaceOptions {
            density: o.dens,
            opt_agres: o.agres,
            targets,
        },
    );
    let carried = info
        .weapons
        .iter()
        .filter(|&&i| i < 32)
        .fold(0u32, |m, &i| m | 1 << i);
    let (infos, anims): (Vec<_>, Vec<_>) = weapon_info.iter().cloned().unzip();
    let weapons = Weapons::new(infos, anims, carried, false);
    let opts = HuntOptions {
        targets,
        radar: false,
        opt_agres: o.agres,
        opt_sens: o.sens,
        // ctViewR at the lowest view-range option (42 cells).
        view_r: 42,
        camo: false,
        scent: false,
        tranq: false,
        observer: false,
        trophy: false,
    };
    let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
    hunt.ship_anim_ms = ship_ms;
    hunt
}

impl Game {
    pub fn load(vfs: &Vfs, seed: u32, progress: &mut dyn FnMut(&str)) -> Result<Self> {
        let info = PackInfo::parse(&String::from_utf8_lossy(
            &vfs.read("C2RS/INFO.TXT")
                .context("not a c2rs pack (no C2RS/INFO.TXT)")?,
        ));
        if info.pack < PACK_VERSION {
            bail!("this pack was made by an older c2-cook; cook it again");
        }
        // The area first, while the heap is empty: its map is the biggest
        // file, and is briefly in memory twice (file and parsed).
        // `c2-cook`'s `load_peak` repeats these steps; keep them in step.
        progress(&format!("area {}", info.area));
        let area = load::area_with(vfs, &info.area, info.time, true)?;
        progress("rules (_RES.TXT)");
        let res = load::res(vfs)?;
        progress("animals");
        let (species, cars) = load::species(vfs, &res);
        progress("weapons");
        let weapons = load::weapons(vfs, &res);
        let mut weapon_cars = Vec::new();
        let mut weapon_info = Vec::new();
        for w in weapons {
            weapon_info.push((w.info, w.anims));
            weapon_cars.push(w.car);
        }
        let ship_ms = load::ship_anim_ms(vfs);
        let seed = next_seed(seed);
        let hunt = new_hunt(&area, &info, &species, &weapon_info, ship_ms, seed);
        Ok(Self {
            prev_cam: hunt.camera().0,
            info,
            area,
            hunt,
            cars,
            weapon_cars,
            events: Vec::new(),
            acc_ms: 0.0,
            run: false,
            seed,
            species,
            weapon_info,
            ship_ms,
        })
    }

    /// Frees load-time data; call once the GPU copies exist.
    pub fn trim(&mut self) {
        trim(&mut self.area, &mut self.cars);
        for c in self.weapon_cars.iter_mut().flatten() {
            c.mesh.texture = Vec::new();
        }
    }

    /// `ReInitGame`: a fresh hunt (after dying).
    pub fn restart(&mut self) {
        self.seed = next_seed(self.seed);
        self.hunt = new_hunt(
            &self.area,
            &self.info,
            &self.species,
            &self.weapon_info,
            self.ship_ms,
            self.seed,
        );
        self.prev_cam = self.hunt.camera().0;
        self.acc_ms = 0.0;
        self.events.clear();
    }

    /// Runs the simulation for `dt` seconds of real time.
    pub fn update(&mut self, pad: &Pad, dt: f32) {
        if pad.run {
            self.run = !self.run;
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
            crouch: pad.crouch,
            jump: pad.jump,
            run: self.run,
            mouse: pad.look,
            mouse_sens: f32::from(128u8),
        };
        if let Some(ai) = pad.select_call {
            self.hunt.target_call = ai;
        }
        let select = if let Some(w) = pad.select_weapon {
            Some(w)
        } else if pad.next_weapon {
            let w = &self.hunt.weapons;
            let carried: Vec<usize> = self.info.weapons.clone();
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
            call: pad.call,
            ..Default::default()
        };
        self.acc_ms = (self.acc_ms + dt * 1000.0).min(250.0);
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
        }
    }

    /// Fraction of a tick since the last one (for camera interpolation).
    pub fn alpha(&self) -> f32 {
        self.acc_ms / TICK_MS as f32
    }
}
