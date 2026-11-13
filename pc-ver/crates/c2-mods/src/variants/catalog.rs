//! What the variant editor can pick: the huntable land characters of the
//! base game, of each standalone and of each add-on (by source and name),
//! and the maps (by source and area stem). Read the way a build reads them,
//! so the choices match what a recipe can reference. A standalone's are
//! named as in the game itself, without the parts left out of it.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use c2_formats::res_txt::{c_atoi, Block, ResTxt};
use c2_vfs::Vfs;

use super::{canonical, AreaId};
use crate::pack::{build, load_mod, vanilla};
use crate::res_merge::Change;
use crate::standalone::Skip;
use crate::ModError;

/// Huntable land animals have AI 10 to 29 (`balance` uses the same bound).
const LAND_AI: i32 = 10;

/// The source ID of the base game in recipe references.
pub const BASE: &str = "base";

/// A character a group member can be. Its identity is `(source, name)`
/// compared without case or quotes, as `res_merge::block_id` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dino {
    /// Recipe source ID, or `base`.
    pub source: String,
    /// The source's name for people (its manifest name, or "Base game").
    pub source_name: String,
    /// The character's name as its block writes it.
    pub name: String,
}

impl Dino {
    /// The identity used to recognise the same character across lists.
    pub fn key(&self) -> (String, String) {
        (self.source.clone(), canonical(&self.name))
    }
}

/// A map a distribution can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapChoice {
    pub map: AreaId,
    /// The first line of the map's menu text, or its area stem.
    pub title: String,
    pub source_name: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub dinos: Vec<Dino>,
    pub maps: Vec<MapChoice>,
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('\'').trim().to_string()
}

fn read_res(v: &Vfs, path: &str) -> Option<ResTxt> {
    let data = v.read(path).ok()?;
    ResTxt::parse(&String::from_utf8_lossy(&data)).ok()
}

fn name_of(b: &Block) -> Option<String> {
    b.get(&["name"]).map(unquote).filter(|n| !n.is_empty())
}

fn ai_of(b: &Block) -> Option<i32> {
    b.get(&["ai"]).map(c_atoi)
}

