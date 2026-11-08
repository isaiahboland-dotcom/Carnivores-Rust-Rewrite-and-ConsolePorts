//! Games made for the original engine read as dialect games
//! ([`to_dialect`]), and default spawn tables for dialect games that have
//! none ([`spawn_layer`]).

use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, Block, Entry, ResTxt, Sub};
use c2_vfs::{MemBackend, Vfs};

use super::{blocks, header, is_condition, section_mut, set, unquote, Result};

/// A standalone made for the original engine, read as a dialect game over
/// `base` (the Modder's Edition's `_RES.TXT`): the original engine fixes
/// each AI's animation order, gait and placement, so each animal takes the
/// base's animal of the same AI's animation and movement keys (unless it
/// has its own) and its spawn blocks, its "picked" condition moved to the
/// animal's own menu slot (`callNo`, set here); the base's spawn and pack
/// tables come along, so the numbers stay right when they are moved.
///
/// An animal no base animal shares an AI with (an extra slot a patched
/// engine added, AI 19) takes the kind of the base's Allosaurus when it is
/// `danger`ous, else of its Parasaurolophus (or the closest kind the base
/// has), and its animations by their
/// names in its model (`model` reads one by its `file`); its own numbers
/// (health, mass, scores, senses) stay. The original's brachiosaur (AI 6;
/// the dialect's 6 is the pig) takes the base's brachiosaur's.
/// Returns the notes (what was decided, animals left out).
pub fn to_dialect(
    g: &ResTxt,
    base: &ResTxt,
    model: &dyn Fn(&str) -> Option<c2_formats::car::Car>,
) -> (ResTxt, Vec<String>) {
    // Entries without a name (stray blocks) aren't animals.
    let mut g = g.clone();
    let mut notes = Vec::new();
    if let Some(sec) = g
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
    {
        let before = sec.blocks.len();
        sec.blocks.retain(|b| {
            b.get(&["name"])
                .is_some_and(|n| !unquote(n).trim().is_empty())
        });
        if sec.blocks.len() < before {
            notes.push(format!(
                "{} entries without a name left out",
                before - sec.blocks.len()
            ));
        }
    }
    let g = &g;
    let mut out = g.clone();
    let donors = blocks(base, "characters");
    let ai_of = |b: &Block| b.get(&["ai"]).map(c_atoi);
    let menu = MenuData::from_res(g);
    let slot_of = |b: &Block| {
        let n = b.get(&["name"]).map(unquote)?;
        menu.huntables
            .iter()
            .position(|h| h.name.eq_ignore_ascii_case(&n))
    };
    let mut chars = Vec::new();
    for b in blocks(g, "characters") {
        let mut b = b.clone();
        let ai = ai_of(&b);
        // The hunter (AI 0) as it is.
        if ai.unwrap_or(0) == 0 {
            chars.push(b);
            continue;
        }
        let name = b.get(&["name"]).map(unquote).unwrap_or_default();
        let slot = slot_of(&b);
        let fits = |d: &Block| match ai {
            Some(6) => matches!(ai_of(d), Some(30 | 34)),
            _ => ai_of(d) == ai,
        };
        let Some(d) = donors
            .iter()
            .skip(1)
            .find(|d| fits(d) && crate::loose::has_anims(d))
        else {
            match by_names(&b, &name, slot, donors, model) {
                Ok((e, note)) => {
                    notes.push(note);
                    chars.push(e);
                }
                Err(why) => notes.push(format!("'{name}' left out: {why}")),
            }
            continue;
        };
        if ai == Some(6) {
            if let Some(a) = d.get(&["ai"]) {
                set(&mut b, "ai", a.trim().to_string());
            }
        }
        if !crate::loose::has_anims(&b) {
            crate::loose::copy_anims(d, &mut b);
        }
        b.entries.retain(|e| !e.key.eq_ignore_ascii_case("callNo"));
        if let Some(n) = slot {
            set(&mut b, "callNo", n.to_string());
        }
        // The donor's spawns, its slot's condition moved to this one's.
        let donor_slot = d.get(&["callNo"]).map(c_atoi);
        let at = b.entries.len();
        for s in d
            .subs
            .iter()
            .filter(|s| crate::loose::is_spawn_sub(&s.name) || crate::loose::has_spawns(&s.block))
        {
            let mut s = s.clone();
            if is_condition(&s.name) {
                let Some(n) = slot else { continue };
                slot_condition(&mut s, donor_slot, n);
            }
            s.at = at;
            b.subs.push(s);
        }
        chars.push(b);
    }
    section_mut(&mut out, "characters").blocks = chars;
    for name in ["spawntable", "packtable"] {
        if let Some(s) = base
            .sections
            .iter()
            .find(|s| s.name.to_ascii_lowercase().contains(name))
        {
            out.sections
                .retain(|x| !x.name.to_ascii_lowercase().contains(name));
            out.sections.push(s.clone());
        }
    }
    (out, notes)
}

