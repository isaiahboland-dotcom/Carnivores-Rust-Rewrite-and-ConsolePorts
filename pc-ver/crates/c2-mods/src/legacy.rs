//! `_RES.TXT` written for the Modder's Edition before 1.0.5, read as the
//! current dialect. Those releases (and games built on them) differ in
//! four ways, none of which the current engine or ours reads:
//!
//! - Animals are split over `oldambients`, `corpseambients`, `characters`
//!   and `mapambients` sections. Since 1.0.5 "all creatures are defined
//!   within the characters section" (manual 1.1.1).
//! - `clone` names the AI ("As of 1.0.5, AI should be used to set a
//!   creatures AI, not Clone"). Map ambients carry only `clone`, and the
//!   families added by the dialect have negative numbers there.
//! - A map ambient's `spawninfo { spawnrate spawnmax spawnmin xmin ... }`
//!   is a spawn group of its own, not a link to a `spawntable` group; its
//!   pack size is `packMin` / `packMax` / `packDensity` beside it.
//! - Idles are listed as `idleAnim` lines, not `idlegroup` / `fishIdleAnim`.
//!
//! No source or manual of those releases is at hand. The negative `clone`
//! numbers are read from the entries that use them: -3 with depths (fish),
//! -4 with depths and a jump (Mosasaurus), -5 and -6 brachiosaurs that
//! kill. That is the current AI list from 30 on in order (30 brachiosaur,
//! 31 Ichthyornis, 32 fish, 33 Mosasaurus, 34 dangerous brachiosaur, 35
//! land brachiosaur), so -n is taken as 29 + n; -1 and -2 are unseen.

use c2_formats::res_txt::{c_atoi, Block, Entry, ResTxt, Sub};
use c2_vfs::{MemBackend, Vfs};

use crate::detect::{read_res, RES_TXT};
use crate::ModError;

/// The layer [`layer`] makes.
pub const LAYER: &str = "older Modder's Edition rules";

/// Sections of animals that are `characters` entries today, in the order
/// they are appended. `corpseambients` (AI 6-9, bodies lying about) has no
/// counterpart: those numbers are the pig, poacher and dog now.
const MOVED: [&str; 2] = ["oldambients", "mapambients"];
const DROPPED: &str = "corpseambients";

/// Idle chances for an `idlegroup` made from `idleAnim` lines: those the
/// Modder's Edition's own rules give most of its grazers.
const IDLE_CHANCES: [(&str, &str); 2] = [("startChance", "0.03125"), ("endChance", "0.5")];

/// The AI a `clone` number means (see the module notes for negatives).
pub fn ai_of_clone(clone: i32) -> i32 {
    if clone < 0 {
        29_i32.saturating_sub(clone)
    } else {
        clone
    }
}

/// A block's lines and sub-blocks in reading order.
enum Item {
    Line(Entry),
    Sub(String, Block),
}

fn items(b: &Block) -> Vec<Item> {
    let mut out = Vec::with_capacity(b.entries.len() + b.subs.len());
    let mut subs = b.subs.iter().peekable();
    for i in 0..=b.entries.len() {
        while let Some(s) = subs.next_if(|s| s.at <= i) {
            out.push(Item::Sub(s.name.clone(), s.block.clone()));
        }
        if let Some(e) = b.entries.get(i) {
            out.push(Item::Line(e.clone()));
        }
    }
    out
}

fn block(items: Vec<Item>) -> Block {
    let mut b = Block::default();
    for it in items {
        match it {
            Item::Line(e) => b.entries.push(e),
            Item::Sub(name, block) => b.subs.push(Sub {
                name,
                at: b.entries.len(),
                block,
            }),
        }
    }
    b
}

fn line(key: &str, value: impl Into<String>) -> Item {
    Item::Line(Entry {
        key: key.into(),
        value: value.into(),
    })
}

fn is(key: &str, name: &str) -> bool {
    key.trim().eq_ignore_ascii_case(name)
}

