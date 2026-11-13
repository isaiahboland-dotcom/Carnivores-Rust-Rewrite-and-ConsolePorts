//! Variant groups as the editor works with them: dinosaurs and percentages.
//! The functions change the recipe's `variants` only. Group and member IDs
//! are derived from the dinosaur's identity and kept hidden; no asset is
//! read or written.

use c2_mods::recipe::Recipe;
use c2_mods::variants::{canonical, AreaId, Dino, Group, MapWeight, Member, TOTAL};

/// A dinosaur's identity in a recipe: its source and canonical name.
pub fn key_of(m: &Member) -> (String, String) {
    (m.source.clone(), canonical(&m.character))
}

/// The identities placed in some group, except the row at `skip`
/// (`(group, member)`), which may keep its own choice.
fn used(r: &Recipe, skip: Option<(usize, usize)>) -> Vec<(String, String)> {
    let mut keys = Vec::new();
    for (gi, g) in r.variants.iter().enumerate() {
        for (mi, m) in g.members.iter().enumerate() {
            if skip != Some((gi, mi)) && !m.character.trim().is_empty() {
                keys.push(key_of(m));
            }
        }
    }
    keys
}

/// The dinosaurs a row may choose: those no other row uses.
pub fn choices<'a>(dinos: &'a [Dino], r: &Recipe, skip: Option<(usize, usize)>) -> Vec<&'a Dino> {
    let used = used(r, skip);
    dinos.iter().filter(|d| !used.contains(&d.key())).collect()
}

/// A hidden ID from a dinosaur's identity, lowercase and hyphenated.
fn slug(d: &Dino) -> String {
    let mut out = String::new();
    for c in format!("{}-{}", d.source, d.name).chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "dinosaur".into()
    } else {
        out.to_string()
    }
}

/// `base`, or `base-2`, `base-3`, ... whichever `taken` doesn't report.
fn unique(base: String, taken: impl Fn(&str) -> bool) -> String {
    if !taken(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let id = format!("{base}-{n}");
        if !taken(&id) {
            return id;
        }
        n += 1;
    }
}

fn member(id: String, source: &str, name: &str, weight: f64) -> Member {
    Member {
        id,
        source: source.into(),
        child: None,
        character: name.into(),
        weight,
        maps: None,
        map_weights: Vec::new(),
    }
}

/// A new group whose menu animal is `dino`, at the full 100%.
pub fn add_group(r: &mut Recipe, dino: &Dino) {
    let taken: Vec<String> = r
        .variants
        .iter()
        .flat_map(|g| std::iter::once(g.id.clone()).chain(g.members.iter().map(|m| m.id.clone())))
        .collect();
    let id = unique(slug(dino), |s| taken.iter().any(|t| t == s));
    r.variants.push(Group {
        id: id.clone(),
        menu_owner: id.clone(),
        members: vec![member(id, &dino.source, &dino.name, TOTAL)],
    });
}

/// Replaces group `gi`'s menu animal. The group takes the new animal's ID,
/// as it is identified by its parent. The parent's percentage is kept.
pub fn set_parent(r: &mut Recipe, gi: usize, dino: &Dino) {
    // Every ID in use, except this card's own group ID and parent ID.
    let mut taken: Vec<String> = Vec::new();
    for (i, g) in r.variants.iter().enumerate() {
        if i != gi {
            taken.push(g.id.clone());
        }
        for m in &g.members {
            if i != gi || m.id != g.menu_owner {
                taken.push(m.id.clone());
            }
        }
    }
    let id = unique(slug(dino), |s| taken.iter().any(|t| t == s));
    let Some(g) = r.variants.get_mut(gi) else {
        return;
    };
    let owner_id = g.menu_owner.clone();
    if let Some(owner) = g.members.iter_mut().find(|m| m.id == owner_id) {
        owner.id = id.clone();
        owner.source = dino.source.clone();
        owner.character = dino.name.clone();
    }
    g.menu_owner = id.clone();
    g.id = id;
}