/// The layer [`spawn_layer`] makes.
pub const SPAWN_LAYER: &str = "default spawn tables";

/// The Modder's Edition's three default spawn groups (its `_RES.TXT`'s
/// `spawntable`): ambients, a random roster, the picked animals.
const DEFAULT_SPAWNTABLE: &str = "spawntable {\n spawngroup {\n  spawnmax = 6\n  spawnmin = 0\n  spawnrate = 1\n  densityMulti = 2\n  moveForward = TRUE\n }\n spawngroup {\n  spawnmax = 4\n  spawnmin = 0\n  spawnrate = 1\n  randomise = TRUE\n }\n spawngroup {\n  spawnmax = 13\n  spawnmin = 12\n  spawnrate = 0.5\n  densityMulti = 4\n }\n}\n.\n";

/// For a game read in the dialect (it has its animation keys) but without
/// spawn tables, nothing would be placed: its `_RES.TXT` with the
/// Modder's Edition's default spawn groups, each ambient (AI below 10) in
/// the ambients' group and each huntable in the random roster, and in the
/// picked group when picked (`overwrite charN`, its menu slot as `callNo`).
pub fn spawn_layer(vfs: &Vfs) -> Result<Option<MemBackend>> {
    let Ok(data) = vfs.read("HUNTDAT/_RES.TXT") else {
        return Ok(None);
    };
    let mut res = ResTxt::parse(&String::from_utf8_lossy(&data))?;
    let has_table = res
        .sections
        .iter()
        .any(|s| s.name.to_ascii_lowercase().contains("spawntable"));
    if !c2_formats::mee::is_dialect(&res) || has_table {
        return Ok(None);
    }
    let menu = match vfs.read("HUNTDAT/_MENU.TXT") {
        Ok(d) => MenuData::from_res(&ResTxt::parse(&String::from_utf8_lossy(&d))?),
        Err(_) => MenuData::from_res(&res),
    };
    let spawn = |group: usize| Sub {
        name: "spawninfo".into(),
        at: 0,
        block: Block {
            entries: vec![
                Entry {
                    key: "spawngroup".into(),
                    value: group.to_string(),
                },
                Entry {
                    key: "spawnratio".into(),
                    value: "1".into(),
                },
            ],
            subs: Vec::new(),
        },
    };
    if let Some(sec) = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
    {
        for b in sec.blocks.iter_mut() {
            let ai = b.get(&["ai"]).map(c_atoi).unwrap_or(0);
            // The hunter (AI 0): first in most games; one keeps it in a
            // `hunterinfo` section of its own, its first character a
            // menu animal.
            if ai == 0 || crate::loose::has_spawns(b) {
                continue;
            }
            let at = b.entries.len();
            if ai < 10 {
                b.subs.push(Sub { at, ..spawn(0) });
                continue;
            }
            let name = b.get(&["name"]).map(unquote).unwrap_or_default();
            let slot = menu
                .huntables
                .iter()
                .position(|h| h.name.eq_ignore_ascii_case(&name));
            b.subs.push(Sub { at, ..spawn(1) });
            if let Some(n) = slot {
                if b.get(&["callNo"]).is_none() {
                    set(b, "callNo", n.to_string());
                }
                let n = b.get(&["callNo"]).map(c_atoi).unwrap_or(n as i32);
                b.subs.push(Sub {
                    name: format!("overwrite char{n}"),
                    at: b.entries.len(),
                    block: Block {
                        entries: Vec::new(),
                        subs: vec![spawn(2)],
                    },
                });
            }
        }
    }
    let table = ResTxt::parse(DEFAULT_SPAWNTABLE)?;
    res.sections.extend(table.sections);
    let mut m = MemBackend::new();
    m.insert("HUNTDAT/_RES.TXT", res.write().into_bytes());
    Ok(Some(m))
}

