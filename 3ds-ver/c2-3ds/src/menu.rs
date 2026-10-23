//! The menus, touch-first (the owner's design, after the mobile game).
//!
//! The title: Play, Trophies (the trophy room), Quit, over the original's
//! main menu art. Play walks through three steps on the bottom screen while
//! the top screen sums up the picks:
//!
//! 1. Area: a hexagon grid of the areas (and the trophy room), and the time
//!    of day as sun / sunset / moon.
//! 2. Animals: a hexagon grid of the huntable species (several can be
//!    picked).
//! 3. Weapons and gear: a hexagon grid of both (plus extra ammo, 3DS only),
//!    tranquilizer and observer toggles, then Hunt.
//!
//! Prices, points and what is remembered follow `c2-pc`'s `menu.rs`
//! (`Menu.cpp`): picks can't cost more than the hunter's score; camouflage,
//! radar, cover scent and the tranquilizer are kept in the save.

use c2_core::area::TimeOfDay;
use c2_core::dino::DinoInfo;
use c2_core::game::SurvivalKind;
use c2_formats::sav::Save;
use c2_vfs::Vfs;

use crate::draw2d::{in_hex, inside, Canvas, Rgba, WHITE};
use crate::gpu::{Gpu, Texture, BOTTOM_W, TOP_W};
use crate::hunt::{PackInfo, Setup};
use crate::rules::{Roam, Role, Rules};
use crate::settings::Settings;
use crate::ui::load;

/// Accessories (`EQUIPn.TGA`), in `prices.acces` order. The original's
/// fourth, Double Ammo, isn't offered on the 3DS: the extra ammo gear takes
/// its place and its picture.
const EQUIPMENT: [&str; 3] = ["Camouflage", "Radar", "Cover Scent"];

/// At most this many animals and weapons per hunt: each one's model,
/// textures and sounds are loaded, and an Old 3DS has 64 MiB.
pub const MAX_PICKS: u32 = 3;
/// Time-of-day choices as the original lists them (`dtm=`).
const TIMES: [(TimeOfDay, &str); 3] = [
    (TimeOfDay::Dusk, "Dawn"),
    (TimeOfDay::Day, "Day"),
    (TimeOfDay::Night, "Night"),
];

/// Extra ammo gear (3DS only, the owner's idea): (price, refills, factor
/// on kill scores) for x1, x2, x3.
const EXTRA_AMMO: [(i32, u32, f32); 3] = [(10, 1, 0.95), (50, 2, 0.95), (150, 3, 1.0)];

/// Title screen buttons.
const PLAY: [f32; 4] = [60.0, 6.0, 200.0, 30.0];
const SURVIVAL: [f32; 4] = [60.0, 42.0, 200.0, 30.0];
/// With both survivals: ours on the left, classic on the right.
const SURVIVAL_L: [f32; 4] = [60.0, 42.0, 98.0, 30.0];
const CLASSIC_R: [f32; 4] = [162.0, 42.0, 98.0, 30.0];
const HUNTER: [f32; 4] = [60.0, 78.0, 200.0, 30.0];
const TROPHIES: [f32; 4] = [60.0, 114.0, 200.0, 30.0];
const SETTINGS: [f32; 4] = [60.0, 150.0, 200.0, 30.0];
const QUIT: [f32; 4] = [60.0, 186.0, 200.0, 30.0];
/// Settings: remove every unlocked trophy (a second tap does it).
const CLEAR_TROPHIES: [f32; 4] = [60.0, 80.0, 200.0, 26.0];
/// The hunters list: row `i`.
fn hunter_row(i: usize) -> [f32; 4] {
    [40.0, 8.0 + i as f32 * 30.0, 240.0, 26.0]
}

/// Settings screen: run speed bar with - / +, then the toggles.
// The hunt options screen.
/// Settings: the crouch button (cycles).
const CROUCH_KEY: [f32; 4] = [60.0, 112.0, 200.0, 28.0];
const O_SPEED_BAR: [f32; 4] = [64.0, 24.0, 192.0, 20.0];
const O_SPEED_DOWN: [f32; 4] = [20.0, 20.0, 36.0, 28.0];
const O_SPEED_UP: [f32; 4] = [264.0, 20.0, 36.0, 28.0];
const O_AMBIENT: [f32; 4] = [40.0, 56.0, 240.0, 24.0];
const O_RANDOM: [f32; 4] = [40.0, 84.0, 240.0, 24.0];
const O_FRIENDLY: [f32; 4] = [40.0, 112.0, 240.0, 24.0];
const O_STEALTH: [f32; 4] = [40.0, 140.0, 240.0, 24.0];
const BLOOD: [f32; 4] = [60.0, 144.0, 200.0, 28.0];
const DEATH_ANIM: [f32; 4] = [60.0, 176.0, 200.0, 28.0];

const BAR_Y: f32 = 212.0;
const BACK: [f32; 4] = [4.0, BAR_Y, 76.0, 24.0];
const NEXT: [f32; 4] = [240.0, BAR_Y, 76.0, 24.0];

const BG: Rgba = [16, 22, 16, 255];
const PANEL: Rgba = [32, 44, 30, 255];
const GOLD: Rgba = [240, 200, 70, 255];
const TEXT: Rgba = [160, 210, 130, 255];
const DIM: Rgba = [110, 110, 110, 255];
const RED: Rgba = [240, 80, 60, 255];
const DIMMED: Rgba = [140, 140, 140, 255];
/// Roaming add-ons (no call), as the grid rim and the hunt map show them.
const BLUE: Rgba = [120, 180, 255, 255];
/// Space between hexagons.
const GAP: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Title,
    Settings,
    Area,
    Animals,
    Gear,
    /// Hunt options, before each hunt.
    Options,
    /// Pick a hunter, or make one.
    Hunters,
}

struct Item {
    name: String,
    price: i32,
    pic: Option<Texture>,
}

/// An animal in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    /// A menu slot: a hunt target (call id 10 + slot).
    Slot(usize),
    /// An add-on that roams (no menu slot): hunted under its own call
    /// slot, which can't be called (+10% score), with a picture `c2-cook`
    /// renders from its model.
    Roamer(Roam),
}

/// What memory the picks need and what the console has, KiB: linear, then
/// main memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Memory {
    linear: (usize, usize),
    heap: (usize, usize),
}

/// Area, time, animals, weapons, ambient on.
type MemKey = (usize, usize, u64, u64, bool);

/// One cell of a hexagon grid: centre and radius.
type Cell = (f32, f32, f32);

/// `n` flat-topped hexagons in rows of `cols`, centred across the bottom
/// screen, the first row's centres at `top + h/2`.
fn grid(n: usize, cols: usize, r: f32, top: f32) -> Vec<Cell> {
    // Laid out for r + GAP/2, drawn at r.
    let rr = r + GAP / 2.0;
    let h = rr * 3f32.sqrt();
    let cols = cols.min(n).max(1);
    let width = (cols - 1) as f32 * 1.5 * rr + 2.0 * rr;
    let x0 = (BOTTOM_W as f32 - width) / 2.0 + rr;
    (0..n)
        .map(|i| {
            let (c, row) = (i % cols, i / cols);
            let y = top + h / 2.0 + row as f32 * h + if c % 2 == 1 { h / 2.0 } else { 0.0 };
            (x0 + c as f32 * 1.5 * rr, y, r)
        })
        .collect()
}

