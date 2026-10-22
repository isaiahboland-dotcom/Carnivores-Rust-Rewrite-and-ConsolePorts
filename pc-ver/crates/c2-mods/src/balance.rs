//! Balance files: the player's own prices and scores for a game's areas,
//! animals and weapons (the launcher's balance editor writes them), applied
//! over the game as a layer of rewritten text files, so the PC game and
//! `c2-cook` read them like any other data.
//!
//! Where the values live (as `menu_txt::MenuData` reads them): the menu's
//! lists come from `_MENU.TXT` if there is one, else `_RES.TXT`. A price is
//! the entry's `price`, unless that file's `prices` section has the n-th
//! `area` / `dino` / `weapon` line; areas from `.c2map` files keep theirs
//! in the file. Scores are `basescore` in `_RES.TXT` (the entry with the
//! animal's name, and any `overwrite` / `addition` block in it that sets
//! one too).
//!
//! Entries are matched by name (a repeated name gets ` #2`, ` #3`...), so a
//! balance file survives add-ons being reordered; names not in the game are
//! skipped.

use serde::{Deserialize, Serialize};

use c2_formats::res_txt::{c_atof, c_atoi, Block, Entry, ResTxt};
use c2_vfs::{MemBackend, Vfs};

use crate::ModError;

const RES: &str = "HUNTDAT/_RES.TXT";
const MENU: &str = "HUNTDAT/_MENU.TXT";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Balance {
    /// A new hunter's points (`prices.start`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animals: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapons: Vec<Item>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<i32>,
    /// Animals only: points for a kill (`basescore`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
}

impl Balance {
    pub fn from_toml(text: &str) -> Result<Self, ModError> {
        toml::from_str(text).map_err(|e| ModError::Recipe(format!("balance file: {e}")))
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn load(path: &std::path::Path) -> Result<Self, ModError> {
        let text = std::fs::read_to_string(path).map_err(|source| ModError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&text)
    }

    /// Whether it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.start.is_none()
            && [&self.areas, &self.animals, &self.weapons]
                .iter()
                .all(|l| l.iter().all(|i| i.price.is_none() && i.score.is_none()))
    }
}

/// Where a price is kept.
#[derive(Debug, Clone)]
enum At {
    /// `price` in block `i` of the menu file's section.
    Block(usize),
    /// Entry `i` of the menu file's `prices` section.
    Line(usize),
    /// A `.c2map` file.
    C2map(String),
    /// Nowhere: a roaming add-on (a huntable AI in `_RES.TXT` with no menu
    /// entry) has a score but no price of its own.
    Nowhere,
}

/// One listed item: its name (made unique), price and where it is.
struct Found {
    key: String,
    price: Option<i32>,
    at: At,
}

/// The game's files as the balance sees them.
struct Game {
    res: ResTxt,
    /// `_MENU.TXT`, if the game has one (else the menu reads `_RES.TXT`).
    menu: Option<ResTxt>,
    areas: Vec<Found>,
    animals: Vec<Found>,
    weapons: Vec<Found>,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    match (v.find('\''), v.rfind('\'')) {
        (Some(a), Some(b)) if b > a => v[a + 1..b].to_string(),
        _ => v.to_string(),
    }
}

fn parse(vfs: &Vfs, path: &str) -> Result<Option<ResTxt>, ModError> {
    if vfs.resolve(path).is_none() {
        return Ok(None);
    }
    Ok(Some(ResTxt::parse(&String::from_utf8_lossy(
        &vfs.read(path)?,
    ))?))
}

/// Unique keys: the second "Raptor" is "Raptor #2".
fn unique(names: Vec<(String, i32, At)>) -> Vec<Found> {
    let mut seen: Vec<String> = Vec::new();
    names
        .into_iter()
        .map(|(name, price, at)| {
            let n = seen
                .iter()
                .filter(|s| s.eq_ignore_ascii_case(&name))
                .count();
            seen.push(name.clone());
            let key = if n == 0 {
                name
            } else {
                format!("{name} #{}", n + 1)
            };
            let price = (!matches!(at, At::Nowhere)).then_some(price);
            Found { key, price, at }
        })
        .collect()
}

