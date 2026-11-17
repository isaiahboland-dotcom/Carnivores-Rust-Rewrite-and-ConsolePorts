//! The field manual's armoury files (DHC OS on PC and Android): what the
//! terminal says about one weapon of the catalog, from its `_RES.TXT`
//! entry, the island rules and the hunter's record. The areas it is
//! restricted to are links to their surveys, in [`links`]'s order.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::specimen::{areas_allowing, length, Line};
use super::{Catalog, Rules, Tab};
use crate::record::Record;

/// One weapon's facts, as the platform read them (`WeaponInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    /// `shots`: rounds carried (0: not given).
    pub rounds: i32,
    /// `reload`: rounds per magazine (0: none).
    pub magazine: i32,
    /// `trace`: pellets per shot.
    pub pellets: i32,
    pub scope: bool,
    /// Fires under water (the Modder's Edition's `harpoon`).
    pub harpoon: bool,
    /// `c2rs_allow_tranq`: may fire darts.
    pub tranq: bool,
    /// `c2rs_allow_trophy`: its kills go to the trophy room.
    pub trophy: bool,
}

impl Default for Arm {
    fn default() -> Self {
        Arm {
            rounds: 0,
            magazine: 0,
            pellets: 1,
            scope: false,
            harpoon: false,
            tranq: true,
            trophy: true,
        }
    }
}

/// The files weapon `weapon`'s armoury file links to: the areas whose
/// island rules issue it, only when that is not every area.
pub fn links(cat: &Catalog, rules: &Rules, weapon: usize) -> Vec<(Tab, usize)> {
    areas_allowing(cat, rules, Tab::Arms, weapon, None)
        .unwrap_or_default()
        .into_iter()
        .map(|a| (Tab::Area, a))
        .collect()
}

impl Arm {
    /// Weapon `weapon`'s file: its specifications, price, restrictions,
    /// the areas that issue it (links, or ALL AREAS) and the hunter's
    /// kills with it (`rec`; none: no hunter signed in). `us`: US units.
    pub fn lines(
        &self,
        cat: &Catalog,
        rules: &Rules,
        weapon: usize,
        rec: Option<&Record>,
        us: bool,
    ) -> Vec<Line> {
        let Some(it) = cat.weapons.get(weapon) else {
            return Vec::new();
        };
        let mut v = vec![Line::Head("SPECIFICATIONS")];
        if self.rounds > 0 {
            v.push(Line::Fact("ROUNDS", self.rounds.to_string()));
        }
        if self.magazine > 0 {
            v.push(Line::Fact("MAGAZINE", self.magazine.to_string()));
        }
        if self.pellets > 1 {
            v.push(Line::Fact("PELLETS", format!("{} PER SHOT", self.pellets)));
        }
        if self.scope {
            v.push(Line::Fact("OPTICS", "TELESCOPIC SIGHT".into()));
        }
        if self.harpoon {
            v.push(Line::Fact("WATER", "FIRES BELOW THE SURFACE".into()));
        }
        v.push(Line::Blank);
        v.push(Line::Head("LICENSING"));
        v.push(Line::Fact("PRICE", format!("{} PTS", it.price)));
        v.push(Line::Blank);
        v.push(Line::Head("RESTRICTIONS"));
        let mut none = true;
        if !self.tranq {
            v.push(Line::Fact("TRANQ", "DARTS NOT PERMITTED".into()));
            none = false;
        }
        if !self.trophy {
            v.push(Line::Fact("TROPHIES", "KILLS ARE NOT KEPT".into()));
            none = false;
        }
        if none {
            v.push(Line::Item("NONE".into()));
        }
        v.push(Line::Blank);
        v.push(Line::Head("AUTHORISED AREAS"));
        match areas_allowing(cat, rules, Tab::Arms, weapon, None) {
            None => v.push(Line::Item("ALL AREAS".into())),
            Some(a) if a.is_empty() => v.push(Line::Item("NO AUTHORISED AREA".into())),
            Some(_) => v.extend(links(cat, rules, weapon).iter().enumerate().map(
                |(k, &(_, a))| {
                    let name = cat.areas.get(a).map_or("", |x| x.name.as_str());
                    Line::Link(k, name.trim().to_uppercase())
                },
            )),
        }
        v.push(Line::Blank);
        v.push(Line::Head("HUNTER RECORDS"));
        let Some(rec) = rec else {
            v.push(Line::Item("NO HUNTER SIGNED IN".into()));
            return v;
        };
        let kills: Vec<_> = rec.kills_with(&it.name).collect();
        if kills.is_empty() {
            v.push(Line::Item("NO KILLS ON FILE".into()));
            return v;
        }
        v.push(Line::Fact("KILLS", kills.len().to_string()));
        if let Some(k) = kills.iter().max_by(|a, b| a.range.total_cmp(&b.range)) {
            let s: String = format!(
                "{} ({})",
                length(k.range, us),
                k.animal.trim().to_uppercase()
            );
            v.push(Line::Fact("LONG SHOT", s));
        }
        v
    }
}
