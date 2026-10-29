//! The hunt's simulation in the headset: `c2_core::game::Hunt`, the same
//! as the PC's, ticked every 10 ms (`c2-pc hunt_view.rs: TICK_MS`) with the
//! hunter where the head is. For now an observer's hunt (README M17): the
//! animals live, roam and feed but ignore the hunter; no weapons or calls.
//! Every huntable species is placed (there's no hunt setup in VR yet).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use c2_core::area::Area;
use c2_core::dino::{pose, DinoInfo, Herd, PlaceOptions, Species};
use c2_core::game::{GameEvent, Hunt, HuntInput, HuntOptions, Weapons};
use c2_core::model::PreparedModel;
use c2_core::player::Player;
use c2_core::rng::MsRand;
use c2_core::CELL;
use c2_formats::bits::Bits;
use c2_formats::car::{self, Car};
use c2_formats::res_txt::ResTxt;
use glam::Vec3;

use crate::animals::Posed;

const TICK_MS: f32 = 10.0;
/// `RenderCharacterPost` draws characters with a fixed light of 210.
const CHAR_LIGHT: f32 = 210.0 / 255.0;

/// `name` (a file name or a relative path, either slash) in `dir`, each
/// part matched ignoring case (`c2-pc area_view.rs: find_file`).
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    let mut any = false;
    for part in name
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
    {
        cur = crate::panel::find_ci(&cur, part)?;
        any = true;
    }
    any.then_some(cur)
}

/// The game's characters (`_RES.TXT` and their `.CAR`s), read once.
pub struct Roster {
    pub species: Vec<Species>,
    /// Per species; `None` if its `.CAR` didn't load.
    pub cars: Vec<Option<(Car, PreparedModel)>>,
}

impl Roster {
    /// From the first of `huntdat` (overlays first) with a `_RES.TXT`.
    pub fn load(huntdat: &[PathBuf]) -> Result<Self> {
        let (dir, res) = huntdat
            .iter()
            .find_map(|d| Some((d, find_file(d, "_RES.TXT")?)))
            .ok_or_else(|| anyhow!("no _RES.TXT in the game's HUNTDAT"))?;
        let res = ResTxt::parse(&String::from_utf8_lossy(&std::fs::read(&res)?))?.to_data();
        let (mut species, mut cars) = (Vec::new(), Vec::new());
        for def in &res.characters {
            let info = DinoInfo::from_def(def);
            let car = huntdat
                .iter()
                .chain(std::iter::once(dir))
                .find_map(|d| find_file(d, &info.file))
                .and_then(|p| match std::fs::read(&p).map(|d| car::parse(&d)) {
                    Ok(Ok(c)) => Some(c),
                    Ok(Err(e)) => {
                        println!("{}: {e}", p.display());
                        None
                    }
                    Err(e) => {
                        println!("{}: {e}", p.display());
                        None
                    }
                });
            let empty = Car {
                name_raw: [0; 32],
                mesh: Default::default(),
                animations: vec![],
                sounds: vec![],
                anim_sounds: None,
                trailing: Vec::new(),
            };
            species.push(Species::new(info, car.as_ref().unwrap_or(&empty)));
            cars.push(car.map(|c| {
                let p = PreparedModel::from_mesh(&c.mesh);
                (c, p)
            }));
        }
        Ok(Self { species, cars })
    }

    /// The prepared models, a texture layer each (`animals::Animals`).
    pub fn models(&self) -> Vec<Option<&PreparedModel>> {
        self.cars
            .iter()
            .map(|c| c.as_ref().map(|(_, p)| p))
            .collect()
    }
}

/// A hunt going on.
pub struct HuntWorld {
    pub hunt: Hunt,
    acc_ms: f32,
    /// Each character's place before the last tick (drawn between it and
    /// now, as the PC's camera is: ticks and frames don't line up).
    prev: Vec<[f32; 3]>,
    events: Vec<GameEvent>,
}

impl HuntWorld {
    /// A new observer's hunt with the hunter at world (`x`, `z`).
    pub fn new(area: &Area, roster: &Roster, x: f32, z: f32, seed: u32) -> Self {
        let cell = ((x / CELL) as u16, (z / CELL) as u16);
        let mut player = Player::new(area, cell);
        (player.x, player.z) = (x, z);
        let mut targets = Bits::NONE;
        for s in &roster.species {
            if s.info.ai >= 10 {
                targets.set(s.info.ai as usize);
            }
        }
        let mut rng = MsRand::new(seed);
        let mut herd = Herd {
            species: roster.species.clone(),
            chars: Vec::new(),
            demo_index: None,
            ..Default::default()
        };
        let room = c2_core::profile::new_player("", 0, 0);
        herd.place(
            area,
            (x, z),
            &mut rng,
            PlaceOptions {
                density: room.options.dens,
                opt_agres: room.options.agres,
                targets,
                max_ambient: None,
            },
        );
        println!(
            "Placed {} animals (observer: they ignore you).",
            herd.chars.len()
        );
        let opts = HuntOptions {
            targets,
            observer: true,
            ..Default::default()
        };
        let weapons = Weapons::new(Vec::new(), Vec::new(), Bits::NONE, false);
        let hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
        Self {
            prev: hunt.herd.chars.iter().map(|c| c.pos).collect(),
            hunt,
            acc_ms: 0.0,
            events: Vec::new(),
        }
    }

    /// Advances the hunt by `dt` seconds with the hunter's head at `head`
    /// (world) looking along `heading` (0 looks to -z).
    pub fn step(&mut self, area: &Area, roster: &Roster, head: Vec3, heading: f32, dt: f32) {
        let cars: Vec<Option<&Car>> = roster
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        self.acc_ms = (self.acc_ms + dt * 1000.0).min(250.0);
        while self.acc_ms >= TICK_MS {
            let p = &mut self.hunt.player;
            (p.x, p.z) = (head.x, head.z);
            // The PC's camera yaw is -alpha.
            p.alpha = -heading;
            (p.vspeed, p.sspeed) = (0.0, 0.0);
            self.prev.clear();
            self.prev.extend(self.hunt.herd.chars.iter().map(|c| c.pos));
            self.hunt.tick(
                area,
                &cars,
                &HuntInput::default(),
                TICK_MS as i32,
                &mut self.events,
            );
            self.events.clear();
            self.acc_ms -= TICK_MS;
        }
    }

    /// The characters within `radius` (world units) of `eye`, posed.
    pub fn posed(&self, roster: &Roster, eye: Vec3, radius: f32) -> Vec<Posed> {
        let t = self.acc_ms / TICK_MS;
        let herd = &self.hunt.herd;
        let mut out = Vec::new();
        for (i, c) in herd.chars.iter().enumerate() {
            let pos = match self.prev.get(i) {
                Some(&p) if herd.chars.len() == self.prev.len() => {
                    Vec3::from(p).lerp(Vec3::from(c.pos), t)
                }
                _ => Vec3::from(c.pos),
            };
            let d = pos - eye;
            if d.x * d.x + d.z * d.z > radius * radius {
                continue;
            }
            let Some(Some((car, prepared))) = roster.cars.get(c.ctype) else {
                continue;
            };
            let anims = herd.species.get(c.ctype).map_or(&[][..], |s| s.anims());
            out.push(Posed {
                model: c.ctype,
                positions: pose::pose(c, car, anims).unwrap_or_else(|| prepared.positions.clone()),
                offset: pos.to_array(),
                turns: pose::quarter_turns(c),
                light: CHAR_LIGHT,
            });
        }
        out
    }
}
