//! The hunt menu's lists, as the Modder's Edition Engine menu builds them
//! (`Menu/Resources.cpp: LoadResourcesScript` in the modernized fork):
//! from `HUNTDAT/_MENU.TXT` when there is one, else from `_RES.TXT`, plus
//! `HUNTDAT/AREAS/*.c2map` area descriptors.
//!
//! - `weapons { ... }`: every block, in order (menu picture `WEAPONn.TGA`,
//!   n = position + 1).
//! - `characters { ... }`: only blocks with `ai >= 10` are huntable; their
//!   position in that list is the menu slot (picture `DINOn.TGA`, n is the
//!   slot plus one, unless `pic` names one). A hunt passes the picked slots
//!   as bits (`din=`), which the game reads as bit `10 + slot` (`char<slot>`
//!   in `overwrite` conditions). In vanilla data slot n is AI 10 + n.
//! - `prices { ... }`: `start`, then `area` (area n = the n-th line, its
//!   files `AREAn.MAP`/`.RSC`; slot 6 may be named `external`), `dino` (by
//!   slot), `weapon` (by position), `acces` (accessories).
//! - `accessories { ... }`: score multipliers by name (`camo radar scent
//!   double tranq observe`).
//!
//! Which areas exist is for the caller to check (it knows the files).

use crate::res_txt::{c_atof, c_atoi, Block, ResTxt};

/// A weapon or huntable as the menu lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub name: String,
    pub price: i32,
    /// For characters, the `ai` written in the entry (informational in the
    /// dialect: the menu slot is what a hunt passes on).
    pub ai: i32,
    /// An explicit picture path (`pic = '...'`), if given.
    pub pic: Option<String>,
}

/// An area the menu offers.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuArea {
    /// Shown name; `None` = the first line of `MENU/TXT/AREAn.TXT`.
    pub name: Option<String>,
    /// File stem under `HUNTDAT/AREAS/` (`area3`, or a `.c2map`'s map).
    pub project: String,
    pub price: i32,
    /// 1-based number for `prices`-listed areas (`AREAn.TGA`, `AREAn.TXT`).
    pub number: Option<usize>,
    /// Picture and description paths from a `.c2map`.
    pub pic: Option<String>,
    pub text: Option<String>,
    /// It snows there (`snow = TRUE` in a `.c2map`: ours, for Ice Age's
    /// areas beside C2's; the dialect gives snow by area number).
    pub snow: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuData {
    /// A new hunter's points (`prices.start`, default 100).
    pub start: i32,
    pub weapons: Vec<MenuItem>,
    /// Huntables in menu-slot order.
    pub huntables: Vec<MenuItem>,
    pub areas: Vec<MenuArea>,
    pub accessory_prices: Vec<i32>,
    /// `accessories { name = multiplier }`.
    pub accessory_scores: Vec<(String, f32)>,
}

/// Quoted text value (`'Velociraptor'`), else the raw value.
fn text(v: &str) -> String {
    let v = v.trim();
    match (v.find('\''), v.rfind('\'')) {
        (Some(a), Some(b)) if b > a => v[a + 1..b].to_string(),
        _ => v.to_string(),
    }
}

fn item(b: &Block) -> MenuItem {
    MenuItem {
        name: b.get(&["name"]).map(text).unwrap_or_default(),
        price: b.get(&["price"]).map_or(0, c_atoi),
        ai: b.get(&["ai"]).map_or(0, c_atoi),
        pic: b.get(&["pic"]).map(text).filter(|p| !p.is_empty()),
    }
}

impl MenuData {
    /// From a parsed `_MENU.TXT` (or `_RES.TXT`).
    pub fn from_res(res: &ResTxt) -> Self {
        let blocks = |name: &str| {
            res.section(name)
                .map(|s| s.blocks.as_slice())
                .unwrap_or_default()
        };
        let mut out = MenuData {
            start: 100,
            weapons: blocks("weapons").iter().map(item).collect(),
            huntables: blocks("characters")
                .iter()
                .map(item)
                .filter(|i| i.ai >= 10)
                .collect(),
            ..Default::default()
        };
        if let Some(p) = res.section("prices") {
            let (mut area, mut dino, mut weapon) = (0usize, 0usize, 0usize);
            for e in &p.entries {
                let v = c_atoi(&e.value);
                match e.key.to_ascii_lowercase().as_str() {
                    "start" => out.start = v,
                    "area" => {
                        area += 1;
                        out.areas.push(MenuArea {
                            name: None,
                            project: format!("area{area}"),
                            price: v,
                            number: Some(area),
                            pic: None,
                            text: None,
                            snow: false,
                        });
                    }
                    "dino" => {
                        if let Some(d) = out.huntables.get_mut(dino) {
                            d.price = v;
                        }
                        dino += 1;
                    }
                    "weapon" => {
                        if let Some(w) = out.weapons.get_mut(weapon) {
                            w.price = v;
                        }
                        weapon += 1;
                    }
                    "acces" => out.accessory_prices.push(v),
                    _ => {}
                }
            }
        }
        if let Some(a) = res.section("accessories") {
            out.accessory_scores = a
                .entries
                .iter()
                .map(|e| (e.key.to_ascii_lowercase(), c_atof(&e.value)))
                .collect();
        }
        out
    }

    /// Adds a `.c2map` area unless one with the same file stem is listed
    /// (the script's entry wins).
    pub fn add_c2map(&mut self, area: MenuArea) {
        if !self
            .areas
            .iter()
            .any(|a| a.project.eq_ignore_ascii_case(&area.project))
        {
            self.areas.push(area);
        }
    }
}

/// A `.c2map` descriptor (`info { name mapfile rscfile pic text price }`).
/// `None` without a `mapfile`.
pub fn parse_c2map(src: &str) -> Option<MenuArea> {
    let mut a = MenuArea {
        name: None,
        project: String::new(),
        price: 0,
        number: None,
        pic: None,
        text: None,
        snow: false,
    };
    let mut inside = false;
    for line in src.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with('.') {
            break;
        }
        if !inside {
            inside = l.contains("info") || l.contains('{');
            continue;
        }
        if l.contains('}') {
            break;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let v = text(v.split('#').next().unwrap_or(""));
        let path = || Some(v.replace('\\', "/").replace("//", "/"));
        match k.trim() {
            "name" => a.name = Some(v.clone()),
            "price" => a.price = c_atoi(&v),
            "pic" => a.pic = path().filter(|p| !p.is_empty()),
            "text" => a.text = path().filter(|p| !p.is_empty()),
            "snow" => a.snow = v.contains("TRUE"),
            "mapfile" => {
                let p = v.replace('\\', "/");
                let stem = p.rsplit('/').next().unwrap_or(&p);
                a.project = stem
                    .strip_suffix(".map")
                    .or_else(|| stem.strip_suffix(".MAP"))
                    .unwrap_or(stem)
                    .to_string();
            }
            _ => {}
        }
    }
    (!a.project.is_empty()).then_some(a)
}
