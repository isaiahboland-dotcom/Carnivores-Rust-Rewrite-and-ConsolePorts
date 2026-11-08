//! A mobile animal's animations: which clip plays which role (by name, by
//! a known order or by the clips' motion), the PC animal it takes its kind
//! from, and its clips put in a PC AI's order. Shared by the add-on import
//! ([`crate::mobile`]) and the APK as a base game ([`crate::mobile_base`]).

use std::collections::BTreeMap;

use c2_formats::car::Car;
use c2_formats::mobile::Cat;

/// The PC animals a mobile one takes its kind from, in order of preference
/// (the first the game has).
pub(crate) const DONORS: &[(&str, &[&str])] = &[
    // After the Carnivores 2 kind, Ice Age's (its animals' names there).
    ("amargasaurus", &["Stegosaurus", "Mammoth"]),
    ("carnotaurus", &["Ceratosaurus", "Allosaurus", "Smilodon"]),
    ("coelophysis", &["Velociraptor", "Wolf"]),
    ("dilophosaurus", &["Allosaurus", "Wolf"]),
    ("gigantoraptor", &["Allosaurus", "Diatryma"]),
    ("iguanodon", &["Parasaurolophus", "Brontoteriy"]),
    ("oviraptor", &["Parasaurolophus", "Giant deer"]),
    ("pach", &["Chasmosaurus", "Wooly rhinoceros"]),
    ("troodon", &["Velociraptor", "Wolf"]),
    ("utahraptor", &["Velociraptor", "Wolf"]),
    ("andrewsarchus", &["Bear", "Allosaurus"]),
    ("doedicurus", &["Ankylosaurus", "Brontoteriy"]),
    ("hyaenodon", &["Wolf", "Velociraptor"]),
    ("indricothere", &["Stegosaurus", "Brontoteriy"]),
    ("titanis", &["Diatryma", "Velociraptor"]),
    ("diatr", &["Velociraptor"]),
];

/// `stem`'s donors in [`DONORS`] (none if it has no entry).
pub(crate) fn donors(stem: &str) -> &'static [&'static str] {
    DONORS
        .iter()
        .find(|(s, _)| *s == stem)
        .map_or(&[][..], |d| d.1)
}

/// Numbered PC-style animation sets in a known order: each matches a PC
/// model's clip for clip (frame counts and speeds), whose names give the
/// roles. The Diatryma is Ice Age's `Diatr.car` (`Dm_wlk, Dm_run, Dm_idle1,
/// Dm_idle2, Dm_jmp, Dm_sld, Dm_die, Dm_slp, Dm_hero`), the
/// Pachycephalosaurus the Modder's Edition's `PACH.CAR` (`Pac_wlk, Pac_run,
/// Pac_sld, Pac_die1, Pac_idle1, Pac_idle2, Pac_slp`). The slide and the
/// trophy pose have no role here.
const PC_SETS: &[(&str, &[Option<&str>])] = &[
    (
        "diatr",
        &[
            Some("walk"),
            Some("run"),
            Some("idle"),
            Some("idle"),
            Some("jump"),
            None,
            Some("die"),
            Some("sleep"),
            None,
        ],
    ),
    (
        "pach",
        &[
            Some("walk"),
            Some("run"),
            None,
            Some("die"),
            Some("idle"),
            Some("idle"),
            Some("sleep"),
        ],
    ),
];

/// The animation keys an entry's role names give (`carnotaurus_run`).
pub(crate) fn role(file: &str) -> Option<&'static str> {
    let f = file.to_ascii_lowercase();
    [
        ("_walk", "walk"),
        ("_run", "run"),
        ("_swim", "swim"),
        ("_jump", "jump"),
        ("_die", "die"),
        ("_sleep", "sleep"),
        ("_eat", "eat"),
        ("_growl", "growl"),
        ("_idle_active", "idle"),
        ("_idle_passive", "idle"),
        ("_idle", "idle"),
        ("_call", "call"),
        ("_attack", "attack"),
    ]
    .iter()
    .find(|(s, _)| f.ends_with(s))
    .map(|(_, r)| *r)
}

