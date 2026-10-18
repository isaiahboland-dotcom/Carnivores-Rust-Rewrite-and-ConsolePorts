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
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            return Err(ResolveError::Duplicate { id: id.to_string() });
        }
    }
    for m in mods {
        for r in &m.deps.requires {
            if !ids.contains(&r.as_str()) {
                return Err(ResolveError::Missing {
                    id: m.info.id.clone(),
                    missing: r.clone(),
                });
            }
        }
        for c in &m.deps.conflicts {
            if ids.contains(&c.as_str()) {
                return Err(ResolveError::Conflict {
                    a: m.info.id.clone(),
                    b: c.clone(),
                });
            }
        }
    }
    // Depth-first, visiting in the given order.
    let mut out = Vec::new();
    let mut state = vec![0u8; mods.len()]; // 0 new, 1 visiting, 2 done
    fn visit(
        i: usize,
        mods: &[Manifest],
        ids: &[&str],
        state: &mut [u8],
        out: &mut Vec<usize>,
    ) -> Result<(), ResolveError> {
        match state[i] {
            2 => return Ok(()),
            1 => {
                return Err(ResolveError::Cycle {
                    id: ids[i].to_string(),
                })
            }
            _ => {}
        }
        state[i] = 1;
        for r in &mods[i].deps.requires {
            if let Some(j) = ids.iter().position(|id| id == r) {
                visit(j, mods, ids, state, out)?;
            }
        }
        state[i] = 2;
        out.push(i);
        Ok(())
    }
    for i in 0..mods.len() {
        visit(i, mods, &ids, &mut state, &mut out)?;
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
