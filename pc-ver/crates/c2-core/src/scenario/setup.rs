//! Building a scenario's hunt: the hunter, four pinned animals, every
//! weapon carried; and holding that layout between ticks.

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use crate::portable_math::FloatMath;
#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use super::{Scenario, Script, SpeciesRef};
use crate::area::Area;
use crate::dino::{Herd, Species};
use crate::game::{Hunt, HuntOptions, WeaponAnims, WeaponInfo, Weapons, MAX_HEALTH};
use crate::player::Player;
use crate::rng::MsRand;
use c2_formats::bits::Bits;
use c2_formats::sav::Save;

#[derive(Debug, Clone, PartialEq)]
pub enum SetupError {
    NoLanding(usize),
    MissingSpecies(String),
    NoModel(String),
    WrongCount { species: usize, offsets: usize },
    SpawnFailed(String),
    InLineOfFire { species: String, degrees: f32 },
    NoWeapons,
}

impl core::fmt::Display for SetupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SetupError::NoLanding(n) => write!(f, "the area has no landing {n}"),
            SetupError::MissingSpecies(s) => write!(f, "species {s} is not in _RES.TXT"),
            SetupError::NoModel(s) => write!(
                f,
                "species {s} has no model here (missing .CAR, or a blank.car placeholder)"
            ),
            SetupError::WrongCount { species, offsets } => {
                write!(f, "{species} species but {offsets} offsets")
            }
            SetupError::SpawnFailed(s) => write!(f, "species {s} could not be placed"),
            SetupError::InLineOfFire { species, degrees } => write!(
                f,
                "{species} is {degrees:.1} degrees from the line of fire (below the clearance)"
            ),
            SetupError::NoWeapons => write!(f, "the game has no weapons"),
        }
    }
}

impl core::error::Error for SetupError {}

/// The positions [`Hold::apply`] restores before every tick.
#[derive(Debug, Clone)]
pub struct Hold {
    pub player: [f32; 2],
    pub view: [f32; 2],
    /// `(character index, x, z)`.
    pub animals: Vec<(usize, f32, f32)>,
    /// The character the kill script has let go of (not pinned).
    pub released: Option<usize>,
    /// `--inject freeze-body`: the hunter's body is held on frame 0.
    pub freeze_body: bool,
    /// Behaviours: the hunter walks (his view stays pinned), and the
    /// animals slain on purpose, which are neither pinned nor lost.
    pub hunter_free: bool,
    pub dead: Vec<usize>,
}

impl Hold {
    /// Pins the hunter's place and view and the animals' places, and keeps
    /// the hunter alive until something kills him (the kill script).
    /// Returns the animals that are gone or dead.
    pub fn apply(&self, hunt: &mut Hunt, area: &Area) -> Vec<usize> {
        let p = &mut hunt.player;
        if !self.hunter_free {
            [p.x, p.z] = self.player;
        }
        [p.alpha, p.beta] = self.view;
        match hunt.death {
            None => hunt.health = MAX_HEALTH,
            Some(d) if self.freeze_body => {
                if let Some(b) = hunt.herd.chars.get_mut(d.body) {
                    b.ftime = 0;
                }
            }
            Some(_) => {}
        }
        let mut lost = Vec::new();
        for &(i, x, z) in &self.animals {
            if self.dead.contains(&i) {
                continue;
            }
            match hunt.herd.chars.get_mut(i) {
                Some(c) if c.health > 0 => {
                    if self.released != Some(i) {
                        c.pos = [x, area.land_height(x, z), z];
                    }
                }
                _ => lost.push(i),
            }
        }
        lost
    }
}

/// A placed animal, as the director names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cast {
    /// Character index in the hunt.
    pub index: usize,
    pub name: String,
}

pub struct Built {
    pub hunt: Hunt,
    pub hold: Hold,
    /// Species index of each placed animal, in scenario order.
    pub placed: Vec<usize>,
    /// The placed animals, in scenario order.
    pub cast: Vec<Cast>,
}

fn resolve(species: &[Species], r: &SpeciesRef) -> Result<usize, SetupError> {
    let found = match r {
        SpeciesRef::Ai(a) => species.iter().position(|s| s.info.ai == *a),
        SpeciesRef::Index(i) => (*i < species.len()).then_some(*i),
        SpeciesRef::Model(m) => species.iter().position(|s| {
            let f = s.info.file.replace('\\', "/");
            let base = f.rsplit('/').next().unwrap_or(&f);
            base.eq_ignore_ascii_case(m) || f.eq_ignore_ascii_case(m)
        }),
    };
    found.ok_or_else(|| SetupError::MissingSpecies(format!("{r:?}")))
}

/// `(sin, -cos)`: the ground direction a yaw looks along (`ProcessShoot`).
fn forward(yaw: f32) -> [f32; 2] {
    [yaw.sin(), -yaw.cos()]
}

/// Where a scenario puts the hunter (`x, y, z`: on his landing's ground)
/// and each animal (`x, z`), in scenario order.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub player: [f32; 3],
    pub animals: Vec<[f32; 2]>,
}