/// An old `spawninfo` (a group of its own, not a link to one).
fn own_group(name: &str, b: &Block) -> bool {
    is(name, "spawninfo")
        && b.get(&["spawngroup"]).is_none()
        && ["spawnmax", "spawnmin", "spawnrate"]
            .iter()
            .any(|k| b.get(&[k]).is_some())
}

const BOUNDS: [&str; 4] = ["xmin", "xmax", "ymin", "ymax"];
const PACK_KEYS: [&str; 3] = ["packMin", "packMax", "packDensity"];

/// An old `spawninfo` as a `spawngroup`, its bounds as a `region`, inside
/// a `packgroup` of `pack`'s size when the animal comes in packs.
fn spawn_group(old: &Block, pack: &[Entry]) -> Item {
    let (bounds, rest): (Vec<Entry>, Vec<Entry>) = old
        .entries
        .iter()
        .cloned()
        .partition(|e| BOUNDS.iter().any(|k| is(&e.key, k)));
    let mut group: Vec<Item> = rest.into_iter().map(Item::Line).collect();
    if !bounds.is_empty() {
        group.push(Item::Sub(
            "region".into(),
            Block {
                entries: bounds,
                subs: Vec::new(),
            },
        ));
    }
    let group = Item::Sub("spawngroup".into(), block(group));
    let packs = pack
        .iter()
        .any(|e| is(&e.key, "packMax") && c_atoi(&e.value) > 1);
    if !packs {
        return group;
    }
    let mut p: Vec<Item> = pack.iter().cloned().map(Item::Line).collect();
    p.push(group);
    Item::Sub("packgroup".into(), block(p))
}

/// Rewrites the old `spawninfo` blocks under `b`, at any depth. Returns
/// whether there were any.
fn spawns(b: &mut Block, pack: &[Entry]) -> bool {
    let mut changed = false;
    let out = items(b)
        .into_iter()
        .map(|it| match it {
            Item::Sub(name, inner) if own_group(&name, &inner) => {
                changed = true;
                spawn_group(&inner, pack)
            }
            Item::Sub(name, mut inner) => {
                changed |= spawns(&mut inner, pack);
                Item::Sub(name, inner)
            }
            line => line,
        })
        .collect();
    if changed {
        *b = block(out);
    }
    changed
}

/// One animal's entry in today's keys. Returns whether it changed.
pub fn modernize(b: &mut Block) -> bool {
    let mut changed = false;
    // `clone` is the AI where no `ai` says so.
    if b.get(&["ai"]).is_none() {
        if let Some(e) = b.entries.iter_mut().find(|e| is(&e.key, "clone")) {
            e.value = ai_of_clone(c_atoi(&e.value)).to_string();
            e.key = "ai".into();
            changed = true;
        }
    }
    let ai = b.get(&["ai"]).map(c_atoi);
    // Fish and Mosasaurus (`AnimateFish`).
    let fish = matches!(ai, Some(32 | 33));

    // Idles: a fish's are `fishIdleAnim` lines, the others' one group
    // where the first stood (animations are numbered in reading order).
    if b.entries.iter().any(|e| is(&e.key, "idleAnim")) {
        changed = true;
        if fish {
            for e in b.entries.iter_mut().filter(|e| is(&e.key, "idleAnim")) {
                e.key = "fishIdleAnim".into();
            }
        } else {
            let mut group: Vec<Item> = b
                .all("idleAnim")
                .map(|v| line("anim", v.to_string()))
                .collect();
            group.extend(IDLE_CHANCES.iter().map(|(k, v)| line(k, *v)));
            let mut group = Some(Item::Sub("idlegroup".into(), block(group)));
            let out = items(b)
                .into_iter()
                .filter_map(|it| match it {
                    Item::Line(e) if is(&e.key, "idleAnim") => group.take(),
                    other => Some(other),
                })
                .collect();
            *b = block(out);
        }
    }

    // Spawn groups of its own, with its pack size. The pack keys stay
    // where no such group takes them (a menu animal's).
    let pack: Vec<Entry> = b
        .entries
        .iter()
        .filter(|e| PACK_KEYS.iter().any(|k| is(&e.key, k)))
        .cloned()
        .collect();
    if spawns(b, &pack) {
        changed = true;
        let out = items(b)
            .into_iter()
            .filter(|it| match it {
                Item::Line(e) => !PACK_KEYS.iter().any(|k| is(&e.key, k)),
                Item::Sub(..) => true,
            })
            .collect();
        *b = block(out);
    }
    changed
}