pub struct Menu {
    c: Canvas,
    /// The title screen's art.
    title: Option<Texture>,
    trophy_room: bool,
    /// Quit was picked.
    pub quit: bool,
    /// Extra ammo tier: 0 none, 1..=3 (`EXTRA_AMMO`).
    extra_ammo: usize,
    /// Its picture: the original's Double Ammo (`EQUIP4`).
    ammo_pic: Option<Texture>,
    info: PackInfo,
    pub save: Save,
    step: Step,
    /// Areas (`AREA1`..), then the trophy room if packed.
    areas: Vec<(String, Item)>,
    /// The animals offered (at most 64).
    dinos: Vec<(Pick, Item)>,
    /// The areas (index in `areas`) each roamer spawns in, by character.
    roams: std::collections::HashMap<usize, Vec<usize>>,
    rules: Rules,
    /// The last memory figure and the picks it was for.
    mem: std::cell::Cell<Option<(MemKey, Memory)>>,
    settings: Settings,
    weapons: Vec<Item>,
    equip: Vec<Item>,
    area: usize,
    time: usize,
    /// Bits by position in `dinos`, `weapons`, `equip`.
    picked_dinos: u64,
    picked_weapons: u64,
    picked_equip: u64,
    observer: bool,
    /// The last item tapped, for its name and price under the grid.
    focus: Option<String>,
    message: Option<String>,
    /// The menu loop shows the system keyboard for a new hunter's name.
    pub want_name: bool,
    /// Settings: "Remove unlocked trophies" tapped once.
    clear_armed: bool,
    /// The hunter's record (the title shows it).
    record: c2_core::record::Record,
    /// A survival (README M13) instead of a hunt: free, the animals come.
    survival: Option<SurvivalKind>,
    /// Per animal in `dinos`: dangerous, a mod's, the T-Rex.
    kinds: Vec<(bool, bool, bool)>,
    /// Animals (bits by position in `dinos`) the survival brings besides
    /// the picks: hardcore's 1-3 mods' animals, custom's T-Rex, the mobile
    /// one's animals.
    extra_dinos: u64,
    /// The animals among `extra_dinos` that are hardcore's or custom's (not
    /// in the waves).
    special_dinos: u64,
    /// The mobile game's Survive mode (`C2RS/SURVIVE.TXT`; classic
    /// survival), and whether ours is offered too (the launcher's choice).
    mobile_survival: Option<c2_formats::mobile_arcade::Survive>,
    ours: bool,
}

pub(crate) fn title(vfs: &Vfs, file: &str, fallback: String) -> String {
    vfs.read(&format!("HUNTDAT/MENU/TXT/{file}"))
        .ok()
        .and_then(|d| {
            String::from_utf8_lossy(&d)
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_string)
        })
        .unwrap_or(fallback)
}

