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

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, Block, Entry, ResTxt, Section, Sub};
use c2_vfs::{key, MemBackend, Vfs};

use crate::loose::MapBackend;
use crate::{io, ModError};

type Result<T> = std::result::Result<T, ModError>;

fn fail(msg: impl Into<String>) -> ModError {
    ModError::Standalone(msg.into())
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
    /// Per game (in `games` order): what of it is left out. Missing: none.
    pub skip: Vec<Skip>,
}

mod dialect;
mod game;
pub mod leave_out;
mod skip;
pub use dialect::{spawn_layer, to_dialect, SPAWN_LAYER};
pub use skip::{base_parts, parts, parts_in, Parts, Skip};

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

fn area_number(name: &str) -> Option<u32> {
    let prefix = name.get(.."area".len())?;
    if !prefix.eq_ignore_ascii_case("area") {
        return None;
    }
    let rest = name.get("area".len()..)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// `DINO7.TGA` with prefix `dino`, 7 -> n: renamed, keeping case and suffix.
fn renumber(name: &str, prefix: &str, map: &dyn Fn(usize) -> Option<usize>) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    if !lower.starts_with(prefix) {
        return None;
    }
    let rest = name.get(prefix.len()..)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let to = map(digits.parse().ok()?)?;
    Some(format!(
        "{}{to}{}",
        name.get(..prefix.len())?,
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

/// Whether an animal is kept to its game's maps: all are, but ambients
/// when they roam (`Ambients::Random`).
fn ties(o: &Options, b: &Block) -> bool {
    !(o.ambients == Ambients::Random && is_ambient(b))
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
    let res = ResTxt::parse(&String::from_utf8_lossy(&vfs.read("HUNTDAT/_RES.TXT")?))?;
    // The base's menu: its `_MENU.TXT`, or its `_RES.TXT` when that lists
    // the areas itself (a game built from a mobile APK; the game reads its
    // menu from there too, `menu_txt`). The combined menu is then written
    // as a new `_MENU.TXT`.
    let (menu_path, menu) = match vfs.resolve("huntdat/_menu.txt") {
        Some(r) => {
            let p = r.path.clone();
            let m = ResTxt::parse(&String::from_utf8_lossy(&vfs.read(&p)?))?;
            (p, m)
        }
        None if !MenuData::from_res(&res).areas.is_empty() => {
            ("HUNTDAT/_MENU.TXT".to_string(), res.clone())
        }
        None => {
            return Err(fail(
                "the base has no _MENU.TXT: give the Modder's Edition as an overlay",
            ))
        }
    };
    let base_menu = MenuData::from_res(&menu);
    let names: HashSet<String> = blocks(&res, "characters")
        .iter()
        .chain(blocks(&menu, "characters"))
        .filter_map(|b| b.get(&["name"]).map(|n| unquote(n).to_ascii_lowercase()))
        .collect();
    let base_area_count = base_menu.areas.len() as u32;
    let base_chars = blocks(&res, "characters").len();

    // The hunter model all kills are merged into.
    let hunter_file = blocks(&res, "characters")
        .first()
        .and_then(|b| b.get(&["file"]))
        .map(unquote)
        .ok_or_else(|| fail("no hunter in the base _RES.TXT"))?;
    let hunter_path = vfs
        .resolve(&key(&format!("HUNTDAT/{hunter_file}")))
        .map_or_else(|| format!("HUNTDAT/{hunter_file}"), |r| r.path.clone());
    let hunter = c2_formats::car::parse(&vfs.read(&hunter_path)?)?;

    let weapon_cars: Vec<(String, Vec<u8>)> = blocks(&res, "weapons")
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

    let mut c = game::Combined {
        o,
        report: String::from("# Combined game\n\n"),
        base_res: res.clone(),
        res,
        menu,
        names,
        n_dinos: base_menu.huntables.len(),
        n_weapons: base_menu.weapons.len(),
        n_areas: base_menu.areas.len(),
        hunter,
        files: Vec::new(),
        weapon_cars,
        slugs: HashSet::new(),
    };
    // The base's own animals stay on the base's maps.
    let base_areas: Vec<u32> = (1..=base_area_count).collect();
    if let Some(s) = c
        .res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
    {
        for b in s.blocks.iter_mut().skip(1) {
            if ties(o, b) {
                tie(b, &base_areas);
            }
        }
    }
    for (gi, dir) in games.iter().enumerate() {
        c.add(dir, &o.skip.get(gi).cloned().unwrap_or_default())?;
    }

    let (n_areas, n_dinos, n_weapons) = (c.n_areas, c.n_dinos, c.n_weapons);
    let mut report = c.report;
    report.push_str(&format!(
        "\nIn all: {n_areas} areas, {n_dinos} huntables, {n_weapons} weapons; hunter model {} animations.\n",
        c.hunter.animations.len()
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
    out_vfs.push("games", Box::new(MapBackend { map: c.files }));
    let mut mem = MemBackend::new();
    mem.insert("HUNTDAT/_RES.TXT", c.res.write().into_bytes());
    mem.insert(&menu_path, c.menu.write().into_bytes());
    mem.insert(&hunter_path, c2_formats::car::write(&c.hunter));
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
            if added.contains(&id) && ties(o, b) {
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

#[cfg(test)]
mod filename_tests {
    use super::*;
    #[test]
    fn parses_only_numbered_area_names() {
        for name in ["", "ar", "aréa2.map", "area.map", "area4294967296.map"] {
            assert_eq!(area_number(name), None);
        }
        assert_eq!(area_number("AREA12.MAP"), Some(12));
        assert_eq!(area_number("Area2.rsc"), Some(2));
        assert_eq!(renumber("aréa2.map", "area", &Some), None);
        assert_eq!(
            renumber("ArEa2.rsc", "area", &|n| Some(n + 3)),
            Some("ArEa5.rsc".into())
        );
    }
}
