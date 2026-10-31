//! The game this app was installed with: a folder of the merged game's
//! files on the SD card (`c2-cook --target wiiu`), read through a [`Vfs`],
//! and what the menus offer from it. The rules are the 3DS's
//! (`c2-3ds/src/rules.rs`): the original `_RES.TXT` or the Modder's Edition
//! dialect, resolved per hunt; the menu's lists from `_MENU.TXT` if there is
//! one, else `_RES.TXT`; `INFO.TXT` (written by the cooker) lists the areas
//! and where roaming add-ons spawn.
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use c2_core::area::TimeOfDay;
use c2_core::dino::DinoInfo;
use c2_formats::bits::Bits;
use c2_formats::mee::Tables;
use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, ResData, ResTxt};
use c2_vfs::Vfs;

/// How a character is loaded (the 3DS's `Role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The hunter: every hunt.
    Hunter,
    /// Small ambient animals and flyers (AI under 10): every hunt with
    /// ambient animals on (a dialect one its tables put on other maps only:
    /// [`Role::Off`]).
    Ambient,
    /// A menu animal: loaded when its slot is picked (call id 10 + slot).
    Slot(usize),
    /// A huntable AI (10-29) without a menu slot (add-ons that only roam):
    /// offered in the menu, loaded when picked.
    Roamer,
    /// The big ambient wanderers (AI 30 and up) without a slot: not loaded.
    Off,
}

/// The first call slot given to roaming add-ons, unless the menu's own go
/// further; call id 10 + slot stays under `Bits::LEN`.
pub const ROAM_SLOT0: usize = 64;

/// A roaming add-on as a hunt target: its character, the call slot it is
/// hunted under (never called: its add-on has no call) and the menu slot
/// of an animal with its AI, whose price it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roam {
    pub char: usize,
    pub slot: usize,
    pub donor: Option<usize>,
}

/// One hunt's characters and weapons.
pub struct HuntRules {
    pub res: ResData,
    /// The dialect's spawn and pack tables.
    pub tables: Option<Tables>,
    pub roles: Vec<Role>,
}

pub struct Rules {
    /// The dialect `_RES.TXT`, or `None` for an original one.
    raw: Option<ResTxt>,
    vanilla: Option<ResData>,
    pub menu: MenuData,
}

fn parse(vfs: &Vfs, path: &str) -> Option<ResTxt> {
    ResTxt::parse(&String::from_utf8_lossy(&vfs.read(path).ok()?)).ok()
}

impl Rules {
    pub fn load(vfs: &Vfs) -> Option<Self> {
        let raw = parse(vfs, "HUNTDAT/_RES.TXT")?;
        let menu = match parse(vfs, "HUNTDAT/_MENU.TXT") {
            Some(m) => MenuData::from_res(&m),
            None => MenuData::from_res(&raw),
        };
        Some(if c2_formats::mee::is_dialect(&raw) {
            Self { raw: Some(raw), vanilla: None, menu }
        } else {
            Self { vanilla: Some(raw.to_data()), raw: None, menu }
        })
    }

    /// Whether the rules are the Modder's Edition's.
    pub fn dialect(&self) -> bool {
        self.raw.is_some()
    }

    /// The rules for a hunt in `area` (`AREA3`, `TROPHY`) at `time` with
    /// `targets` (bit 10 + menu slot) picked.
    pub fn hunt(&self, area: &str, time: TimeOfDay, targets: Bits) -> HuntRules {
        self.hunt_with(area, time, targets, &[])
    }