/// The scenario's [`Layout`] on `area`.
pub fn layout(area: &Area, s: &Scenario) -> Result<Layout, SetupError> {
    let cell = match s.cell {
        Some(c) => c,
        None => *area
            .landings
            .get(s.landing)
            .ok_or(SetupError::NoLanding(s.landing))?,
    };
    let p = Player::new(area, cell);
    let bearing = s.yaw + s.anchor_bearing;
    let [fx, fz] = forward(bearing);
    let [rx, rz] = forward(bearing + core::f32::consts::FRAC_PI_2);
    let anchor = [p.x + fx * s.anchor_distance, p.z + fz * s.anchor_distance];
    let animals = s
        .offsets
        .iter()
        .map(|off| {
            [
                anchor[0] + rx * off[0] + fx * off[1],
                anchor[1] + rz * off[0] + fz * off[1],
            ]
        })
        .collect();
    Ok(Layout {
        player: [p.x, p.y, p.z],
        animals,
    })
}

/// Builds the hunt. `weapons` and `anims` are the game's whole registry, in
/// `_RES.TXT` order; every one is carried with full ammo.
pub fn build(
    area: &Area,
    species: Vec<Species>,
    weapons: Vec<WeaponInfo>,
    anims: Vec<WeaponAnims>,
    s: &Scenario,
    room: Save,
) -> Result<Built, SetupError> {
    if s.species.len() != s.offsets.len() {
        return Err(SetupError::WrongCount {
            species: s.species.len(),
            offsets: s.offsets.len(),
        });
    }
    if weapons.is_empty() {
        return Err(SetupError::NoWeapons);
    }
    let spots = layout(area, s)?;
    let cell = match s.cell {
        Some(c) => c,
        None => area.landings[s.landing],
    };
    let mut player = Player::new(area, cell);
    player.alpha = s.yaw;
    player.beta = s.pitch;
    let mut rng = MsRand::new(s.seed);
    let mut herd = Herd {
        species,
        ..Default::default()
    };

    let fire = forward(s.yaw);
    let mut placed = Vec::new();
    let mut cast = Vec::new();
    let mut animals = Vec::new();
    let mut targets = Bits::NONE;
    for (r, &[x, z]) in s.species.iter().zip(&spots.animals) {
        let ctype = resolve(&herd.species, r)?;
        let sp = &herd.species[ctype];
        let name = format!("{} ({r:?})", sp.info.name);
        // Dialect games give an animal `blank.car` on maps it is not on:
        // placed, it would pass every check unseen.
        let placeholder = sp.info.file.to_ascii_lowercase().ends_with("blank.car");
        if sp.ani_time.is_empty() || placeholder {
            return Err(SetupError::NoModel(name));
        }
        // The animal's edge, as seen from the hunter, must stay clear of
        // the line of fire (shots go straight along `yaw`).
        let (dx, dz) = (x - player.x, z - player.z);
        let dist = (dx * dx + dz * dz).sqrt().max(1.0);
        let cross = dx * fire[1] - dz * fire[0];
        let angle = cross.abs().atan2(dx * fire[0] + dz * fire[1]);
        let half = (sp.info.radius * 2.0 / dist).atan();
        let edge = angle - half;
        // Nothing is fired in the kill script.
        if edge < s.fire_clearance && s.script == Script::Tour {
            return Err(SetupError::InLineOfFire {
                species: name,
                degrees: edge.to_degrees(),
            });
        }
        if sp.info.ai >= 10 {
            targets.set(sp.info.ai as usize);
        }
        let index = herd.chars.len();
        if !herd.spawn(ctype, [x, area.land_height(x, z), z], &mut rng, 128) {
            return Err(SetupError::SpawnFailed(name));
        }
        placed.push(ctype);
        cast.push(Cast {
            index,
            name: herd.species[ctype].info.name.clone(),
        });
        animals.push((index, x, z));
    }
    let n = weapons.len();
    let carried = Bits::first(n.min(Bits::LEN));
    let weapons = Weapons::new(weapons, anims, carried, false);
    let opts = HuntOptions {
        targets,
        ..Default::default()
    };
    let hold = Hold {
        player: [player.x, player.z],
        view: [s.yaw, s.pitch],
        animals,
        released: None,
        freeze_body: false,
        hunter_free: false,
        dead: Vec::new(),
    };
    let hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
    Ok(Built {
        hunt,
        hold,
        placed,
        cast,
    })
}

/// FNV-1a over the state a deterministic run must reproduce: the tick, the
/// hunter, the weapon and every animal's pose, and the random stream.
pub fn state_hash(h: &Hunt) -> u64 {
    let mut x: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u32| {
        for b in v.to_le_bytes() {
            x ^= u64::from(b);
            x = x.wrapping_mul(0x0100_0000_01b3);
        }
    };
    eat(h.takt);
    let p = &h.player;
    for v in [p.x, p.y, p.z, p.alpha, p.beta] {
        eat(v.to_bits());
    }
    let w = &h.weapons;
    eat(w.current as u32);
    eat(w.state as u32);
    eat(w.ftime as u32);
    for &s in &w.shots_left {
        eat(s as u32);
    }
    for c in &h.herd.chars {
        eat(c.ctype as u32);
        for v in [c.pos[0], c.pos[1], c.pos[2], c.alpha] {
            eat(v.to_bits());
        }
        eat(c.phase as u32);
        eat(c.ftime as u32);
        eat(c.state as u32);
        eat(c.health as u32);
    }
    eat(h.rng.clone().rand() as u32);
    x
}
