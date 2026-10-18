//! Entry-level `_RES.TXT` merging.
//!
//! Each mod's `_RES.TXT` is diffed against vanilla into changes:
//! - blocks (`weapons`, `characters`) are matched by `name`; a changed block is
//!   a modification of that vanilla slot. A renamed block in the same position
//!   as an unmatched vanilla one *replaces* that slot: slots matter, because
//!   saves record trophies by character index and the menu lists dinosaurs by
//!   AI. Anything else is an addition, appended.
//! - bare entries (`prices`) are matched by key and occurrence (`dino` #3).
//!
//! Changes from all mods are then applied in load order. Two mods changing
//! the same slot, entry or added id differently is a conflict: the later mod
//! wins and the report says so.

use c2_formats::res_txt::{Block, Entry, ResTxt, Section};

/// A block's id: its `name`, unquoted, lower case.
pub fn block_id(b: &Block) -> Option<String> {
    b.get(&["name"])
        .map(|n| n.trim().trim_matches('\'').trim().to_ascii_lowercase())
        .filter(|n| !n.is_empty())
}

fn norm(b: &Block) -> Vec<(String, String)> {
    b.entries
        .iter()
        .map(|e| (e.key.to_ascii_lowercase(), e.value.trim().to_string()))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A new block, appended to `section`.
    Add { section: String, block: Block },
    /// Vanilla slot `index` of `section` becomes `block` (`was` = old id when
    /// the name changed).
    Modify {
        section: String,
        index: usize,
        block: Block,
        was: Option<String>,
    },
    /// The `n`-th `key` entry of `section` becomes `value` (appended if absent).
    Entry {
        section: String,
        key: String,
        n: usize,
        value: String,
    },
}

impl Change {
    /// `(section, what)` for reports and conflict keys.
    pub fn target(&self) -> (String, String) {
        match self {
            Change::Add { section, block } => (
                section.to_ascii_lowercase(),
                format!("+{}", block_id(block).unwrap_or_default()),
            ),
            Change::Modify { section, index, .. } => {
                (section.to_ascii_lowercase(), format!("#{index}"))
            }
            Change::Entry {
                section, key, n, ..
            } => (
                section.to_ascii_lowercase(),
                format!("{}[{n}]", key.to_ascii_lowercase()),
            ),
        }
    }

    pub fn describe(&self, vanilla: &ResTxt) -> String {
        match self {
            Change::Add { section, block } => format!(
                "{section}: adds '{}'",
                block_id(block).unwrap_or_else(|| "?".into())
            ),
            Change::Modify {
                section,
                index,
                block,
                was,
            } => {
                let id = block_id(block).unwrap_or_else(|| "?".into());
                match was {
                    Some(w) => format!("{section}: replaces '{w}' (slot {index}) with '{id}'"),
                    None => {
                        let old = vanilla
                            .section(section)
                            .and_then(|s| s.blocks.get(*index))
                            .map(norm)
                            .unwrap_or_default();
                        let new = norm(block);
                        let keys: Vec<String> = new
                            .iter()
                            .filter(|e| !old.contains(e))
                            .map(|(k, _)| k.clone())
                            .collect();
                        format!("{section}: modifies '{id}' ({})", keys.join(", "))
                    }
                }
            }
            Change::Entry {
                section,
                key,
                n,
                value,
            } => format!("{section}: {key} #{} = {value}", n + 1),
        }
    }
}