    /// The roaming add-ons, in `_RES.TXT` order, each with its call slot
    /// and donor, from a hunt with everything picked (`neutral`).
    pub fn roams(neutral: &HuntRules) -> Vec<Roam> {
        let ai = |i: usize| {
            neutral
                .tables
                .as_ref()
                .and_then(|t| t.characters.get(i))
                .and_then(|c| c.block.get(&["ai"]))
                .map(c_atoi)
        };
        let offered = |i: usize| {
            neutral
                .tables
                .as_ref()
                .and_then(|t| t.characters.get(i))
                .is_none_or(|c| c2_core::mee::offered(&c.block))
        };
        let roamers = (0..neutral.roles.len()).filter(|&i| neutral.roles[i] == Role::Roamer && offered(i));
        let first = neutral
            .roles
            .iter()
            .filter_map(|r| match r {
                Role::Slot(k) => Some(k + 1),
                _ => None,
            })
            .fold(ROAM_SLOT0, usize::max);
        roamers
            .zip(first..Bits::LEN - 10)
            .map(|(char, slot)| Roam {
                char,
                slot,
                donor: neutral.roles.iter().enumerate().find_map(|(j, r)| match r {
                    Role::Slot(k) if ai(j).is_some() && ai(j) == ai(char) => Some(*k),
                    _ => None,
                }),
            })
            .collect()
    }

    /// [`Rules::hunt`] as played: picked animals and roamers come on this
    /// map even if their tables put them on others; each picked roamer
    /// takes its call slot, so it loads and hunts as a menu animal.
    pub fn hunt_with(&self, area: &str, time: TimeOfDay, targets: Bits, roamers: &[Roam]) -> HuntRules {
        let (res, tables) = match (&self.raw, &self.vanilla) {
            (Some(raw), _) => {
                let h = c2_formats::mee::Hunt {
                    area: c2_formats::mee::area_of(area),
                    time: match time {
                        TimeOfDay::Dusk => 0,
                        TimeOfDay::Day => 1,
                        TimeOfDay::Night => 2,
                    },
                    picked: targets.down(10),
                };
                let chars: Vec<usize> = roamers.iter().map(|r| r.char).collect();
                let mut m = c2_core::mee::read_picked_anywhere(raw, &h, &chars);
                for r in roamers {
                    if let Some(c) = m.tables.characters.get_mut(r.char) {
                        c.block.entries.push(c2_formats::res_txt::Entry { key: "callNo".into(), value: r.slot.to_string() });
                    }
                }
                (m.res, Some(m.tables))
            }
            (None, Some(v)) => (v.clone(), None),
            (None, None) => (ResData::default(), None),
        };
        let infos: Vec<DinoInfo> = res.characters.iter().map(DinoInfo::from_def).collect();
        // Dialect ambients whose tables don't spawn them here.
        let elsewhere = |i: usize| match (&self.raw, &tables) {
            (Some(raw), Some(t)) => {
                let raw_char = raw.section("characters").and_then(|s| s.blocks.get(i)).is_some_and(c2_formats::mee::has_spawns);
                raw_char && !t.characters.get(i).is_some_and(|c| c.spawns())
            }
            _ => false,
        };
        let roles = infos
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let raw_char = |i: usize| self.raw.as_ref()?.section("characters")?.blocks.get(i);
                let slot = match &tables {
                    Some(t) => t
                        .characters
                        .get(i)
                        .and_then(|c| c2_formats::mee::menu_slot(raw_char(i), &c.block))
                        .map(|n| n as i32),
                    None => (d.ai >= 10).then_some(d.ai - 10),
                };
                match slot {
                    _ if d.ai == 0 => Role::Hunter,
                    Some(n) => Role::Slot(n as usize),
                    None if (0..10).contains(&d.ai) && elsewhere(i) => Role::Off,
                    None if (0..10).contains(&d.ai) => Role::Ambient,
                    None if (10..30).contains(&d.ai) => Role::Roamer,
                    None => Role::Off,
                }
            })
            .collect();
        HuntRules { res, tables, roles }
    }
}

/// `INFO.TXT`: the game's name, the areas (`AREA1`.., then `TROPHY`),
/// where it snows, and the areas each roaming add-on spawns in (by
/// character).
#[derive(Debug, PartialEq)]
pub struct Info {
    pub name: String,
    pub areas: Vec<String>,
    pub snow: Vec<String>,
    pub roam: BTreeMap<usize, Vec<String>>,
}

