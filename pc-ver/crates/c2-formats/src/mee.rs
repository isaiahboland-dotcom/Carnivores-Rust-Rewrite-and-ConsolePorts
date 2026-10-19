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

use crate::res_txt::{c_atof, c_atoi, Block, Entry, ResTxt, Sub};

/// What a hunt is: the conditions blocks are tested against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunt {
    /// 1-based area number (10 = the tenth; 0 = a map whose name gives
    /// none, so no `area` condition matches); `None` = the trophy room.
    pub area: Option<u32>,
    /// 0 dawn, 1 day, 2 night.
    pub time: u8,
    /// Picked menu slots, bit `i` = slot `i` (`char<i>`).
    pub picked: u128,
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
    if header.contains("area") && (area == 0 || !header.contains(&format!("area{}", area % 10))) {
        return false;
    }
    if header.contains("time") && !header.contains(&format!("time{}", h.time)) {
        return false;
    }
    if header.contains("char")
        && !(0..32).any(|i| h.picked & (1 << i) != 0 && mentions_char(header, i))
    {
        return false;
    }
    true
}

/// Does `header` name menu slot `i` (`char6`)? The engine tests `char0` to
/// `char9` as substrings; with slots 10 and up (add-ons given slots of
/// their own) `char1` must not also match `char10`, so a number counts only
/// when no digit follows it.
fn mentions_char(header: &str, i: u32) -> bool {
    let pat = format!("char{i}");
    header.match_indices(&pat).any(|(at, _)| {
        !header[at + pat.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
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

// ------------------------------------------------------------ spawn tables

/// A rectangle of cells (`region{` / `avoid{`: `xmin xmax ymin ymax`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Region {
    pub xmin: i32,
    pub xmax: i32,
    pub ymin: i32,
    pub ymax: i32,
}

/// `spawngroup{`: how many of its linked packs spawn, and where.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpawnGroup {
    pub min: i32,
    pub max: i32,
    /// Chance (0..1) for each of the `max - min` extra spawns.
    pub rate: f32,
    /// Spawns more or fewer with the density option: `(dens - 128) / 128 *
    /// densityMulti`.
    pub density_multi: i32,
    /// Spawned around the hunter and moved along with him (the classic
    /// ambient animals).
    pub move_forward: bool,
    /// Pick packs at random by ratio, rather than in turn.
    pub randomise: bool,
    pub only_active_nearby: bool,
    pub stay_in_region: bool,
    /// Where to spawn; none = the whole map (cells 12..988).
    pub regions: Vec<Region>,
    pub avoid: Vec<Region>,
}

/// `spawninfo{ spawngroup ratio }`: a link to a spawn group.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpawnLink {
    pub group: usize,
    pub ratio: f32,
}

/// `packgroup{`: a pack type and the spawn groups it spawns in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackGroup {
    pub min: i32,
    pub max: i32,
    pub density: f32,
    pub spawn: Vec<SpawnLink>,
}

/// `packinfo{ group ratio }`: a character's membership of a pack type.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PackLink {
    pub group: usize,
    pub ratio: f32,
}

/// One `characters` entry after the hunt's conditions are applied.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Character {
    /// Every `key = value` (and other sub-blocks) as resolved.
    pub block: Block,
    pub spawn: Vec<SpawnLink>,
    pub packs: Vec<PackLink>,
}

/// The dialect's tables for one hunt (`LoadResourcesScript` in file order).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tables {
    pub spawn_groups: Vec<SpawnGroup>,
    pub pack_groups: Vec<PackGroup>,
    /// In `characters` order (the character type index).
    pub characters: Vec<Character>,
}

/// The area a map is, as the engine reads it from the launch path
/// (`huntdat/areas/<name>`, the character after `area`): `areaN` is N,
/// `area1x...` the tenth, `trophy...` the trophy room (`None`); the sixth
/// slot may be `external`; anything else matches no area (0).
pub fn area_of(map_stem: &str) -> Option<u32> {
    let s = map_stem.to_ascii_lowercase();
    if s.starts_with("trophy") {
        return None;
    }
    if s == "external" {
        return Some(6);
    }
    let rest = s.strip_prefix("area").unwrap_or("");
    let mut chars = rest.chars();
    Some(
        match (chars.next().and_then(|c| c.to_digit(10)), chars.next()) {
            (Some(1), Some(_)) => 10,
            (Some(d), _) if d > 0 => d,
            _ => 0,
        },
    )
}

/// Whether `_RES.TXT` uses the dialect (any of its tables, or nested
/// blocks in a character).
pub fn is_dialect(res: &ResTxt) -> bool {
    res.sections.iter().any(|s| {
        let n = s.name.to_ascii_lowercase();
        n.contains("spawntable") || n.contains("packtable") || n.contains("areatable")
    }) || res
        .section("characters")
        .is_some_and(|s| s.blocks.iter().any(|b| !b.subs.is_empty()))
}