/// The order the mobile animals' 11 animations come in (named so in most,
/// numbered in some: Coelophysis, Iguanodon).
const ORDER: [&str; 11] = [
    "walk", "run", "idle", "idle", "growl", "die", "sleep", "eat", "swim", "attack", "call",
];

/// Each animation's role: from its name; for a known PC-style set (by
/// model, `stem`) its order; else from the clips themselves. `None`: no
/// clip, or more than one, looks like a death.
///
/// Numbered sets come in two layouts. The Dinosaur Hunter's 11 (`ORDER`:
/// the death 6th) and the PC's, which Ice Age's mobile animals keep (its
/// Bear, Wolf, Mammoth... are its PC models clip for clip): walk and run,
/// the idles, the death, the sleep, then extras (a jump, trophy poses). So
/// the death is found by its motion (the one clip that ends much lower than
/// it starts), the idles are the clips before it, the sleep the one after,
/// and the jump the extra that lifts the body highest.
pub(crate) fn roles(cat: &Cat, stem: &str, car: &Car) -> Option<Vec<Option<&'static str>>> {
    let named: Vec<_> = cat.animations.iter().map(|(f, _)| role(f)).collect();
    let n = cat.animations.len();
    if named.contains(&Some("die")) {
        return Some(named);
    }
    if let Some((_, set)) = PC_SETS.iter().find(|(s, set)| *s == stem && set.len() == n) {
        return Some(set.to_vec());
    }
    let falling: Vec<usize> = (0..n).filter(|&i| falls(car, i)).collect();
    let [d] = falling[..] else { return None };
    if n == ORDER.len() && ORDER.get(d) == Some(&"die") {
        return Some(ORDER.iter().map(|r| Some(*r)).collect());
    }
    if d < 2 {
        return None;
    }
    let mut r = vec![None; n];
    // The walk is the slower cycle of the first two.
    let ms = |i: usize| {
        car.animations
            .get(i)
            .map_or(0, |a| a.frames as i64 * 1000 / i64::from(a.kps.max(1)))
    };
    let (walk, run) = if ms(0) >= ms(1) { (0, 1) } else { (1, 0) };
    r[walk] = Some("walk");
    r[run] = Some("run");
    for x in r.iter_mut().take(d).skip(2) {
        *x = Some("idle");
    }
    r[d] = Some("die");
    // The sleep lies as the death ends (an armoured Doedicurus barely drops).
    let stand = heights(car, 0).first().copied().unwrap_or(0.0);
    let lying = heights(car, d).last().copied().unwrap_or(0.0);
    let low = |i: usize| {
        heights(car, i)
            .iter()
            .all(|&h| h < lying * 1.25 + stand * 0.05)
    };
    let mut extras = d + 1;
    if d + 1 < n && low(d + 1) {
        r[d + 1] = Some("sleep");
        extras = d + 2;
    }
    let peak = |i: usize| heights(car, i).into_iter().fold(0.0, f64::max);
    if let Some(j) = (extras..n)
        .filter(|&i| peak(i) > stand * 1.2)
        .max_by(|&a, &b| peak(a).total_cmp(&peak(b)))
    {
        r[j] = Some("jump");
    }
    Some(r)
}

/// Is this a mobile animal (it has a death animation, or numbered ones)?
pub(crate) fn is_animal(cat: &Cat) -> bool {
    cat.animations.len() >= 6
        && cat
            .animations
            .iter()
            .all(|(f, _)| role(f).is_some() || f.contains("_animation_"))
}

/// Mean height of animation `i`'s vertices, frame by frame.
fn heights(car: &Car, i: usize) -> Vec<f64> {
    let vc = car.mesh.vertices.len();
    let Some(a) = car.animations.get(i).filter(|_| vc > 0) else {
        return Vec::new();
    };
    (0..a.frames)
        .filter_map(|f| a.data.get(f * vc * 3..(f + 1) * vc * 3))
        .map(|frame| {
            frame
                .iter()
                .skip(1)
                .step_by(3)
                .map(|&y| f64::from(y))
                .sum::<f64>()
                / vc as f64
        })
        .collect()
}