/// The first non-empty line of `MENU/TXT/AREAn.TXT`, as the menus show it.
fn area_title(vfs: &Vfs, n: usize) -> String {
    vfs.read(&format!("HUNTDAT/MENU/TXT/AREA{n}.TXT"))
        .ok()
        .and_then(|d| {
            String::from_utf8_lossy(&d)
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("Area {n}"))
}

impl Game {
    fn read(vfs: &Vfs) -> Result<Self, ModError> {
        let res = parse(vfs, RES)?.ok_or_else(|| ModError::MissingFile(RES.into()))?;
        let menu = parse(vfs, MENU)?;
        let m = menu.as_ref().unwrap_or(&res);
        let blocks = |name: &str| {
            m.section(name)
                .map(|s| s.blocks.as_slice())
                .unwrap_or_default()
        };
        let price_of = |b: &Block| b.get(&["price"]).map_or(0, c_atoi);
        let name_of = |b: &Block| b.get(&["name"]).map(unquote).unwrap_or_default();
        let mut weapons: Vec<(String, i32, At)> = blocks("weapons")
            .iter()
            .enumerate()
            .map(|(i, b)| (name_of(b), price_of(b), At::Block(i)))
            .collect();
        let mut animals: Vec<(String, i32, At)> = blocks("characters")
            .iter()
            .enumerate()
            .filter(|(_, b)| b.get(&["ai"]).map_or(0, c_atoi) >= 10)
            .map(|(i, b)| (name_of(b), price_of(b), At::Block(i)))
            .collect();
        let mut areas = Vec::new();
        if let Some(p) = m.section("prices") {
            let (mut a, mut d, mut w) = (0, 0, 0);
            for (i, e) in p.entries.iter().enumerate() {
                let v = c_atoi(&e.value);
                let slot = match e.key.to_ascii_lowercase().as_str() {
                    "area" => {
                        a += 1;
                        areas.push((area_title(vfs, a), v, At::Line(i)));
                        continue;
                    }
                    "dino" => {
                        d += 1;
                        animals.get_mut(d - 1)
                    }
                    "weapon" => {
                        w += 1;
                        weapons.get_mut(w - 1)
                    }
                    _ => None,
                };
                if let Some(s) = slot {
                    (s.1, s.2) = (v, At::Line(i));
                }
            }
        }
        // Roaming add-ons: huntable AIs (10-29) in `_RES.TXT` with no menu
        // entry and no `callNo` (the 3DS offers them; a score only).
        if let Some(chars) = res.section("characters") {
            for b in &chars.blocks {
                let name = name_of(b);
                let ai = b.get(&["ai"]).map_or(0, c_atoi);
                let menu = animals.iter().any(|a| a.0.eq_ignore_ascii_case(&name));
                let slot = b.get(&["callNo"]).is_some_and(|v| c_atoi(v) >= 0);
                if (10..30).contains(&ai) && !slot && !menu {
                    animals.push((name, 0, At::Nowhere));
                }
            }
        }
        // `.c2map` areas the `prices` lines don't already list.
        let listed = areas.len();
        let mut c2maps: Vec<String> = vfs
            .files()
            .map(|(_, r)| r.path.clone())
            .filter(|p| {
                let l = p.to_ascii_lowercase();
                l.starts_with("huntdat/areas/") && l.ends_with(".c2map")
            })
            .collect();
        c2maps.sort();
        for path in c2maps {
            let Ok(d) = vfs.read(&path) else { continue };
            let Some(a) = c2_formats::menu_txt::parse_c2map(&String::from_utf8_lossy(&d)) else {
                continue;
            };
            let numbered = a
                .project
                .to_ascii_lowercase()
                .strip_prefix("area")
                .and_then(|n| n.parse::<usize>().ok())
                .is_some_and(|n| n >= 1 && n <= listed);
            if !numbered {
                let name = a.name.clone().unwrap_or_else(|| a.project.clone());
                areas.push((name, a.price, At::C2map(path)));
            }
        }
        Ok(Game {
            areas: unique(areas),
            animals: unique(animals),
            weapons: unique(weapons),
            res,
            menu,
        })
    }

    fn start(&self) -> i32 {
        self.menu
            .as_ref()
            .unwrap_or(&self.res)
            .section("prices")
            .and_then(|p| {
                p.entries
                    .iter()
                    .find(|e| e.key.eq_ignore_ascii_case("start"))
            })
            .map_or(100, |e| c_atoi(&e.value))
    }

    /// The animal's `_RES.TXT` entry (by its menu name; "Raptor #2" is
    /// the second entry named Raptor).
    fn res_block(&self, key: &str) -> Option<usize> {
        let (name, n) = split_number(key);
        self.res
            .section("characters")?
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.get(&["name"])
                    .map(unquote)
                    .is_some_and(|x| x.eq_ignore_ascii_case(name))
            })
            .nth(n - 1)
            .map(|(i, _)| i)
    }
}

