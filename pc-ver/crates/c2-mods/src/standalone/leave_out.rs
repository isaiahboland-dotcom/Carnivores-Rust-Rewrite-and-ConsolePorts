//! Parts of the base game itself left out (`[recipe.skip]`: the stock
//! game's, the Modder's Edition's or a standalone's used as the base, so
//! add-ons or standalones can stand in for them). A layer of rewritten
//! files over the built game, like a balance file: the areas, huntables and
//! weapons left out go from `_RES.TXT` and `_MENU.TXT` (their blocks and
//! `prices` lines), what is kept is renumbered with no gaps (conditions,
//! calls, the map, menu and call files), and conditions that named only
//! left-out parts go with them.
//!
//! Numbers are the game's own menu order, as [`super::base_parts`] lists them;
//! add-ons' entries and standalone games combined beside it (they come
//! after the base's) keep their places after the kept ones.

use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, Block, ResTxt};
use c2_vfs::{MemBackend, Vfs};

use super::{renumber, skip::drop_refs, unquote, Moves, Result, Skip};

const RES: &str = "HUNTDAT/_RES.TXT";
const MENU: &str = "HUNTDAT/_MENU.TXT";

fn parse(vfs: &Vfs, path: &str) -> Result<Option<ResTxt>> {
    if vfs.resolve(path).is_none() {
        return Ok(None);
    }
    Ok(Some(ResTxt::parse(&String::from_utf8_lossy(
        &vfs.read(path)?,
    ))?))
}

/// `n` (from 1) among the `kept`: its new number (from 1).
fn new_number(kept: &[u32], n: u32) -> Option<u32> {
    kept.iter().position(|&k| k == n).map(|i| i as u32 + 1)
}

/// Removes the blocks of section `name` at `gone` (numbers from 1 among
/// those `pick` takes, in order).
fn drop_blocks(res: &mut ResTxt, name: &str, gone: &[u32], pick: &dyn Fn(&Block) -> bool) {
    let Some(s) = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case(name))
    else {
        return;
    };
    let mut n = 0;
    s.blocks.retain(|b| {
        if !pick(b) {
            return true;
        }
        n += 1;
        !gone.contains(&n)
    });
}

/// Removes the `prices` lines of the left-out parts (the n-th `area`,
/// `dino`, `weapon` line).
fn drop_prices(m: &mut ResTxt, s: &Skip) {
    let Some(p) = m
        .sections
        .iter_mut()
        .find(|x| x.name.eq_ignore_ascii_case("prices"))
    else {
        return;
    };
    let (mut a, mut d, mut w) = (0, 0, 0);
    p.entries
        .retain(|e| match e.key.to_ascii_lowercase().as_str() {
            "area" => {
                a += 1;
                !s.areas.contains(&a)
            }
            "dino" => {
                d += 1;
                !s.huntables.contains(&d)
            }
            "weapon" => {
                w += 1;
                !s.weapons.contains(&w)
            }
            _ => true,
        });
}

fn is_huntable(b: &Block) -> bool {
    b.get(&["ai"]).map_or(0, c_atoi) >= 10
}

