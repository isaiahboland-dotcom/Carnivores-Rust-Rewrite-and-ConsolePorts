//! The Modder's Edition Engine `_RES.TXT` dialect: conditional blocks.
//!
//! `overwrite <conds> { ... }` and `addition <conds> { ... }` may appear in
//! any block (characters, weapons, spawn and pack groups). Whether one
//! applies depends on the hunt: the area, the time of day and which menu
//! slots were picked (`Hunt/Loaders/ScriptParser.cpp: ReadCharacters` in the
//! modernized fork, the same in upstream `Resources.cpp`). Conditions are
//! matched the way the engine does, by substring:
//! - if the header contains `area`, it must contain `area<n>` for the hunt's
//!   area (area 10 is written `area0`). As in the engine, `area10` also
//!   contains `area1`, so it applies on area 1 too;
//! - if it contains `time`, it must contain `time<t>` (0 dawn, 1 day,
//!   2 night);
//! - if it contains `char`, some `char<i>` must name a picked slot.
//!
//! In the trophy room the engine skips these checks, so every block
//! applies.
//!
//! An `overwrite` replaces: the first sub-block of each kind it brings
//! (`spawninfo`, `idlegroup`, ...) wipes that kind's earlier ones, and so
//! does the first value of a list key (`lookAnim`, ...); plain values are
//! simply set again (the last one wins). An `addition` only adds.
//! [`resolve`] applies all of this and returns blocks without conditions.

use crate::res_txt::{Block, Entry, Sub};

/// What a hunt is: the conditions blocks are tested against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunt {
    /// 1-based area number (10 = the tenth); `None` = the trophy room.
    pub area: Option<u32>,
    /// 0 dawn, 1 day, 2 night.
    pub time: u8,
    /// Picked menu slots, bit `i` = slot `i` (`char<i>`).
    pub picked: u32,
}

/// Keys whose repeated values form a list (wiped by an `overwrite`).
const LIST_KEYS: [&str; 5] = [
    "lookanim",
    "fishidleanim",
    "smellanim",
    "roaranim",
    "waterdanim",
];

/// Which list a sub-block or key belongs to, for wiping.
fn kind(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        // `spawngroup` inside a character also replaces its spawn infos, and
        // `packgroup` its pack memberships (`spawnGroupOverwrite`,
        // `memberOverwrite`).
        "spawninfo" | "spawngroup" => "spawn".into(),
        "packinfo" | "packgroup" => "pack".into(),
        "fishidleanim" => "lookanim".into(),
        _ => n,
    }
}

/// `Some(true)` for `overwrite`, `Some(false)` for `addition`, `None` for an
/// ordinary block.
fn conditional(name: &str) -> Option<bool> {
    if name.contains("overwrite") {
        Some(true)
    } else if name.contains("addition") {
        Some(false)
    } else {
        None
    }
}

/// Whether a conditional block's header matches the hunt.
pub fn applies(header: &str, h: &Hunt) -> bool {
    let Some(area) = h.area else {
        return true;
    };
    if header.contains("area") && !header.contains(&format!("area{}", area % 10)) {
        return false;
    }
    if header.contains("time") && !header.contains(&format!("time{}", h.time)) {
        return false;
    }
    if header.contains("char")
        && !(0..10).any(|i| h.picked & (1 << i) != 0 && header.contains(&format!("char{i}")))
    {
        return false;
    }
    true
}

/// `b` with every applicable conditional block merged in and the others
/// dropped, recursively.
pub fn resolve(b: &Block, h: &Hunt) -> Block {
    let mut out = Block::default();
    let mut subs = b.subs.iter().peekable();
    for i in 0..=b.entries.len() {
        while let Some(s) = subs.next_if(|s| s.at <= i) {
            match conditional(&s.name) {
                Some(over) if applies(&s.name, h) => merge(&mut out, &resolve(&s.block, h), over),
                Some(_) => {}
                None => out.subs.push(Sub {
                    name: s.name.clone(),
                    at: out.entries.len(),
                    block: resolve(&s.block, h),
                }),
            }
        }
        if let Some(e) = b.entries.get(i) {
            out.entries.push(e.clone());
        }
    }
    out
}

/// Adds `inner` (already resolved) to `out`; an `overwrite` first wipes
/// each kind of list it brings.
fn merge(out: &mut Block, inner: &Block, overwrite: bool) {
    let mut wiped: Vec<String> = Vec::new();
    let mut wipe = |out: &mut Block, k: String, is_sub: bool| {
        if !overwrite || wiped.contains(&k) {
            return;
        }
        if is_sub {
            out.subs.retain(|s| kind(&s.name) != k);
        } else {
            // Earlier sub-blocks move up by the entries removed before them.
            let gone: Vec<usize> = (0..out.entries.len())
                .filter(|&i| kind(&out.entries[i].key) == k)
                .collect();
            for s in &mut out.subs {
                s.at -= gone.iter().filter(|&&i| i < s.at).count();
            }
            out.entries.retain(|e| kind(&e.key) != k);
        }
        wiped.push(k);
    };
    let mut subs = inner.subs.iter().peekable();
    for i in 0..=inner.entries.len() {
        while let Some(s) = subs.next_if(|s| s.at <= i) {
            wipe(out, kind(&s.name), true);
            out.subs.push(Sub {
                name: s.name.clone(),
                at: out.entries.len(),
                block: s.block.clone(),
            });
        }
        if let Some(e) = inner.entries.get(i) {
            let k = kind(&e.key);
            if LIST_KEYS.contains(&k.as_str()) {
                wipe(out, k, false);
            }
            out.entries.push(Entry {
                key: e.key.clone(),
                value: e.value.clone(),
            });
        }
    }
}