/// "Raptor #2" -> ("Raptor", 2); "Raptor" -> ("Raptor", 1).
fn split_number(key: &str) -> (&str, usize) {
    match key.rsplit_once(" #").map(|(a, k)| (a, k.parse::<usize>())) {
        Some((a, Ok(k))) if k >= 1 => (a, k),
        _ => (key, 1),
    }
}

/// The game's current prices and scores, every field filled in.
pub fn read(vfs: &Vfs) -> Result<Balance, ModError> {
    let g = Game::read(vfs)?;
    let score = |key: &str| {
        let b = g.res_block(key)?;
        let block = g.res.section("characters")?.blocks.get(b)?;
        Some(block.get(&["basescore"]).map_or(0.0, c_atof))
    };
    let item = |f: &Found| Item {
        name: f.key.clone(),
        price: f.price,
        score: None,
    };
    Ok(Balance {
        start: Some(g.start()),
        areas: g.areas.iter().map(item).collect(),
        animals: g
            .animals
            .iter()
            .map(|f| Item {
                score: score(&f.key),
                ..item(f)
            })
            .collect(),
        weapons: g.weapons.iter().map(item).collect(),
    })
}

/// Sets the last `key` entry, or adds one.
fn set(entries: &mut Vec<Entry>, key: &str, value: String) {
    match entries
        .iter_mut()
        .rev()
        .find(|e| e.key.eq_ignore_ascii_case(key))
    {
        Some(e) => e.value = value,
        None => entries.push(Entry {
            key: key.to_string(),
            value,
        }),
    }
}

/// `basescore` in the block and in every sub-block that sets one.
fn set_score(b: &mut Block, value: &str) {
    set(&mut b.entries, "basescore", value.to_string());
    for s in &mut b.subs {
        if s.block.get(&["basescore"]).is_some() {
            set_score(&mut s.block, value);
        }
    }
}

