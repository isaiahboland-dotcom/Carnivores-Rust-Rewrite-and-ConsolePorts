//! Combining standalone games (whole games made from Carnivores 2, README
//! M16) into one, beside the base game.
//!
//! The base is the game with the Modder's Edition release over it
//! ([`crate::vanilla`] with an overlay). A standalone made for the original
//! engine is first read as a dialect game ([`to_dialect`]). Each standalone
//! is then added after the base:
//! - its areas get the next area numbers (`AREA7.MAP`, its menu picture and
//!   text, its `prices` line);
//! - its huntables, weapons the next menu slots (menu pictures, texts, call
//!   sounds renumbered; `_MENU.TXT` extended at its prices);
//! - its models and other files go under `HUNTDAT/MODS/<mod>/`, weapon files
//!   into `WEAPONS/` with a `<mod>_` prefix (the game looks there by name);
//! - its `_RES.TXT` characters are appended with their paths, `areaN` /
//!   `charN` conditions, `callNo` / `fearCall`, spawn and pack table numbers
//!   moved to the combined numbering, and kill animations to the merged
//!   hunter model (`crate::hunter`); its spawn and pack tables are
//!   appended;
//! - its animals stay on its own maps ("tied"): spawning is limited to its
//!   areas (and the base's animals to the base's). Ambients can roam every
//!   map instead (`Ambients::Random`).
//! - a weapon the game already has (same name, same model) is shared, not
//!   listed again.
//!
//! Add-ons are layered on the result by the mod system as usual
//! ([`add_ons`]: kept to the base game's maps, which they were made for).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, Block, Entry, ResTxt, Section, Sub};
use c2_vfs::{key, MemBackend, Vfs};

use crate::loose::MapBackend;
use crate::ModError;

type Result<T> = std::result::Result<T, ModError>;

fn fail(msg: impl Into<String>) -> ModError {
    ModError::Standalone(msg.into())
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ModError + '_ {
    move |source| ModError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Where ambient animals (AI below 10) may live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambients {
    /// Each game's ambients only on its own maps.
    Tied,
    /// Everyone's ambients on every map.
    Random,
}

pub struct Options {
    pub ambients: Ambients,
}

/// One standalone game, read.
struct Game {
    slug: String,
    name: String,
    root: PathBuf,
    res: ResTxt,
    /// Its menu file, or its `_RES.TXT` when it has none.
    menu: ResTxt,
}

fn read_res(p: &Path) -> Result<ResTxt> {
    let d = std::fs::read(p).map_err(io(p))?;
    Ok(ResTxt::parse(&String::from_utf8_lossy(&d))?)
}

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    v.sort();
    for p in v {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
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
                name: name.into(),
                blocks: Vec::new(),
                entries: Vec::new(),
                subs: Vec::new(),
            });
            res.sections.len() - 1
        }
    };
    &mut res.sections[i]
}

/// A game's hunter entry: its character with AI 0 (the first, in most), else
/// the first of a `hunterinfo` section (one game keeps it apart, its first
/// character a menu animal).
fn game_hunter(res: &ResTxt) -> Option<&Block> {
    blocks(res, "characters")
        .iter()
        .find(|b| b.get(&["ai"]).map(c_atoi).unwrap_or(0) == 0)
        .or_else(|| blocks(res, "hunterinfo").first())
}

fn blocks<'a>(res: &'a ResTxt, name: &str) -> &'a [Block] {
    res.section(name)
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default()
}

fn table_subs<'a>(res: &'a ResTxt, section: &str, sub: &'a str) -> Vec<&'a Sub> {
    res.sections
        .iter()
        .filter(|s| s.name.to_ascii_lowercase().contains(section))
        .flat_map(|s| s.subs.iter())
        .filter(|s| s.name.to_ascii_lowercase().contains(sub))
        .collect()
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches('\'').replace('\\', "/")
}