/// Removes group `gi`. Only its recipe entry goes; the dinosaurs' folders
/// and assets stay where they are.
pub fn remove_group(r: &mut Recipe, gi: usize) {
    if gi < r.variants.len() {
        r.variants.remove(gi);
    }
}

/// Adds an empty variant row to group `gi` (0%, no dinosaur yet).
pub fn add_variant(r: &mut Recipe, gi: usize) {
    let Some(g) = r.variants.get_mut(gi) else {
        return;
    };
    let id = unique("row".into(), |s| g.members.iter().any(|m| m.id == s));
    g.members.push(member(id, "", "", 0.0));
}

/// Gives row `mi` of group `gi` the dinosaur `dino`, keeping its percentage.
pub fn set_variant(r: &mut Recipe, gi: usize, mi: usize, dino: &Dino) {
    let Some(g) = r.variants.get_mut(gi) else {
        return;
    };
    if mi >= g.members.len() || g.members[mi].id == g.menu_owner {
        return;
    }
    let taken: Vec<String> = g
        .members
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != mi)
        .map(|(_, m)| m.id.clone())
        .collect();
    let id = unique(slug(dino), |s| taken.iter().any(|t| t == s));
    let m = &mut g.members[mi];
    m.id = id;
    m.source = dino.source.clone();
    m.character = dino.name.clone();
}

/// Removes row `mi` of group `gi`. The menu animal's row can't be removed:
/// remove the group instead.
pub fn remove_variant(r: &mut Recipe, gi: usize, mi: usize) {
    let Some(g) = r.variants.get_mut(gi) else {
        return;
    };
    if mi < g.members.len() && g.members[mi].id != g.menu_owner {
        g.members.remove(mi);
    }
}

/// Sets `m`'s percentage on `map`, adding the override if it isn't there.
pub fn set_override(m: &mut Member, map: AreaId, percent: f64) {
    match m.map_weights.iter_mut().find(|w| w.map == map) {
        Some(w) => w.weight = percent,
        None => m.map_weights.push(MapWeight {
            map,
            weight: percent,
        }),
    }
}

/// Lifts a member's map limit so it can spawn on every map again.
pub fn clear_limit(m: &mut Member) {
    m.maps = None;
}