/// The layer leaving `skip`'s parts out of the game in `vfs`; `None` when
/// nothing is left out.
pub fn layer(vfs: &Vfs, skip: &Skip) -> Result<Option<MemBackend>> {
    if skip.is_empty() {
        return Ok(None);
    }
    let mut res = parse(vfs, RES)?.ok_or_else(|| super::fail("the game has no _RES.TXT"))?;
    let mut menu = parse(vfs, MENU)?;
    let gm = MenuData::from_res(menu.as_ref().unwrap_or(&res));
    let kept_areas = Skip::kept(&skip.areas, gm.areas.len());
    let kept_dinos = Skip::kept(&skip.huntables, gm.huntables.len());
    let kept_weapons = Skip::kept(&skip.weapons, gm.weapons.len());
    if kept_areas.is_empty() {
        return Err(super::fail(
            "every area is left out: keep one, or combine a game that brings its own",
        ));
    }
    let area = |n: u32| new_number(&kept_areas, n);
    // Slots count from 0, the picker's numbers from 1.
    let slot = |n: u32| new_number(&kept_dinos, n + 1).map(|g| g - 1);
    let gone_areas = skip.areas.clone();
    let gone_slots: Vec<u32> = skip.huntables.iter().map(|n| n - 1).collect();
    let gone_names: Vec<String> = gm
        .huntables
        .iter()
        .enumerate()
        .filter(|(i, _)| skip.huntables.contains(&(*i as u32 + 1)))
        .map(|(_, h)| h.name.to_ascii_lowercase())
        .collect();
    let moves = Moves {
        area: &area,
        slot: &slot,
        spawn_offset: 0,
        pack_offset: 0,
        hunter: &[],
    };
    let fix = |b: &mut Block| {
        drop_refs(b, &gone_areas, &gone_slots);
        moves.block(b);
    };

    // _RES.TXT: the left-out animals' characters (by name) and weapons go;
    // the rest is renumbered.
    let left_out = |b: &Block| {
        b.get(&["ai"]).map_or(0, c_atoi) != 0
            && b.get(&["name"])
                .is_some_and(|n| gone_names.contains(&unquote(n).to_ascii_lowercase()))
    };
    for s in &mut res.sections {
        let n = s.name.to_ascii_lowercase();
        if n == "characters" {
            s.blocks.retain(|b| !left_out(b));
            s.blocks.iter_mut().for_each(&fix);
        } else if n.contains("spawntable") || n.contains("packtable") {
            for sub in &mut s.subs {
                fix(&mut sub.block);
            }
        }
    }
    drop_blocks(&mut res, "weapons", &skip.weapons, &|_| true);
    // _MENU.TXT (else the menu's lists in _RES.TXT): the entries and prices.
    match &mut menu {
        Some(m) => {
            drop_blocks(m, "characters", &skip.huntables, &is_huntable);
            drop_blocks(m, "weapons", &skip.weapons, &|_| true);
            for s in &mut m.sections {
                if s.name.eq_ignore_ascii_case("characters") {
                    s.blocks.iter_mut().for_each(&fix);
                }
            }
            drop_prices(m, skip);
        }
        None => drop_prices(&mut res, skip),
    }

    let mut layer = MemBackend::new();
    layer.insert(RES, res.write().into_bytes());
    if let Some(m) = &menu {
        layer.insert(MENU, m.write().into_bytes());
    }
    // Files numbered after an area, huntable or weapon move to the kept
    // one's new number (left-out ones are no longer read).
    let to = |kept: &[u32]| {
        let kept = kept.to_vec();
        move |n: usize| new_number(&kept, n as u32).map(|g| g as usize)
    };
    let (by_area, by_dino, by_weapon) = (to(&kept_areas), to(&kept_dinos), to(&kept_weapons));
    let mut moved: Vec<(String, String)> = Vec::new();
    for (k, r) in vfs.files() {
        let (dir, fname) = r.path.rsplit_once('/').unwrap_or(("", &r.path));
        let renamed = if let Some(sub) = k.strip_prefix("huntdat/areas/") {
            let ext = fname.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            renumber(fname, "area", &by_area)
                .filter(|_| (ext == "map" || ext == "rsc") && !sub.contains('/'))
        } else if k.starts_with("huntdat/menu/") {
            renumber(fname, "dino", &by_dino)
                .or_else(|| renumber(fname, "call", &by_dino))
                .or_else(|| renumber(fname, "weapon", &by_weapon))
                .or_else(|| renumber(fname, "area", &by_area))
        } else if k.starts_with("huntdat/soundfx/calls/") {
            renumber(fname, "call", &by_dino)
        } else {
            None
        };
        if let Some(f) = renamed.filter(|f| !f.eq_ignore_ascii_case(fname)) {
            moved.push((r.path.clone(), format!("{dir}/{f}")));
        }
    }
    for (from, to) in moved {
        layer.insert(&to, vfs.read(&from)?);
    }
    Ok(Some(layer))
}