/// `res` with its animals in one `characters` section and today's keys.
/// `None` when nothing in it is from an older release.
pub fn to_current(res: &ResTxt) -> Option<ResTxt> {
    let named = |s: &c2_formats::res_txt::Section, n: &str| s.name.trim().eq_ignore_ascii_case(n);
    let old_section =
        |s: &c2_formats::res_txt::Section| MOVED.iter().any(|n| named(s, n)) || named(s, DROPPED);
    let mut out = res.clone();
    let mut changed = out.sections.iter().any(old_section);
    let mut moved = Vec::new();
    for name in MOVED {
        for s in out.sections.iter().filter(|s| named(s, name)) {
            moved.extend(s.blocks.iter().cloned());
        }
    }
    out.sections.retain(|s| !old_section(s));
    if !moved.is_empty() && out.section("characters").is_none() {
        out.sections.push(c2_formats::res_txt::Section {
            name: "characters".into(),
            blocks: Vec::new(),
            entries: Vec::new(),
            subs: Vec::new(),
        });
    }
    if let Some(sec) = out.sections.iter_mut().find(|s| named(s, "characters")) {
        // Appended, so the animals already in `characters` keep their
        // indices (saved trophies name them).
        sec.blocks.extend(moved);
        for b in &mut sec.blocks {
            changed |= modernize(b);
        }
    }
    changed.then_some(out)
}