/// `readBool`: `TRUE` sets, `FALSE` clears, anything else leaves it.
pub fn flag(b: &Block, key: &str) -> Option<bool> {
    let v = b.get(&[key])?;
    if v.contains("TRUE") {
        Some(true)
    } else if v.contains("FALSE") {
        Some(false)
    } else {
        None
    }
}

fn int(b: &Block, key: &str) -> Option<i32> {
    b.get(&[key]).map(c_atoi)
}

fn float(b: &Block, key: &str) -> Option<f32> {
    b.get(&[key]).map(c_atof)
}

/// `RecoverOrderedScriptRange`: both within 0..=256, min <= max.
fn ordered(min: i32, max: i32) -> (i32, i32) {
    let (a, b) = (min.clamp(0, 256), max.clamp(0, 256));
    (a.min(b), a.max(b))
}

fn region(b: &Block) -> Region {
    Region {
        xmin: int(b, "xmin").unwrap_or(0),
        xmax: int(b, "xmax").unwrap_or(0),
        ymin: int(b, "ymin").unwrap_or(0),
        ymax: int(b, "ymax").unwrap_or(0),
    }
}

fn spawn_group(b: &Block) -> SpawnGroup {
    let (min, max) = ordered(
        int(b, "spawnmin").unwrap_or(0),
        int(b, "spawnmax").unwrap_or(0),
    );
    SpawnGroup {
        min,
        max,
        rate: float(b, "spawnrate").unwrap_or(0.0),
        density_multi: int(b, "densityMulti").unwrap_or(0),
        move_forward: flag(b, "moveForward").unwrap_or(false),
        randomise: flag(b, "randomise").unwrap_or(false),
        only_active_nearby: flag(b, "onlyActiveNearby").unwrap_or(false),
        stay_in_region: flag(b, "styInRgn").unwrap_or(false),
        regions: b.subs_named("region").map(region).collect(),
        avoid: b.subs_named("avoid").map(region).collect(),
    }
}

fn spawn_link(b: &Block) -> SpawnLink {
    SpawnLink {
        group: int(b, "spawngroup").unwrap_or(0).max(0) as usize,
        ratio: float(b, "spawnratio").unwrap_or(0.0),
    }
}

impl Tables {
    /// Inline `spawngroup{`: a new spawn group, linked with ratio 1.
    fn inline_group(&mut self, b: &Block) -> SpawnLink {
        self.spawn_groups.push(spawn_group(b));
        SpawnLink {
            group: self.spawn_groups.len() - 1,
            ratio: 1.0,
        }
    }

    fn pack_group(&mut self, b: &Block) -> usize {
        let (min, max) = ordered(
            int(b, "packMin").unwrap_or(0),
            int(b, "packMax").unwrap_or(0),
        );
        let mut spawn = Vec::new();
        for s in &b.subs {
            match s.name.to_ascii_lowercase().as_str() {
                "spawninfo" => spawn.push(spawn_link(&s.block)),
                "spawngroup" => {
                    let l = self.inline_group(&s.block);
                    spawn.push(l);
                }
                _ => {}
            }
        }
        self.pack_groups.push(PackGroup {
            min,
            max,
            density: float(b, "packDensity").unwrap_or(0.0),
            spawn,
        });
        self.pack_groups.len() - 1
    }

    /// The tables for hunt `h`.
    pub fn read(res: &ResTxt, h: &Hunt) -> Self {
        let mut t = Tables::default();
        for sec in &res.sections {
            let name = sec.name.to_ascii_lowercase();
            if name.contains("spawntable") {
                t.spawn_groups.clear();
                for s in sec.subs.iter().filter(|s| s.name.contains("spawngroup")) {
                    let g = spawn_group(&resolve(&s.block, h));
                    t.spawn_groups.push(g);
                }
            } else if name.contains("packtable") {
                t.pack_groups.clear();
                for s in sec.subs.iter().filter(|s| s.name.contains("packgroup")) {
                    t.pack_group(&resolve(&s.block, h));
                }
            } else if name.contains("characters") {
                for b in &sec.blocks {
                    let block = resolve(b, h);
                    let mut c = Character::default();
                    for s in &block.subs {
                        match s.name.to_ascii_lowercase().as_str() {
                            "spawninfo" => c.spawn.push(spawn_link(&s.block)),
                            "spawngroup" => {
                                let l = t.inline_group(&s.block);
                                c.spawn.push(l);
                            }
                            "packinfo" => c.packs.push(PackLink {
                                group: int(&s.block, "group").unwrap_or(0).max(0) as usize,
                                ratio: float(&s.block, "ratio").unwrap_or(0.0),
                            }),
                            "packgroup" => {
                                let group = t.pack_group(&s.block);
                                c.packs.push(PackLink { group, ratio: 1.0 });
                            }
                            _ => {}
                        }
                    }
                    c.block = block;
                    t.characters.push(c);
                }
            }
        }
        // Groups without a region spawn anywhere (`LoadResourcesScript`).
        for g in &mut t.spawn_groups {
            if g.regions.is_empty() {
                g.regions.push(Region {
                    xmin: 12,
                    xmax: 988,
                    ymin: 12,
                    ymax: 988,
                });
            }
        }
        t
    }
}
