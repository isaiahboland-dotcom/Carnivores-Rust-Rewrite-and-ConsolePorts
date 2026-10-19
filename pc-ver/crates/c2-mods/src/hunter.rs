//! Merging hunter models. An add-on that kills the hunter in a new way ships
//! a whole hunter `.CAR`: the base hunter with its death animations appended
//! (a kill type's `hunterAnim` picks one). Installing two such add-ons the
//! usual way leaves only one of them working, so instead the extra
//! animations of every add-on's hunter are appended to the base one, and
//! each add-on's `hunterAnim` numbers are moved to where its animations
//! landed.

use c2_formats::car::{Car, ANIFX_SLOTS};
use c2_formats::res_txt::Block;

use crate::res_merge::Change;

/// What became of one add-on's hunter.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Its animation `i` is now `map[i]`.
    Merged { map: Vec<usize> },
    /// Not the same body as the base hunter: left out.
    Incompatible(String),
}

/// Appends each of `extras`' animations that the merged model doesn't
/// already have. `base` is changed in place.
pub fn merge(base: &mut Car, extras: &[Car]) -> Vec<Outcome> {
    extras
        .iter()
        .map(|e| {
            if e.mesh.vertices.len() != base.mesh.vertices.len() {
                return Outcome::Incompatible(format!(
                    "{} vertices, the base hunter has {}",
                    e.mesh.vertices.len(),
                    base.mesh.vertices.len()
                ));
            }
            let mut map = Vec::with_capacity(e.animations.len());
            for (i, a) in e.animations.iter().enumerate() {
                if let Some(j) = base
                    .animations
                    .iter()
                    .position(|b| b.kps == a.kps && b.frames == a.frames && b.data == a.data)
                {
                    map.push(j);
                    continue;
                }
                if base.animations.len() >= ANIFX_SLOTS {
                    return Outcome::Incompatible(format!(
                        "more than {ANIFX_SLOTS} hunter animations in all"
                    ));
                }
                base.animations.push(a.clone());
                let j = base.animations.len() - 1;
                // Its sound: the same one if the base has it, else added.
                let fx = e
                    .anim_sounds
                    .and_then(|t| usize::try_from(t[i]).ok())
                    .and_then(|s| e.sounds.get(s))
                    .map(|s| match base.sounds.iter().position(|b| b.pcm == s.pcm) {
                        Some(k) => k,
                        None => {
                            base.sounds.push(s.clone());
                            base.sounds.len() - 1
                        }
                    });
                let t = base.anim_sounds.get_or_insert([-1; ANIFX_SLOTS]);
                t[j] = fx.map_or(-1, |k| k as i32);
                map.push(j);
            }
            Outcome::Merged { map }
        })
        .collect()
}

/// Moves `hunterAnim` / `hunterCarryAnim` of every kill type in `changes`
/// through `map`.
pub fn remap(changes: &mut [Change], map: &[usize]) {
    fn block(b: &mut Block, map: &[usize]) {
        for s in &mut b.subs {
            if s.name.to_ascii_lowercase().contains("killtype") {
                for e in &mut s.block.entries {
                    let k = e.key.to_ascii_lowercase();
                    if k == "hunteranim" || k == "huntercarryanim" {
                        let v = c2_formats::res_txt::c_atoi(&e.value);
                        if let Some(&n) = usize::try_from(v).ok().and_then(|v| map.get(v)) {
                            e.value = n.to_string();
                        }
                    }
                }
            }
            block(&mut s.block, map);
        }
    }
    for c in changes {
        match c {
            Change::Add { block: b, .. } | Change::Modify { block: b, .. } => block(b, map),
            Change::Entry { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_formats::car::{Animation, Sound};
    use c2_formats::model::Vertex;

    const V: Vertex = Vertex {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        owner: 0,
        hide: 0,
    };

    fn anim(tag: i16) -> Animation {
        Animation {
            name_raw: [0; 32],
            kps: 2,
            frames: 1,
            data: vec![tag; 3],
        }
    }

    fn car(tags: &[i16]) -> Car {
        let mesh = c2_formats::model::Mesh {
            vertices: vec![V],
            ..Default::default()
        };
        Car {
            name_raw: [0; 32],
            mesh,
            animations: tags.iter().map(|&t| anim(t)).collect(),
            sounds: vec![Sound {
                name_raw: [0; 32],
                pcm: vec![1, 2],
            }],
            anim_sounds: None,
            trailing: Vec::new(),
        }
    }

    #[test]
    fn extra_animations_are_appended_once_and_mapped() {
        let mut base = car(&[0, 1, 2]);
        // Two add-ons: one adds 7, the other adds 8 and 9; both keep 0..=2.
        let out = merge(&mut base, &[car(&[0, 1, 2, 7]), car(&[0, 1, 2, 8, 9, 7])]);
        assert_eq!(base.animations.len(), 6);
        assert_eq!(
            out,
            vec![
                Outcome::Merged {
                    map: vec![0, 1, 2, 3]
                },
                Outcome::Merged {
                    map: vec![0, 1, 2, 4, 5, 3]
                },
            ]
        );
        let mut bad = car(&[0]);
        bad.mesh.vertices.push(V);
        assert!(matches!(
            merge(&mut base, &[bad])[0],
            Outcome::Incompatible(_)
        ));
    }

    #[test]
    fn kill_types_follow_the_map() {
        let text = "characters {\n{\n name = 'X'\n killtype {\n  hunterAnim = 4\n  eatAnim = 4\n }\n}\n}\n";
        let res = c2_formats::res_txt::ResTxt::parse(text).unwrap_or_else(|e| panic!("{e}"));
        let block = res.sections[0].blocks[0].clone();
        let mut cs = vec![Change::Add {
            section: "characters".into(),
            block,
        }];
        remap(&mut cs, &[0, 1, 2, 3, 9]);
        let Change::Add { block, .. } = &cs[0] else {
            unreachable!()
        };
        let k = &block.subs[0].block;
        assert_eq!(k.get(&["hunterAnim"]), Some("9"));
        assert_eq!(
            k.get(&["eatAnim"]),
            Some("4"),
            "the animal's own anims stay"
        );
    }
}
