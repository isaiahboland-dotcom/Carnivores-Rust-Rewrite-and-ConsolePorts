//! The pack's rules: `_RES.TXT` (resolved per hunt when it is in the
//! Modder's Edition dialect) and the menu's lists (`_MENU.TXT` if packed,
//! else `_RES.TXT`).

use anyhow::Result;
use c2_core::area::TimeOfDay;
use c2_core::dino::DinoInfo;
use c2_formats::mee::Tables;
use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, ResData, ResTxt};
use c2_vfs::Vfs;

/// How a character is loaded on the 3DS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The hunter: every hunt.
    Hunter,
    /// Small ambient animals and flyers (AI under 10): every hunt, unless
    /// turned off in the settings.
    Ambient,
    /// A menu animal: loaded when its slot is picked (call id 10 + slot).
    Slot(usize),
    /// A huntable AI (10-29) without a menu slot (add-ons that only roam):
    /// offered in the menu, loaded when picked. `c2-cook` sorts models the
    /// same way for its memory figures.
    Roamer,
    /// The big ambient wanderers (brachiosaur-like AIs, fish: 30 and up)
    /// without a slot: not loaded on the 3DS (a couple of MiB each).
    Off,
}

/// The first call slot given to roaming add-ons, after any menu's own
/// (call id 10 + slot; slots stay under 118 for the 128-bit target mask).
pub const ROAM_SLOT0: usize = 64;

/// A roaming add-on as a hunt target: its character (`_RES.TXT` index),
/// the call slot it is hunted under (`ROAM_SLOT0` on, so it is a target
/// and shows on the radar; the slot is never called, as its add-on
/// supplies no call), and the menu slot of an animal with its AI whose
/// price it takes.
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
    /// Per character: its role, and the name its model is cooked under.
    pub roles: Vec<Role>,
    pub keys: Vec<String>,
}

pub struct Rules {
    /// The dialect `_RES.TXT`, or `None` for an original one.
    raw: Option<ResTxt>,
    vanilla: Option<ResData>,
    pub menu: MenuData,
}

fn parse(vfs: &Vfs, path: &str) -> Result<ResTxt> {
    Ok(ResTxt::parse(&String::from_utf8_lossy(&vfs.read(path)?))?)
}

impl Rules {
    pub fn load(vfs: &Vfs) -> Result<Self> {
        let raw = parse(vfs, "HUNTDAT/_RES.TXT")?;
        let menu = match parse(vfs, "HUNTDAT/_MENU.TXT") {
            Ok(m) => MenuData::from_res(&m),
            Err(_) => MenuData::from_res(&raw),
        };
        Ok(if c2_formats::mee::is_dialect(&raw) {
            Self {
                raw: Some(raw),
                vanilla: None,
                menu,
            }
        } else {
            Self {
                vanilla: Some(raw.to_data()),
                raw: None,
                menu,
            }
        })
    }

    /// Whether the pack's rules are the Modder's Edition's (modded games).
    pub fn dialect(&self) -> bool {
        self.raw.is_some()
    }

    /// The rules for a hunt in `area` (`AREA3`, `TROPHY`) at `time`, with
    /// `targets` (bit 10 + menu slot) picked.
    pub fn hunt(&self, area: &str, time: TimeOfDay, targets: u128) -> HuntRules {
        self.hunt_with(area, time, targets, &[])
    }

    /// The roaming add-ons the menu offers, in `_RES.TXT` order, each with
    /// its call slot and donor (see [`Roam`]), from a hunt with everything
    /// picked (`neutral`).
    pub fn roams(neutral: &HuntRules) -> Vec<Roam> {
        let ai = |i: usize| {
            neutral
                .tables
                .as_ref()
                .and_then(|t| t.characters.get(i))
                .and_then(|c| c.block.get(&["ai"]))
                .map(c_atoi)
        };
        let roamers = (0..neutral.roles.len()).filter(|&i| neutral.roles[i] == Role::Roamer);
        roamers
            .zip(ROAM_SLOT0..118)
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

    /// [`Rules::hunt`] as played: picked animals (and picked roamers) come
    /// on this map even if their tables put them on others
    /// (`c2_core::mee::spawn_picked_anywhere`); each picked roamer takes its
    /// call slot (`callNo`), so it loads and hunts as a menu animal.
    pub fn hunt_with(
        &self,
        area: &str,
        time: TimeOfDay,
        targets: u128,
        roamers: &[Roam],
    ) -> HuntRules {
        let (res, tables) = match (&self.raw, &self.vanilla) {
            (Some(raw), _) => {
                let h = c2_formats::mee::Hunt {
                    area: c2_formats::mee::area_of(area),
                    time: match time {
                        TimeOfDay::Dusk => 0,
                        TimeOfDay::Day => 1,
                        TimeOfDay::Night => 2,
                    },
                    picked: targets >> 10,
                };
                let mut m = c2_core::mee::read(raw, &h);
                let chars: Vec<usize> = roamers.iter().map(|r| r.char).collect();
                c2_core::mee::spawn_picked_anywhere(raw, &h, &mut m.tables, &chars);
                for r in roamers {
                    if let Some(c) = m.tables.characters.get_mut(r.char) {
                        c.block.entries.push(c2_formats::res_txt::Entry {
                            key: "callNo".into(),
                            value: r.slot.to_string(),
                        });
                    }
                }
                (m.res, Some(m.tables))
            }
            (None, Some(v)) => (v.clone(), None),
            (None, None) => (ResData::default(), None),
        };
        let infos: Vec<DinoInfo> = res.characters.iter().map(DinoInfo::from_def).collect();
        let roles = infos
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let slot = match &tables {
                    Some(t) => t
                        .characters
                        .get(i)
                        .and_then(|c| c.block.get(&["callNo"]))
                        .map(c_atoi)
                        .filter(|&n| n >= 0),
                    None => (d.ai >= 10).then_some(d.ai - 10),
                };
                match slot {
                    _ if d.ai == 0 => Role::Hunter,
                    Some(n) => Role::Slot(n as usize),
                    None if (0..10).contains(&d.ai) => Role::Ambient,
                    None if (10..30).contains(&d.ai) => Role::Roamer,
                    None => Role::Off,
                }
            })
            .collect();
        let keys = infos
            .iter()
            .map(|d| c2_formats::cooked::model_key(&d.file))
            .collect();
        HuntRules {
            res,
            tables,
            roles,
            keys,
        }
    }
}