fn set(b: &mut Block, k: &str, v: String) {
    match b.entries.iter_mut().find(|e| e.key.eq_ignore_ascii_case(k)) {
        Some(e) => e.value = v,
        None => b.entries.push(Entry {
            key: k.into(),
            value: v,
        }),
    }
}

/// `DINO7.TGA` with prefix `dino`, 7 -> n: renamed, keeping case and suffix.
fn renumber(name: &str, prefix: &str, map: &dyn Fn(usize) -> Option<usize>) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    if !lower.starts_with(prefix) {
        return None;
    }
    let rest = &name[prefix.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let to = map(digits.parse().ok()?)?;
    Some(format!(
        "{}{to}{}",
        &name[..prefix.len()],
        &rest[digits.len()..]
    ))
}

/// Renumbers `areaN` / `charN` words of a conditional header.
fn header(h: &str, area: &dyn Fn(u32) -> Option<u32>, slot: &dyn Fn(u32) -> Option<u32>) -> String {
    h.split_whitespace()
        .map(|w| {
            let l = w.to_ascii_lowercase();
            let num = |p: &str| l.strip_prefix(p).and_then(|d| d.parse::<u32>().ok());
            if let Some(n) = num("area") {
                // The engine's `area0` is area 10.
                let n = if n == 0 { 10 } else { n };
                return area(n).map_or_else(|| w.to_string(), |g| format!("area{g}"));
            }
            if let Some(n) = num("char") {
                return slot(n).map_or_else(|| w.to_string(), |g| format!("char{g}"));
            }
            w.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A game's weapon file in the shared `WEAPONS` folder: `<mod>_` plus its
/// path there with `/` as `_` (the game looks weapons up by name).
fn weapon_file(slug: &str, path: &str) -> String {
    format!("{slug}_{}", path.replace('\\', "/").replace('/', "_"))
}

/// A character's `file` keys, its conditions' too, under `MODS/<slug>/`.
fn prefix_files(b: &mut Block, slug: &str) {
    for e in &mut b.entries {
        if e.key.eq_ignore_ascii_case("file") {
            let f = unquote(&e.value);
            if !f.is_empty() {
                e.value = format!("'MODS/{slug}/{f}'");
            }
        }
    }
    for s in &mut b.subs {
        prefix_files(&mut s.block, slug);
    }
}

/// A weapon's files (`file`, `pic`, `bmodel`; its conditions' too) as
/// they are in the shared `WEAPONS` folder.
fn weapon_files(b: &mut Block, slug: &str) {
    for e in &mut b.entries {
        let k = e.key.to_ascii_lowercase();
        if ["file", "pic", "pic1", "picc", "bmodel"].contains(&k.as_str()) {
            let f = unquote(&e.value);
            if !f.is_empty() {
                e.value = format!("'{}'", weapon_file(slug, &f));
            }
        }
    }
    for s in &mut b.subs {
        weapon_files(&mut s.block, slug);
    }
}

fn is_spawn(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("spawn") || n.contains("pack")
}

fn is_condition(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("overwrite") || n.starts_with("addition")
}

/// Moves a character's numbers to the combined game, recursively.
struct Moves<'a> {
    area: &'a dyn Fn(u32) -> Option<u32>,
    slot: &'a dyn Fn(u32) -> Option<u32>,
    spawn_offset: usize,
    pack_offset: usize,
    hunter: &'a [usize],
}

impl Moves<'_> {
    fn block(&self, b: &mut Block) {
        for e in &mut b.entries {
            let k = e.key.to_ascii_lowercase();
            if k == "callno" || k == "fearcall" || k == "dontfear" {
                let v = c_atoi(&e.value);
                if let Some(g) = u32::try_from(v).ok().and_then(|v| (self.slot)(v)) {
                    e.value = g.to_string();
                }
            }
        }
        for s in &mut b.subs {
            let n = s.name.to_ascii_lowercase();
            if is_condition(&n) {
                s.name = header(&s.name, self.area, self.slot);
            }
            if n.contains("spawninfo") {
                for e in &mut s.block.entries {
                    if e.key.eq_ignore_ascii_case("spawngroup") {
                        e.value =
                            (c_atoi(&e.value).max(0) as usize + self.spawn_offset).to_string();
                    }
                }
            }
            if n.contains("packinfo") {
                for e in &mut s.block.entries {
                    if e.key.eq_ignore_ascii_case("group") {
                        e.value = (c_atoi(&e.value).max(0) as usize + self.pack_offset).to_string();
                    }
                }
            }
            if n.contains("killtype") {
                for e in &mut s.block.entries {
                    let k = e.key.to_ascii_lowercase();
                    if k == "hunteranim" || k == "huntercarryanim" {
                        let v = c_atoi(&e.value);
                        if let Some(&g) = usize::try_from(v).ok().and_then(|v| self.hunter.get(v)) {
                            e.value = g.to_string();
                        }
                    }
                }
            }
            self.block(&mut s.block);
        }
    }
}

/// Keeps a character to `areas`: unconditional spawn and pack entries are
/// wrapped in an `addition` for those areas, and conditional blocks that
/// name no area get them added.
fn tie(b: &mut Block, areas: &[u32]) {
    if areas.is_empty() {
        return;
    }
    let list = areas
        .iter()
        .map(|a| format!("area{a}"))
        .collect::<Vec<_>>()
        .join(" ");
    for s in &mut b.subs {
        let n = s.name.to_ascii_lowercase();
        if is_condition(&n) {
            if !n.contains("area") {
                s.name = format!("{} {list}", s.name);
            }
        } else if is_spawn(&n) {
            let inner = Sub {
                name: s.name.clone(),
                at: 0,
                block: std::mem::take(&mut s.block),
            };
            s.name = format!("addition {list}");
            s.block = Block {
                entries: Vec::new(),
                subs: vec![inner],
            };
        }
    }
}

fn is_ambient(b: &Block) -> bool {
    b.get(&["ai"]).map(c_atoi).is_some_and(|a| a < 10)
}

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

/// What [`build`] made.
pub struct Built {
    pub report: String,
    /// Areas of the base game (1..=this): where add-ons' animals live.
    pub base_areas: u32,
    /// Characters of the base game (the games' come after).
    pub base_chars: usize,
}

/// Builds the modpack: `bases` (vanilla, then the Modder's Edition overlay)
/// plus `games`, written to `out` (a folder this tool made, or empty).
pub fn build(bases: &[PathBuf], games: &[PathBuf], out: &Path, o: &Options) -> Result<Built> {
    let vfs = crate::vanilla(bases)?;
    let mut report = String::from("# Combined game\n\n");
    let mut res = ResTxt::parse(&String::from_utf8_lossy(&vfs.read("HUNTDAT/_RES.TXT")?))?;
    let base_res = res.clone();
    let menu_path = vfs
        .resolve("huntdat/_menu.txt")
        .map(|r| r.path.clone())
        .ok_or_else(|| {
            fail("the base has no _MENU.TXT: give the Modder's Edition as an overlay")
        })?;
    let mut menu = ResTxt::parse(&String::from_utf8_lossy(&vfs.read(&menu_path)?))?;
    let base_menu = MenuData::from_res(&menu);
    let (mut n_dinos, mut n_weapons, mut n_areas) = (
        base_menu.huntables.len(),
        base_menu.weapons.len(),
        base_menu.areas.len(),
    );
    let mut names: HashSet<String> = blocks(&res, "characters")
        .iter()
        .chain(blocks(&menu, "characters"))
        .filter_map(|b| b.get(&["name"]).map(|n| unquote(n).to_ascii_lowercase()))
        .collect();
    // The base's own animals stay on the base's maps.
    let tie_ok = |b: &Block| !(o.ambients == Ambients::Random && is_ambient(b));
    let base_area_count = n_areas as u32;
    let base_chars = blocks(&res, "characters").len();
    let base_areas: Vec<u32> = (1..=base_area_count).collect();
    if let Some(s) = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
    {
        for b in s.blocks.iter_mut().skip(1) {
            if tie_ok(b) {
                tie(b, &base_areas);
            }
        }
    }

    // The hunter model all kills are merged into.
    let hunter_file = blocks(&res, "characters")
        .first()
        .and_then(|b| b.get(&["file"]))
        .map(unquote)
        .ok_or_else(|| fail("no hunter in the base _RES.TXT"))?;
    let hunter_path = vfs
        .resolve(&key(&format!("HUNTDAT/{hunter_file}")))
        .map_or_else(|| format!("HUNTDAT/{hunter_file}"), |r| r.path.clone());
    let mut hunter = c2_formats::car::parse(&vfs.read(&hunter_path)?)?;

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut weapon_cars: Vec<(String, Vec<u8>)> = blocks(&res, "weapons")
        .iter()
        .map(|b| {
            let name = b.get(&["name"]).map(unquote).unwrap_or_default();
            let file = b.get(&["file"]).map(unquote).unwrap_or_default();
            let data = vfs
                .read(&format!("HUNTDAT/WEAPONS/{file}"))
                .unwrap_or_default();
            (name.to_ascii_lowercase(), data)
        })
        .collect();

    let mut slugs: HashSet<String> = HashSet::new();
    for dir in games {
        let root = crate::pack::find_game_root(dir)
            .and_then(|r| find_ci(&r, "HUNTDAT"))
            .ok_or_else(|| ModError::NoHuntdat(dir.clone()))?;
        // Named after the first telling folder up its path (not `Game`).
        let name = root
            .ancestors()
            .skip(1)
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .find(|n| {
                let s = crate::manifest::slug(n);
                !matches!(s.as_str(), "game" | "huntdat" | "carnivores-2")
            })
            .unwrap_or_else(|| "mod".into());
        let mut slug = crate::manifest::slug(&name);
        while !slugs.insert(slug.clone()) {
            slug.push('2');
        }
        let res_path = find_ci(&root, "_RES.TXT")
            .ok_or_else(|| fail(format!("{}: no _RES.TXT", root.display())))?;
        let mut g_res = read_res(&res_path)?;
        // Made for the original engine (or another without spawn tables:
        // nothing of it would be placed).
        let spawns = g_res
            .sections
            .iter()
            .any(|s| s.name.to_ascii_lowercase().contains("spawntable"));
        if !c2_formats::mee::is_dialect(&g_res) || !spawns {
            let groot = root.clone();
            let model = move |f: &str| {
                crate::pack::find_path(&groot, f)
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|d| c2_formats::car::parse(&d).ok())
            };
            let (converted, notes) = to_dialect(&g_res, &base_res, &model);
            g_res = converted;
            report.push_str(&format!(
                "- **{name}**: made for the original engine; its animals take the base's same-AI animals' animations and spawns{}\n",
                if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) }
            ));
        }
        let g = Game {
            slug,
            name,
            menu: match find_ci(&root, "_MENU.TXT") {
                Some(p) => read_res(&p)?,
                None => g_res.clone(),
            },
            res: g_res,
            root,
        };
        let gm = MenuData::from_res(&g.menu);
        let (d0, w0, a0) = (n_dinos, n_weapons, n_areas);
        // Areas its menu lists that it has maps for (a menu may list more).
        let has_map = |n: usize| {
            find_ci(&g.root, "AREAS")
                .is_some_and(|d| find_ci(&d, &format!("AREA{n}.MAP")).is_some())
        };
        let g_areas = (1..=gm.areas.len()).take_while(|&n| has_map(n)).count();
        if g_areas < gm.areas.len() {
            report.push_str(&format!(
                "- **{}**: its menu lists {} areas, it has maps for {}: those only\n",
                g.name,
                gm.areas.len(),
                g_areas
            ));
        }
        let area_map = |n: u32| (n >= 1 && n as usize <= g_areas).then(|| a0 as u32 + n);
        let g_dinos = gm.huntables.len();
        let slot_map = |n: u32| ((n as usize) < g_dinos).then(|| d0 as u32 + n);
        let my_areas: Vec<u32> = (1..=g_areas as u32).map(|n| a0 as u32 + n).collect();

        // Hunter: its kill animations into the shared model.
        let g_hunter = game_hunter(&g.res)
            .and_then(|b| b.get(&["file"]))
            .map(unquote)
            .and_then(|f| crate::pack::find_path(&g.root, &f))
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|d| c2_formats::car::parse(&d).ok());
        let hunter_map = match g_hunter {
            Some(h) => {
                match crate::hunter::merge(&mut hunter, std::slice::from_ref(&h)).remove(0) {
                    crate::hunter::Outcome::Merged { map } => map,
                    crate::hunter::Outcome::Incompatible(why) => {
                        report.push_str(&format!(
                        "- **{}**: its hunter model is left out ({why}); its kills may show the wrong animation\n",
                        g.name
                    ));
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };

        // Weapons: shared when the game already has the same one.
        let mut weapon_slot: Vec<usize> = Vec::new();
        let mut new_weapons: Vec<(usize, Block)> = Vec::new();
        for (i, b) in blocks(&g.res, "weapons").iter().enumerate() {
            let wname = b.get(&["name"]).map(unquote).unwrap_or_default();
            let file = b.get(&["file"]).map(unquote).unwrap_or_default();
            let data = crate::pack::find_path(&g.root, &format!("WEAPONS/{file}"))
                .and_then(|p| std::fs::read(p).ok())
                .unwrap_or_default();
            if let Some(j) = weapon_cars
                .iter()
                .position(|(n, d)| *n == wname.to_ascii_lowercase() && *d == data)
            {
                weapon_slot.push(j);
                continue;
            }
            weapon_cars.push((wname.to_ascii_lowercase(), data));
            n_weapons += 1;
            weapon_slot.push(n_weapons - 1);
            new_weapons.push((i, b.clone()));
        }
        let wslot = |n: usize| weapon_slot.get(n - 1).map(|&j| j + 1);

        // Files.
        let mut all = Vec::new();
        walk(&g.root, &mut all);
        for p in all {
            let Ok(rel) = p.strip_prefix(&g.root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let k = key(&format!("HUNTDAT/{rel}"));
            if !crate::detect::is_data(&k) || k.ends_with("/_res.txt") || k.ends_with("/_menu.txt")
            {
                continue;
            }
            let fname = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            let dest = if let Some(sub) = k.strip_prefix("huntdat/areas/") {
                // The engine reads `AREAS/` itself; a folder in it is the
                // maker's spare copies (another version of a map).
                let ext = fname.rsplit('.').next().unwrap_or("").to_ascii_uppercase();
                renumber(&fname, "area", &|n| area_map(n as u32).map(|g| g as usize))
                    .filter(|_| (ext == "MAP" || ext == "RSC") && !sub.contains('/'))
                    .map(|_| {
                        let n: String = fname[4..]
                            .chars()
                            .take_while(char::is_ascii_digit)
                            .collect();
                        let g = n.parse::<u32>().ok().and_then(area_map).unwrap_or(0);
                        format!("HUNTDAT/AREAS/AREA{g}.{ext}")
                    })
            } else if k.starts_with("huntdat/menu/") {
                let dir = rel.rsplit_once('/').map_or("MENU", |(d, _)| d);
                let to =
                    |n: usize| -> Option<usize> { slot_map(n as u32 - 1).map(|g| g as usize + 1) };
                renumber(&fname, "dino", &to)
                    .or_else(|| renumber(&fname, "call", &to))
                    .or_else(|| renumber(&fname, "weapon", &|n| wslot(n).filter(|&j| j > w0)))
                    .or_else(|| {
                        renumber(&fname, "area", &|n| area_map(n as u32).map(|g| g as usize))
                    })
                    .map(|f| format!("HUNTDAT/{dir}/{f}"))
            } else if k.starts_with("huntdat/soundfx/") {
                renumber(&fname, "call", &|n| {
                    slot_map(n as u32 - 1).map(|g| g as usize + 1)
                })
                .filter(|_| k.contains("/calls/"))
                .map(|f| format!("HUNTDAT/SOUNDFX/CALLS/{f}"))
            } else if k.starts_with("huntdat/weapons/") {
                // Flattened (`MODELS/x.car` -> `<mod>_MODELS_x.car`), as the
                // entries are (`weapon_file`).
                let under = &rel[rel.find('/').map_or(0, |i| i + 1)..];
                Some(format!("HUNTDAT/WEAPONS/{}", weapon_file(&g.slug, under)))
            } else {
                Some(format!("HUNTDAT/MODS/{}/{rel}", g.slug))
            };
            if let Some(d) = dest {
                files.push((d, p.clone()));
            }
        }

        // _RES.TXT: tables, characters, weapons.
        let spawn_offset = table_subs(&res, "spawntable", "spawngroup").len();
        let pack_offset = table_subs(&res, "packtable", "packgroup").len();
        let moves = Moves {
            area: &|n| area_map(n),
            slot: &|n| slot_map(n),
            spawn_offset,
            pack_offset,
            hunter: &hunter_map,
        };
        for (table, sub) in [("spawntable", "spawngroup"), ("packtable", "packgroup")] {
            let mut add: Vec<Sub> = table_subs(&g.res, table, sub)
                .into_iter()
                .cloned()
                .collect();
            for s in &mut add {
                moves.block(&mut s.block);
            }
            let sec = section_mut(&mut res, table);
            let at = sec.entries.len();
            sec.subs.extend(add.into_iter().map(|mut s| {
                s.at = at;
                s
            }));
        }
        // Names already in the game get the game's name after them.
        let mut renames: HashMap<String, String> = HashMap::new();
        let mut added = 0;
        for b in blocks(&g.res, "characters")
            .iter()
            .filter(|b| b.get(&["ai"]).map(c_atoi).unwrap_or(0) != 0)
        {
            let mut b = b.clone();
            if let Some(n) = b.get(&["name"]).map(unquote) {
                let mut u = n.clone();
                if names.contains(&u.to_ascii_lowercase()) {
                    u = format!("{n} ({})", g.name);
                    renames.insert(n.to_ascii_lowercase(), u.clone());
                }
                names.insert(u.to_ascii_lowercase());
                set(&mut b, "name", format!("'{u}'"));
            }
            // Its models (also those its conditions switch to per area).
            prefix_files(&mut b, &g.slug);
            moves.block(&mut b);
            if tie_ok(&b) {
                tie(&mut b, &my_areas);
            }
            section_mut(&mut res, "characters").blocks.push(b);
            added += 1;
        }
        for (_, b) in &new_weapons {
            let mut b = b.clone();
            weapon_files(&mut b, &g.slug);
            section_mut(&mut res, "weapons").blocks.push(b);
        }

        // _MENU.TXT: huntables, weapons, areas, prices.
        let menu_dinos: Vec<&Block> = blocks(&g.menu, "characters")
            .iter()
            .filter(|b| b.get(&["ai"]).map(c_atoi).is_some_and(|a| a >= 10))
            .collect();
        let mut prices: Vec<Entry> = Vec::new();
        for (i, b) in menu_dinos.iter().enumerate() {
            let mut b = (*b).clone();
            // The name its _RES entry got.
            if let Some(n) = b.get(&["name"]).map(unquote) {
                if let Some(u) = renames.get(&n.to_ascii_lowercase()) {
                    set(&mut b, "name", format!("'{u}'"));
                }
            }
            section_mut(&mut menu, "characters").blocks.push(b);
            prices.push(Entry {
                key: "dino".into(),
                value: gm.huntables.get(i).map_or(0, |h| h.price).to_string(),
            });
        }
        for (i, b) in &new_weapons {
            let keep = ["name", "power", "prec", "loud", "rate"];
            let entry = Block {
                entries: b
                    .entries
                    .iter()
                    .filter(|e| keep.iter().any(|k| e.key.eq_ignore_ascii_case(k)))
                    .cloned()
                    .collect(),
                ..Default::default()
            };
            section_mut(&mut menu, "weapons").blocks.push(entry);
            prices.push(Entry {
                key: "weapon".into(),
                value: gm.weapons.get(*i).map_or(0, |w| w.price).to_string(),
            });
        }
        for a in gm.areas.iter().take(g_areas) {
            prices.push(Entry {
                key: "area".into(),
                value: a.price.to_string(),
            });
        }
        section_mut(&mut menu, "prices").entries.extend(prices);
        n_dinos += g_dinos;
        n_areas += g_areas;
        report.push_str(&format!(
            "- **{}**: areas {}–{}, huntables {}–{}, {} new weapons ({} shared), {} characters ({} renamed: same name as one already in), files under `MODS/{}/`\n",
            g.name,
            a0 + 1,
            n_areas,
            d0 + 1,
            n_dinos,
            new_weapons.len(),
            weapon_slot.len() - new_weapons.len(),
            added,
            renames.len(),
            g.slug
        ));
        if w0 + new_weapons.len() != n_weapons {
            return Err(fail("weapon numbering went wrong"));
        }
    }
    report.push_str(&format!(
        "\nIn all: {n_areas} areas, {n_dinos} huntables, {n_weapons} weapons; hunter model {} animations.\n",
        hunter.animations.len()
    ));
    let max = c2_formats::bits::Bits::LEN - 10;
    if n_dinos > max {
        report.push_str(&format!(
            "\n**Warning:** more than {max} huntables; the menu can pick only the first {max}.\n"
        ));
    }
    if n_weapons > 128 {
        report.push_str("\n**Warning:** more than 128 weapons; only the first 128 can be taken.\n");
    }

    // Write: the base, the games' files, the generated files.
    let mut out_vfs = crate::vanilla(bases)?;
    out_vfs.push("games", Box::new(MapBackend { map: files }));
    let mut mem = MemBackend::new();
    mem.insert("HUNTDAT/_RES.TXT", res.write().into_bytes());
    mem.insert(&menu_path, menu.write().into_bytes());
    mem.insert(&hunter_path, c2_formats::car::write(&hunter));
    out_vfs.push("(generated)", Box::new(mem));
    write_out(&out_vfs, out, &report)?;
    Ok(Built {
        report,
        base_areas: base_area_count,
        base_chars,
    })
}

/// Layers add-ons (the mod system) over the combined game `games`; their
/// animals are kept to the base game's maps, which they were made for
/// (unless ambients roam and the animal is one). Write it with
/// [`crate::materialize`].
pub fn add_ons(
    games: &Path,
    addons: &[PathBuf],
    built: &Built,
    o: &Options,
) -> Result<crate::Pack> {
    let v = crate::vanilla(&[games.to_path_buf()])?;
    let sources = addons
        .iter()
        .map(|a| crate::load_mod(&v, a))
        .collect::<Result<Vec<_>>>()?;
    add_on_sources(games, sources, built, o, &crate::BuildOptions::default())
}

/// [`add_ons`] for add-ons already read against the combined game (a
/// recipe's, with its decisions and patches in `opts`).
pub fn add_on_sources(
    games: &Path,
    mut sources: Vec<crate::ModSource>,
    built: &Built,
    o: &Options,
    opts: &crate::BuildOptions,
) -> Result<crate::Pack> {
    use crate::res_merge::{block_id, Change};
    let base_areas = built.base_areas;
    let bases = vec![games.to_path_buf()];
    // An add-on named like one of the games' animals is a different animal:
    // added alongside under its own name, not changing the game's.
    for m in &mut sources {
        // The menu slot its pictures were drawn for (`DINO7.TGA`), if any.
        let drawn_for = m.map.iter().find_map(|(p, _)| {
            let name = p.rsplit('/').next()?;
            match crate::loose::numbered(&name.to_ascii_lowercase())? {
                (k, n) if k == "dino" && key(p).contains("/menu/") => Some(n),
                _ => None,
            }
        });
        let mut slots = Vec::new();
        for c in &mut m.res_changes {
            if let Change::Modify {
                section,
                index,
                block,
                was: None,
            } = c
            {
                if section.eq_ignore_ascii_case("characters") && *index >= built.base_chars {
                    let mut b = block.clone();
                    let named = b.get(&["name"]).map(unquote).unwrap_or_default();
                    if !named.is_empty() {
                        set(&mut b, "name", format!("'{named} (add-on)'"));
                    }
                    // A slot of its own, with its pictures renumbered to it
                    // (else they would replace the base animal's they were
                    // drawn for, and it would have none).
                    if let (Some(from), Some(id)) = (drawn_for, block_id(&b)) {
                        slots.push(crate::loose::SlotAdd {
                            section: "characters".into(),
                            id,
                            from,
                            alongside: named.clone(),
                        });
                    }
                    *c = Change::Add {
                        section: section.clone(),
                        block: b,
                    };
                }
            }
        }
        m.slots.extend(slots);
    }
    let added: HashSet<String> = sources
        .iter()
        .flat_map(|m| m.res_changes.iter())
        .filter_map(|c| match c {
            Change::Add { section, block } if section.eq_ignore_ascii_case("characters") => {
                block_id(block)
            }
            _ => None,
        })
        .collect();
    let mut pack = crate::build_with(&bases, sources, opts)?;
    let mut res = pack.merged.res.clone();
    let areas: Vec<u32> = (1..=base_areas).collect();
    if let Some(s) = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
    {
        for b in &mut s.blocks {
            let id = block_id(b).unwrap_or_default();
            if added.contains(&id) && !(o.ambients == Ambients::Random && is_ambient(b)) {
                tie(b, &areas);
            }
        }
    }
    let path = pack
        .vfs
        .resolve("huntdat/_res.txt")
        .map_or_else(|| "HUNTDAT/_RES.TXT".to_string(), |r| r.path.clone());
    let mut mem = MemBackend::new();
    mem.insert(&path, res.write().into_bytes());
    pack.vfs
        .push("(add-ons kept to the base maps)", Box::new(mem));
    pack.notes.push(format!(
        "add-on animals kept to areas 1-{base_areas} (the maps they were made for)"
    ));
    Ok(pack)
}

/// What the combined game's folder holds besides the game: the report.
pub const REPORT_FILE: &str = "COMBINED.md";

fn write_out(vfs: &Vfs, out: &Path, report: &str) -> Result<()> {
    if out.exists() {
        let empty = std::fs::read_dir(out).map_err(io(out))?.next().is_none();
        if !empty && !out.join(crate::pack::PACK_MARKER).exists() {
            return Err(ModError::NotAPack(out.to_path_buf()));
        }
        std::fs::remove_dir_all(out).map_err(io(out))?;
    }
    std::fs::create_dir_all(out).map_err(io(out))?;
    for (_, r) in vfs.files() {
        let dst = out.join(&r.path);
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(io(p))?;
        }
        match vfs.disk_path(&r.path) {
            Some(src) => {
                let src = std::fs::canonicalize(&src).map_err(io(&src))?;
                if std::fs::hard_link(&src, &dst).is_err() {
                    std::fs::copy(&src, &dst).map_err(io(&dst))?;
                }
            }
            None => std::fs::write(&dst, vfs.read(&r.path)?).map_err(io(&dst))?,
        }
    }
    std::fs::write(out.join(REPORT_FILE), report).map_err(io(out))?;
    std::fs::write(out.join(crate::pack::PACK_MARKER), "built by c2rs\n").map_err(io(out))?;
    Ok(())
}