impl Menu {
    /// The menu, with the hunter's save (a new hunter gets `prices.start`).
    pub fn new(vfs: &Vfs, info: PackInfo) -> anyhow::Result<Self> {
        let rules = Rules::load(vfs)?;
        let menu = &rules.menu;
        let settings = Settings::load();
        crate::save::use_hunter(settings.hunter);
        let save = crate::save::load(menu.start);
        let price = |v: &Vec<i32>, i: usize| v.get(i).copied().unwrap_or(0);
        let mut areas = Vec::new();
        for (i, name) in info.areas.iter().enumerate() {
            // The trophy room has its own button on the title screen.
            if name.eq_ignore_ascii_case("TROPHY") {
                continue;
            }
            let n = i + 1;
            let item = Item {
                name: title(vfs, &format!("AREA{n}.TXT"), format!("Area {n}")),
                price: menu.areas.get(i).map_or(0, |a| a.price),
                pic: load(vfs, &format!("C2RS/MENU/AREA{n}.TX")),
            };
            areas.push((name.clone(), item));
        }
        // The menu's animals by slot (`DINOn.TGA`), then the ones that only
        // roam (add-ons without a slot), by name.
        let mut dinos: Vec<(Pick, Item)> = menu
            .huntables
            .iter()
            .enumerate()
            .map(|(k, d)| {
                let item = Item {
                    name: d.name.clone(),
                    price: d.price,
                    pic: load(vfs, &format!("C2RS/MENU/DINO{}.TX", k + 1)),
                };
                (Pick::Slot(k), item)
            })
            .collect();
        let first = info.areas.first().cloned().unwrap_or_default();
        let neutral = rules.hunt(&first, TimeOfDay::Day, u128::MAX);
        for r in Rules::roams(&neutral) {
            let name = neutral
                .res
                .characters
                .get(r.char)
                .map(|d| DinoInfo::from_def(d).name)
                .unwrap_or_default();
            let key = &neutral.keys[r.char];
            let cooked = info.costs.contains_key(&format!("m:{key}"));
            if cooked && !dinos.iter().any(|d| d.1.name == name) {
                // Priced as the menu animal with its AI.
                let price = r
                    .donor
                    .and_then(|k| menu.huntables.get(k))
                    .map_or(0, |d| d.price);
                let item = Item {
                    name,
                    price,
                    pic: load(vfs, &format!("C2RS/MENU/R/{key}.TX")),
                };
                dinos.push((Pick::Roamer(r), item));
            }
        }
        dinos.truncate(64);
        // Cheapest first, left to right and top to bottom (equal prices
        // keep their order: the game's, then add-ons).
        dinos.sort_by_key(|d| d.1.price);
        // The mobile Survive mode: only on its area (and if offered).
        let mobile_survival = vfs
            .read("C2RS/SURVIVE.TXT")
            .ok()
            .map(|d| c2_formats::mobile_arcade::Survive::parse(&String::from_utf8_lossy(&d)))
            .filter(|s| {
                info.areas
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(&format!("AREA{}", s.area)))
            })
            .filter(|_| info.survival != "ours");
        let ours = mobile_survival.is_none() || info.survival != "mobile";
        let dialect = rules.dialect();
        let kinds = dinos
            .iter()
            .map(|d| {
                let char = match d.0 {
                    Pick::Roamer(r) => Some(r.char),
                    Pick::Slot(k) => neutral.roles.iter().position(|&r| r == Role::Slot(k)),
                };
                let info = char
                    .and_then(|j| neutral.res.characters.get(j))
                    .map(DinoInfo::from_def);
                let aggress = char
                    .and_then(|j| neutral.tables.as_ref()?.characters.get(j))
                    .and_then(|c| c.block.get(&["aggress"]))
                    .map(c2_formats::res_txt::c_atoi);
                info.map_or((false, false, false), |i| {
                    (
                        c2_core::game::dangerous_ai(i.ai, dialect.then(|| aggress.unwrap_or(0))),
                        c2_core::game::modded(&d.1.name),
                        c2_core::game::is_trex(&d.1.name, i.ai, dialect),
                    )
                })
            })
            .collect();
        // Where each roamer spawns (its tables, per area).
        let mut roams = std::collections::HashMap::new();
        if dinos.iter().any(|d| matches!(d.0, Pick::Roamer(_))) {
            for (a, (name, _)) in areas.iter().enumerate() {
                let hr = rules.hunt(name, TimeOfDay::Day, u128::MAX);
                let Some(t) = &hr.tables else { continue };
                for d in &dinos {
                    if let Pick::Roamer(r) = d.0 {
                        let i = r.char;
                        let c = t.characters.get(i);
                        let spawns = c.is_some_and(|c| !c.spawn.is_empty() || !c.packs.is_empty());
                        if spawns {
                            roams.entry(i).or_insert_with(Vec::new).push(a);
                        }
                    }
                }
            }
        }
        let weapons = menu
            .weapons
            .iter()
            .enumerate()
            .map(|(i, w)| Item {
                name: w.name.clone(),
                price: w.price,
                pic: load(vfs, &format!("C2RS/MENU/WEAPON{}.TX", i + 1)),
            })
            .collect();
        let prices = menu.accessory_prices.clone();
        let equip = EQUIPMENT
            .iter()
            .enumerate()
            .map(|(i, n)| Item {
                name: n.to_string(),
                price: price(&prices, i),
                pic: load(vfs, &format!("C2RS/MENU/EQUIP{}.TX", i + 1)),
            })
            .collect();
        // Accessories are kept in the save (`CamoMode`, `RadarMode`,
        // `ScentMode`).
        let o = save.options;
        let picked_equip = [o.camo, o.radar, o.scent]
            .iter()
            .enumerate()
            .filter(|(_, &on)| on != 0)
            .fold(0, |m, (i, _)| m | 1 << i);
        let trophy_room = info.areas.iter().any(|a| a.eq_ignore_ascii_case("TROPHY"));
        let mut m = Self {
            c: Canvas::new(load(vfs, "C2RS/FONT.TX")),
            title: load(vfs, "C2RS/MENU/TITLE.TX"),
            trophy_room,
            quit: false,
            extra_ammo: 0,
            ammo_pic: load(vfs, "C2RS/MENU/EQUIP4.TX"),
            info,
            save,
            step: Step::Title,
            areas,
            dinos,
            roams,
            rules,
            mem: std::cell::Cell::new(None),
            settings,
            want_name: false,
            clear_armed: false,
            record: crate::save::load_record(),
            survival: None,
            kinds,
            extra_dinos: 0,
            special_dinos: 0,
            ours,
            mobile_survival,
            weapons,
            equip,
            area: 0,
            time: 1,
            picked_dinos: 0,
            picked_weapons: 0,
            picked_equip,
            observer: false,
            focus: None,
            message: None,
        };
        if m.remaining() < 0 {
            m.picked_equip = 0;
        }
        Ok(m)
    }

    fn trophy(&self) -> bool {
        self.areas
            .get(self.area)
            .is_some_and(|a| a.0.eq_ignore_ascii_case("TROPHY"))
    }

    fn sum(items: &[Item], mask: u64) -> i32 {
        items
            .iter()
            .enumerate()
            .filter(|(i, _)| *i < 32 && mask & (1 << i) != 0)
            .map(|(_, it)| it.price)
            .sum()
    }

    /// Points left after paying for the picks (none needed to observe).
    fn remaining(&self) -> i32 {
        if self.observer || self.trophy() || self.survival.is_some() {
            return self.save.score;
        }
        let area = self.areas.get(self.area).map_or(0, |a| a.1.price);
        let dinos: i32 = self
            .dinos
            .iter()
            .enumerate()
            .filter(|(i, _)| self.picked_dinos & (1 << i) != 0)
            .map(|(_, d)| d.1.price)
            .sum();
        self.save.score
            - area
            - dinos
            - Self::sum(&self.weapons, self.picked_weapons)
            - Self::sum(&self.equip, self.picked_equip)
            - self.extra().map_or(0, |e| e.0)
    }

    /// The hunt targets (bit 10 + slot, roamers' slots too) and roaming
    /// add-ons picked.
    fn picks(&self) -> (u128, Vec<Roam>) {
        let mut targets = 0u128;
        let mut roamers = Vec::new();
        let mask = self.dino_mask();
        for (k, d) in self.dinos.iter().enumerate() {
            if mask & (1 << k) == 0 {
                continue;
            }
            match d.0 {
                Pick::Slot(s) if s < 118 => targets |= 1 << (s + 10),
                Pick::Slot(_) => {}
                Pick::Roamer(r) => {
                    targets |= 1 << (r.slot + 10);
                    roamers.push(r);
                }
            }
        }
        (targets, roamers)
    }

    /// Memory the picks need, against what the console has: the area with
    /// the hunter and ambient animals, then each loaded model and weapon
    /// (`c2-cook`'s figures; which model an animal uses can depend on the
    /// area in Modder's Edition rules).
    fn memory(&self) -> Memory {
        let key = (
            self.area,
            self.time,
            self.dino_mask(),
            self.picked_weapons,
            self.settings.ambient,
        );
        if let Some((k, m)) = self.mem.get() {
            if k == key {
                return m;
            }
        }
        let i = &self.info;
        let setup = self.setup();
        let area = self.areas.get(self.area).map_or("AREA1", |a| a.0.as_str());
        let mut lin = i.cost("hunt") + i.cost(area);
        let mut heap = i.heap(area);
        let rules = self
            .rules
            .hunt_with(area, TIMES[self.time].0, setup.targets, &setup.roamers);
        for (c, &role) in rules.roles.iter().enumerate() {
            if !setup.keeps_char(c, role) {
                continue;
            }
            let m = format!("m:{}", rules.keys[c]);
            lin += i.cost(&m);
            // The hunter and ambient animals are in the area's figure.
            if matches!(role, Role::Slot(_) | Role::Roamer) {
                heap += i.heap(&m);
            }
        }
        for w in setup.carried() {
            lin += i.cost(&format!("wpn{w}"));
            heap += i.heap(&format!("wpn{w}"));
        }
        let m = Memory {
            linear: (lin, i.linear_kb),
            heap: (heap, i.heap_kb),
        };
        self.mem.set(Some((key, m)));
        m
    }

    fn can_hunt(&self) -> Result<(), &'static str> {
        if self.trophy() {
            return Ok(());
        }
        if !self.observer && self.picked_weapons == 0 {
            return Err("Pick a weapon");
        }
        if !self.observer && self.picked_dinos == 0 && self.survival.is_none() {
            return Err("Pick an animal and a weapon");
        }
        if self.survival.is_some() && self.extra_dinos == 0 {
            return Err("No dangerous animal fits");
        }
        if self.remaining() < 0 {
            return Err("Not enough points");
        }
        let m = self.memory();
        if m.linear.0 > m.linear.1 || m.heap.0 > m.heap.1 {
            return Err("Too much for this console");
        }
        Ok(())
    }

    fn extra(&self) -> Option<(i32, u32, f32)> {
        self.extra_ammo
            .checked_sub(1)
            .and_then(|i| EXTRA_AMMO.get(i))
            .copied()
            .filter(|_| !self.observer)
    }

    /// The trophy room.
    fn trophy_setup(&self) -> Setup {
        Setup {
            area: "TROPHY".into(),
            time: TimeOfDay::Day,
            targets: 0,
            roamers: Vec::new(),
            silent_calls: Vec::new(),
            weapons: 0,
            double_ammo: false,
            observer: false,
            resupplies: 0,
            score_factor: 1.0,
            max_ambient: 0,
            dino_run_k: 1.0,
            blood: true,
            death_anim: true,
            notice_k: 1.0,
            stealth: false,
            random_ambients: false,
            crouch_key: self.settings.crouch_key,
            survival: None,
            survival_extra: 0,
            mobile_survival: None,
        }
    }

    fn setup(&self) -> Setup {
        let (targets, roamers) = self.picks();
        Setup {
            area: self
                .areas
                .get(self.area)
                .map_or("AREA1".into(), |a| a.0.clone()),
            time: TIMES[self.time].0,
            targets: if self.observer { 0 } else { targets },
            roamers: if self.observer { Vec::new() } else { roamers },
            silent_calls: Vec::new(),
            weapons: if self.observer {
                0
            } else {
                self.picked_weapons as u32
            },
            double_ammo: false,
            observer: self.observer,
            resupplies: self.extra().map_or(0, |e| e.1),
            score_factor: self.extra().map_or(1.0, |e| e.2),
            max_ambient: self.settings.max_ambient(),
            dino_run_k: self.settings.run_pct as f32 / 100.0,
            blood: self.settings.blood,
            death_anim: self.settings.death_anim,
            notice_k: if self.settings.friendly {
                crate::settings::FRIENDLY_NOTICE
            } else {
                1.0
            },
            stealth: self.settings.stealth,
            random_ambients: self.settings.random_ambients && self.rules.dialect(),
            crouch_key: self.settings.crouch_key,
            survival: self.survival.filter(|_| !self.observer),
            mobile_survival: self.mobile_survival.clone(),
            survival_extra: self.extra_targets(),
        }
    }

    /// The animals loaded: a survival's own, else the picks.
    fn dino_mask(&self) -> u64 {
        if self.survival.is_some() {
            self.extra_dinos
        } else {
            self.picked_dinos
        }
    }

    /// Hardcore's and custom's animals' target bits (bit 10 + slot).
    fn extra_targets(&self) -> u128 {
        let mut bits = 0u128;
        for (k, d) in self.dinos.iter().enumerate() {
            if self.special_dinos & (1 << k) != 0 {
                let slot = match d.0 {
                    Pick::Slot(s) => s,
                    Pick::Roamer(r) => r.slot,
                };
                if slot < 118 {
                    bits |= 1 << (slot + 10);
                }
            }
        }
        bits
    }

    /// Our survival's random area, time of day and animals (up to three
    /// dangerous ones that fit, the T-Rex aside; hardcore: and 1-3 mods'
    /// ones, custom: and the T-Rex, as they fit).
    fn roll_survival(&mut self) {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos() ^ d.as_secs() as u32)
            | 1;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        self.area = next() as usize % self.areas.len().max(1);
        self.time = next() as usize % 3;
        (self.extra_dinos, self.special_dinos) = (0, 0);
        let n = self.dinos.len();
        let order = crate::hunt::pick(&(0..n).collect::<Vec<_>>(), n, next());
        let fits = |m: &Self| {
            let mem = m.memory();
            mem.linear.0 <= mem.linear.1 && mem.heap.0 <= mem.heap.1
        };
        let take =
            |m: &mut Self, ok: &dyn Fn(&(bool, bool, bool)) -> bool, most: usize, special: bool| {
                let mut got = 0;
                for &i in &order {
                    if got == most {
                        break;
                    }
                    if m.extra_dinos & (1 << i) != 0 || !ok(&m.kinds[i]) {
                        continue;
                    }
                    m.extra_dinos |= 1 << i;
                    if fits(m) {
                        got += 1;
                        if special {
                            m.special_dinos |= 1 << i;
                        }
                    } else {
                        m.extra_dinos &= !(1 << i);
                    }
                }
            };
        take(self, &|k| k.0 && !k.2, 3, false);
        match self.survival {
            Some(SurvivalKind::Hardcore) => {
                let most = 1 + next() as usize % 3;
                take(self, &|k| k.0 && k.1 && !k.2, most, true);
            }
            Some(SurvivalKind::Custom) => take(self, &|k| k.2, 1, true),
            _ => {}
        }
    }

    /// The survival buttons on the title: (rectangle, label, classic).
    fn survival_buttons(&self) -> Vec<([f32; 4], &'static str, bool)> {
        match (self.ours, self.mobile_survival.is_some()) {
            (true, true) => vec![
                (SURVIVAL_L, "SURVIVAL", false),
                (CLASSIC_R, "CLASSIC", true),
            ],
            (_, true) => vec![(SURVIVAL, "CLASSIC SURVIVAL", true)],
            _ => vec![(SURVIVAL, "SURVIVAL", false)],
        }
    }

    /// Which survival button is at (`x`, `y`): `Some(classic)`.
    fn survival_button(&self, x: f32, y: f32) -> Option<bool> {
        self.survival_buttons()
            .into_iter()
            .find(|b| inside(b.0, x, y))
            .map(|b| b.2)
    }

    /// Classic survival: the mobile game's, on its area, with its animals
    /// and the shotgun.
    fn classic_setup(&mut self) -> Result<Setup, &'static str> {
        let m = self.mobile_survival.clone().ok_or("No classic survival")?;
        // The hunt's picks come back afterwards.
        let kept = (
            self.area,
            self.time,
            self.picked_weapons,
            self.picked_equip,
            self.extra_ammo,
        );
        let out = self.classic_with(&m);
        (
            self.area,
            self.time,
            self.picked_weapons,
            self.picked_equip,
            self.extra_ammo,
        ) = kept;
        out
    }

    fn classic_with(
        &mut self,
        m: &c2_formats::mobile_arcade::Survive,
    ) -> Result<Setup, &'static str> {
        let name = format!("AREA{}", m.area);
        self.area = self
            .areas
            .iter()
            .position(|a| a.0.eq_ignore_ascii_case(&name))
            .ok_or("Mount Ravan isn't in this pack")?;
        self.time = 1;
        self.survival = Some(SurvivalKind::Mobile);
        self.observer = false;
        let norm = |n: &str| -> String {
            n.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect()
        };
        let wanted: Vec<String> = m.animals().iter().map(|n| norm(n)).collect();
        self.extra_dinos = (0..self.dinos.len())
            .filter(|&i| wanted.contains(&norm(&self.dinos[i].1.name)))
            .fold(0, |a, i| a | 1 << i);
        self.special_dinos = 0;
        let names: Vec<String> = self
            .weapons
            .iter()
            .map(|w| w.name.to_ascii_lowercase())
            .collect();
        let shotgun = names
            .iter()
            .position(|n| n.trim() == "shotgun")
            .or_else(|| names.iter().position(|n| n.contains("shotgun")))
            .unwrap_or(0);
        self.picked_weapons = 1 << shotgun;
        self.picked_equip = 0;
        self.extra_ammo = 0;
        self.can_hunt()?;
        Ok(self.setup())
    }

    /// Keeps the accessory choice in the save, as the original does.
    fn store_equipment(&mut self) {
        let e = if self.observer { 0 } else { self.picked_equip };
        let o = &mut self.save.options;
        o.camo = i32::from(e & 1 != 0);
        o.radar = i32::from(e & 2 != 0);
        o.scent = i32::from(e & 4 != 0);
        if let Err(e) = crate::save::store(&self.save) {
            self.message = Some(format!("save failed: {e}"));
        }
    }

    fn roamer_picked(&self) -> bool {
        self.dinos
            .iter()
            .enumerate()
            .any(|(i, d)| matches!(d.0, Pick::Roamer(_)) && self.picked_dinos & (1 << i) != 0)
    }

    /// "Native to Area 2, Area 3" (picked, it comes on any map).
    fn roam_text(&self, i: usize) -> String {
        let names: Vec<String> = self
            .roams
            .get(&i)
            .map(|v| {
                v.iter()
                    .filter_map(|&a| self.areas.get(a))
                    .map(|a| a.1.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            "roams; comes when picked".into()
        } else {
            format!("native to {}", names.join(", "))
        }
    }

    /// Toggles bit `i` of `mask` unless that costs more than is left.
    fn toggle(&mut self, which: Step, i: usize, gear: bool) -> String {
        if which == Step::Animals {
            if let Some((Pick::Roamer(r), item)) = self.dinos.get(i) {
                let text = format!("{}  {}", item.name, self.roam_text(r.char));
                self.toggle_bit(which, i, gear);
                return text;
            }
        }
        self.toggle_bit(which, i, gear)
    }

    fn toggle_bit(&mut self, which: Step, i: usize, gear: bool) -> String {
        let (name, price) = match (which, gear) {
            (Step::Animals, _) => (self.dinos[i].1.name.clone(), self.dinos[i].1.price),
            (_, false) => (self.weapons[i].name.clone(), self.weapons[i].price),
            (_, true) => (self.equip[i].name.clone(), self.equip[i].price),
        };
        let mask = match (which, gear) {
            (Step::Animals, _) => &mut self.picked_dinos,
            (_, false) => &mut self.picked_weapons,
            (_, true) => &mut self.picked_equip,
        };
        *mask ^= 1 << i;
        let on = *mask & (1 << i) != 0;
        let counted = which == Step::Animals || !gear;
        if on && counted && mask.count_ones() > MAX_PICKS {
            *mask ^= 1 << i;
            let what = if which == Step::Animals {
                "animals"
            } else {
                "weapons"
            };
            self.message = Some(format!("{MAX_PICKS} {what} at most"));
            return format!("{name}  {price} pts");
        }
        if on && self.remaining() < 0 {
            let mask = match (which, gear) {
                (Step::Animals, _) => &mut self.picked_dinos,
                (_, false) => &mut self.picked_weapons,
                (_, true) => &mut self.picked_equip,
            };
            *mask ^= 1 << i;
            self.message = Some("Not enough points".into());
        }
        format!("{name}  {price} pts")
    }

    fn area_cells(&self) -> Vec<Cell> {
        grid(self.areas.len(), 3, 34.0, 2.0)
    }

    fn time_cells() -> [Cell; 3] {
        [
            (116.0, 176.0, 16.0),
            (160.0, 176.0, 16.0),
            (204.0, 176.0, 16.0),
        ]
    }

    /// Smaller hexagons when there are many animals (add-ons).
    fn dino_cells(&self) -> Vec<Cell> {
        let n = self.dinos.len();
        match n {
            0..=15 => grid(n, 5, 32.0, 2.0),
            16..=24 => grid(n, 7, 24.0, 2.0),
            25..=40 => grid(n, 8, 19.0, 2.0),
            _ => grid(n, 10, 15.0, 2.0),
        }
    }

    fn gear_cells(&self) -> Vec<Cell> {
        // Weapons, accessories, extra ammo.
        grid(self.weapons.len() + self.equip.len() + 1, 6, 30.0, 2.0)
    }

    const TRANQ: [f32; 4] = [56.0, 178.0, 96.0, 24.0];
    const OBSERVE: [f32; 4] = [168.0, 178.0, 96.0, 24.0];

    /// A tap at bottom-screen (x, y); `Some` = start this hunt.
    pub fn tap(&mut self, x: f32, y: f32) -> Option<Setup> {
        self.message = None;
        if inside(BACK, x, y) {
            self.back();
            return None;
        }
        if inside(NEXT, x, y) {
            return self.next();
        }
        let hit = |cells: &[Cell]| {
            cells
                .iter()
                .position(|&(cx, cy, r)| in_hex(cx, cy, r, x, y))
        };
        match self.step {
            Step::Title => {
                if inside(PLAY, x, y) {
                    self.survival = None;
                    self.extra_dinos = 0;
                    self.step = Step::Area;
                } else if let Some(classic) = self.survival_button(x, y) {
                    if classic {
                        match self.classic_setup() {
                            Ok(s) => return Some(s),
                            Err(e) => {
                                self.survival = None;
                                self.message = Some(e.into());
                            }
                        }
                    } else {
                        // Ours: only the guns are picked.
                        self.survival = Some(SurvivalKind::Normal);
                        self.observer = false;
                        self.step = Step::Gear;
                    }
                } else if inside(TROPHIES, x, y) && self.trophy_room {
                    return Some(self.trophy_setup());
                } else if inside(SETTINGS, x, y) {
                    self.step = Step::Settings;
                } else if inside(HUNTER, x, y) {
                    self.step = Step::Hunters;
                } else if inside(QUIT, x, y) {
                    self.quit = true;
                }
            }
            Step::Hunters => {
                let list = crate::save::hunters();
                if let Some(i) = (0..list.len() + 1).find(|&i| inside(hunter_row(i), x, y)) {
                    match list.get(i) {
                        Some(&(n, _)) => self.switch_hunter(n),
                        None if crate::save::free_slot().is_some() => self.want_name = true,
                        None => self.message = Some("Six hunters already".into()),
                    }
                }
            }
            Step::Settings if inside(CLEAR_TROPHIES, x, y) => {
                if self.clear_armed {
                    let n = c2_core::profile::clear_unlocked(&mut self.save);
                    self.message = Some(match crate::save::store(&self.save) {
                        Ok(()) => format!("{n} trophies removed"),
                        Err(e) => format!("save failed: {e}"),
                    });
                }
                self.clear_armed = !self.clear_armed;
            }
            Step::Settings => {
                self.clear_armed = false;
                let s = &mut self.settings;
                if inside(CROUCH_KEY, x, y) {
                    s.crouch_key = s.crouch_key.next();
                } else if inside(BLOOD, x, y) {
                    s.blood = !s.blood;
                } else if inside(DEATH_ANIM, x, y) {
                    s.death_anim = !s.death_anim;
                }
            }
            Step::Area => {
                if let Some(i) = hit(&self.area_cells()) {
                    self.area = i;
                    let a = &self.areas[i].1;
                    self.focus = Some(format!("{}  {} pts", a.name, a.price));
                } else if let Some(t) = hit(&Self::time_cells()) {
                    self.time = t;
                }
            }
            Step::Animals => {
                if let Some(i) = hit(&self.dino_cells()) {
                    self.focus = Some(self.toggle(Step::Animals, i, false));
                }
            }
            Step::Options => {
                let dialect = self.rules.dialect();
                let s = &mut self.settings;
                if inside(O_SPEED_DOWN, x, y) {
                    s.run_pct = (s.run_pct - 5).max(50);
                } else if inside(O_SPEED_UP, x, y) {
                    s.run_pct = (s.run_pct + 5).min(100);
                } else if inside(O_SPEED_BAR, x, y) {
                    let f = (x - O_SPEED_BAR[0]) / O_SPEED_BAR[2];
                    s.run_pct = (50 + (f * 50.0 / 5.0).round() as i32 * 5).clamp(50, 100);
                } else if inside(O_AMBIENT, x, y) {
                    s.ambient = !s.ambient;
                } else if inside(O_RANDOM, x, y) && dialect {
                    s.random_ambients = !s.random_ambients;
                } else if inside(O_FRIENDLY, x, y) {
                    s.friendly = !s.friendly;
                    self.focus = Some(if s.friendly {
                        "Animals notice you from closer".into()
                    } else {
                        "Animals notice you as on PC".into()
                    });
                } else if inside(O_STEALTH, x, y) {
                    s.stealth = !s.stealth;
                    self.focus = Some(if s.stealth {
                        "Crouch toggles; turn the 3DS to aim".into()
                    } else {
                        "Crouch while held".into()
                    });
                }
            }
            Step::Gear => {
                if let Some(i) = hit(&self.gear_cells()) {
                    let (nw, ne) = (self.weapons.len(), self.equip.len());
                    self.focus = Some(if i < nw {
                        self.toggle(Step::Gear, i, false)
                    } else if i < nw + ne {
                        self.toggle(Step::Gear, i - nw, true)
                    } else {
                        self.cycle_extra_ammo()
                    });
                } else if self.survival.is_some()
                    && (inside(Self::TRANQ, x, y) || inside(Self::OBSERVE, x, y))
                {
                    let all = SurvivalKind::ALL;
                    let i = all
                        .iter()
                        .position(|&k| Some(k) == self.survival)
                        .unwrap_or(0);
                    let k = all[(i + 1) % all.len()];
                    self.survival = Some(k);
                    self.focus = Some(
                        match k {
                            SurvivalKind::Hardcore => "1-3 mods' animals join each wave",
                            SurvivalKind::Custom => "Unlimited ammo; T-Rex from wave 15",
                            _ => "Random dangerous animals, random area",
                        }
                        .into(),
                    );
                } else if inside(Self::TRANQ, x, y) {
                    let t = &mut self.save.options.tranq;
                    *t = i32::from(*t == 0);
                } else if inside(Self::OBSERVE, x, y) {
                    self.observer = !self.observer;
                }
            }
        }
        None
    }

    /// Extra ammo: off -> x1 -> x2 -> x3 -> off, skipping what can't be
    /// paid for.
    fn cycle_extra_ammo(&mut self) -> String {
        let old = self.extra_ammo;
        loop {
            self.extra_ammo = (self.extra_ammo + 1) % (EXTRA_AMMO.len() + 1);
            if self.extra_ammo == 0 || self.remaining() >= 0 {
                break;
            }
        }
        if old == 0 && self.extra_ammo == 0 {
            self.message = Some("Not enough points".into());
        }
        match self.extra() {
            Some((price, n, f)) if f < 1.0 => {
                format!("{n} refill(s), {price} pts, -5% score")
            }
            Some((price, n, _)) => format!("{n} refills, {price} pts"),
            None => "Extra ammo: off".into(),
        }
    }

    /// B / the Back button.
    pub fn back(&mut self) {
        if matches!(self.step, Step::Settings | Step::Options) {
            if let Err(e) = self.settings.store() {
                self.message = Some(format!("settings not saved: {e}"));
            }
        }
        self.step = match self.step {
            Step::Title | Step::Settings | Step::Area | Step::Hunters => Step::Title,
            Step::Animals => Step::Area,
            Step::Gear if self.survival.is_some() => Step::Title,
            Step::Gear => Step::Animals,
            Step::Options => Step::Gear,
        };
    }

    /// START: back to the title, or quit from it.
    pub fn start_button(&mut self) {
        if self.step == Step::Title {
            self.quit = true;
        } else {
            self.back();
            self.step = Step::Title;
        }
    }

    /// A / the Next (or Hunt) button.
    pub fn next(&mut self) -> Option<Setup> {
        match self.step {
            Step::Title => self.step = Step::Area,
            Step::Settings | Step::Hunters => self.back(),
            Step::Area if !self.trophy() => self.step = Step::Animals,
            Step::Animals => self.step = Step::Gear,
            // Our survival: the guns, then in.
            Step::Gear if self.survival.is_some() => {
                self.roll_survival();
                match self.can_hunt() {
                    Ok(()) => return Some(self.setup()),
                    Err(e) => self.message = Some(e.into()),
                }
            }
            Step::Gear => self.step = Step::Options,
            _ => match self.can_hunt() {
                Ok(()) => {
                    self.store_equipment();
                    if let Err(e) = self.settings.store() {
                        self.message = Some(format!("settings not saved: {e}"));
                    }
                    return Some(self.setup());
                }
                Err(e) => self.message = Some(e.into()),
            },
        }
        None
    }

    /// Hunter `n` plays (its save and record), from now on and next time.
    fn switch_hunter(&mut self, n: i32) {
        crate::save::use_hunter(n);
        self.save = crate::save::load(self.rules.menu.start);
        self.record = crate::save::load_record();
        let o = self.save.options;
        self.picked_equip = [o.camo, o.radar, o.scent]
            .iter()
            .enumerate()
            .filter(|(_, &on)| on != 0)
            .fold(0, |m, (i, _)| m | 1 << i);
        self.settings.hunter = n;
        if let Err(e) = self.settings.store() {
            self.message = Some(format!("settings not saved: {e}"));
        }
        self.step = Step::Title;
    }

    /// A new hunter called `name` (from the system keyboard).
    pub fn new_hunter(&mut self, name: &str) {
        let Some(n) = crate::save::free_slot() else {
            return;
        };
        let name = if name.is_empty() { "Hunter" } else { name };
        match crate::save::create(n, name, self.rules.menu.start) {
            Ok(_) => self.switch_hunter(n),
            Err(e) => self.message = Some(format!("save failed: {e}")),
        }
    }

    pub fn set_message(&mut self, m: String) {
        self.message = Some(m);
    }

    /// Both screens for this frame (call between `Gpu::begin` and `end`).
    pub fn draw(&mut self, gpu: &mut Gpu) {
        self.c.new_frame();
        self.draw_top(gpu);
        self.draw_bottom(gpu);
    }

    fn draw_hex_item(c: &mut Canvas, gpu: &mut Gpu, cell: Cell, it: &Item, on: bool, afford: bool) {
        let (cx, cy, r) = cell;
        if on {
            c.hex(cx, cy, r + 3.0, GOLD);
        }
        c.hex(cx, cy, r, PANEL);
        c.flush(gpu);
        let tint = if afford || on { WHITE } else { DIMMED };
        if let Some(t) = &it.pic {
            // Pictures are 357x208 fitted to 128 (or 256) wide: their top
            // 0.58 of the canvas; show the middle 2/3 of their width.
            c.hex_image(gpu, t, cell, [0.17, 0.0, 0.83, 0.58], tint);
        } else {
            let short: String = it.name.chars().take(8).collect();
            c.text_c(gpu, cx, cy - 4.0, 1.0, tint, &short);
        }
        if it.price > 0 {
            let p = format!("{}", it.price);
            let w = p.len() as f32 * 8.0 + 4.0;
            c.rect(
                [cx - w / 2.0, cy + r * 0.866 - 11.0, w, 10.0],
                [0, 0, 0, 170],
            );
            c.text_c(
                gpu,
                cx,
                cy + r * 0.866 - 10.0,
                1.0,
                if afford || on { GOLD } else { DIM },
                &p,
            );
        }
    }

    /// Sun, sunset or moon, drawn with shapes.
    fn time_icon(c: &mut Canvas, (cx, cy, r): Cell, which: usize) {
        match which {
            0 => {
                // Sunset: half a sun on the horizon.
                c.disc(cx, cy + 4.0, r * 0.6, [250, 140, 50, 255]);
                c.rect([cx - r, cy + 4.0, 2.0 * r, r * 0.7], [70, 40, 50, 255]);
                c.rect([cx - r, cy + 3.0, 2.0 * r, 2.0], [250, 190, 90, 255]);
            }
            1 => {
                for k in 0..8 {
                    let a = k as f32 / 8.0 * std::f32::consts::TAU;
                    let (s, co) = a.sin_cos();
                    c.disc(
                        cx + co * r * 0.8,
                        cy + s * r * 0.8,
                        2.5,
                        [250, 220, 80, 255],
                    );
                }
                c.disc(cx, cy, r * 0.5, [255, 230, 90, 255]);
            }
            _ => {
                c.disc(cx, cy, r * 0.6, [220, 225, 240, 255]);
                c.disc(cx + r * 0.3, cy - r * 0.2, r * 0.55, PANEL);
            }
        }
    }

    fn draw_bottom(&mut self, gpu: &mut Gpu) {
        gpu.begin_2d(Some([BG[0], BG[1], BG[2]]));
        let left = self.remaining();
        let step = self.step;
        let mut c = std::mem::replace(&mut self.c, Canvas::new(None));
        match step {
            Step::Title => {
                let mut buttons = vec![(PLAY, "PLAY", true)];
                buttons.extend(
                    self.survival_buttons()
                        .into_iter()
                        .map(|b| (b.0, b.1, true)),
                );
                buttons.extend([
                    (TROPHIES, "TROPHIES", self.trophy_room),
                    (SETTINGS, "SETTINGS", true),
                    (QUIT, "QUIT", true),
                    (HUNTER, "HUNTER", true),
                ]);
                for &(r, _, on) in &buttons {
                    c.rect(
                        [r[0] - 2.0, r[1] - 2.0, r[2] + 4.0, r[3] + 4.0],
                        if on { GOLD } else { DIM },
                    );
                    c.rect(r, PANEL);
                }
                c.flush(gpu);
                for (r, label, on) in buttons {
                    let col = if on { WHITE } else { DIM };
                    // Half-width buttons and long labels smaller.
                    let k = if label.len() * 16 > r[2] as usize {
                        1.0
                    } else {
                        2.0
                    };
                    c.text_c(
                        gpu,
                        r[0] + r[2] / 2.0,
                        r[1] + r[3] / 2.0 - 4.0 * k,
                        k,
                        col,
                        label,
                    );
                }
                if let Some(m) = self.message.clone() {
                    let m: String = m.chars().take(38).collect();
                    c.text_c(gpu, 160.0, 224.0, 1.0, TEXT, &m);
                }
                self.c = c;
                return;
            }
            Step::Settings => {
                self.draw_settings(&mut c, gpu);
                self.c = c;
                return;
            }
            Step::Hunters => {
                let list = crate::save::hunters();
                let me = crate::save::hunter();
                let mut rows: Vec<(String, bool)> = list
                    .iter()
                    .map(|(n, name)| (name.clone(), *n == me))
                    .collect();
                if crate::save::free_slot().is_some() {
                    rows.push(("+ New hunter".into(), false));
                }
                for (i, (_, on)) in rows.iter().enumerate() {
                    let r = hunter_row(i);
                    if *on {
                        c.rect([r[0] - 2.0, r[1] - 2.0, r[2] + 4.0, r[3] + 4.0], GOLD);
                    }
                    c.rect(r, PANEL);
                }
                c.rect(BACK, PANEL);
                c.flush(gpu);
                for (i, (name, _)) in rows.iter().enumerate() {
                    let r = hunter_row(i);
                    let n: String = name.chars().take(26).collect();
                    c.text_c(gpu, r[0] + r[2] / 2.0, r[1] + 9.0, 1.0, WHITE, &n);
                }
                c.text_c(
                    gpu,
                    BACK[0] + BACK[2] / 2.0,
                    BAR_Y + 8.0,
                    1.0,
                    WHITE,
                    "Back",
                );
                if let Some(m) = self.message.clone() {
                    let m: String = m.chars().take(38).collect();
                    c.text_c(gpu, 160.0, 200.0, 1.0, TEXT, &m);
                }
                self.c = c;
                return;
            }
            Step::Area => {
                for (i, cell) in self.area_cells().into_iter().enumerate() {
                    let it = &self.areas[i].1;
                    let afford = it.price <= self.save.score;
                    Self::draw_hex_item(&mut c, gpu, cell, it, i == self.area, afford);
                }
                for (i, cell) in Self::time_cells().into_iter().enumerate() {
                    let (cx, cy, r) = cell;
                    if i == self.time {
                        c.hex(cx, cy, r + 3.0, GOLD);
                    }
                    c.hex(cx, cy, r, PANEL);
                    Self::time_icon(&mut c, cell, i);
                }
                c.flush(gpu);
            }
            Step::Animals => {
                for (i, cell) in self.dino_cells().into_iter().enumerate() {
                    let on = self.picked_dinos & (1 << i) != 0;
                    let (pick, item) = &self.dinos[i];
                    // Roaming add-ons: a blue rim.
                    if matches!(pick, Pick::Roamer(_)) && !on {
                        c.hex(cell.0, cell.1, cell.2 + 2.0, [60, 120, 230, 255]);
                    }
                    Self::draw_hex_item(&mut c, gpu, cell, item, on, item.price <= left);
                }
            }
            Step::Options => self.draw_options(&mut c, gpu),
            Step::Gear => {
                let (nw, ne) = (self.weapons.len(), self.equip.len());
                for (i, cell) in self.gear_cells().into_iter().enumerate() {
                    if i >= nw + ne {
                        self.draw_extra_ammo(&mut c, gpu, cell, left);
                        continue;
                    }
                    let (it, on) = if i < nw {
                        (&self.weapons[i], self.picked_weapons & (1 << i) != 0)
                    } else {
                        (
                            &self.equip[i - nw],
                            self.picked_equip & (1 << (i - nw)) != 0,
                        )
                    };
                    Self::draw_hex_item(&mut c, gpu, cell, it, on, it.price <= left);
                }
                let tranq = self.save.options.tranq != 0;
                let toggles = match self.survival {
                    // A survival: its kind in their place.
                    Some(k) => vec![(
                        [
                            Self::TRANQ[0],
                            Self::TRANQ[1],
                            Self::OBSERVE[0] + Self::OBSERVE[2] - Self::TRANQ[0],
                            Self::TRANQ[3],
                        ],
                        true,
                        k.name(),
                    )],
                    None => vec![
                        (Self::TRANQ, tranq, "Tranq"),
                        (Self::OBSERVE, self.observer, "Observe"),
                    ],
                };
                for (r, on, label) in toggles {
                    c.rect(r, if on { GOLD } else { PANEL });
                    c.flush(gpu);
                    let col = if on { [0, 0, 0, 255] } else { WHITE };
                    c.text_c(gpu, r[0] + r[2] / 2.0, r[1] + 8.0, 1.0, col, label);
                }
            }
        }
        // What was tapped, or a message; then the bar.
        let line = self.message.clone().or_else(|| self.focus.clone());
        if let Some(l) = line {
            let col = if self.message.is_some() { RED } else { TEXT };
            let l: String = l.chars().take(38).collect();
            // On the gear screen the Tranq / Observe buttons sit there: the
            // line goes above them, on a dark strip.
            let y = if step == Step::Gear { 164.0 } else { 200.0 };
            if step == Step::Gear {
                c.rect([4.0, y - 3.0, 312.0, 13.0], [0, 0, 0, 200]);
                c.flush(gpu);
            }
            c.text_c(gpu, 160.0, y, 1.0, col, &l);
        }
        let next = match step {
            Step::Area if self.trophy() => "Enter",
            Step::Options => "HUNT",
            _ => "Next",
        };
        if step != Step::Area {
            c.rect(BACK, PANEL);
        }
        let ready = step != Step::Options || self.can_hunt().is_ok();
        c.rect(NEXT, if ready { [60, 110, 60, 255] } else { PANEL });
        c.flush(gpu);
        if step != Step::Area {
            c.text_c(
                gpu,
                BACK[0] + BACK[2] / 2.0,
                BAR_Y + 8.0,
                1.0,
                WHITE,
                "Back",
            );
        }
        c.text_c(gpu, NEXT[0] + NEXT[2] / 2.0, BAR_Y + 8.0, 1.0, WHITE, next);
        let pts = format!("{left} pts");
        c.text_c(
            gpu,
            160.0,
            BAR_Y + 8.0,
            1.0,
            if left < 0 { RED } else { GOLD },
            &pts,
        );
        self.c = c;
    }

    /// The extra ammo hexagon: "x1".."x3" when picked, the next tier's
    /// price otherwise.
    fn draw_extra_ammo(&self, c: &mut Canvas, gpu: &mut Gpu, (cx, cy, r): Cell, left: i32) {
        let on = self.extra_ammo > 0;
        if on {
            c.hex(cx, cy, r + 3.0, GOLD);
        }
        c.hex(cx, cy, r, PANEL);
        c.flush(gpu);
        if let Some(t) = &self.ammo_pic {
            c.hex_image(gpu, t, (cx, cy, r), [0.17, 0.0, 0.83, 0.58], WHITE);
        } else {
            // A few cartridges.
            for k in 0..3 {
                let x = cx - 9.0 + k as f32 * 9.0;
                c.rect([x - 2.5, cy - 16.0, 5.0, 12.0], [200, 160, 60, 255]);
                c.rect([x - 2.5, cy - 20.0, 5.0, 4.0], [170, 90, 50, 255]);
            }
            c.flush(gpu);
        }
        if let Some((_, n, _)) = self.extra() {
            c.text_c(gpu, cx, cy - 1.0, 2.0, WHITE, &format!("x{n}"));
        } else {
            c.text_c(gpu, cx, cy, 1.0, WHITE, "Ammo+");
            let next = EXTRA_AMMO[0].0;
            let col = if next <= left { GOLD } else { DIM };
            c.text_c(gpu, cx, cy + r * 0.866 - 10.0, 1.0, col, &next.to_string());
        }
    }

    /// The settings screen (bottom).
    fn draw_settings(&self, c: &mut Canvas, gpu: &mut Gpu) {
        let s = self.settings;
        c.rect(BACK, PANEL);
        c.rect(
            CLEAR_TROPHIES,
            if self.clear_armed {
                [140, 60, 30, 255]
            } else {
                PANEL
            },
        );
        c.flush(gpu);
        let unlocked = self
            .save
            .bodies
            .iter()
            .filter(|b| b.ctype != 0 && !b.locked())
            .count();
        let t = if self.clear_armed {
            format!("Tap again: remove {unlocked}")
        } else {
            "Remove unlocked trophies".into()
        };
        c.text_c(gpu, 160.0, CLEAR_TROPHIES[1] + 9.0, 1.0, WHITE, &t);
        let toggles = [
            (BLOOD, s.blood, "Blood"),
            (DEATH_ANIM, s.death_anim, "Death animation"),
        ];
        for (r, on, _) in toggles {
            c.rect(r, if on { [60, 110, 60, 255] } else { PANEL });
        }
        c.rect(CROUCH_KEY, PANEL);
        c.flush(gpu);
        c.text_c(
            gpu,
            160.0,
            CROUCH_KEY[1] + 10.0,
            1.0,
            WHITE,
            &format!("Crouch: {}", s.crouch_key.name()),
        );
        c.text_c(
            gpu,
            160.0,
            40.0,
            1.0,
            TEXT,
            "Run speed, ambient animals, 3DS",
        );
        c.text_c(
            gpu,
            160.0,
            52.0,
            1.0,
            TEXT,
            "friendly and stealth crouch are",
        );
        c.text_c(
            gpu,
            160.0,
            64.0,
            1.0,
            TEXT,
            "in the hunt options (before a hunt).",
        );
        for (r, on, name) in toggles {
            let t = format!("{name}: {}", if on { "ON" } else { "OFF" });
            c.text_c(gpu, 160.0, r[1] + 10.0, 1.0, WHITE, &t);
        }
        c.text_c(
            gpu,
            BACK[0] + BACK[2] / 2.0,
            BAR_Y + 8.0,
            1.0,
            WHITE,
            "Back",
        );
    }

    /// The hunt options (bottom; the bar is drawn after).
    fn draw_options(&self, c: &mut Canvas, gpu: &mut Gpu) {
        let s = self.settings;
        let dialect = self.rules.dialect();
        c.rect(O_SPEED_BAR, PANEL);
        let f = (s.run_pct - 50) as f32 / 50.0;
        c.rect(
            [
                O_SPEED_BAR[0],
                O_SPEED_BAR[1],
                O_SPEED_BAR[2] * f.max(0.02),
                O_SPEED_BAR[3],
            ],
            [60, 110, 60, 255],
        );
        for r in [O_SPEED_DOWN, O_SPEED_UP] {
            c.rect(r, PANEL);
        }
        let mut toggles = vec![(O_AMBIENT, s.ambient, "Ambient animals")];
        if dialect {
            toggles.push((O_RANDOM, s.random_ambients, "Random ambients (4)"));
        }
        toggles.push((O_FRIENDLY, s.friendly, "3DS friendly"));
        toggles.push((O_STEALTH, s.stealth, "Stealth crouch"));
        for (r, on, _) in &toggles {
            c.rect(*r, if *on { [60, 110, 60, 255] } else { PANEL });
        }
        c.flush(gpu);
        c.text_c(
            gpu,
            160.0,
            6.0,
            1.0,
            TEXT,
            &format!("Animal run speed {}%", s.run_pct),
        );
        c.text_c(
            gpu,
            O_SPEED_DOWN[0] + 18.0,
            O_SPEED_DOWN[1] + 6.0,
            2.0,
            WHITE,
            "-",
        );
        c.text_c(
            gpu,
            O_SPEED_UP[0] + 18.0,
            O_SPEED_UP[1] + 6.0,
            2.0,
            WHITE,
            "+",
        );
        for (r, on, name) in toggles {
            let t = format!("{name}: {}", if on { "ON" } else { "OFF" });
            c.text_c(gpu, 160.0, r[1] + 8.0, 1.0, WHITE, &t);
        }
    }

    /// What the settings do (top).
    fn draw_settings_top(&self, c: &mut Canvas, gpu: &mut Gpu) {
        let lines = [
            ("Settings", GOLD),
            ("", WHITE),
            ("Hunt options (before each hunt)", TEXT),
            ("  Run speed, ambient animals, 3DS friendly", WHITE),
            ("  (noticed from closer), stealth crouch", WHITE),
            ("  (crouch toggles, gyro aim, half as seen).", WHITE),
            ("", WHITE),
            ("Crouch: ZL (New 3DS); Hold L (tap L jumps);", TEXT),
            ("  D-Pad Up (walk, run, crouch in turn)", WHITE),
            ("", WHITE),
            ("Blood / Death animation", TEXT),
            ("  Off: no blood sprays or trails; when", WHITE),
            ("  you die, a black screen and the prompt.", WHITE),
        ];
        for (i, (l, col)) in lines.iter().enumerate() {
            c.text(gpu, 12.0, 14.0 + 14.0 * i as f32, 1.0, *col, l);
        }
    }

    /// The title: the main menu's art, the hunter's record over its
    /// painted buttons.
    fn draw_title_top(&mut self, c: &mut Canvas, gpu: &mut Gpu) {
        if let Some(t) = &self.title {
            c.image(
                gpu,
                t,
                [0.0, 0.0, 400.0, 240.0],
                [0.0, 0.0, 400.0 / 512.0, 240.0 / 256.0],
                WHITE,
            );
        }
        // Over the painted Hunt / Options / Trophy / Credits / Quit.
        c.rect([0.0, 74.0, 168.0, 166.0], [10, 14, 10, 255]);
        c.flush(gpu);
        let name = String::from_utf8_lossy(&self.save.player_name_raw)
            .trim_end_matches('\0')
            .to_string();
        let ranks = ["Beginner", "Advanced", "Expert"];
        let rank = ranks
            .get(self.save.rank.clamp(0, 2) as usize)
            .unwrap_or(&"");
        let trophies = self.save.bodies.iter().filter(|b| b.ctype != 0).count();
        let lines = [
            (name, WHITE),
            (rank.to_string(), TEXT),
            (format!("Score   {}", self.save.score), GOLD),
            (format!("Trophies {trophies}"), WHITE),
            (format!("Kills   {}", self.save.total.success), WHITE),
        ];
        let r = &self.record;
        let mut lines = lines.to_vec();
        if let Some(k) = r.longest() {
            lines.push((format!("Longest {:.0} m", k.range), TEXT));
        }
        if let Some(k) = r.best() {
            lines.push((format!("Best    {} {}", k.score, k.animal), TEXT));
        }
        if r.best_wave > 0 {
            lines.push((format!("Wave    {}", r.best_wave), TEXT));
        }
        for (i, (l, col)) in lines.iter().enumerate() {
            let l: String = l.chars().take(18).collect();
            c.text(gpu, 12.0, 92.0 + 16.0 * i as f32, 1.0, *col, &l);
        }
    }

    fn draw_top(&mut self, gpu: &mut Gpu) {
        gpu.begin_2d(None);
        let mut c = std::mem::replace(&mut self.c, Canvas::new(None));
        if matches!(self.step, Step::Title | Step::Hunters) {
            self.draw_title_top(&mut c, gpu);
            self.c = c;
            return;
        }
        if self.step == Step::Settings {
            self.draw_settings_top(&mut c, gpu);
            self.c = c;
            return;
        }
        let name = String::from_utf8_lossy(&self.save.player_name_raw)
            .trim_end_matches('\0')
            .to_string();
        let ranks = ["Beginner", "Advanced", "Expert"];
        let rank = ranks
            .get(self.save.rank.clamp(0, 2) as usize)
            .unwrap_or(&"");
        c.text(gpu, 8.0, 6.0, 1.0, TEXT, &format!("{name} ({rank})"));
        let pts = format!("Points: {} / {}", self.remaining(), self.save.score);
        c.text(
            gpu,
            TOP_W as f32 - 8.0 - pts.len() as f32 * 8.0,
            6.0,
            1.0,
            GOLD,
            &pts,
        );

        // The area and the time of day (a survival's are random).
        let survival = self.survival.is_some() && !self.trophy();
        if survival {
            c.rect([8.0, 22.0, 192.0, 112.0], PANEL);
            c.flush(gpu);
            c.text(gpu, 8.0, 140.0, 1.0, WHITE, "Random area and time");
        } else if let Some((_, a)) = self.areas.get(self.area) {
            if let Some(t) = &a.pic {
                c.image(
                    gpu,
                    t,
                    [8.0, 22.0, 192.0, 112.0],
                    [0.0, 0.0, 1.0, 0.58],
                    WHITE,
                );
            } else {
                c.rect([8.0, 22.0, 192.0, 112.0], PANEL);
            }
            let n: String = a.name.chars().take(24).collect();
            c.text(gpu, 8.0, 140.0, 1.0, WHITE, &n);
        }
        if !self.trophy() && !survival {
            Self::time_icon(&mut c, (24.0, 168.0, 16.0), self.time);
            c.flush(gpu);
            c.text(gpu, 46.0, 164.0, 1.0, WHITE, TIMES[self.time].1);
            // A roaming add-on picked (blue in the list): it has no call.
            if !self.observer && self.roamer_picked() {
                c.text(gpu, 8.0, 192.0, 1.0, GOLD, "No calls supplied,");
                c.text(gpu, 8.0, 202.0, 1.0, GOLD, "proceed with caution");
                c.text(gpu, 8.0, 212.0, 1.0, BLUE, "(+10% score)");
            }
        }

        // What is picked, with prices.
        let mut y = 22.0;
        // Items: name, price, colour.
        let mut list =
            |c: &mut Canvas, gpu: &mut Gpu, head: &str, items: Vec<(String, i32, Rgba)>| {
                c.text(gpu, 212.0, y, 1.0, TEXT, head);
                y += 10.0;
                if items.is_empty() {
                    c.text(gpu, 220.0, y, 1.0, DIM, "-");
                    y += 10.0;
                }
                for (n, p, col) in items {
                    let n: String = n.chars().take(16).collect();
                    c.text(gpu, 220.0, y, 1.0, col, &n);
                    if p > 0 {
                        let ps = p.to_string();
                        c.text(gpu, 392.0 - ps.len() as f32 * 8.0, y, 1.0, GOLD, &ps);
                    }
                    y += 9.0;
                }
                y += 4.0;
            };
        if self.trophy() {
            list(&mut c, gpu, "Your trophies", Vec::new());
        } else if self.observer {
            list(&mut c, gpu, "Observer: just watch", Vec::new());
        } else {
            let pick = |items: &[Item], mask: u64| -> Vec<(String, i32, Rgba)> {
                items
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, it)| (it.name.clone(), it.price, WHITE))
                    .collect()
            };
            // Roaming add-ons in blue, as in the grid and on the map.
            let dinos: Vec<(String, i32, Rgba)> = self
                .dinos
                .iter()
                .enumerate()
                .filter(|(i, _)| self.picked_dinos & (1 << i) != 0)
                .map(|(_, d)| match d.0 {
                    Pick::Roamer(_) => (d.1.name.clone(), d.1.price, BLUE),
                    Pick::Slot(_) => (d.1.name.clone(), d.1.price, WHITE),
                })
                .collect();
            let head = self.survival.map_or("Animals", |k| k.name());
            let dinos = if survival {
                vec![("Random, dangerous".to_string(), 0, WHITE)]
            } else {
                dinos
            };
            list(&mut c, gpu, head, dinos);
            list(
                &mut c,
                gpu,
                "Weapons",
                pick(&self.weapons, self.picked_weapons),
            );
            let mut gear = pick(&self.equip, self.picked_equip);
            if self.save.options.tranq != 0 {
                gear.push(("Tranquilizer".into(), 0, WHITE));
            }
            if let Some((price, n, f)) = self.extra() {
                let pen = if f < 1.0 { " -5%" } else { "" };
                gear.push((format!("Ammo x{n}{pen}"), price, WHITE));
            }
            list(&mut c, gpu, "Gear", gear);
        }

        // Will it fit?
        let m = self.memory();
        let over = m.linear.0 > m.linear.1 || m.heap.0 > m.heap.1;
        let (msg, col) = match (self.step, self.can_hunt()) {
            (_, Ok(())) if self.step == Step::Options || self.trophy() => (
                format!(
                    "Ready. Memory {}/{} + {}/{} MiB",
                    m.heap.0 / 1024,
                    m.heap.1 / 1024,
                    m.linear.0 / 1024,
                    m.linear.1 / 1024
                ),
                TEXT,
            ),
            (Step::Options, Err(e)) => (e.to_string(), if over { RED } else { DIM }),
            // Taken by the arm above.
            (Step::Options, Ok(())) => (String::new(), DIM),
            (Step::Area, _) => ("Pick a place and the time of day".into(), DIM),
            (Step::Animals, _) => ("Pick the animals to hunt".into(), DIM),
            (Step::Gear, _) => ("Pick weapons and gear".into(), DIM),
            (Step::Title | Step::Settings | Step::Hunters, _) => (String::new(), DIM),
        };
        c.text(gpu, 8.0, 226.0, 1.0, col, &msg);
        self.c = c;
    }
}