fn number(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// `price = N` in a `.c2map`'s `info` block, replaced or added.
fn c2map_price(src: &str, price: i32) -> String {
    let mut out = Vec::new();
    let mut done = false;
    for line in src.lines() {
        let t = line.trim_start();
        if !done && t.split('=').next().is_some_and(|k| k.trim() == "price") {
            let indent = &line[..line.len() - t.len()];
            out.push(format!("{indent}price = {price}"));
            done = true;
        } else if !done && t.starts_with('}') {
            out.push(format!("    price = {price}"));
            out.push(line.to_string());
            done = true;
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\r\n") + "\r\n"
}

/// The files `b` changes, rewritten, as a layer to put over the game's.
/// Empty when nothing differs.
pub fn apply(vfs: &Vfs, b: &Balance) -> Result<MemBackend, ModError> {
    let mut g = Game::read(vfs)?;
    let mut layer = MemBackend::new();
    let (mut menu_changed, mut res_changed) = (false, false);
    let find =
        |list: &[Found], name: &str| list.iter().position(|f| f.key.eq_ignore_ascii_case(name));
    // Prices go to the menu file (`_MENU.TXT`, else `_RES.TXT`).
    let mut prices: Vec<(&'static str, At, i32)> = Vec::new();
    for (section, items, list) in [
        ("characters", &b.animals, &g.animals),
        ("weapons", &b.weapons, &g.weapons),
        ("", &b.areas, &g.areas),
    ] {
        for it in items {
            let (Some(p), Some(i)) = (it.price, find(list, &it.name)) else {
                continue;
            };
            if list[i].price.is_some_and(|q| q != p) {
                prices.push((section, list[i].at.clone(), p));
            }
        }
    }
    let start = b.start.filter(|&s| s != g.start());
    {
        let m = g.menu.as_mut().unwrap_or(&mut g.res);
        for (section, at, p) in prices {
            match at {
                At::Block(i) => {
                    if let Some(bl) = m
                        .sections
                        .iter_mut()
                        .find(|s| s.name.eq_ignore_ascii_case(section))
                        .and_then(|s| s.blocks.get_mut(i))
                    {
                        set(&mut bl.entries, "price", p.to_string());
                        menu_changed = true;
                    }
                }
                At::Line(i) => {
                    if let Some(e) = m
                        .sections
                        .iter_mut()
                        .find(|s| s.name.eq_ignore_ascii_case("prices"))
                        .and_then(|s| s.entries.get_mut(i))
                    {
                        e.value = p.to_string();
                        menu_changed = true;
                    }
                }
                At::Nowhere => {}
                At::C2map(path) => {
                    let src = String::from_utf8_lossy(&vfs.read(&path)?).into_owned();
                    layer.insert(&path, c2map_price(&src, p).into_bytes());
                }
            }
        }
        if let Some(s) = start {
            match m
                .sections
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case("prices"))
            {
                Some(p) => set(&mut p.entries, "start", s.to_string()),
                None => m.sections.push(c2_formats::res_txt::Section {
                    name: "prices".into(),
                    blocks: Vec::new(),
                    entries: vec![Entry {
                        key: "start".into(),
                        value: s.to_string(),
                    }],
                    subs: Vec::new(),
                }),
            }
            menu_changed = true;
        }
    }
    // Scores go to `_RES.TXT`.
    for it in &b.animals {
        let Some(s) = it.score else { continue };
        let Some(i) = g.res_block(&it.name) else {
            continue;
        };
        if let Some(bl) = g
            .res
            .sections
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case("characters"))
            .and_then(|sec| sec.blocks.get_mut(i))
        {
            if bl.get(&["basescore"]).map_or(0.0, c_atof) != s {
                set_score(bl, &number(s));
                res_changed = true;
            }
        }
    }
    match &g.menu {
        Some(m) if menu_changed => layer.insert(MENU, m.write().into_bytes()),
        None if menu_changed => res_changed = true,
        _ => {}
    }
    if res_changed {
        layer.insert(RES, g.res.write().into_bytes());
    }
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_vfs::Backend;

    const RES: &str = "weapons {\n{\n name = 'Pistol'\n}\n{\n name = 'Rifle'\n}\n}\n\
characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Raptor'\n ai = 10\n basescore = 5\n}\n\
{\n name = 'Raptor'\n ai = 11\n basescore = 7.5\n overwrite area1 {\n  basescore = 9\n }\n}\n}\n\
prices {\n start = 100\n area = 20\n dino = 10\n dino = 15\n weapon = 20\n weapon = 100\n}\n.\n";

    fn game(extra: &[(&str, &str)]) -> Vfs {
        let mut m = MemBackend::new();
        m.insert("HUNTDAT/_RES.TXT", RES.as_bytes().to_vec());
        m.insert("HUNTDAT/MENU/TXT/AREA1.TXT", b"Fern Valley\r\n".to_vec());
        for (p, d) in extra {
            m.insert(p, d.as_bytes().to_vec());
        }
        let mut v = Vfs::new();
        v.push("game", Box::new(m));
        v
    }

    #[test]
    fn reads_prices_and_scores() {
        let b = read(&game(&[])).unwrap_or_default();
        assert_eq!(b.start, Some(100));
        assert_eq!(b.areas[0].name, "Fern Valley");
        let names: Vec<&str> = b.animals.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Raptor", "Raptor #2"]);
        assert_eq!(b.animals[1].price, Some(15));
        assert_eq!(b.weapons[1].price, Some(100));
        assert_eq!(b.animals[0].score, Some(5.0));
    }

    #[test]
    fn roamers_have_a_score_only() {
        let res = "characters {\n{\n name = 'Rex'\n ai = 18\n callNo = 0\n basescore = 20\n}\n\
{\n name = 'Manos'\n ai = 18\n basescore = 22\n}\n}\n.\n";
        let menu = "characters {\n{\n name = 'Rex'\n ai = 18\n price = 400\n}\n}\n.\n";
        let mut m = MemBackend::new();
        m.insert("HUNTDAT/_RES.TXT", res.as_bytes().to_vec());
        m.insert("HUNTDAT/_MENU.TXT", menu.as_bytes().to_vec());
        let mut v = Vfs::new();
        v.push("game", Box::new(m));
        let b = read(&v).unwrap_or_default();
        assert_eq!(b.animals.len(), 2);
        assert_eq!(
            (b.animals[0].price, b.animals[0].score),
            (Some(400), Some(20.0))
        );
        assert_eq!(b.animals[1].name, "Manos");
        assert_eq!((b.animals[1].price, b.animals[1].score), (None, Some(22.0)));
        let mut edit = b.clone();
        edit.animals[1].score = Some(30.0);
        edit.animals[1].price = Some(5);
        let layer = apply(&v, &edit).unwrap_or_default();
        // Only `_RES.TXT` changes (a roamer has no price to set).
        assert_eq!(layer.files(), vec!["HUNTDAT/_RES.TXT".to_string()]);
        v.push("balance", Box::new(layer));
        assert_eq!(read(&v).unwrap_or_default().animals[1].score, Some(30.0));
    }

    #[test]
    fn applies_over_the_game() {
        let v = game(&[(
            "HUNTDAT/AREAS/new.c2map",
            "info {\n name = 'Isle'\n mapfile = 'new.map'\n price = 5\n}\n",
        )]);
        let b = Balance {
            start: Some(250),
            areas: vec![Item {
                name: "Isle".into(),
                price: Some(40),
                score: None,
            }],
            animals: vec![Item {
                name: "Raptor".into(),
                price: Some(12),
                score: Some(6.5),
            }],
            weapons: vec![Item {
                name: "Rifle".into(),
                price: Some(150),
                score: None,
            }],
        };
        let mut v2 = game(&[(
            "HUNTDAT/AREAS/new.c2map",
            "info {\n name = 'Isle'\n mapfile = 'new.map'\n price = 5\n}\n",
        )]);
        v2.push("balance", Box::new(apply(&v, &b).unwrap_or_default()));
        let after = read(&v2).unwrap_or_default();
        assert_eq!(after.start, Some(250));
        assert_eq!(after.areas[1].price, Some(40));
        assert_eq!(after.animals[0].price, Some(12));
        assert_eq!(after.animals[0].score, Some(6.5));
        // The second raptor (and its overwrite) untouched.
        assert_eq!(after.animals[1].score, Some(7.5));
        assert_eq!(after.weapons[1].price, Some(150));
        // Unchanged balance: no files.
        assert!(apply(&v2, &after).is_ok_and(|l| l.files().is_empty()));
    }
}