/// A game folder's `_RES.TXT` in today's dialect, when it was written for
/// an older Modder's Edition.
pub fn layer(vfs: &Vfs) -> Result<Option<MemBackend>, ModError> {
    let Ok(data) = vfs.read(RES_TXT) else {
        return Ok(None);
    };
    let res = read_res(&data)?;
    // An original-engine game has nothing of this, and keys named alike
    // would not mean the same there.
    if !c2_formats::mee::is_dialect(&res) {
        return Ok(None);
    }
    let Some(out) = to_current(&res) else {
        return Ok(None);
    };
    let mut m = MemBackend::new();
    m.insert("HUNTDAT/_RES.TXT", out.write().into_bytes());
    Ok(Some(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(text: &str) -> Block {
        let res = ResTxt::parse(text).expect("parses");
        res.sections[0].blocks[0].clone()
    }

    #[test]
    fn negative_clone_numbers_are_the_families_from_thirty_on() {
        assert_eq!(ai_of_clone(-3), 32);
        assert_eq!(ai_of_clone(-4), 33);
        assert_eq!(ai_of_clone(-5), 34);
        assert_eq!(ai_of_clone(-6), 35);
        assert_eq!(ai_of_clone(16), 16);
    }

    #[test]
    fn a_map_ambient_fish_gets_its_ai_idles_and_spawn_group() {
        let mut b = first(
            "characters {\n{\n name = 'Fish'\n file = 'fish.car'\n clone = -3\n walkAnim = 0\n \
             idleAnim = 2\n idleAnim = 3\n overwrite area1 {\n spawninfo {\n spawnrate = 0.5\n \
             spawnmax = 3\n spawnmin = 2\n xmin = 10\n xmax = 20\n ymin = 30\n ymax = 40\n \
             styInRgn = TRUE\n }\n }\n}\n}\n",
        );
        assert!(modernize(&mut b));
        assert_eq!(b.get(&["ai"]), Some("32"));
        assert!(b.get(&["clone"]).is_none());
        assert_eq!(b.all("fishIdleAnim").collect::<Vec<_>>(), ["2", "3"]);
        let area = b.subs_named("overwrite area1").next().expect("condition");
        assert!(area.subs_named("spawninfo").next().is_none());
        let g = area.subs_named("spawngroup").next().expect("own group");
        assert_eq!(g.get(&["spawnmax"]), Some("3"));
        assert_eq!(g.get(&["styInRgn"]), Some("TRUE"));
        assert!(g.get(&["xmin"]).is_none());
        let r = g.subs_named("region").next().expect("region");
        assert_eq!(r.get(&["xmax"]), Some("20"));
        assert_eq!(r.get(&["ymin"]), Some("30"));
    }

    #[test]
    fn a_pack_animal_spawns_as_a_pack_group_and_idles_as_one_group() {
        let mut b = first(
            "characters {\n{\n name = 'Brach'\n clone = -5\n runAnim = 7\n idleAnim = 1\n \
             idleAnim = 2\n walkAnim = 0\n overwrite area4 {\n spawninfo {\n spawnrate = 1.0\n \
             spawnmax = 4\n spawnmin = 2\n }\n }\n packMax = 4\n packMin = 2\n \
             packDensity = 250\n}\n}\n",
        );
        assert!(modernize(&mut b));
        assert_eq!(b.get(&["ai"]), Some("34"));
        assert!(b.get(&["packMax"]).is_none());
        // The group stands where the first idle did: after `runAnim`.
        let idle = b
            .subs
            .iter()
            .find(|s| s.name == "idlegroup")
            .expect("idle group");
        assert_eq!(b.entries[idle.at - 1].key, "runAnim");
        assert_eq!(b.entries[idle.at].key, "walkAnim");
        assert_eq!(idle.block.all("anim").collect::<Vec<_>>(), ["1", "2"]);
        let area = b.subs_named("overwrite area4").next().expect("condition");
        let p = area.subs_named("packgroup").next().expect("pack");
        assert_eq!(p.get(&["packMin"]), Some("2"));
        assert_eq!(p.get(&["packDensity"]), Some("250"));
        let g = p.subs_named("spawngroup").next().expect("its group");
        assert_eq!(g.get(&["spawnmin"]), Some("2"));
        assert!(g.subs.is_empty());
    }

    #[test]
    fn a_current_entry_is_left_alone() {
        let text = "characters {\n{\n name = 'Rex'\n ai = 18\n clone = 13\n packMax = 3\n \
                    spawninfo {\n spawngroup = 2\n spawnratio = 1\n }\n \
                    idlegroup {\n anim = 2\n }\n}\n}\n";
        let mut b = first(text);
        let before = b.clone();
        assert!(!modernize(&mut b));
        assert_eq!(b, before);
        assert!(to_current(&ResTxt::parse(text).expect("parses")).is_none());
    }

    #[test]
    fn ambient_sections_join_characters_after_its_own_animals() {
        let res = ResTxt::parse(
            "oldambients {\n{\n name = 'Compy'\n ai = 2\n clone = 2\n}\n}\n\
             corpseambients {\n{\n name = 'Body'\n ai = 6\n}\n}\n\
             characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Rex'\n ai = 18\n}\n}\n\
             mapambients {\n{\n name = 'Turtle'\n clone = -3\n}\n}\n",
        )
        .expect("parses");
        let out = to_current(&res).expect("an older file");
        assert_eq!(out.sections.len(), 1);
        let names: Vec<_> = out.sections[0]
            .blocks
            .iter()
            .filter_map(|b| b.get(&["name"]))
            .collect();
        assert_eq!(names, ["'Hunter'", "'Rex'", "'Compy'", "'Turtle'"]);
        assert_eq!(out.sections[0].blocks[3].get(&["ai"]), Some("32"));
    }
}
