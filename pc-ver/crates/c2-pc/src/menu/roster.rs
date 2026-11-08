//! The hunt lists: animals, their descriptions, scores and spawn homes.

use super::*;

/// Filtering never changes an entry's hunt bit or picture number.
pub(super) fn animal_order(names: &[String], first: usize, all: bool) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            ((all || i < first) && seen.insert(name.trim().to_ascii_lowercase())).then_some(i)
        })
        .collect()
}

/// The hunt lists: `_MENU.TXT` if the game has one (Modder's Edition
/// Engine), else `_RES.TXT`; see `c2_formats::menu_txt`.
pub fn menu_data(huntdat: &Path) -> Result<MenuData> {
    let src = find_file(huntdat, "_MENU.TXT")
        .or_else(|| find_file(huntdat, "_RES.TXT"))
        .and_then(|p| std::fs::read(p).ok())
        .map(|d| String::from_utf8_lossy(&d).into_owned())
        .unwrap_or_default();
    Ok(MenuData::from_res(&ResTxt::parse(&src)?))
}

/// A dialect game's `_RES.TXT`.
pub fn dialect_res(huntdat: &Path) -> Option<ResTxt> {
    find_file(huntdat, "_RES.TXT")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| ResTxt::parse(&String::from_utf8_lossy(&d)).ok())
        .filter(c2_formats::mee::is_dialect)
}

/// The roaming add-ons' (character, slot): after the menu's own.
pub fn roamer_slots(huntdat: &Path, res: &ResTxt) -> Vec<(usize, usize)> {
    let first = menu_data(huntdat).map_or(0, |m| m.huntables.len());
    c2_core::mee::roamers(res, first)
}

/// Each huntable's `basescore`: its `_RES.TXT` entry (matched by name),
/// with the game's "when picked" changes applied (`overwrite charN`).
pub(super) fn hunt_scores(huntdat: &Path, menu: &MenuData) -> Vec<Option<f32>> {
    let Some(res) = find_file(huntdat, "_RES.TXT")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| ResTxt::parse(&String::from_utf8_lossy(&d)).ok())
    else {
        return vec![None; menu.huntables.len()];
    };
    let chars = res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default();
    let unquote = |v: &str| v.trim().trim_matches('\'').to_ascii_lowercase();
    menu.huntables
        .iter()
        .enumerate()
        .map(|(k, h)| {
            let name = h.name.to_ascii_lowercase();
            let b = chars
                .iter()
                .find(|b| b.get(&["name"]).map(unquote).as_deref() == Some(&name))?;
            let picked = c2_formats::mee::Hunt {
                area: Some(0),
                time: 1,
                picked: Bits::bit(k),
            };
            let b = c2_formats::mee::resolve(b, &picked);
            b.get(&["basescore"]).map(c2_formats::res_txt::c_atof)
        })
        .collect()
}

/// Dialect games: per huntable (menu slot), the areas (index in `maps`)
/// it spawns on when picked; `None` for an original game (all of them).
/// Read once per area with every slot picked.
pub(super) fn hunt_homes(
    huntdat: &Path,
    menu: &MenuData,
    maps: &[PathBuf],
) -> Option<Vec<Vec<usize>>> {
    let res = find_file(huntdat, "_RES.TXT")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| ResTxt::parse(&String::from_utf8_lossy(&d)).ok())?;
    if !c2_formats::mee::is_dialect(&res) {
        return None;
    }
    Some(spawn_homes(&res, menu.huntables.len(), maps))
}

pub(super) fn spawn_homes(res: &ResTxt, slots: usize, maps: &[PathBuf]) -> Vec<Vec<usize>> {
    let mut homes = vec![Vec::new(); slots];
    for (a, map) in maps.iter().enumerate() {
        let stem = map
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let h = c2_formats::mee::Hunt {
            area: c2_formats::mee::area_of(&stem),
            time: 1,
            picked: Bits::ALL,
        };
        let t = c2_formats::mee::Tables::read(res, &h);
        for c in &t.characters {
            if c.spawn.is_empty() && c.packs.is_empty() {
                continue;
            }
            let slot = c2_formats::mee::menu_slot(None, &c.block);
            if let Some(v) = slot.and_then(|n| homes.get_mut(n)) {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        }
    }
    homes
}

pub(super) fn read_text(dir: Option<&Path>, name: &str) -> Vec<String> {
    dir.and_then(|d| find_file(d, name))
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .map(|l| l.trim_end().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The huntables' descriptions by slot (`DINOn.TXM`, or `.TXU` for `us`);
/// a missing or blank one is made from its `_MENU.TXT` entry.
pub(super) fn dino_texts(
    huntables: &[c2_formats::menu_txt::MenuItem],
    txt: Option<&Path>,
    us: bool,
) -> Vec<Vec<String>> {
    let ext = if us { "TXU" } else { "TXM" };
    huntables
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let t = read_text(txt, &format!("DINO{}.{ext}", k + 1));
            if c2_formats::menu_txt::is_blank(&t) {
                d.fallback_text(us)
            } else {
                t
            }
        })
        .collect()
}

/// The game's name for the credits (`MOBILE/GAME.TXT`, which the APK
/// conversion writes).
pub(super) fn game_title(huntdat: &Path) -> String {
    find_file(huntdat, "MOBILE")
        .and_then(|d| find_file(&d, "GAME.TXT"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "the mobile game".into())
}

#[cfg(test)]
mod roster_tests {
    use super::{animal_order, spawn_homes};
    use c2_formats::res_txt::ResTxt;

    #[test]
    #[allow(clippy::expect_used)]
    fn spawn_locations_follow_menu_identity_instead_of_source_calls() {
        let res = ResTxt::parse(
            "characters {\n{\n ai = 13\n callNo = 0\n menuSlot = 2\n spawninfo {\n spawngroup = 0\n }\n}\n{\n ai = 13\n callNo = 1\n menuSlot = -1\n spawninfo {\n spawngroup = 0\n }\n}\n}\n.\n",
        )
        .expect("synthetic roster");
        let homes = spawn_homes(&res, 3, &["AREA1.MAP".into(), "AREA2.MAP".into()]);
        assert_eq!(homes, [vec![], vec![], vec![0, 1]]);
    }

    #[test]
    fn filtering_and_duplicate_names_keep_the_original_slot_numbers() {
        let names: Vec<_> = [
            "Parasaurolophus",
            "Mobile animal",
            " parasaurolophus ",
            "Suchomimus",
            "Atrociraptor",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(animal_order(&names, 3, false), [0, 1]);
        assert_eq!(animal_order(&names, 3, true), [0, 1, 3, 4]);
    }
}
