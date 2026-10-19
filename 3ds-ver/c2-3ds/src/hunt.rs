//! Starting a hunt from a cooked pack and turning 3DS input into the
//! simulation's controls.

use std::collections::HashMap;

use anyhow::Result;
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{GameEvent, Hunt, HuntInput, HuntOptions, Weapons};
use c2_core::load;
use c2_core::player::{Controls, Player};
use c2_core::rng::MsRand;
use c2_formats::car::Car;
use c2_formats::sav::Save;
use c2_vfs::Vfs;

use crate::world::DrawInfo;

/// Simulation step, as on PC.
pub const TICK_MS: u32 = 10;

/// The INFO.TXT `pack` this build reads: 3 = every area, animal and weapon
/// (a "library"; the menu picks), per-area `C2RS/AREAn/`, `cost.*` lines.
pub const PACK_VERSION: u32 = 3;

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
    /// `hunt` (always), `AREAn`, `char{i}`, `wpn{i}`.
    pub linear_kb: usize,
    pub costs: HashMap<String, usize>,
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
            costs: m
                .iter()
                .filter_map(|(k, v)| Some((k.strip_prefix("cost.")?.to_string(), v.parse().ok()?)))
                .collect(),
        }
    }

    pub fn cost(&self, piece: &str) -> usize {
        self.costs.get(piece).copied().unwrap_or(0)
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
    /// Bit `ai` per target species (`TargetDino`).
    pub targets: u32,
    /// Bit per carried weapon.
    pub weapons: u32,
    pub double_ammo: bool,
    pub observer: bool,
}

impl Setup {
    pub fn trophy(&self) -> bool {
        self.area.eq_ignore_ascii_case("TROPHY")
    }

    /// Characters to load: the hunter and ambient animals always, the
    /// targets; in the trophy room, every species (the trophies).
    pub fn keeps_char(&self, ai: i32) -> bool {
        self.trophy() || ai < 10 || (0..32).contains(&ai) && self.targets & (1 << ai) != 0
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
    pub weapon_cars: Vec<Option<Car>>,
    pub events: Vec<GameEvent>,
    acc_ms: f32,
    /// `RunMode`.
    pub run: bool,
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

/// `ReInitGame`: the hunter at a random landing spot (or the trophy room's
/// door) and the herd placed (or the trophies), as `c2-pc`'s `new_hunt`.
fn new_hunt(
    area: &Area,
    setup: &Setup,
    species: &[c2_core::dino::Species],
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
    };
    if trophy {
        herd.place_trophies(area, &room, &mut rng);
    } else {
        herd.place(
            area,
            (player.x, player.z),
            &mut rng,
            PlaceOptions {
                density: o.dens,
                opt_agres: o.agres,
                targets: setup.targets,
            },
        );
    }
    let carried = if trophy || setup.observer {
        0
    } else {
        setup.weapons
    };
    let (infos, anims): (Vec<_>, Vec<_>) = weapon_info.iter().cloned().unzip();
    let weapons = Weapons::new(infos, anims, carried, setup.double_ammo);
    let opts = HuntOptions {
        targets: if setup.observer { 0 } else { setup.targets },
        radar: o.radar != 0,
        opt_agres: o.agres,
        opt_sens: o.sens,
        // ctViewR at the lowest view-range option (42 cells); 60 in the
        // trophy room.
        view_r: if trophy { 60 } else { 42 },
        camo: o.camo != 0,
        scent: o.scent != 0,
        tranq: o.tranq != 0,
        observer: setup.observer,
        trophy,
    };
    let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
    hunt.ship_anim_ms = ship_ms;
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
        progress(&format!("area {}", setup.area));
        let area = load::area_with(vfs, &setup.area, setup.time, true)?;
        progress("rules (_RES.TXT)");
        let res = load::res(vfs)?;
        progress("animals");
        let ais: Vec<i32> = res
            .characters
            .iter()
            .map(|c| c2_core::dino::DinoInfo::from_def(c).ai)
            .collect();
        let (species, cars) = load::species_with(vfs, &res, &|i| {
            ais.get(i).is_some_and(|&ai| setup.keeps_char(ai))
        });
        progress("weapons");
        let carried = setup.carried();
        let weapons = load::weapons_with(vfs, &res, &|i| carried.contains(&i));
        let mut weapon_cars = Vec::new();
        let mut weapon_info = Vec::new();
        for w in weapons {
            weapon_info.push((w.info, w.anims));
            weapon_cars.push(w.car);
        }
        let ship_ms = load::ship_anim_ms(vfs);
        let seed = next_seed(seed);
        let hunt = new_hunt(&area, &setup, &species, &weapon_info, ship_ms, seed, room);
        Ok(Self {
            prev_cam: hunt.camera().0,
            draw: info.draw_info(vfs, &setup.area),
            info,
            setup,
            area,
            hunt,
            cars,
            weapon_cars,
            events: Vec::new(),
            acc_ms: 0.0,
            run: false,
        })
    }

    /// Frees load-time data; call once the GPU copies exist.
    pub fn trim(&mut self) {
        trim(&mut self.area, &mut self.cars);
        for c in self.weapon_cars.iter_mut().flatten() {
            c.mesh.texture = Vec::new();
        }
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