/// Does animation `i` look like a death: the body ends much lower than it
/// starts?
pub(crate) fn falls(car: &Car, i: usize) -> bool {
    let h = heights(car, i);
    match (h.first(), h.last()) {
        (Some(&start), Some(&end)) if h.len() >= 2 => start > 0.0 && end < start * 0.8,
        _ => false,
    }
}

/// Each PC AI's clips in the order it plays them by number (the PC
/// models': `Par_wlk, Par_run1, Par_idle1, Par_idle2, Par_die, Par_slp`...).
pub(crate) fn layout(ai: i32) -> &'static [&'static str] {
    match ai {
        10 => &["walk", "run", "idle", "idle", "die", "sleep"],
        11 => &["run", "walk", "idle", "idle", "die", "sleep"],
        12 => &["run", "walk", "die", "idle", "idle", "sleep"],
        13 => &[
            "run", "walk", "swim", "slide", "jump", "die", "eat", "sleep",
        ],
        14 => &["run", "walk", "idle", "idle", "idle", "die", "sleep"],
        15 => &[
            "run", "walk", "slide", "swim", "jump", "die", "eat", "sleep",
        ],
        16 => &[
            "run", "walk", "slide", "idle", "idle", "jump", "die", "eat", "sleep",
        ],
        17 => &["walk", "run", "idle", "idle", "idle", "die", "sleep", "eat"],
        _ => &[],
    }
}

/// The clip for a role an animal lacks: the nearest it has.
fn stand_in(role: &str) -> &'static [&'static str] {
    match role {
        "slide" => &["run"],
        "swim" => &["walk", "run"],
        "jump" => &["attack", "run"],
        "eat" => &["attack", "idle", "walk"],
        "idle" => &["growl", "call", "walk"],
        "sleep" => &["die"],
        _ => &["walk"],
    }
}