/// The first line of a map's menu text, if it has one.
fn title(v: &Vfs, stem: &str) -> Option<String> {
    let d = v
        .read(&format!(
            "HUNTDAT/MENU/TXT/{}.TXT",
            stem.to_ascii_uppercase()
        ))
        .ok()?;
    String::from_utf8_lossy(&d)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// A standalone's kept huntables and maps (`crate::standalone::parts`).
fn standalone(id: &str, dir: &std::path::Path, skip: &Skip, c: &mut Catalog) {
    let Some(parts) = crate::standalone::parts(dir) else {
        return;
    };
    let source_name = dir
        .file_name()
        .map_or_else(|| id.to_string(), |n| n.to_string_lossy().into_owned());
    for n in Skip::kept(&skip.huntables, parts.huntables.len()) {
        c.dinos.push(Dino {
            source: id.into(),
            source_name: source_name.clone(),
            name: parts.huntables[n as usize - 1].clone(),
        });
    }
    let v = vanilla(&[dir.to_path_buf()]).ok();
    for n in Skip::kept(&skip.areas, parts.areas) {
        let stem = format!("area{n}");
        c.maps.push(MapChoice {
            title: v
                .as_ref()
                .and_then(|v| title(v, &stem))
                .unwrap_or_else(|| stem.clone()),
            map: AreaId {
                source: id.into(),
                area: stem,
            },
            source_name: source_name.clone(),
        });
    }
}

/// Lists the choices for the base game (`bases`), standalones combined
/// beside it (`standalones`: recipe source ID, folder and what of it is
/// left out, in combine order) and add-ons (`addons`: recipe source ID and
/// folder, in load order). Reads the add-ons and builds them once, which is
/// what the editor needs to show maps.
pub fn catalog(
    bases: &[PathBuf],
    standalones: &[(String, PathBuf, Skip)],
    addons: &[(String, PathBuf)],
) -> Result<Catalog, ModError> {
    let v = vanilla(bases)?;
    // The menu's character list decides what is huntable, as in `balance`.
    let base_list = read_res(&v, "HUNTDAT/_MENU.TXT").or_else(|| read_res(&v, "HUNTDAT/_RES.TXT"));
    let mut base_ai: HashMap<String, i32> = HashMap::new();
    let mut dinos = Vec::new();
    for b in base_list
        .as_ref()
        .and_then(|r| r.section("characters"))
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default()
    {
        let (Some(name), Some(ai)) = (name_of(b), ai_of(b)) else {
            continue;
        };
        base_ai.insert(canonical(&name), ai);
        if ai >= LAND_AI {
            dinos.push(Dino {
                source: BASE.into(),
                source_name: "Base game".into(),
                name,
            });
        }
    }

    let mut mods = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for (id, dir) in addons {
        let mut m = load_mod(&v, dir)?;
        m.variant_origin = Some((id.clone(), None));
        names.insert(id.clone(), m.manifest.info.name.clone());
        mods.push(m);
    }
    for m in &mods {
        let Some((source, _)) = &m.variant_origin else {
            continue;
        };
        for c in &m.res_changes {
            let block = match c {
                Change::Add { section, block } | Change::Modify { section, block, .. }
                    if section.eq_ignore_ascii_case("characters") =>
                {
                    block
                }
                _ => continue,
            };
            let Some(name) = name_of(block) else {
                continue;
            };
            let ai = ai_of(block)
                .or_else(|| base_ai.get(&canonical(&name)).copied())
                .unwrap_or(0);
            if ai >= LAND_AI {
                dinos.push(Dino {
                    source: source.clone(),
                    source_name: names.get(source).cloned().unwrap_or_else(|| source.clone()),
                    name,
                });
            }
        }
    }

    // Build once: the layers say which sources provide each map.
    let pack = build(bases, mods)?;
    let mut found: BTreeMap<(String, String), ()> = BTreeMap::new();
    for (path, res) in pack.vfs.files() {
        let up = path.to_ascii_uppercase();
        let Some(file) = up
            .strip_prefix("HUNTDAT/AREAS/")
            .and_then(|f| f.strip_suffix(".MAP"))
        else {
            continue;
        };
        if file.contains('/') {
            continue;
        }
        for &layer in &res.providers {
            let source = if layer < pack.bases {
                BASE.to_string()
            } else {
                match pack
                    .mods
                    .get(layer - pack.bases)
                    .and_then(|m| m.variant_origin.as_ref())
                {
                    Some((id, _)) => id.clone(),
                    None => continue,
                }
            };
            found.insert((source, file.to_ascii_lowercase()), ());
        }
    }
    let maps: Vec<MapChoice> = found
        .into_keys()
        .map(|(source, stem)| {
            let title = title(&pack.vfs, &stem).unwrap_or_else(|| stem.clone());
            let source_name = if source == BASE {
                "Base game".to_string()
            } else {
                names
                    .get(&source)
                    .cloned()
                    .unwrap_or_else(|| source.clone())
            };
            MapChoice {
                map: AreaId { source, area: stem },
                title,
                source_name,
            }
        })
        .collect();
    // The standalones' after the base's (the combine's order), before the
    // add-ons'.
    let mut own = Catalog::default();
    for (id, dir, skip) in standalones {
        standalone(id, dir, skip, &mut own);
    }
    let (mut dinos, rest): (Vec<_>, Vec<_>) = dinos.into_iter().partition(|d| d.source == BASE);
    dinos.extend(own.dinos.into_iter().chain(rest));
    let (mut maps, rest): (Vec<_>, Vec<_>) = maps
        .into_iter()
        .partition(|m: &MapChoice| m.map.source == BASE);
    maps.extend(own.maps.into_iter().chain(rest));
    Ok(Catalog { dinos, maps })
}
