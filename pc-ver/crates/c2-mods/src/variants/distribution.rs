//! Spawn percentages. A member's weight is its percentage of spawns on a
//! map: its override for that map, else its default weight where its map
//! limit (if any) allows the map. Every distribution a group uses must total
//! 100: the default distribution (maps with no override or limit) and each
//! map that has an override or is named in a limit.

use super::{AreaId, Group, Member};
use crate::ModError;

/// Totals must be this, within `EPSILON`.
pub const TOTAL: f64 = 100.0;
const EPSILON: f64 = 1e-6;

/// The member's percentage on `map`; `None` is the default distribution.
pub fn weight_on(m: &Member, map: Option<&AreaId>) -> f64 {
    if let Some(map) = map {
        if let Some(w) = m.map_weights.iter().find(|w| &w.map == map) {
            return w.weight;
        }
    }
    let allowed = match (&m.maps, map) {
        (None, _) => true,
        (Some(list), Some(map)) => list.contains(map),
        (Some(_), None) => false,
    };
    if allowed {
        m.weight
    } else {
        0.0
    }
}

/// One distribution of a group and its total. `map` is `None` for the
/// default distribution.
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    pub map: Option<AreaId>,
    pub total: f64,
}

/// Every distribution the group uses: the default one first, then each map
/// that has an override or a limit, in first-mention order.
pub fn distributions(g: &Group) -> Vec<Distribution> {
    let mut maps: Vec<AreaId> = Vec::new();
    for m in &g.members {
        let named = m
            .maps
            .iter()
            .flatten()
            .chain(m.map_weights.iter().map(|w| &w.map));
        for id in named {
            if !maps.contains(id) {
                maps.push(id.clone());
            }
        }
    }
    let total = |map: Option<&AreaId>| -> f64 { g.members.iter().map(|m| weight_on(m, map)).sum() };
    std::iter::once(Distribution {
        map: None,
        total: total(None),
    })
    .chain(maps.into_iter().map(|id| Distribution {
        total: total(Some(&id)),
        map: Some(id),
    }))
    .collect()
}

/// Every group's distributions must total 100. Nothing is normalized: the
/// percentages are checked as written.
pub fn check_totals(groups: &[Group]) -> Result<(), ModError> {
    for g in groups {
        for d in distributions(g) {
            if (d.total - TOTAL).abs() > EPSILON {
                let which = match &d.map {
                    None => "the default distribution".to_string(),
                    Some(map) => format!("map {}:{}", map.source, map.area),
                };
                return Err(ModError::Recipe(format!(
                    "variants: group '{}': {which} totals {:.2}%, not {TOTAL:.0}%",
                    g.id, d.total
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::MapWeight;
    use super::*;

    fn map(area: &str) -> AreaId {
        AreaId {
            source: "base".into(),
            area: area.into(),
        }
    }

    fn member(id: &str, weight: f64) -> Member {
        Member {
            id: id.into(),
            source: "pack".into(),
            child: None,
            character: id.into(),
            weight,
            maps: None,
            map_weights: Vec::new(),
        }
    }

    fn group(members: Vec<Member>) -> Group {
        Group {
            id: "parent".into(),
            menu_owner: "parent".into(),
            members,
        }
    }

    #[test]
    fn default_percentages_must_total_one_hundred() {
        let g = group(vec![
            member("parent", 50.0),
            member("a", 25.0),
            member("b", 25.0),
        ]);
        assert!(check_totals(std::slice::from_ref(&g)).is_ok());
        let short = group(vec![member("parent", 50.0), member("a", 25.0)]);
        let err = check_totals(&[short]).expect_err("75% is not valid");
        assert!(err.to_string().contains("75.00%"), "{err}");
    }

    #[test]
    fn a_map_override_is_its_own_distribution() {
        let mut parent = member("parent", 50.0);
        parent.map_weights.push(MapWeight {
            map: map("area2"),
            weight: 0.0,
        });
        let mut a = member("a", 25.0);
        a.map_weights.push(MapWeight {
            map: map("area2"),
            weight: 50.0,
        });
        let mut b = member("b", 25.0);
        b.map_weights.push(MapWeight {
            map: map("area2"),
            weight: 50.0,
        });
        let g = group(vec![parent, a, b]);
        let totals = distributions(&g);
        assert_eq!(totals[0].map, None);
        assert_eq!(totals[0].total, 100.0);
        assert_eq!(totals[1].map, Some(map("area2")));
        assert_eq!(totals[1].total, 100.0);
        assert!(check_totals(&[g]).is_ok());
    }

    #[test]
    fn a_zero_override_disables_a_member_on_that_map() {
        let mut parent = member("parent", 50.0);
        parent.map_weights.push(MapWeight {
            map: map("area3"),
            weight: 0.0,
        });
        let mut a = member("a", 50.0);
        a.map_weights.push(MapWeight {
            map: map("area3"),
            weight: 100.0,
        });
        let g = group(vec![parent, a]);
        assert_eq!(weight_on(&g.members[0], Some(&map("area3"))), 0.0);
        assert_eq!(weight_on(&g.members[0], Some(&map("area1"))), 50.0);
        assert!(check_totals(&[g]).is_ok());
    }

    #[test]
    fn an_override_map_that_misses_one_hundred_is_reported() {
        let mut a = member("a", 50.0);
        a.map_weights.push(MapWeight {
            map: map("area2"),
            weight: 60.0,
        });
        let g = group(vec![member("parent", 50.0), a]);
        let err = check_totals(&[g]).expect_err("area2 totals 110%");
        assert!(err.to_string().contains("base:area2"), "{err}");
    }

    #[test]
    fn a_limited_member_counts_only_on_its_maps() {
        let mut a = member("a", 50.0);
        a.maps = Some(vec![map("area1")]);
        let g = group(vec![member("parent", 50.0), a]);
        let totals = distributions(&g);
        assert_eq!(
            totals[0].total, 50.0,
            "the default misses the limited member"
        );
        assert_eq!(totals[1].map, Some(map("area1")));
        assert_eq!(totals[1].total, 100.0);
    }
}