/// Pushes [`layer`] over `vfs`: whether anything was left out.
pub fn push(vfs: &mut Vfs, skip: &Skip) -> Result<bool> {
    match layer(vfs, skip)? {
        Some(l) => {
            vfs.push("left out", Box::new(l));
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Vfs {
        let mut m = MemBackend::new();
        m.insert(
            RES,
            b"weapons {\n{\n name = 'P'\n}\n{\n name = 'Q'\n}\n}\n\
characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'A'\n ai = 10\n}\n\
{\n name = 'B'\n ai = 11\n overwrite area2 {\n  health = 3\n }\n addition char0 {\n  x = 1\n }\n}\n\
{\n name = 'C'\n ai = 12\n callNo = 2\n overwrite area3 char2 {\n  health = 9\n }\n}\n}\n\
prices {\n start = 100\n area = 10\n area = 20\n area = 30\n dino = 1\n dino = 2\n dino = 3\n\
 weapon = 5\n weapon = 6\n}\n.\n"
                .to_vec(),
        );
        for n in 1..=3 {
            m.insert(&format!("HUNTDAT/AREAS/AREA{n}.MAP"), vec![n as u8]);
            m.insert(&format!("HUNTDAT/MENU/DINO{n}.TGA"), vec![10 + n as u8]);
        }
        let mut v = Vfs::new();
        v.push("game", Box::new(m));
        v
    }

    #[test]
    fn left_out_parts_go_and_the_rest_moves_up() {
        let mut v = game();
        let skip = Skip {
            areas: vec![2],
            huntables: vec![1],
            weapons: vec![1],
        };
        let l = layer(&v, &skip).unwrap().expect("a layer");
        v.push("leave out", Box::new(l));
        let res = ResTxt::parse(&String::from_utf8_lossy(&v.read(RES).unwrap())).unwrap();
        let gm = MenuData::from_res(&res);
        let names: Vec<&str> = gm.huntables.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["B", "C"]);
        assert_eq!(gm.weapons.len(), 1);
        assert_eq!(gm.weapons[0].name, "Q");
        assert_eq!(gm.weapons[0].price, 6);
        assert_eq!(
            gm.areas.iter().map(|a| a.price).collect::<Vec<_>>(),
            [10, 30]
        );
        assert_eq!(gm.huntables[0].price, 2);
        let text = String::from_utf8_lossy(&v.read(RES).unwrap()).to_string();
        // B's area2 condition and char0 condition named only left-out parts.
        assert!(
            !text.contains("health = 3") && !text.contains("x = 1"),
            "{text}"
        );
        // C: area3 is now area2, slot 2 is now slot 1.
        assert!(text.contains("area2 char1"), "{text}");
        assert!(text.contains("callNo = 1"), "{text}");
        // Area 3's map is now area 2's; huntable 2's picture is now 1's.
        assert_eq!(v.read("HUNTDAT/AREAS/AREA2.MAP").unwrap(), [3]);
        assert_eq!(v.read("HUNTDAT/MENU/DINO1.TGA").unwrap(), [12]);
        assert_eq!(v.read("HUNTDAT/MENU/DINO2.TGA").unwrap(), [13]);
        assert!(layer(&game(), &Skip::default()).unwrap().is_none());
    }

    #[test]
    fn the_base_parts_are_read_from_the_whole_game() {
        let p = crate::standalone::parts_in(&game()).expect("a game");
        assert_eq!(p.areas, 3);
        assert_eq!(p.huntables, ["A", "B", "C"]);
        assert_eq!(p.weapons, ["P", "Q"]);
    }

    #[test]
    fn some_area_must_be_kept() {
        let all = Skip {
            areas: vec![1, 2, 3],
            ..Default::default()
        };
        assert!(layer(&game(), &all).is_err());
        let mut v = game();
        assert!(!push(&mut v, &Skip::default()).unwrap());
        let one = Skip {
            weapons: vec![2],
            ..Default::default()
        };
        assert!(push(&mut v, &one).unwrap());
        assert_eq!(crate::standalone::parts_in(&v).unwrap().weapons, ["P"]);
    }
}
