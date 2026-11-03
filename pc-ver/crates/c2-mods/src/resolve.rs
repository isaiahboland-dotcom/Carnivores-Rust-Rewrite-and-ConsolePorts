//! Load order: dependencies first, declared conflicts refused.

use crate::manifest::Manifest;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("mod '{id}' requires '{missing}', which is not loaded")]
    Missing { id: String, missing: String },
    #[error("mods '{a}' and '{b}' declare a conflict")]
    Conflict { a: String, b: String },
    #[error("dependency cycle involving '{id}'")]
    Cycle { id: String },
    #[error("mod id '{id}' is loaded twice")]
    Duplicate { id: String },
}

/// Indices of `mods` in load order: the given order, except that a mod is
/// moved after everything it requires.
pub fn order(mods: &[Manifest]) -> Result<Vec<usize>, ResolveError> {
    let ids: Vec<&str> = mods.iter().map(|m| m.info.id.as_str()).collect();
    let mut indices = std::collections::HashMap::with_capacity(mods.len());
    for (i, id) in ids.iter().enumerate() {
        if indices.insert(*id, i).is_some() {
            return Err(ResolveError::Duplicate { id: id.to_string() });
        }
    }
    for m in mods {
        for r in &m.deps.requires {
            if !indices.contains_key(r.as_str()) {
                return Err(ResolveError::Missing {
                    id: m.info.id.clone(),
                    missing: r.clone(),
                });
            }
        }
        for c in &m.deps.conflicts {
            if indices.contains_key(c.as_str()) {
                return Err(ResolveError::Conflict {
                    a: m.info.id.clone(),
                    b: c.clone(),
                });
            }
        }
    }
    let mut out = Vec::new();
    let mut state = vec![0u8; mods.len()];
    for root in 0..mods.len() {
        if state[root] == 2 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0usize)];
        while let Some((node, next)) = stack.last_mut() {
            let i = *node;
            if *next == mods[i].deps.requires.len() {
                state[i] = 2;
                out.push(i);
                stack.pop();
                continue;
            }
            let required = &mods[i].deps.requires[*next];
            *next += 1;
            // Missing requirements were rejected above.
            let Some(j) = indices.get(required.as_str()).copied() else {
                return Err(ResolveError::Missing {
                    id: ids[i].to_string(),
                    missing: required.clone(),
                });
            };
            match state[j] {
                2 => {}
                1 => {
                    return Err(ResolveError::Cycle {
                        id: ids[j].to_string(),
                    })
                }
                _ => {
                    state[j] = 1;
                    stack.push((j, 0));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, req: &[&str], conf: &[&str]) -> Manifest {
        let mut m = Manifest::default();
        m.info.id = id.into();
        m.deps.requires = req.iter().map(|s| s.to_string()).collect();
        m.deps.conflicts = conf.iter().map(|s| s.to_string()).collect();
        m
    }

    #[test]
    fn deep_chains_do_not_use_the_call_stack() {
        let mut mods = Vec::new();
        for i in 0..20_000 {
            let mut item = m(&format!("m{i}"), &[], &[]);
            if i + 1 < 20_000 {
                item.deps.requires.push(format!("m{}", i + 1));
            }
            mods.push(item);
        }
        assert_eq!(order(&mods), Ok((0..20_000).rev().collect()));
    }

    #[test]
    fn diamonds_and_cycles_preserve_traversal_order() {
        let mods = [
            m("a", &["b", "c"], &[]),
            m("b", &["d"], &[]),
            m("c", &["d"], &[]),
            m("d", &[], &[]),
            m("e", &[], &[]),
        ];
        assert_eq!(order(&mods), Ok(vec![3, 1, 2, 0, 4]));
        assert_eq!(
            order(&[m("a", &["b"], &[]), m("b", &["a"], &[])]),
            Err(ResolveError::Cycle { id: "a".into() })
        );
    }

    #[test]
    fn dependencies_load_first() {
        let mods = [m("a", &["c"], &[]), m("b", &[], &[]), m("c", &[], &[])];
        assert_eq!(order(&mods), Ok(vec![2, 0, 1]));
        assert!(matches!(
            order(&[m("a", &["x"], &[])]),
            Err(ResolveError::Missing { .. })
        ));
        assert!(matches!(
            order(&[m("a", &[], &["b"]), m("b", &[], &[])]),
            Err(ResolveError::Conflict { .. })
        ));
        assert!(matches!(
            order(&[m("a", &["b"], &[]), m("b", &["a"], &[])]),
            Err(ResolveError::Cycle { .. })
        ));
    }
}