impl Default for Info {
    fn default() -> Self {
        Info { name: String::from("Carnivores 2"), areas: Vec::new(), snow: Vec::new(), roam: BTreeMap::new() }
    }
}

impl Info {
    pub fn parse(text: &str) -> Self {
        let list = |v: &str| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
        let mut i = Info::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let k = k.trim();
            match k {
                "name" if !v.trim().is_empty() => i.name = String::from(v.trim()),
                "areas" => i.areas = list(v),
                "snow" => i.snow = list(v),
                _ => {
                    if let Some(c) = k.strip_prefix("roam.").and_then(|c| c.parse().ok()) {
                        i.roam.insert(c, list(v));
                    }
                }
            }
        }
        i
    }
}

/// What a menu animal is: a menu slot, or a roaming add-on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Slot(usize),
    Roamer(Roam),
}

/// An animal the menu offers.
#[derive(Debug, Clone)]
pub struct Animal {
    pub pick: Pick,
    pub name: String,
    pub price: i32,
    /// It comes for the hunter (a survival's waves).
    pub dangerous: bool,
    /// Its model (`HUNTDAT/MANOS.CAR`): a roamer has no menu picture, and
    /// a menu animal's may be an empty frame, so the menu draws one from
    /// the model, as the 3DS cook does.
    pub model: Option<String>,
}

/// An area the menu offers (the trophy room is apart).
#[derive(Debug, Clone)]
pub struct AreaItem {
    /// `AREA3`: its files under `HUNTDAT/AREAS/`.
    pub file: String,
    /// 3: its menu picture and text (`AREA3.TGA`, `AREA3.TXT`).
    pub number: usize,
    pub name: String,
}

/// At most this many of each are offered (the menu's grids).
pub const MAX_AREAS: usize = 12;
pub const MAX_ANIMALS: usize = 24;
pub const MAX_WEAPONS: usize = 8;

pub struct Install {
    pub vfs: Vfs,
    pub rules: Rules,
    pub info: Info,
    pub areas: Vec<AreaItem>,
    /// Cheapest first (the game's order, then add-ons, for equal prices).
    pub animals: Vec<Animal>,
    /// The menu's own animals (the first in `animals`); roamers follow.
    pub menu_animals: usize,
    pub weapons: Vec<String>,
    pub trophy_room: bool,
}

/// The first non-empty line of `HUNTDAT/MENU/TXT/<file>`.
pub fn title(vfs: &Vfs, file: &str) -> Option<String> {
    let d = vfs.read(&format!("HUNTDAT/MENU/TXT/{file}")).ok()?;
    String::from_utf8_lossy(&d).lines().map(str::trim).find(|l| !l.is_empty()).map(String::from)
}

impl Install {
    /// The install in `vfs`, with its `INFO.TXT`; `None` without rules.
    #[allow(dead_code)] // the host tests' entry
    pub fn open(vfs: Vfs, info: Info) -> Option<Self> {
        Self::open_traced(vfs, info, &|_| {})
    }