/// A donor's slot condition (`overwrite char5 { ... spawninfo{} }`) as one
/// for slot `n`: only its spawn part (its other keys are the donor's own).
fn slot_condition(s: &mut Sub, donor_slot: Option<i32>, n: usize) {
    s.name = header(&s.name, &Some, &|c| {
        Some(if Some(c as i32) == donor_slot {
            n as u32
        } else {
            c
        })
    });
    s.block.entries.clear();
    s.block.subs.retain(|x| crate::loose::is_spawn_sub(&x.name));
}

/// Roles of a model's animations by their names (`BR_Walk`, `Par_slp`,
/// `death`...): `None` for those that say none (trophy poses, slides).
fn roles_by_name(car: &c2_formats::car::Car) -> Vec<Option<&'static str>> {
    let mut seen: Vec<&str> = Vec::new();
    car.animations
        .iter()
        .map(|a| {
            let n = a.name().to_ascii_lowercase();
            let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
            let r = if has(&["die", "death", "dead"]) {
                "die"
            } else if has(&["slp", "sleep"]) {
                "sleep"
            } else if has(&["swim", "swm"]) {
                "swim"
            } else if has(&["jmp", "jump"]) {
                "jump"
            } else if has(&["run"]) {
                "run"
            } else if has(&["wlk", "walk"]) {
                "walk"
            } else if has(&["eat", "kill"]) {
                "eat"
            } else if has(&["roar", "scr", "growl"]) {
                "growl"
            } else if has(&["idle", "look", "see", "sml", "smell"]) {
                "idle"
            } else {
                return None;
            };
            // One walk, run, death and sleep; any number of idles.
            if r != "idle" && seen.contains(&r) {
                return None;
            }
            seen.push(r);
            Some(r)
        })
        .collect()
}

/// The keys an original-engine entry keeps over its donor's: its own size,
/// health, score and senses.
const OWN_KEYS: [&str; 14] = [
    "health",
    "mass",
    "length",
    "radius",
    "basescore",
    "smellK",
    "hearK",
    "lookK",
    "smell",
    "hear",
    "look",
    "shipdelta",
    "scale0",
    "scaleA",
];

/// An animal no base animal shares an AI with: the kind of a dangerous or
/// harmless base animal, its animations by name (see [`to_dialect`]).
fn by_names(
    b: &Block,
    name: &str,
    slot: Option<usize>,
    donors: &[Block],
    model: &dyn Fn(&str) -> Option<c2_formats::car::Car>,
) -> std::result::Result<(Block, String), String> {
    let file = b.get(&["file"]).map(unquote).ok_or("no model")?;
    let car = model(&file).ok_or_else(|| format!("its model {file} isn't there"))?;
    let roles = roles_by_name(&car);
    if !roles.contains(&Some("die")) {
        return Err("no AI of the base fits it, and its model names no death animation".into());
    }
    let danger = b
        .get(&["danger"])
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true") || c_atoi(v) != 0);
    // The Allosaurus, or the closest kind the base has (hunters / grazers).
    let kinds: &[i32] = if danger {
        &[13, 17, 16, 15, 18]
    } else {
        &[10, 12, 11, 14]
    };
    let d = kinds
        .iter()
        .find_map(|&k| {
            donors
                .iter()
                .skip(1)
                .find(|d| d.get(&["ai"]).map(c_atoi) == Some(k) && crate::loose::has_anims(d))
        })
        .ok_or("the base has no animal to take its kind from")?;
    let mut e = crate::mobile::make_entry(d, name, &file, &roles, slot.is_some());
    for k in OWN_KEYS {
        if let Some(v) = b.get(&[k]) {
            set(&mut e, k, v.trim().to_string());
        }
    }
    let donor_slot = d.get(&["callNo"]).map(c_atoi);
    e.entries.retain(|x| !x.key.eq_ignore_ascii_case("callNo"));
    if let Some(n) = slot {
        set(&mut e, "callNo", n.to_string());
        for s in e.subs.iter_mut().filter(|s| is_condition(&s.name)) {
            slot_condition(s, donor_slot, n);
        }
    }
    let donor = d.get(&["name"]).map(unquote).unwrap_or_default();
    Ok((
        e,
        format!(
            "'{name}' (AI {}): the {donor}'s kind, animations by their names",
            b.get(&["ai"]).unwrap_or("?").trim()
        ),
    ))
}