/// What a mod's `_RES.TXT` changes relative to vanilla.
pub fn diff(vanilla: &ResTxt, modded: &ResTxt) -> Vec<Change> {
    let mut out = Vec::new();
    for ms in &modded.sections {
        let empty = Section {
            name: ms.name.clone(),
            blocks: Vec::new(),
            entries: Vec::new(),
        };
        let vs = vanilla.section(&ms.name).unwrap_or(&empty);
        // Blocks: match by id first.
        let vids: Vec<Option<String>> = vs.blocks.iter().map(block_id).collect();
        let mut used = vec![false; vs.blocks.len()];
        let mut unmatched = Vec::new();
        for (mi, mb) in ms.blocks.iter().enumerate() {
            let id = block_id(mb);
            let hit = (0..vs.blocks.len()).find(|&i| !used[i] && id.is_some() && vids[i] == id);
            match hit {
                Some(i) => {
                    used[i] = true;
                    if norm(&vs.blocks[i]) != norm(mb) {
                        out.push(Change::Modify {
                            section: vs.name.clone(),
                            index: i,
                            block: mb.clone(),
                            was: None,
                        });
                    }
                }
                None => unmatched.push(mi),
            }
        }
        for mi in unmatched {
            let mb = &ms.blocks[mi];
            if mi < vs.blocks.len() && !used[mi] {
                used[mi] = true;
                out.push(Change::Modify {
                    section: vs.name.clone(),
                    index: mi,
                    block: mb.clone(),
                    was: vids[mi].clone(),
                });
            } else {
                out.push(Change::Add {
                    section: ms.name.clone(),
                    block: mb.clone(),
                });
            }
        }
        // Bare entries: by key and occurrence.
        let mut seen: Vec<(String, usize)> = Vec::new();
        for e in &ms.entries {
            let k = e.key.to_ascii_lowercase();
            let n = seen.iter().filter(|(s, _)| *s == k).count();
            seen.push((k.clone(), n));
            let v = vs
                .entries
                .iter()
                .filter(|ve| ve.key.eq_ignore_ascii_case(&k))
                .nth(n);
            if v.map(|v| v.value.trim()) != Some(e.value.trim()) {
                out.push(Change::Entry {
                    section: vs.name.clone(),
                    key: e.key.clone(),
                    n,
                    value: e.value.clone(),
                });
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub section: String,
    pub what: String,
    /// Mods that changed it, in load order; the last one won.
    pub mods: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Merged {
    pub res: ResTxt,
    /// `(mod, change)` that were applied, in order.
    pub applied: Vec<(String, Change)>,
    pub conflicts: Vec<Conflict>,
}

fn section_mut<'a>(res: &'a mut ResTxt, name: &str) -> &'a mut Section {
    let i = match res
        .sections
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(name))
    {
        Some(i) => i,
        None => {
            res.sections.push(Section {
                name: name.to_string(),
                blocks: Vec::new(),
                entries: Vec::new(),
            });
            res.sections.len() - 1
        }
    };
    &mut res.sections[i]
}

/// Applies each mod's changes (in load order) to vanilla.
pub fn merge(vanilla: &ResTxt, mods: &[(String, Vec<Change>)]) -> Merged {
    let mut res = vanilla.clone();
    res.warnings.clear();
    let mut applied: Vec<(String, Change)> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();
    // Where each added id went: (section, id) -> block index.
    let mut added: Vec<((String, String), usize)> = Vec::new();
    for (m, changes) in mods {
        for c in changes {
            let target = c.target();
            // A different earlier change to the same thing is a conflict.
            if let Some((prev, _)) = applied
                .iter()
                .rev()
                .find(|(pm, pc)| pm != m && pc.target() == target && pc != c)
            {
                match conflicts
                    .iter_mut()
                    .find(|k| (k.section.clone(), k.what.clone()) == target)
                {
                    Some(k) => {
                        if !k.mods.contains(m) {
                            k.mods.push(m.clone());
                        }
                    }
                    None => conflicts.push(Conflict {
                        section: target.0.clone(),
                        what: target.1.clone(),
                        mods: vec![prev.clone(), m.clone()],
                    }),
                }
            }
            match c {
                Change::Add { section, block } => {
                    let key = (target.0.clone(), target.1.clone());
                    let s = section_mut(&mut res, section);
                    match added.iter().find(|(k, _)| *k == key) {
                        Some(&(_, i)) => s.blocks[i] = block.clone(),
                        None => {
                            s.blocks.push(block.clone());
                            added.push((key, s.blocks.len() - 1));
                        }
                    }
                }
                Change::Modify {
                    section,
                    index,
                    block,
                    ..
                } => {
                    let s = section_mut(&mut res, section);
                    if let Some(b) = s.blocks.get_mut(*index) {
                        *b = block.clone();
                    }
                }
                Change::Entry {
                    section,
                    key,
                    n,
                    value,
                } => {
                    let s = section_mut(&mut res, section);
                    let idx: Vec<usize> = s
                        .entries
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.key.eq_ignore_ascii_case(key))
                        .map(|(i, _)| i)
                        .collect();
                    match idx.get(*n) {
                        Some(&i) => s.entries[i].value = value.clone(),
                        None => {
                            let at = idx.last().map_or(s.entries.len(), |&i| i + 1);
                            s.entries.insert(
                                at,
                                Entry {
                                    key: key.clone(),
                                    value: value.clone(),
                                },
                            );
                        }
                    }
                }
            }
            applied.push((m.clone(), c.clone()));
        }
    }
    Merged {
        res,
        applied,
        conflicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VANILLA: &str = "weapons {\r\n{\r\n name = 'Pistol'\r\n power = 1\r\n}\r\n{\r\n name = 'Rifle'\r\n power = 4\r\n}\r\n}\r\ncharacters {\r\n{\r\n name = 'Hunter'\r\n ai = 0\r\n}\r\n{\r\n name = 'T-Rex'\r\n ai = 18\r\n}\r\n}\r\nprices {\r\n start = 100\r\n dino = 10\r\n weapon = 20\r\n weapon = 100\r\n}\r\n.\r\n";

    fn res(t: &str) -> ResTxt {
        ResTxt::parse(t).unwrap_or_default()
    }

    #[test]
    fn modify_replace_add_and_prices() {
        let v = res(VANILLA);
        // Stronger rifle; T-Rex slot becomes a Giganotosaurus; a new shotgun
        // with its price; start money unchanged.
        let a = res(&VANILLA
            .replace("power = 4", "power = 6")
            .replace("'T-Rex'", "'Giga'")
            .replace(
                "}\r\ncharacters",
                "{\r\n name = 'Shotgun'\r\n power = 2\r\n}\r\n}\r\ncharacters",
            )
            .replace("weapon = 100\r\n", "weapon = 100\r\n weapon = 50\r\n"));
        let d = diff(&v, &a);
        assert_eq!(d.len(), 4, "{d:?}");
        let m = merge(&v, &[("a".into(), d)]);
        assert!(m.conflicts.is_empty());
        let data = m.res.to_data();
        let names: Vec<_> = data.weapons.iter().map(|w| w.name.clone()).collect();
        assert_eq!(
            names,
            ["Pistol", "Rifle", "Shotgun"].map(|s| Some(s.to_string()))
        );
        assert_eq!(data.weapons[1].power, Some(6.0));
        assert_eq!(data.characters[1].name.as_deref(), Some("Giga"));
        let p = data.prices.unwrap_or_default();
        assert_eq!(p.weapon, vec![20, 100, 50]);
        assert_eq!(p.start, Some(100));
        // Written and re-read, the merge is stable.
        assert_eq!(res(&m.res.write()).to_data(), m.res.to_data());
    }

    #[test]
    fn two_mods_on_one_slot_conflict_and_the_last_wins() {
        let v = res(VANILLA);
        let a = res(&VANILLA.replace("power = 4", "power = 6"));
        let b = res(&VANILLA.replace("power = 4", "power = 9"));
        let c = res(&VANILLA.replace("power = 1", "power = 2"));
        let m = merge(
            &v,
            &[
                ("a".into(), diff(&v, &a)),
                ("b".into(), diff(&v, &b)),
                ("c".into(), diff(&v, &c)),
            ],
        );
        assert_eq!(
            m.conflicts,
            vec![Conflict {
                section: "weapons".into(),
                what: "#1".into(),
                mods: vec!["a".into(), "b".into()],
            }]
        );
        let w = m.res.to_data().weapons;
        assert_eq!((w[0].power, w[1].power), (Some(2.0), Some(9.0)));
        // Identical changes from two mods are not a conflict.
        let m = merge(
            &v,
            &[("a".into(), diff(&v, &a)), ("a2".into(), diff(&v, &a))],
        );
        assert!(m.conflicts.is_empty());
    }
}
