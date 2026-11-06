//! Parts of a standalone game left out when it is combined (the launcher's
//! per-standalone picker, owner request 2026-11-05): some of its areas,
//! huntables or weapons. What is kept is renumbered with no gaps; the
//! characters of the huntables left out are not added, and conditions
//! that name only left-out areas or huntables go with them.

use c2_formats::res_txt::{c_atoi, Block};
use serde::{Deserialize, Serialize};

/// Numbers from 1, in the game's own menu order (`AREA2`, its first
/// huntable, its third weapon...).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Skip {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub huntables: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub weapons: Vec<u32>,
}

impl Skip {
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty() && self.huntables.is_empty() && self.weapons.is_empty()
    }

    /// As a command-line value: `area2,dino1,weapon3`.
    pub fn to_arg(&self) -> String {
        let mut words = Vec::new();
        for (prefix, v) in [
            ("area", &self.areas),
            ("dino", &self.huntables),
            ("weapon", &self.weapons),
        ] {
            words.extend(v.iter().map(|n| format!("{prefix}{n}")));
        }
        words.join(",")
    }

    /// [`Self::to_arg`] read back; words it doesn't know are an error.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut out = Skip::default();
        for w in s.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            let l = w.to_ascii_lowercase();
            let (list, digits) = if let Some(d) = l.strip_prefix("area") {
                (&mut out.areas, d)
            } else if let Some(d) = l.strip_prefix("dino") {
                (&mut out.huntables, d)
            } else if let Some(d) = l.strip_prefix("weapon") {
                (&mut out.weapons, d)
            } else {
                return Err(format!("not area/dino/weapon: {w:?}"));
            };
            match digits.parse::<u32>() {
                Ok(n) if n >= 1 => list.push(n),
                _ => return Err(format!("no number from 1 in {w:?}")),
            }
        }
        Ok(out)
    }

    /// The numbers 1..=`count` kept from `left_out`.
    pub fn kept(left_out: &[u32], count: usize) -> Vec<u32> {
        (1..=count as u32)
            .filter(|n| !left_out.contains(n))
            .collect()
    }
}

/// What a standalone game has to pick from: its areas with maps, its
/// huntables' and weapons' names, in its menu order (as [`super::build`]
/// numbers them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts {
    pub areas: usize,
    pub huntables: Vec<String>,
    pub weapons: Vec<String>,
}

/// [`Parts`] of the game in `dir`; `None` if it isn't one.
pub fn parts(dir: &std::path::Path) -> Option<Parts> {
    use super::find_ci;
    let root = crate::pack::find_game_root(dir).and_then(|r| find_ci(&r, "HUNTDAT"))?;
    let res = super::read_res(&find_ci(&root, "_RES.TXT")?).ok()?;
    let menu = match find_ci(&root, "_MENU.TXT") {
        Some(p) => super::read_res(&p).ok()?,
        None => res.clone(),
    };
    let gm = c2_formats::menu_txt::MenuData::from_res(&menu);
    let maps = find_ci(&root, "AREAS");
    let has_map = |n: usize| {
        maps.as_ref()
            .is_some_and(|d| find_ci(d, &format!("AREA{n}.MAP")).is_some())
    };
    Some(Parts {
        areas: (1..=gm.areas.len()).take_while(|&n| has_map(n)).count(),
        huntables: gm.huntables.iter().map(|h| h.name.clone()).collect(),
        weapons: super::blocks(&res, "weapons")
            .iter()
            .map(|b| b.get(&["name"]).map(super::unquote).unwrap_or_default())
            .collect(),
    })
}

/// [`Parts`] of the base game itself: `bases` (the game, then its
/// overlays) read as one, as [`super::leave_out`] numbers them. `None` if
/// there is no game there.
pub fn base_parts(bases: &[std::path::PathBuf]) -> Option<Parts> {
    let vfs = crate::vanilla(bases).ok()?;
    parts_in(&vfs)
}

