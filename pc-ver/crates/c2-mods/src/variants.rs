//! Recipe-side variant identities and map-conditioned probability rules.
//! These are authoring data, not `_RES.TXT` extensions.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ModError;

mod pack;
pub(crate) use pack::{finish, preserve};

pub const PATH: &str = "HUNTDAT/VARIANTS.TOML";

/// Versioned, resolved pack data. Character indices are checked against the
/// private model path when loaded; authoring references never use indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packed {
    pub version: u32,
    pub groups: Vec<PackedGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackedGroup {
    pub rule: Group,
    pub owner: usize,
    pub slot: usize,
    pub members: Vec<PackedMember>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackedMember {
    pub index: usize,
    pub name: String,
    pub file: String,
    pub token: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaId {
    /// Recipe source ID, or `base` for the base game.
    pub source: String,
    /// Source area file stem, independent of packed menu numbering.
    pub area: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapWeight {
    pub map: AreaId,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// Stable within the group; also retained with a trophy.
    pub id: String,
    pub source: String,
    /// Stable collection child ID; never a generated `--part-N` index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    /// A source character name, using `res_merge::block_id` normalization.
    pub character: String,
    #[serde(default = "one")]
    pub weight: f64,
    /// None allows every map; Some([]) excludes every map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maps: Option<Vec<AreaId>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_weights: Vec<MapWeight>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    /// Member ID providing the shared menu entry, picture, price and call.
    pub menu_owner: String,
    pub members: Vec<Member>,
}

fn canonical(s: &str) -> String {
    s.trim().trim_matches('\'').trim().to_ascii_lowercase()
}

fn bad(message: impl Into<String>) -> ModError {
    ModError::Recipe(format!("variants: {}", message.into()))
}

fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_weight(weight: f64) -> bool {
    weight.is_finite() && weight >= 0.0
}

/// Validate authoring rules without reading game assets. Reference resolution
/// must additionally check that sources, characters and areas actually exist.
pub fn validate(groups: &[Group]) -> Result<(), ModError> {
    let mut group_ids = BTreeSet::new();
    let mut characters = BTreeSet::new();
    for g in groups {
        if !valid_id(&g.id) || !group_ids.insert(&g.id) {
            return Err(bad(format!("invalid or duplicate group ID '{}'", g.id)));
        }
        if g.members.is_empty() {
            return Err(bad(format!("group '{}' has no members", g.id)));
        }
        let mut member_ids = BTreeSet::new();
        for m in &g.members {
            if !valid_id(&m.id) || !member_ids.insert(&m.id) {
                return Err(bad(format!(
                    "group '{}': invalid or duplicate member ID '{}'",
                    g.id, m.id
                )));
            }
            if m.source.trim().is_empty()
                || m.child.as_ref().is_some_and(|s| s.trim().is_empty())
                || canonical(&m.character).is_empty()
            {
                return Err(bad(format!("member '{}': empty character identity", m.id)));
            }
            if !characters.insert((&m.source, &m.child, canonical(&m.character))) {
                return Err(bad(format!(
                    "member '{}': character belongs to more than one member",
                    m.id
                )));
            }
            if !valid_weight(m.weight) || m.map_weights.iter().any(|w| !valid_weight(w.weight)) {
                return Err(bad(format!(
                    "member '{}': weights must be finite and nonnegative",
                    m.id
                )));
            }
            let check_map = |map: &AreaId| -> Result<(), ModError> {
                if map.source.trim().is_empty() || !valid_id(&map.area) {
                    Err(bad(format!("member '{}': invalid map identity", m.id)))
                } else {
                    Ok(())
                }
            };
            if let Some(maps) = &m.maps {
                let mut seen = BTreeSet::new();
                for map in maps {
                    check_map(map)?;
                    if !seen.insert(map) {
                        return Err(bad(format!("member '{}': duplicate eligible map", m.id)));
                    }
                }
            }
            let mut overrides = BTreeSet::new();
            for w in &m.map_weights {
                check_map(&w.map)?;
                if !overrides.insert(&w.map) {
                    return Err(bad(format!("member '{}': duplicate map weight", m.id)));
                }
                if m.maps.as_ref().is_some_and(|maps| !maps.contains(&w.map)) {
                    return Err(bad(format!(
                        "member '{}': weight override for an excluded map",
                        m.id
                    )));
                }
            }
        }
        if !member_ids.contains(&g.menu_owner) {
            return Err(bad(format!(
                "group '{}': menu_owner '{}' is not a member",
                g.id, g.menu_owner
            )));
        }
    }
    Ok(())
}

impl Group {
    /// Effective probabilities in persisted member order. All-zero results
    /// deliberately mean no spawn, with no fallback to an excluded member.
    pub fn probabilities(&self, map: &AreaId) -> Result<Vec<f64>, ModError> {
        validate(std::slice::from_ref(self))?;
        let weights: Vec<f64> = self
            .members
            .iter()
            .map(|m| {
                if m.maps.as_ref().is_some_and(|maps| !maps.contains(map)) {
                    return 0.0;
                }
                let overrides: BTreeMap<_, _> =
                    m.map_weights.iter().map(|w| (&w.map, w.weight)).collect();
                overrides.get(map).copied().unwrap_or(m.weight)
            })
            .collect();
        // Scale first so even multiple f64::MAX weights normalize correctly.
        let max = weights.iter().copied().fold(0.0, f64::max);
        if max == 0.0 {
            return Ok(weights);
        }
        let total: f64 = weights.iter().map(|w| w / max).sum();
        Ok(weights.iter().map(|w| (w / max) / total).collect())
    }
}