    /// [`Install::open`], saying how far it got (the console's startup log).
    pub fn open_traced(vfs: Vfs, info: Info, mark: &dyn Fn(&str)) -> Option<Self> {
        let rules = Rules::load(&vfs)?;
        mark("Install: rules");
        let areas: Vec<AreaItem> = info
            .areas
            .iter()
            .filter(|a| !a.eq_ignore_ascii_case("TROPHY"))
            .filter_map(|a| {
                let number: usize = a.get(4..)?.parse().ok()?;
                let name = title(&vfs, &format!("AREA{number}.TXT")).unwrap_or_else(|| format!("Area {number}"));
                Some(AreaItem { file: a.clone(), number, name })
            })
            .take(MAX_AREAS)
            .collect();
        mark("Install: areas");
        let trophy_room = info.areas.iter().any(|a| a.eq_ignore_ascii_case("TROPHY"));
        let menu = &rules.menu;
        let dialect = rules.dialect();
        let first = areas.first().map_or("AREA1", |a| a.file.as_str());
        let neutral = rules.hunt(first, TimeOfDay::Day, Bits::ALL);
        mark("Install: rules resolved");
        let char_of = |pick: &Pick| match pick {
            Pick::Roamer(r) => Some(r.char),
            Pick::Slot(k) => neutral.roles.iter().position(|&r| r == Role::Slot(*k)),
        };
        let mut animals: Vec<Animal> = menu
            .huntables
            .iter()
            .enumerate()
            .map(|(k, d)| Animal { pick: Pick::Slot(k), name: d.name.clone(), price: d.price, dangerous: false, model: None })
            .collect();
        // Roaming add-ons that spawn on one of the areas offered (once
        // picked, they come on any).
        for r in Rules::roams(&neutral) {
            let Some(on) = info.roam.get(&r.char) else { continue };
            let spawns = areas.iter().any(|a| on.iter().any(|n| n.eq_ignore_ascii_case(&a.file)));
            let info = neutral.res.characters.get(r.char).map(DinoInfo::from_def);
            let name = info.as_ref().map(|d| d.name.clone()).unwrap_or_default();
            if !spawns || animals.iter().any(|a| a.name == name) {
                continue;
            }
            let price = r.donor.and_then(|k| menu.huntables.get(k)).map_or(0, |d| d.price);
            let model = info.map(|d| format!("HUNTDAT/{}", d.file));
            animals.push(Animal { pick: Pick::Roamer(r), name, price, dangerous: false, model });
        }
        mark("Install: roamers");
        for a in &mut animals {
            let char = char_of(&a.pick);
            let ai = char.and_then(|j| neutral.res.characters.get(j)).map(|d| DinoInfo::from_def(d).ai);
            if a.model.is_none() {
                a.model = char.and_then(|j| neutral.res.characters.get(j)?.file.clone()).map(|f| format!("HUNTDAT/{f}"));
            }
            let aggress = char.and_then(|j| neutral.tables.as_ref()?.characters.get(j)).and_then(|c| c.block.get(&["aggress"])).map(c_atoi);
            a.dangerous = ai.is_some_and(|ai| c2_core::game::dangerous_ai(ai, dialect.then(|| aggress.unwrap_or(0))));
        }
        mark("Install: dangerous");
        animals.truncate(MAX_ANIMALS);
        // Roamers after the menu's own, so "All dinos huntable" off shows
        // the first ones only.
        animals.sort_by_key(|a| (matches!(a.pick, Pick::Roamer(_)), a.price));
        let menu_animals = animals.iter().filter(|a| matches!(a.pick, Pick::Slot(_))).count();
        let weapons = neutral
            .res
            .weapons
            .iter()
            .enumerate()
            .map(|(i, w)| menu.weapons.get(i).map(|m| m.name.clone()).filter(|n| !n.is_empty()).or_else(|| w.name.clone()).unwrap_or_else(|| format!("Weapon {}", i + 1)))
            .take(MAX_WEAPONS)
            .collect();
        Some(Install { vfs, rules, info, areas, animals, menu_animals, weapons, trophy_room })
    }

    /// The picked animals (bits by position in `animals`) as hunt targets
    /// (bit 10 + call slot) and the roamers among them.
    pub fn targets(&self, picked: u64) -> (Bits, Vec<Roam>) {
        let mut targets = Bits::NONE;
        let mut roamers = Vec::new();
        for (_, a) in self.animals.iter().enumerate().filter(|(i, _)| picked & (1 << i) != 0) {
            match a.pick {
                Pick::Slot(k) => targets.set(10 + k),
                Pick::Roamer(r) => {
                    targets.set(10 + r.slot);
                    roamers.push(r);
                }
            }
        }
        (targets, roamers)
    }
}