/// [`Parts`] of the game in `vfs`.
pub fn parts_in(vfs: &c2_vfs::Vfs) -> Option<Parts> {
    let read = |p: &str| {
        vfs.resolve(p)?;
        let data = vfs.read(p).ok()?;
        c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(&data)).ok()
    };
    let res = read("HUNTDAT/_RES.TXT")?;
    let menu = read("HUNTDAT/_MENU.TXT");
    let gm = c2_formats::menu_txt::MenuData::from_res(menu.as_ref().unwrap_or(&res));
    let has_map = |n: usize| vfs.resolve(&format!("HUNTDAT/AREAS/AREA{n}.MAP")).is_some();
    Some(Parts {
        areas: (1..=gm.areas.len()).take_while(|&n| has_map(n)).count(),
        huntables: gm.huntables.iter().map(|h| h.name.clone()).collect(),
        weapons: super::blocks(&res, "weapons")
            .iter()
            .map(|b| b.get(&["name"]).map(super::unquote).unwrap_or_default())
            .collect(),
    })
}

/// Takes the references to left-out areas (`areaN`, numbers from 1; the
/// engine's `area0` is area 10) and huntable slots (`charN`, `callNo`,
/// `fearCall`, `dontFear`: numbers from 0) out of `b`, recursively. A
/// condition that named some and is left naming none of that kind is
/// dropped whole: it can no longer hold.
pub(super) fn drop_refs(b: &mut Block, areas: &[u32], slots: &[u32]) {
    if areas.is_empty() && slots.is_empty() {
        return;
    }
    b.entries.retain(|e| {
        let k = e.key.to_ascii_lowercase();
        let slot_key = k == "callno" || k == "fearcall" || k == "dontfear";
        !(slot_key && u32::try_from(c_atoi(&e.value)).is_ok_and(|v| slots.contains(&v)))
    });
    b.subs.retain_mut(|s| {
        if super::is_condition(&s.name) {
            match header(&s.name, areas, slots) {
                Some(h) => s.name = h,
                None => return false,
            }
        }
        drop_refs(&mut s.block, areas, slots);
        true
    });
}

/// A condition header without the left-out words; `None` when a kind
/// (areas or huntables) it named is left with none.
fn header(h: &str, areas: &[u32], slots: &[u32]) -> Option<String> {
    let (mut had_area, mut has_area, mut had_char, mut has_char) = (false, false, false, false);
    let words: Vec<&str> = h
        .split_whitespace()
        .filter(|w| {
            let l = w.to_ascii_lowercase();
            let num = |p: &str| l.strip_prefix(p).and_then(|d| d.parse::<u32>().ok());
            if let Some(n) = num("area") {
                let n = if n == 0 { 10 } else { n };
                had_area = true;
                let keep = !areas.contains(&n);
                has_area |= keep;
                return keep;
            }
            if let Some(n) = num("char") {
                had_char = true;
                let keep = !slots.contains(&n);
                has_char |= keep;
                return keep;
            }
            true
        })
        .collect();
    ((!had_area || has_area) && (!had_char || has_char)).then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_round_trip_and_reject_unknown_words() {
        let s = Skip {
            areas: vec![2],
            huntables: vec![1, 4],
            weapons: vec![3],
        };
        assert_eq!(s.to_arg(), "area2,dino1,dino4,weapon3");
        assert_eq!(Skip::parse(&s.to_arg()).unwrap(), s);
        assert!(Skip::parse("map2").is_err());
        assert!(Skip::parse("dino0").is_err());
        assert!(Skip::parse("").unwrap().is_empty());
    }

    #[test]
    fn conditions_lose_left_out_words_or_go() {
        assert_eq!(
            header("overwrite area1 area2 char3", &[2], &[]).as_deref(),
            Some("overwrite area1 char3")
        );
        assert_eq!(header("addition area2", &[2], &[]), None);
        assert_eq!(header("overwrite char0", &[], &[0]), None);
        assert_eq!(
            header("overwrite area0", &[], &[]).as_deref(),
            Some("overwrite area0")
        );
        assert_eq!(header("overwrite area0", &[10], &[]), None);
        assert_eq!(Skip::kept(&[2], 4), vec![1, 3, 4]);
    }
}