/// `car`'s clips (and their sounds) in `layout`'s order, from `roles`
/// (each clip's). Repeated roles (two idles) take the animal's in turn.
pub(crate) fn arrange(car: &Car, roles: &[Option<&str>], layout: &[&str]) -> Option<Car> {
    let mut used: BTreeMap<&str, usize> = BTreeMap::new();
    let mut pick = Vec::with_capacity(layout.len());
    for &want in layout {
        let n = used.entry(want).or_insert(0);
        let of = |r: &str| -> Vec<usize> {
            (0..roles.len())
                .filter(|&i| roles[i] == Some(r) && i < car.animations.len())
                .collect()
        };
        let own = of(want);
        let i = own
            .get(*n)
            .or(own.first())
            .copied()
            .or_else(|| stand_in(want).iter().find_map(|r| of(r).first().copied()))?;
        *n += 1;
        pick.push(i);
    }
    let mut out = car.clone();
    out.animations = pick.iter().map(|&i| car.animations[i].clone()).collect();
    if let Some(s) = car.anim_sounds {
        let mut a = [-1; c2_formats::car::ANIFX_SLOTS];
        for (slot, &i) in a.iter_mut().zip(&pick) {
            *slot = s.get(i).copied().unwrap_or(-1);
        }
        out.anim_sounds = Some(a);
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use c2_formats::car::Animation;

    fn clip(name: &str) -> Animation {
        let mut name_raw = [0u8; 32];
        name_raw[..name.len()].copy_from_slice(name.as_bytes());
        Animation {
            name_raw,
            kps: 10,
            frames: 0,
            data: Vec::new(),
        }
    }

    /// A two-vertex model; each animation is `(frames, heights)`, the
    /// height of both vertices per frame.
    pub(crate) fn model(anims: &[(i32, &[i16])]) -> Car {
        use c2_formats::car::Animation;
        use c2_formats::model::{Mesh, Vertex};
        let v = Vertex {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            owner: 0,
            hide: 0,
        };
        Car {
            name_raw: [0; 32],
            mesh: Mesh {
                vertices: vec![v; 2],
                faces: vec![],
                texture: vec![],
            },
            animations: anims
                .iter()
                .map(|&(kps, hs)| Animation {
                    name_raw: [0; 32],
                    kps,
                    frames: hs.len(),
                    data: hs.iter().flat_map(|&h| [0, h, 0, 0, h, 0]).collect(),
                })
                .collect(),
            sounds: vec![],
            anim_sounds: None,
            trailing: Vec::new(),
        }
    }

    #[test]
    fn numbered_pc_style_sets_get_roles_from_their_clips() {
        let cat = Cat {
            animations: (0..9).map(|i| (format!("x_animation_{i}"), None)).collect(),
            ..Default::default()
        };
        // Ice Age's layout: run (fast) and walk swapped here, two idles, the
        // death, the sleep, a trophy pose, a jump, another pose.
        let car = model(&[
            (20, &[100, 104, 100]),
            (5, &[100, 110, 100]),
            (10, &[100, 98, 100]),
            (10, &[100, 101, 100]),
            (10, &[100, 60, 20]),
            (5, &[20, 21, 20]),
            (10, &[90, 95, 90]),
            (10, &[100, 180, 100]),
            (10, &[100, 105, 100]),
        ]);
        let r = roles(&cat, "x", &car).unwrap_or_default();
        assert_eq!(
            r,
            [
                Some("run"),
                Some("walk"),
                Some("idle"),
                Some("idle"),
                Some("die"),
                Some("sleep"),
                None,
                Some("jump"),
                None
            ]
        );
        // Two falling clips: not guessed.
        let car = model(&[
            (10, &[100, 100]),
            (10, &[100, 100]),
            (10, &[100, 20]),
            (10, &[100, 20]),
            (10, &[100, 100]),
            (10, &[100, 100]),
        ]);
        let cat6 = Cat {
            animations: (0..6).map(|i| (format!("x_animation_{i}"), None)).collect(),
            ..Default::default()
        };
        assert!(roles(&cat6, "x", &car).is_none());
        // The Dinosaur Hunter's 11, the death 6th.
        let mut eleven: Vec<(i32, &[i16])> = vec![(10, &[100, 100]); 11];
        eleven[5] = (10, &[100, 10]);
        let cat11 = Cat {
            animations: (0..11)
                .map(|i| (format!("x_animation_{i}"), None))
                .collect(),
            ..Default::default()
        };
        let r = roles(&cat11, "x", &model(&eleven)).unwrap_or_default();
        assert_eq!(
            (r[4], r[5], r[10]),
            (Some("growl"), Some("die"), Some("call"))
        );
    }

    #[test]
    fn clips_take_the_ais_order() {
        let names = [
            "walk", "run", "idle", "idle2", "growl", "die", "sleep", "eat", "attack",
        ];
        let car = Car {
            name_raw: [0; 32],
            mesh: c2_formats::model::Mesh {
                vertices: Vec::new(),
                faces: Vec::new(),
                texture: Vec::new(),
            },
            animations: names.iter().map(|n| clip(n)).collect(),
            sounds: Vec::new(),
            anim_sounds: Some({
                let mut a = [-1; c2_formats::car::ANIFX_SLOTS];
                a[5] = 3;
                a
            }),
            trailing: Vec::new(),
        };
        let roles = [
            Some("walk"),
            Some("run"),
            Some("idle"),
            Some("idle"),
            Some("growl"),
            Some("die"),
            Some("sleep"),
            Some("eat"),
            Some("attack"),
        ];
        // The raptor's: run, walk, slide, swim, jump, die, eat, sleep.
        let a = arrange(&car, &roles, layout(15)).unwrap_or_else(|| panic!("arranged"));
        let got: Vec<String> = a.animations.iter().map(Animation::name).collect();
        assert_eq!(
            got,
            ["run", "walk", "run", "walk", "attack", "die", "eat", "sleep"]
        );
        // The death's sound went with it.
        assert_eq!(a.anim_sounds.map(|s| s[5]), Some(3));
        // Two idles take the animal's two.
        let a = arrange(&car, &roles, layout(14)).unwrap_or_else(|| panic!("arranged"));
        let got: Vec<String> = a.animations.iter().map(Animation::name).collect();
        assert_eq!(got[2..5], ["idle", "idle2", "idle"]);
    }
}
