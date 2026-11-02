//! Add-ons shipped as loose files with install instructions: a `.CAR`, a
//! `_RES.TXT` entry to paste in (often in a readme), maybe menu pictures,
//! call sounds and a hunter model. This works out where each file goes and
//! what the entry changes, as a person following the instructions would:
//!
//! - A folder holding `MENU`, `SOUNDFX`, `WEAPONS` or `AREAS` stands for
//!   `HUNTDAT`; its files keep their places.
//! - Any other file named like a base game file replaces it (`CALL7_A.WAV`,
//!   `DINO7.TGA`); a new `.CAR` goes in `HUNTDAT`; menu pictures and texts
//!   (`DINOn`, `CALLn`, `WEAPONn`) and call sounds go to their folders.
//! - Entries are the `{ ... }` blocks found in the text files. When the same
//!   animal has several (one per target game), the one introduced as for the
//!   Modder's Edition is taken on that base, else the first.
//! - Menu art for slot n (`DINO7.TGA`) means the entry was made for menu
//!   slot n ("replace the Spinosaurus entry"); `WEAPONn` likewise. It is
//!   added in a new slot instead (`SlotAdd`), except an animal on the
//!   original game's data, which places animals by AI number and so can
//!   only take the slot over. An entry named like a base one changes it.
//!   Anything else is added.
//! - A `.CAR` named `hunter...` is the hunter model with the add-on's kill
//!   animations: merged with the others at build time (`crate::hunter`).
//! - Variant folders: one for "Legacy" (another game) is skipped; of a
//!   Modder's Edition folder and a plain one, the one matching the base.
//! - Fix-ups a person would make: an entry naming a model the add-on
//!   doesn't ship takes its closest-named one; an entry for the original
//!   game taking over a slot of a Modder's Edition base keeps the slot's
//!   spawn tables; an add-on with no entry at all (a skin pack) only adds
//!   files.

use std::path::{Path, PathBuf};

use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, Block, ResTxt};
use c2_vfs::{key, Backend, Vfs, VfsError};

use crate::detect::{read_res, RES_TXT};
use crate::res_merge::{block_id, Change};
use crate::ModError;

/// A loose add-on, placed.
#[derive(Debug, Clone, Default)]
pub struct Loose {
    /// `(path in the game, file on disk)`.
    pub files: Vec<(String, PathBuf)>,
    pub res_changes: Vec<Change>,
    /// Its hunter model, to merge.
    pub hunter: Option<PathBuf>,
    /// What was decided, for the merge report.
    pub notes: Vec<String>,
    /// Entries drawn for a menu slot that get one of their own instead.
    pub slots: Vec<SlotAdd>,
    /// The text files its entries were read from (pinned by recipes).
    pub texts: Vec<PathBuf>,
}

/// An add-on animal or weapon made for menu slot `from` (its `DINOn` /
/// `WEAPONn` files, 1-based) that is added in a new slot instead of
/// replacing that slot's occupant (`alongside`). The slot is handed out
/// when the game is built, as is renumbering its menu files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotAdd {
    /// `characters` or `weapons`.
    pub section: String,
    pub id: String,
    pub from: usize,
    pub alongside: String,
}

/// Is `dir` a loose add-on (a folder with a model or an entry, no
/// `HUNTDAT`)?
pub fn is_loose(dir: &Path) -> bool {
    dir.is_dir() && {
        let mut all = Vec::new();
        walk(dir, &mut all);
        all.iter().any(|p| ext(p) == "car" || ext(p) == "txt")
    }
}