/// Maps that `m` has no override for yet, in catalog order.
pub fn free_maps(maps: &[c2_mods::variants::MapChoice], m: &Member) -> Vec<AreaId> {
    maps.iter()
        .map(|c| c.map.clone())
        .filter(|id| !m.map_weights.iter().any(|w| &w.map == id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_mods::variants::{distributions, BASE};

    fn dino(source: &str, name: &str) -> Dino {
        Dino {
            source: source.into(),
            source_name: source.into(),
            name: name.into(),
        }
    }

    /// Stegosaurus from the base game, and two Stegosaurus skins.
    fn catalog() -> Vec<Dino> {
        vec![
            dino(BASE, "Stegosaurus"),
            dino("skin-a", "Stegosaurus"),
            dino("skin-b", "Stegosaurus"),
            dino("skin-a", "Raptor"),
        ]
    }

    #[test]
    fn stegosaurus_with_two_skins_at_50_25_25_saves_and_reopens() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[0]);
        add_variant(&mut r, 0);
        add_variant(&mut r, 0);
        set_variant(&mut r, 0, 1, &all[1]);
        set_variant(&mut r, 0, 2, &all[2]);
        r.variants[0].members[0].weight = 50.0;
        r.variants[0].members[1].weight = 25.0;
        r.variants[0].members[2].weight = 25.0;

        assert_eq!(distributions(&r.variants[0])[0].total, 100.0);
        assert!(c2_mods::variants::check_totals(&r.variants).is_ok());
        let text = r.to_toml();
        let back = Recipe::parse(&text).expect("reopen");
        assert_eq!(back.variants, r.variants);
        assert_eq!(back.variants[0].id, "base-stegosaurus");
        assert_eq!(back.variants[0].menu_owner, "base-stegosaurus");
    }

    #[test]
    fn a_dinosaur_belongs_to_one_row_at_a_time() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[0]);
        add_variant(&mut r, 0);
        set_variant(&mut r, 0, 1, &all[1]);
        // Another row cannot pick the skin; the row holding it still can.
        let for_new_row = choices(&all, &r, None);
        assert!(!for_new_row.iter().any(|d| d.key() == all[1].key()));
        let for_holder = choices(&all, &r, Some((0, 1)));
        assert!(for_holder.iter().any(|d| d.key() == all[1].key()));
        assert!(!for_holder.iter().any(|d| d.key() == all[0].key()));
    }

    #[test]
    fn changing_the_parent_moves_the_group_identity_and_keeps_the_percentage() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[0]);
        r.variants[0].members[0].weight = 40.0;
        set_parent(&mut r, 0, &all[3]);
        let g = &r.variants[0];
        assert_eq!(g.id, "skin-a-raptor");
        assert_eq!(g.menu_owner, g.id);
        assert_eq!(g.members[0].character, "Raptor");
        assert_eq!(g.members[0].weight, 40.0);
    }

    #[test]
    fn identities_stay_unique_when_names_collide() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[1]);
        add_group(&mut r, &all[1]);
        assert_eq!(r.variants[0].id, "skin-a-stegosaurus");
        assert_eq!(r.variants[1].id, "skin-a-stegosaurus-2");
    }

    #[test]
    fn removing_a_group_leaves_the_rest_of_the_recipe_alone() {
        let all = catalog();
        let mut r = Recipe::default();
        r.recipe.name = "kept".into();
        add_group(&mut r, &all[0]);
        add_group(&mut r, &all[3]);
        remove_group(&mut r, 0);
        assert_eq!(r.variants.len(), 1);
        assert_eq!(r.variants[0].id, "skin-a-raptor");
        assert_eq!(r.recipe.name, "kept");
    }

    #[test]
    fn the_menu_animal_row_cannot_be_removed_as_a_variant() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[0]);
        add_variant(&mut r, 0);
        remove_variant(&mut r, 0, 0);
        assert_eq!(r.variants[0].members.len(), 2);
        remove_variant(&mut r, 0, 1);
        assert_eq!(r.variants[0].members.len(), 1);
    }

    #[test]
    fn map_overrides_are_added_updated_and_checked() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[0]);
        add_variant(&mut r, 0);
        set_variant(&mut r, 0, 1, &all[1]);
        r.variants[0].members[0].weight = 50.0;
        r.variants[0].members[1].weight = 50.0;
        let area2 = AreaId {
            source: BASE.into(),
            area: "area2".into(),
        };
        // Off on area2: the parent 0%, the skin takes the whole map.
        set_override(&mut r.variants[0].members[0], area2.clone(), 0.0);
        set_override(&mut r.variants[0].members[1], area2.clone(), 100.0);
        set_override(&mut r.variants[0].members[1], area2.clone(), 100.0);
        assert_eq!(r.variants[0].members[1].map_weights.len(), 1);
        assert!(c2_mods::variants::check_totals(&r.variants).is_ok());

        set_override(&mut r.variants[0].members[1], area2.clone(), 60.0);
        let err = c2_mods::variants::check_totals(&r.variants).expect_err("area2 at 60%");
        assert!(err.to_string().contains("base:area2"), "{err}");
    }

    #[test]
    fn a_limit_can_be_lifted() {
        let all = catalog();
        let mut r = Recipe::default();
        add_group(&mut r, &all[0]);
        add_variant(&mut r, 0);
        r.variants[0].members[1].maps = Some(vec![]);
        clear_limit(&mut r.variants[0].members[1]);
        assert!(r.variants[0].members[1].maps.is_none());
    }
}
