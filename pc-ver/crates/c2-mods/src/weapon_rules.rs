//! Recipe restrictions baked into weapon definitions, so all hunt loaders agree.
use crate::ModError;
use c2_formats::res_txt::{Block, Entry, ResTxt};
use c2_vfs::{MemBackend, Vfs};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WeaponRule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_tranq: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_trophy: Option<bool>,
}

fn set(block: &mut Block, key: &str, enabled: bool) {
    block.entries.retain(|e| !e.key.eq_ignore_ascii_case(key));
    block.entries.push(Entry {
        key: key.into(),
        value: u8::from(enabled).to_string(),
    });
    // Conditional weapon variants must keep the instance restriction too.
    for sub in &mut block.subs {
        set(&mut sub.block, key, enabled);
    }
}

pub fn push(vfs: &mut Vfs, rules: &[WeaponRule]) -> Result<(), ModError> {
    if rules.is_empty() {
        return Ok(());
    }
    const PATH: &str = "HUNTDAT/_RES.TXT";
    let mut res = ResTxt::parse(&String::from_utf8_lossy(&vfs.read(PATH)?))?;
    let blocks = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("weapons"))
        .ok_or_else(|| ModError::Recipe("weapon restrictions: no weapons section".into()))?;
    for rule in rules {
        let matching: Vec<_> = blocks
            .blocks
            .iter_mut()
            .filter(|b| {
                b.get(&["name"]).is_some_and(|n| {
                    n.trim()
                        .trim_matches(['\'', '"'])
                        .eq_ignore_ascii_case(rule.name.trim())
                })
            })
            .collect();
        if matching.len() != 1 {
            return Err(ModError::Recipe(format!(
                "weapon restriction {:?}: expected one matching weapon, found {}",
                rule.name,
                matching.len()
            )));
        }
        for block in matching {
            if let Some(v) = rule.allow_tranq {
                set(block, "c2rs_allow_tranq", v);
            }
            if let Some(v) = rule.allow_trophy {
                set(block, "c2rs_allow_trophy", v);
            }
        }
    }
    let mut layer = MemBackend::new();
    layer.insert(PATH, res.write().into_bytes());
    vfs.push("weapon restrictions", Box::new(layer));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_rules_survive_resolution_and_unknown_names_fail() {
        let mut vfs = Vfs::new();
        let mut base = MemBackend::new();
        base.insert("HUNTDAT/_RES.TXT", b"weapons {\n{\n name = 'Shotgun'\n overwrite area1 {\n shots = 9\n }\n}\n{\n name = 'Rifle'\n}\n}\n".to_vec());
        vfs.push("base", Box::new(base));
        let mut rule = WeaponRule {
            name: "shotgun".into(),
            allow_tranq: Some(false),
            allow_trophy: Some(false),
        };
        push(&mut vfs, &[rule.clone()]).unwrap();
        let res = ResTxt::parse(&String::from_utf8_lossy(
            &vfs.read("HUNTDAT/_RES.TXT").unwrap(),
        ))
        .unwrap();
        let blocks = &res.sections[0].blocks;
        assert_eq!(blocks[0].get(&["c2rs_allow_tranq"]), Some("0"));
        assert_eq!(
            blocks[0].subs[0].block.get(&["c2rs_allow_trophy"]),
            Some("0")
        );
        assert_eq!(blocks[1].get(&["c2rs_allow_tranq"]), None);
        rule.name = "misspelled gun".into();
        assert!(push(&mut vfs, &[rule]).is_err());
    }
}