fn ext(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Which variant a folder name is.
#[derive(PartialEq, Clone, Copy)]
enum Variant {
    Mee,
    Plain,
    Other,
}

fn variant(name: &str) -> Variant {
    let n = name.to_ascii_lowercase();
    if n.contains("mee") || n.contains("modder") {
        Variant::Mee
    } else if n.contains("speed patch") || n.contains("vanilla") || n.contains("classic") {
        Variant::Plain
    } else {
        Variant::Other
    }
}

/// A folder of a copy for Carnivores: Ice Age (`ICE AGE (MEE 1.1.1)`).
fn for_ice_age(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("ice age") || n.contains("iceage") || n.contains("ice_age")
}

/// The files to consider, after dropping variant folders that don't fit:
/// the Modder's Edition's or the original's, and Ice Age's or Carnivores
/// 2's (`ice`: the base is Ice Age) where both are given.
fn gather(dir: &Path, dialect: bool, ice: bool, notes: &mut Vec<String>, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    let subs: Vec<&PathBuf> = entries.iter().filter(|p| p.is_dir()).collect();
    let kinds: Vec<Variant> = subs.iter().map(|p| variant(&lower_name(p))).collect();
    let both = kinds.contains(&Variant::Mee) && kinds.contains(&Variant::Plain);
    // A copy for each game (the ARK Yutyrannus: `CARNIVORES (MEE 1.1.1)`
    // with its slot 9 files, `ICE AGE (MEE 1.1.1)` with slot 10's).
    let games = subs.iter().any(|p| for_ice_age(&lower_name(p)))
        && subs.iter().zip(&kinds).any(|(p, k)| {
            let n = lower_name(p);
            !for_ice_age(&n) && (*k != Variant::Other || n.contains("carnivores"))
        });
    for p in &entries {
        if !p.is_dir() {
            out.push(p.clone());
            continue;
        }
        let name = lower_name(p);
        if name.contains("legacy") {
            notes.push(format!("skipped `{}` (for another game)", p.display()));
            continue;
        }
        let v = variant(&name);
        if both && ((v == Variant::Mee) != dialect) && v != Variant::Other {
            notes.push(format!("skipped `{}` (the other variant)", p.display()));
            continue;
        }
        if games
            && for_ice_age(&name) != ice
            && (for_ice_age(&name) || v != Variant::Other || name.contains("carnivores"))
        {
            notes.push(format!("skipped `{}` (for the other game)", p.display()));
            continue;
        }
        gather(p, dialect, ice, notes, out);
    }
}

/// `DINO7.TGA` -> `("dino", 7)`.
pub(crate) fn numbered(name: &str) -> Option<(String, usize)> {
    let stem = name.split('.').next()?;
    let stem = stem.split('_').next()?;
    let digits = stem.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let prefix = &stem[..stem.len() - digits.len()];
    Some((prefix.to_string(), digits.parse().ok()?))
}

/// Where a loose file goes in the game, if anywhere.
fn place(p: &Path, base: &Vfs, root: Option<&Path>) -> Option<String> {
    // Under a folder standing for HUNTDAT: keep its place.
    if let Some(r) = root {
        if let Ok(rel) = p.strip_prefix(r) {
            if rel.components().count() > 1 {
                let rel = rel.to_string_lossy().replace('\\', "/");
                return Some(format!("HUNTDAT/{rel}"));
            }
        }
    }
    let name = p.file_name()?.to_string_lossy().into_owned();
    let lname = name.to_ascii_lowercase();
    let e = ext(p);
    // A model goes where its entry looks: `HUNTDAT/<name>` (replacing the
    // base's file there, if any).
    if e == "car" {
        let k = key(&format!("HUNTDAT/{name}"));
        return Some(
            base.resolve(&k)
                .map_or_else(|| format!("HUNTDAT/{name}"), |r| r.path.clone()),
        );
    }
    // Same name as a game file (a menu picture, a call): replace it. Not a
    // file another game keeps in its own folder (`MODS/...`).
    if let Some((_, r)) = base.files().find(|(k, _)| {
        k.rsplit('/').next() == Some(lname.as_str())
            && k.starts_with("huntdat/")
            && !k.starts_with("huntdat/mods/")
    }) {
        return Some(r.path.clone());
    }
    let num = numbered(&lname);
    let dir = match (e.as_str(), num.as_ref().map(|(k, _)| k.as_str())) {
        ("car", _) => "HUNTDAT",
        ("tga", Some("dino" | "call" | "weapon")) => "HUNTDAT/MENU/PICS",
        ("txm" | "txu", Some("dino" | "call")) | ("txt", Some("weapon")) => "HUNTDAT/MENU/TXT",
        ("wav", Some("call")) => "HUNTDAT/SOUNDFX/CALLS",
        _ => return None,
    };
    Some(format!("{dir}/{name}"))
}

/// Is `p` only a note for people (a list of links, a picture, a readme
/// without an entry), which an add-on's install would leave out?
pub fn is_note(p: &Path) -> bool {
    match ext(p).as_str() {
        "txt" | "md" => std::fs::read(p).is_ok_and(|d| {
            let text = String::from_utf8_lossy(&d).replace('\r', "");
            !blocks_in(&text).iter().any(|(_, b)| entry(b).is_some())
        }),
        // Pages and pictures for people, and settings files (an APK
        // import's `mobile-values.toml` beside its add-ons).
        "url" | "htm" | "html" | "pdf" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "doc"
        | "docx" | "rtf" | "toml" | "ini" | "json" | "cfg" => true,
        _ => false,
    }
}

/// The `{ ... }` blocks in `text` with what precedes each.
fn blocks_in(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    let mut before = String::new();
    for line in text.lines() {
        let t = line.trim();
        if depth == 0 {
            if t.starts_with('{') {
                depth = 1;
                cur = format!("{line}\n");
                if t.contains('}') && t.matches('{').count() == t.matches('}').count() {
                    depth = 0;
                }
                continue;
            }
            before.push_str(line);
            before.push('\n');
            continue;
        }
        cur.push_str(line);
        cur.push('\n');
        depth += t.matches('{').count();
        depth = depth.saturating_sub(t.matches('}').count());
        if depth == 0 {
            out.push((std::mem::take(&mut before), std::mem::take(&mut cur)));
        }
    }
    out
}

/// A pasted entry as a block of `characters` or `weapons`.
fn entry(text: &str) -> Option<(String, Block)> {
    let probe = ResTxt::parse(&format!("characters {{\n{text}\n}}\n")).ok()?;
    let b = probe.sections.first()?.blocks.first()?.clone();
    block_id(&b)?;
    let has = |k: &str| b.get(&[k]).is_some();
    let section = if has("power") || has("shots") || has("prec") {
        "weapons"
    } else if has("file") || has("ai") {
        "characters"
    } else {
        return None;
    };
    Some((section.into(), b))
}

/// The base character at menu slot `n` (1-based): from `_MENU.TXT` when
/// the base has one (the Modder's Edition), else the original's AI `9 + n`.
fn menu_slot(base: &Vfs, res: &ResTxt, n: usize) -> Option<usize> {
    let chars = &res.section("characters")?.blocks;
    let menu = base
        .read("huntdat/_menu.txt")
        .ok()
        .and_then(|d| read_res(&d).ok())
        .map(|m| MenuData::from_res(&m));
    // The menu's list: `_MENU.TXT`, else in a dialect game `_RES.TXT`'s
    // animals with AI 10 and up in order (Ice Age rewritten in the dialect
    // numbers its AIs its own way); in the original's data slot n is AI 9 + n.
    let m = menu
        .filter(|m| !m.huntables.is_empty())
        .or_else(|| c2_formats::mee::is_dialect(res).then(|| MenuData::from_res(res)));
    match m {
        Some(m) => {
            let name = m.huntables.get(n - 1)?.name.to_ascii_lowercase();
            chars
                .iter()
                .position(|b| block_id(b).as_deref() == Some(&name))
        }
        None => {
            let ai = 9 + n as i32;
            chars
                .iter()
                .position(|b| b.get(&["ai"]).map(c_atoi) == Some(ai))
        }
    }
}

/// Reads the loose add-on in `dir` against the base game `base`.
pub fn read(dir: &Path, base: &Vfs) -> Result<Loose, ModError> {
    read_only(dir, base, &|_| true)
}

/// [`read`], looking only at the files `keep` allows: a recipe's pinned
/// files, so other files in the user's copy change nothing.
pub fn read_only(dir: &Path, base: &Vfs, keep: &dyn Fn(&Path) -> bool) -> Result<Loose, ModError> {
    let res = read_res(&base.read(RES_TXT)?)?;
    let dialect = c2_formats::mee::is_dialect(&res);
    let mut out = Loose::default();
    let mut files = Vec::new();
    // Ice Age's mammoth: the base is Ice Age (either format).
    let ice = res.section("characters").is_some_and(|s| {
        s.blocks.iter().any(|b| {
            b.get(&["file"])
                .is_some_and(|f| f.trim().trim_matches('\'').eq_ignore_ascii_case("mamm.car"))
        })
    });
    gather(dir, dialect, ice, &mut out.notes, &mut files);
    files.retain(|f| keep(f));
    // A folder holding the game's own folders stands for HUNTDAT.
    let root = files.iter().find_map(|f| {
        f.ancestors().skip(1).find(|a| {
            a.starts_with(dir)
                && ["menu", "soundfx", "weapons", "areas"]
                    .iter()
                    .any(|s| a.join(s).is_dir() || dir_ci(a, s))
        })
    });
    let root = root.map(Path::to_path_buf);
    let mut texts = Vec::new();
    let mut menu_slot_n = None;
    let mut call_slot_n = None;
    let mut weapon_slot_n = None;
    for f in &files {
        let name = lower_name(f);
        let e = ext(f);
        if e == "car" && name.contains("hunter") {
            if out.hunter.is_none() {
                out.hunter = Some(f.clone());
            }
            continue;
        }
        let placed = place(f, base, root.as_deref());
        // Text files outside MENU are instructions / entries.
        if e == "txt" && placed.as_deref().is_none_or(|p| !key(p).contains("/menu/")) {
            if name.contains("trophy") {
                out.notes
                    .push(format!("skipped `{}` (trophy-room entry)", f.display()));
            } else {
                texts.push(f.clone());
            }
            continue;
        }
        match placed {
            Some(p) => {
                if let Some((kind, n)) = numbered(&name) {
                    if kind == "dino" && key(&p).contains("/menu/") {
                        menu_slot_n = Some(n);
                    }
                    if kind == "weapon" && key(&p).contains("/menu/") {
                        weapon_slot_n = Some(n);
                    }
                    if kind == "call" && e == "wav" {
                        call_slot_n = Some(n);
                    }
                }
                out.files.push((p, f.clone()));
            }
            None => out.notes.push(format!("not used: `{}`", f.display())),
        }
    }

    // The entries, one per name.
    let mut picked: Vec<(String, Block, bool)> = Vec::new();
    for t in &texts {
        let text = String::from_utf8_lossy(&std::fs::read(t).map_err(|source| ModError::Io {
            path: t.clone(),
            source,
        })?)
        .replace('\r', "");
        for (before, body) in blocks_in(&text) {
            let Some((section, b)) = entry(&body) else {
                continue;
            };
            if !out.texts.contains(t) {
                out.texts.push(t.clone());
            }
            let mee = before.to_ascii_lowercase().contains("modders edition");
            let id = block_id(&b);
            // A `_MENU.TXT` entry (`MENU.txt`, `MENURES.txt` beside the
            // `_RES.TXT` one: name, AI, senses) names no model: the
            // `_RES.TXT` entry is the one to take.
            let has_file = |b: &Block| b.get(&["file"]).is_some();
            match picked.iter_mut().find(|(_, p, _)| block_id(p) == id) {
                Some(slot) if has_file(&b) && !has_file(&slot.1) => *slot = (section, b, mee),
                Some(slot) if dialect && mee && !slot.2 && (has_file(&b) || !has_file(&slot.1)) => {
                    *slot = (section, b, mee)
                }
                Some(_) => {}
                None => picked.push((section, b, mee)),
            }
        }
    }
    if picked.is_empty() {
        // Nothing says what its files are for (a skin pack): add its new
        // files, replace nothing.
        let before = out.files.len();
        out.files.retain(|(p, _)| base.resolve(&key(p)).is_none());
        out.notes.push(format!(
            "no `_RES.TXT` entry: only its new files are added; {} replacements left out",
            before - out.files.len()
        ));
    }

    // What each entry changes.
    for (section, mut b, _) in picked {
        // An animal whose model is in neither the game nor the add-on
        // (entries for another game's files: Tany's `_RES.TXT` is
        // Carnivores Cordelia's, `3D/AREA1/1F.car`...) would be loaded
        // without one: left out.
        // (The hunter's model is merged into the game's, not placed.)
        let hunter_model = |b: &Block| {
            let first = res
                .section("characters")
                .and_then(|s| s.blocks.first())
                .and_then(block_id);
            if first.is_some() && block_id(b) == first {
                return true;
            }
            let file = b
                .get(&["file"])
                .map(|f| f.trim().trim_matches('\'').to_ascii_lowercase());
            out.hunter
                .as_ref()
                .and_then(|h| h.file_name())
                .is_some_and(|n| {
                    file.as_deref().is_some_and(|f| {
                        f.rsplit(['/', '\\']).next()
                            == Some(&n.to_string_lossy().to_ascii_lowercase())
                    })
                })
        };
        if section == "characters" && !hunter_model(&b) {
            let lost = if fix_model(&mut b, base, &out.files, &mut out.notes) {
                // Models an `overwrite` gives it on some maps.
                sub_models(&b)
                    .into_iter()
                    .find(|f| !model_found(f, base, &out.files))
            } else {
                Some(
                    b.get(&["file"])
                        .map_or("", |f| f.trim().trim_matches('\''))
                        .to_string(),
                )
            };
            if let Some(f) = lost {
                out.notes.push(format!(
                    "'{}' left out: its model `{f}` is in neither the game nor the add-on",
                    block_id(&b).unwrap_or_default(),
                ));
                continue;
            }
        }
        let id = block_id(&b);
        let blocks = res
            .section(&section)
            .map(|s| s.blocks.as_slice())
            .unwrap_or_default();
        let named = blocks.iter().position(|x| block_id(x) == id);
        // Mobile imports can have calls but no menu picture. Their donor
        // slot still needs moving, or they overwrite a base animal's calls
        // and disappear when "All dinos huntable" is off.
        let animal_slot = menu_slot_n.or(call_slot_n);
        let slot = match section.as_str() {
            "characters" if b.get(&["ai"]).map(c_atoi).is_some_and(|a| a >= 10) => {
                animal_slot.and_then(|n| menu_slot(base, &res, n))
            }
            "weapons" => weapon_slot_n
                .and_then(|n| n.checked_sub(1))
                .filter(|&i| i < blocks.len()),
            _ => None,
        };
        // An entry for the original game has no animation keys: that engine
        // fixes each AI's animation order. The Modder's Edition reads them
        // from the entry, so take those of the slot it was made for, or of
        // another animal with its AI (same order).
        if dialect && section == "characters" && named.is_none() && !has_anims(&b) {
            let ai = b.get(&["ai"]).map(c_atoi);
            let donor = slot
                .and_then(|i| blocks.get(i))
                .filter(|d| has_anims(d))
                .or_else(|| {
                    blocks
                        .iter()
                        .find(|d| has_anims(d) && d.get(&["ai"]).map(c_atoi) == ai)
                });
            if let Some(d) = donor {
                out.notes.push(format!(
                    "'{}' has no animation keys (an entry for the original game): uses those of '{}'",
                    id.clone().unwrap_or_default(),
                    block_id(d).unwrap_or_default()
                ));
                copy_anims(d, &mut b);
            }
        }
        let change = match (named, slot) {
            (Some(i), _) => Change::Modify {
                section,
                index: i,
                block: b,
                was: None,
            },
            (None, Some(i)) => {
                let was = block_id(&blocks[i]);
                if dialect && section == "characters" && !has_spawns(&b) {
                    let kept: Vec<_> = blocks[i]
                        .subs
                        .iter()
                        .filter(|s| is_spawn_sub(&s.name) || has_spawns(&s.block))
                        .cloned()
                        .collect();
                    if !kept.is_empty() {
                        out.notes.push(format!(
                            "'{}' has no spawn tables (an entry for the original game): keeps those of '{}'",
                            id.clone().unwrap_or_default(),
                            was.clone().unwrap_or_default()
                        ));
                        let at = b.entries.len();
                        b.subs.extend(kept.into_iter().map(|mut s| {
                            s.at = at;
                            s
                        }));
                    }
                }
                let num = if section == "characters" {
                    animal_slot
                } else {
                    weapon_slot_n
                };
                // The original engine places animals by AI number, so on its
                // data an animal can only take over a slot; weapons and the
                // Modder's Edition's animals get a slot of their own.
                let has_motion = b.entries.iter().any(|e| {
                    ["runAnim", "walkAnim", "flyAnim", "swimAnim"]
                        .iter()
                        .any(|k| e.key.eq_ignore_ascii_case(k))
                });
                if section == "characters" && !dialect && !has_motion {
                    out.notes.push(format!(
                        "takes over {section} slot {i} ('{}'), as its menu files say (the original game has no room for another)",
                        was.clone().unwrap_or_default()
                    ));
                    Change::Modify {
                        section,
                        index: i,
                        block: b,
                        was,
                    }
                } else {
                    let from = num.unwrap_or(i + 1);
                    out.slots.push(SlotAdd {
                        section: section.clone(),
                        id: id.clone().unwrap_or_default(),
                        from,
                        alongside: was.unwrap_or_default(),
                    });
                    Change::Add { section, block: b }
                }
            }
            (None, None) => Change::Add { section, block: b },
        };
        out.res_changes.push(change);
    }
    Ok(out)
}

/// Animation keys (`runAnim`, `lookAnim`...) and blocks (`idlegroup`,
/// `deathtype`, `killtype`).
fn is_anim_key(key: &str) -> bool {
    key.to_ascii_lowercase().ends_with("anim")
}

fn is_anim_sub(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    ["idlegroup", "idle2group", "deathtype", "killtype"].contains(&n.as_str())
}

pub(crate) fn has_anims(b: &Block) -> bool {
    b.entries.iter().any(|e| is_anim_key(&e.key)) || b.subs.iter().any(|s| is_anim_sub(&s.name))
}

/// Speeds as some patched games write them (`runspeed = 1.3`): another
/// engine's keys and units, which the Modder's Edition doesn't read. Its
/// own speed keys come from the donor instead, as the original engine fixes
/// speeds per AI (a raptor add-on given them walked at a crawl).
const OTHER_SPEEDS: [&str; 5] = [
    "runspeed",
    "wlkspeed",
    "walkspeed",
    "swimspeed",
    "jumpspeed",
];

/// Movement keys the Modder's Edition reads per animal (the original
/// engine fixes them per AI), taken from the donor when missing.
const MOVE_KEYS: [&str; 15] = [
    "runspd",
    "wlkspd",
    "swmspd",
    "jmpspd",
    "runrotatespeed",
    "killdist",
    "canswim",
    "waterLevel",
    "weaveRange",
    "CamYLand",
    "CamYWater",
    "CamBaseLand",
    "CamBaseWater",
    "dogSmell",
    "aggress",
];

/// Makes an entry for the original game readable by the Modder's
/// Edition: `from`'s animation keys and blocks and the movement keys it
/// lacks (same AI, so the same animation order and gait); other engines'
/// speed keys dropped.
pub(crate) fn copy_anims(from: &Block, to: &mut Block) {
    to.entries
        .retain(|e| !OTHER_SPEEDS.iter().any(|k| e.key.eq_ignore_ascii_case(k)));
    let has = |b: &Block, k: &str| b.entries.iter().any(|e| e.key.eq_ignore_ascii_case(k));
    let missing: Vec<_> = from
        .entries
        .iter()
        .filter(|e| MOVE_KEYS.iter().any(|k| e.key.eq_ignore_ascii_case(k)) && !has(to, &e.key))
        .cloned()
        .collect();
    to.entries.extend(missing);
    to.entries
        .extend(from.entries.iter().filter(|e| is_anim_key(&e.key)).cloned());
    let end = to.entries.len();
    to.subs.extend(
        from.subs
            .iter()
            .filter(|s| is_anim_sub(&s.name))
            .cloned()
            .map(|mut s| {
                s.at = end;
                s
            }),
    );
}

/// Spawn or pack tables (`spawninfo`, `spawngroup`, `packinfo`,
/// `packgroup`).
pub(crate) fn is_spawn_sub(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("spawn") || n.contains("pack")
}

/// Does `b` (or an `overwrite` / `addition` in it) say where it spawns?
pub(crate) fn has_spawns(b: &Block) -> bool {
    b.subs
        .iter()
        .any(|s| is_spawn_sub(&s.name) || has_spawns(&s.block))
}

/// Whether model `file` (from `HUNTDAT`) is in the game or the add-on.
fn model_found(file: &str, base: &Vfs, placed: &[(String, PathBuf)]) -> bool {
    let k = key(&format!("HUNTDAT/{}", file.replace('\\', "/")));
    base.resolve(&k).is_some() || placed.iter().any(|(p, _)| key(p) == k)
}

/// The models `b`'s sub-blocks name (`overwrite area1 { file = ... }`).
fn sub_models(b: &Block) -> Vec<String> {
    let mut out = Vec::new();
    for s in &b.subs {
        if let Some(f) = s.block.get(&["file"]) {
            out.push(f.trim().trim_matches('\'').to_string());
        }
        out.extend(sub_models(&s.block));
    }
    out
}

/// An entry naming a model that exists neither in the base nor in the
/// add-on (`'indor.car'` beside `INDO.CAR`): use the add-on's model whose
/// name shares the longest start with it. Whether the entry has a model
/// then (or names none: it changes an animal the game has).
fn fix_model(
    b: &mut Block,
    base: &Vfs,
    placed: &[(String, PathBuf)],
    notes: &mut Vec<String>,
) -> bool {
    let Some(file) = b
        .get(&["file"])
        .map(|f| f.trim().trim_matches('\'').to_string())
    else {
        return true;
    };
    if model_found(&file, base, placed) {
        return true;
    }
    let want = file.to_ascii_lowercase();
    let common = |n: &str| {
        want.chars()
            .zip(n.chars())
            .take_while(|(a, b)| a == b)
            .count()
    };
    let best = placed
        .iter()
        .filter(|(p, _)| key(p).ends_with(".car") && !key(p).contains("hunter"))
        .filter_map(|(p, _)| {
            let name = p.rsplit('/').next()?.to_string();
            Some((common(&name.to_ascii_lowercase()), name))
        })
        // Ties: the shorter name (the main model, not `..._alt`).
        .max_by_key(|(n, name)| (*n, std::cmp::Reverse(name.len())));
    if let Some((_, name)) = best.filter(|(n, _)| *n >= 3) {
        for e in &mut b.entries {
            if e.key.eq_ignore_ascii_case("file") {
                e.value = format!("'{name}'");
            }
        }
        notes.push(format!(
            "'{}' names `{file}`, which it doesn't ship: uses its `{name}`",
            block_id(b).unwrap_or_default()
        ));
        return true;
    }
    false
}

fn dir_ci(dir: &Path, name: &str) -> bool {
    std::fs::read_dir(dir).ok().is_some_and(|rd| {
        rd.flatten().any(|e| {
            e.file_name().to_string_lossy().eq_ignore_ascii_case(name)
                && e.file_type().is_ok_and(|t| t.is_dir())
        })
    })
}

/// A loose add-on's files under their game paths.
pub struct MapBackend {
    pub map: Vec<(String, PathBuf)>,
}

impl Backend for MapBackend {
    fn files(&self) -> Vec<String> {
        self.map.iter().map(|(k, _)| k.clone()).collect()
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let k = key(path);
        let (_, disk) =
            self.map
                .iter()
                .find(|(p, _)| key(p) == k)
                .ok_or_else(|| VfsError::NotFound {
                    path: path.to_string(),
                })?;
        std::fs::read(disk).map_err(|e| VfsError::Io {
            path: disk.display().to_string(),
            source: e,
        })
    }
    fn disk_path(&self, path: &str) -> Option<PathBuf> {
        let k = key(path);
        self.map
            .iter()
            .find(|(p, _)| key(p) == k)
            .map(|(_, d)| d.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ARK Yutyrannus: a Modder's Edition copy for each game, its menu
    /// files numbered for each game's slot. Only the base game's is taken.
    #[test]
    fn the_copy_for_the_other_game_is_left_out() {
        let dir = std::env::temp_dir().join(format!("c2-mods-games-{}", std::process::id()));
        for (d, f) in [
            ("CARNIVORES (MEE 1.1.1)", "DINO9.tga"),
            ("ICE AGE (MEE 1.1.1)", "DINO10.tga"),
        ] {
            std::fs::create_dir_all(dir.join(d)).unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(dir.join(d).join(f), b"x").unwrap_or_else(|e| panic!("{e}"));
        }
        let names = |ice: bool| {
            let (mut notes, mut out) = (Vec::new(), Vec::new());
            gather(&dir, true, ice, &mut notes, &mut out);
            out.iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>()
        };
        let (c2, ia) = (names(false), names(true));
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(c2, ["DINO9.tga"]);
        assert_eq!(ia, ["DINO10.tga"]);
    }

    #[test]
    fn finds_entries_after_prose() {
        let text = "Drop the car in.\nFor Modders Edition use this:\n\n{\n name = 'X'\n file = 'x.car'\n deathtype{\n  dieAnim = 2\n }\n}\nThanks!\n";
        let b = blocks_in(text);
        assert_eq!(b.len(), 1);
        assert!(b[0].0.contains("Modders Edition"));
        let (section, block) = entry(&b[0].1).unwrap_or_else(|| panic!("no entry"));
        assert_eq!(section, "characters");
        assert_eq!(block.subs.len(), 1);
        assert_eq!(numbered("call7_a.wav"), Some(("call".into(), 7)));
        assert_eq!(numbered("dino10.txm"), Some(("dino".into(), 10)));
        assert_eq!(numbered("hunter1_brach.car"), Some(("hunter".into(), 1)));
    }
}
