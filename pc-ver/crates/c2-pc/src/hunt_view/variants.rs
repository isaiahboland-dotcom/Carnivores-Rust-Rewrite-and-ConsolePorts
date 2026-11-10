use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use c2_core::dino::{variants::Variant, Species};
use c2_mods::variants::{AreaId, Packed};

/// Load before placement, after dialect tuning, so every spawn route shares
/// the same map-conditioned candidate list and shared target/call identity.
pub(super) fn load(huntdat: &Path, map: &Path, species: &mut [Species]) -> Result<()> {
    let Some(path) = crate::area_view::find_file(huntdat, "VARIANTS.TOML") else {
        return Ok(());
    };
    let packed: Packed = toml::from_str(&std::fs::read_to_string(&path)?)
        .with_context(|| format!("reading {}", path.display()))?;
    ensure!(
        packed.version == 1,
        "unsupported variant side-file version {}",
        packed.version
    );
    let rules: Vec<_> = packed.groups.iter().map(|g| g.rule.clone()).collect();
    c2_mods::variants::validate(&rules)?;
    let area = AreaId {
        source: "base".into(),
        area: map
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase(),
    };
    let mut used = BTreeSet::new();
    let mut tokens = BTreeSet::new();
    // Validate every binding before mutating the species table.
    for group in &packed.groups {
        ensure!(
            group.members.len() == group.rule.members.len(),
            "variant member binding count mismatch"
        );
        ensure!(
            group.slot < c2_formats::bits::Bits::LEN - 10,
            "variant menu slot exceeds target capacity"
        );
        let owner = group
            .rule
            .members
            .iter()
            .position(|m| m.id == group.rule.menu_owner)
            .context("missing variant menu owner")?;
        ensure!(
            group.members[owner].index == group.owner,
            "variant menu owner binding mismatch"
        );
        for member in &group.members {
            ensure!(
                used.insert(member.index),
                "duplicate variant character binding"
            );
            ensure!(
                member.token != 0 && tokens.insert(member.token),
                "duplicate or empty variant trophy identity"
            );
            let sp = species
                .get(member.index)
                .context("variant character index out of range")?;
            ensure!(
                sp.info.file.eq_ignore_ascii_case(&member.file),
                "variant model binding changed: {}",
                member.name
            );
            ensure!(
                !sp.ani_time.is_empty() && sp.info.ai >= 0,
                "variant model or AI unavailable: {}",
                member.name
            );
        }
    }
    for group in packed.groups {
        let probabilities = group.rule.probabilities(&area)?;
        let choices: Vec<_> = group
            .members
            .iter()
            .zip(probabilities)
            .map(|(m, p)| (m.index, p))
            .collect();
        let score = species[group.owner].info.base_score;
        for member in group.members {
            let sp = &mut species[member.index];
            sp.info.name = member.name;
            sp.info.base_score = score;
            sp.tuning.menu_slot = Some(group.slot as i32);
            if let Some(m) = &mut sp.mee {
                m.call_no = group.slot as i32;
            }
            sp.variant = Some(Variant {
                owner: group.owner,
                token: member.token,
                choices: if member.index == group.owner {
                    choices.clone()
                } else {
                    Vec::new()
                },
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserved_pack_loads_both_models_and_resolves_map_rules() {
        use c2_formats::car::{self, Animation, Car};
        use c2_mods::{build_with, load_mod, materialize, vanilla, BuildOptions};
        let root = std::env::temp_dir().join(format!("c2-pc-variant-load-{}", std::process::id()));
        let write = |file: &str, bytes: &[u8]| {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, bytes).expect("write");
        };
        let base =
            "characters {\n{\n name = 'Hunter'\n ai = 0\n}\n{\n name = 'Para'\n ai = 10\n}\n}\n.\n";
        write("base/HUNTDAT/_RES.TXT", base.as_bytes());
        write("base/HUNTDAT/AREAS/AREA1.MAP", b"map");
        let addon = base.replace(
            "}\n.\n",
            "{\n name = 'Rex'\n ai = 18\n file = 'rex.car'\n}\n}\n.\n",
        );
        for (id, tag) in [("classic", 1), ("striped", 2)] {
            let mut name_raw = [0; 32];
            name_raw[0] = tag;
            let model = car::write(&Car {
                name_raw,
                mesh: c2_formats::model::Mesh {
                    vertices: vec![c2_formats::model::Vertex {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                        owner: 0,
                        hide: 0,
                    }],
                    ..Default::default()
                },
                animations: vec![Animation {
                    name_raw: [0; 32],
                    kps: 2,
                    frames: 1,
                    data: vec![0; 3],
                }],
                sounds: vec![],
                anim_sounds: None,
                trailing: vec![],
            });
            write(&format!("{id}/HUNTDAT/_RES.TXT"), addon.as_bytes());
            write(&format!("{id}/HUNTDAT/rex.car"), &model);
        }
        let recipe = c2_mods::recipe::Recipe::parse(
            r#"
[[variants]]
id = "rex"
menu_owner = "classic"
[[variants.members]]
id = "classic"
source = "classic"
character = "Rex"
maps = []
[[variants.members]]
id = "striped"
source = "striped"
character = "Rex"
maps = [{ source = "base", area = "area1" }]
"#,
        )
        .expect("rules");
        let bases = vec![root.join("base")];
        let v = vanilla(&bases).expect("base");
        let sources = ["classic", "striped"]
            .iter()
            .map(|id| load_mod(&v, &root.join(id)).expect("source"))
            .collect();
        let pack = build_with(
            &bases,
            sources,
            &BuildOptions {
                variants: recipe.variants,
                ..Default::default()
            },
        )
        .expect("build");
        let out = root.join("output");
        materialize(&pack, &out).expect("materialize");
        let huntdat = out.join("HUNTDAT");
        let (mut species, cars) =
            crate::dinos::load_species(&huntdat, &pack.merged.res.to_data()).expect("load species");
        load(&huntdat, Path::new("AREA1.MAP"), &mut species).expect("load variants");
        let owner = species
            .iter()
            .position(|sp| sp.variant.as_ref().is_some_and(|v| !v.choices.is_empty()))
            .expect("owner");
        let choices = &species[owner].variant.as_ref().expect("variant").choices;
        assert_eq!(
            choices.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
            [0.0, 1.0]
        );
        assert!(choices.iter().all(|(i, _)| cars[*i].is_some()));
        assert!(choices.iter().all(|(i, _)| species[*i].info.name == "Rex"));
        load(&huntdat, Path::new("AREA2.MAP"), &mut species).expect("other map");
        assert!(species[owner]
            .variant
            .as_ref()
            .expect("variant")
            .choices
            .iter()
            .all(|(_, p)| *p == 0.0));
        let index = choices_index(&species, owner);
        species[index].info.file = "wrong.car".into();
        assert!(load(&huntdat, Path::new("AREA1.MAP"), &mut species).is_err());
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    fn choices_index(species: &[Species], owner: usize) -> usize {
        species[owner].variant.as_ref().expect("variant").choices[1].0
    }
}
