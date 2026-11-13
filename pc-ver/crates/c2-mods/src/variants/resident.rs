//! Members whose character is already in the game a build starts from: the
//! base game's own, and a standalone's once it is combined beside the base
//! (`crate::standalone`). They are not add-ons, so there is no `ModSource`
//! to take them from; the combined game's `_RES.TXT` holds them, under the
//! names the combine gave them.

use std::collections::BTreeMap;

use c2_formats::res_txt::{c_atoi, Block, ResTxt};
use c2_vfs::{MemBackend, Vfs};

use super::{bad, canonical, Group, BASE};
use crate::res_merge::block_id;
use crate::standalone::Unlisted;
use crate::ModError;

/// A source already in the game a build starts from, and where its parts
/// are there (`crate::standalone::Built::residents`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resident {
    /// Recipe source ID, or `base`.
    pub source: String,
    /// Character name in the source (canonical) -> name in the game.
    pub characters: Vec<(String, String)>,
    /// Area stem in the source -> stem in the game.
    pub areas: Vec<(String, String)>,
}

/// The name a non-owner member's character is given: no menu entry has it,
/// so the member shares its group owner's entry.
pub fn private_name(group: &str, member: &str) -> String {
    format!("__variant_{group}_{member}")
}

/// The non-owner members of standalone games (`ids`: the standalones'
/// recipe source IDs, in combine order), per game: their huntables are
/// taken off the menu as the games are combined.
pub fn unlisted(groups: &[Group], ids: &[String]) -> Vec<Vec<Unlisted>> {
    ids.iter()
        .map(|id| {
            groups
                .iter()
                .flat_map(|g| {
                    g.members
                        .iter()
                        .filter(|m| &m.source == id && m.id != g.menu_owner)
                        .map(|m| Unlisted {
                            name: m.character.clone(),
                            rename: private_name(&g.id, &m.id),
                        })
                })
                .collect()
        })
        .collect()
}

/// Where `source`'s map `area` is in the game: `Ok(None)` when the source
/// is not resident (an add-on, or `base` with no standalones combined).
pub(super) fn area(
    residents: &[Resident],
    source: &str,
    area: &str,
) -> Result<Option<String>, ModError> {
    let Some(r) = residents.iter().find(|r| r.source == source) else {
        return Ok(None);
    };
    let want = canonical(area);
    r.areas
        .iter()
        .find(|(own, _)| canonical(own) == want)
        .map(|(_, to)| Some(to.clone()))
        .ok_or_else(|| {
            bad(format!(
                "map {source}:{area} is left out or not in that game"
            ))
        })
}

/// What [`resolve`] found for a member.
pub(super) struct Found {
    /// Its character's name in the game (canonical): the merge key.
    pub key: String,
    /// Its character, with model and sounds moved to the private namespace.
    pub block: Block,
    /// Each file value moved: as in the game -> private.
    pub swaps: Vec<(String, String)>,
}

/// A resident member's character, its assets copied under `prefix` into
/// `out`. `None` when its source is not resident (an add-on).
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve(
    residents: &[Resident],
    g: &Group,
    member: &super::Member,
    res: &ResTxt,
    base: &Vfs,
    prefix: &str,
    out: &mut MemBackend,
) -> Result<Option<Found>, ModError> {
    let r = residents.iter().find(|r| r.source == member.source);
    if r.is_none() && member.source != BASE {
        return Ok(None);
    }
    let at = format!("{}:{}", g.id, member.id);
    let own = canonical(&member.character);
    let key = match r {
        Some(r) => r
            .characters
            .iter()
            .find(|(n, _)| *n == own)
            .map(|(_, to)| canonical(to))
            .ok_or_else(|| {
                bad(format!(
                    "{at}: '{}' is not a character of {} (left out of it?)",
                    member.character, member.source
                ))
            })?,
        None => own,
    };
    let owner = member.id == g.menu_owner;
    if !owner {
        // An add-on's character is added beside the menu animal; a resident
        // one already has a menu entry, which the combine must have taken.
        if member.source == BASE {
            return Err(bad(format!(
                "{at}: a base game animal can only be its group's menu animal"
            )));
        }
        if key != private_name(&g.id, &member.id) {
            return Err(bad(format!(
                "{at}: the standalone's animal was not taken off its menu"
            )));
        }
    }
    let found: Vec<&Block> = res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|b| block_id(b).as_deref() == Some(&key))
        .collect();
    let [original] = found.as_slice() else {
        return Err(bad(format!(
            "{at}: expected one character '{key}', found {}",
            found.len()
        )));
    };
    if !(10..30).contains(&original.get(&["ai"]).map_or(0, c_atoi)) {
        return Err(bad(format!("{at}: variants require a huntable character")));
    }
    let mut block = (*original).clone();
    super::pack::assets(&mut block, None, base, prefix, out)?;
    let mut swaps = BTreeMap::new();
    moved(original, &block, &mut swaps);
    Ok(Some(Found {
        key,
        block,
        swaps: swaps.into_iter().collect(),
    }))
}

fn value(v: &str) -> String {
    v.trim().trim_matches('\'').to_string()
}

/// The values `assets` changed between `a` and `b` (same shape).
fn moved(a: &Block, b: &Block, out: &mut BTreeMap<String, String>) {
    for (x, y) in a.entries.iter().zip(&b.entries) {
        if x.value != y.value {
            out.insert(value(&x.value), value(&y.value));
        }
    }
    for (x, y) in a.subs.iter().zip(&b.subs) {
        moved(&x.block, &y.block, out);
    }
}

/// Moves `b`'s file values as `swaps` say (after merging, so other changes
/// to the character are kept).
pub(super) fn swap(b: &mut Block, swaps: &[(String, String)]) {
    for e in &mut b.entries {
        let v = value(&e.value);
        if let Some((_, to)) = swaps.iter().find(|(from, _)| *from == v) {
            e.value = format!("'{to}'");
        }
    }
    for s in &mut b.subs {
        swap(&mut s.block, swaps);
    }
}
