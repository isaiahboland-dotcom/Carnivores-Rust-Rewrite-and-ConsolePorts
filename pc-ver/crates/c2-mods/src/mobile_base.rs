//! The user's mobile APK (Carnivores: Dinosaur Hunter or Ice Age for
//! Android) as a base game, with no PC install: its pieces rebuilt as a PC
//! game folder (`HUNTDAT/...`), which every frontend then reads as it reads
//! the PC's. Built locally from the user's own APK; nothing is shipped.
//!
//! So far: the areas and the trophy room (`AREAS/AREAn.MAP`, `.RSC`); the
//! animals, weapons, hunter and ship (`.CAR`s); `_RES.TXT` from the game's
//! own tables (`c2_formats::mobile_chars`): a plain (not Modder's Edition)
//! one, each new animal running as the PC animal of its kind with its clips
//! put in that animal's order.

use crate::mobile::{falls, roles, Apk, DONORS};
use crate::ModError;
use c2_formats::car::Car;
use c2_formats::mobile::{car_from_in, parse_cat, Assets, Cat};
use c2_formats::mobile_area;
use c2_formats::mobile_chars::{self, Characters, Stored};
use c2_vfs::Backend;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// What [`build`] wrote.
#[derive(Debug, Default)]
pub struct Built {
    /// Each area's files, in order (the trophy room last).
    pub areas: Vec<PathBuf>,
    /// The animals in `_RES.TXT`, as `name (file, ai)`.
    pub animals: Vec<String>,
    pub weapons: Vec<String>,
    /// What was left out, and why.
    pub notes: Vec<String>,
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> ModError + '_ {
    move |source| ModError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn write(path: &Path, data: &[u8]) -> Result<(), ModError> {
    std::fs::write(path, data).map_err(io(path))
}

/// One area (`assets/areas/<name>.man` and `.rst`) as `<stem>.MAP` and
/// `.RSC` in `dir`.
fn area(apk: &Apk, name: &str, dir: &Path, stem: &str) -> Result<PathBuf, ModError> {
    let man = apk.0.read(&format!("assets/areas/{name}.man"))?;
    let rst = apk.0.read(&format!("assets/areas/{name}.rst"))?;
    let rst = mobile_area::parse_rst(&String::from_utf8_lossy(&rst))?;
    let map = mobile_area::parse_man(&man)?;
    let rsc = mobile_area::rsc_from(&rst, apk)?;
    let path = dir.join(format!("{stem}.MAP"));
    write(&path, &c2_formats::map::write(&map))?;
    write(
        &dir.join(format!("{stem}.RSC")),
        &c2_formats::rsc::write(&rsc),
    )?;
    Ok(path)
}

/// The library holding the game's code (the largest arm64 one with its
/// tuning table) and that table.
fn library(apk: &Apk) -> Option<(Vec<u8>, BTreeMap<String, f64>)> {
    let mut libs: Vec<String> = apk
        .0
        .files()
        .into_iter()
        .filter(|f| f.starts_with("lib/arm64-v8a/") && f.ends_with(".so"))
        .collect();
    libs.sort();
    let mut found: Vec<(Vec<u8>, BTreeMap<String, f64>)> = libs
        .iter()
        .filter_map(|f| apk.0.read(f).ok())
        .filter_map(|d| {
            let v = c2_formats::mobile_values::defaults(&d).ok()?;
            (!v.is_empty()).then_some((d, v))
        })
        .collect();
    found.sort_by_key(|(d, _)| std::cmp::Reverse(d.len()));
    found.into_iter().next()
}

/// The PC's ambient animals by index (their files aren't named in
/// `Game::LoadCharacters`); their AI is the index less 6, as the
/// huntables' (16 Parasaurolophus: AI 10 ... 24 T-Rex: AI 18).
const AMBIENTS: [(i32, &str); 6] = [
    (7, "mosh"),
    (8, "gall"),
    (9, "dimor2"),
    (10, "ptera"),
    (11, "dimet"),
    (12, "brachi1"),
];

/// The PC's huntable AIs by animal name (the donors in `DONORS`).
const PC_AI: [(&str, i32); 9] = [
    ("Parasaurolophus", 10),
    ("Ankylosaurus", 11),
    ("Stegosaurus", 12),
    ("Allosaurus", 13),
    ("Chasmosaurus", 14),
    ("Velociraptor", 15),
    ("Spinosaurus", 16),
    ("Ceratosaurus", 17),
    ("T-Rex", 18),
];

/// Each PC AI's clips in the order it plays them by number (the PC
/// models': `Par_wlk, Par_run1, Par_idle1, Par_idle2, Par_die, Par_slp`...).
fn layout(ai: i32) -> &'static [&'static str] {
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
fn arrange(car: &Car, roles: &[Option<&str>], layout: &[&str]) -> Option<Car> {
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

/// A weapon's clip role from its file name (`revolver_up`); `None` for a
/// numbered one (`pistol_animation_0`, already in the PC's order).
fn weapon_role(file: &str) -> Option<usize> {
    let f = file.to_ascii_lowercase();
    [
        ("_up", 0),
        ("_get", 0),
        ("_fire", 1),
        ("_shot", 1),
        ("_down", 2),
        ("_put", 2),
    ]
    .iter()
    .chain(&[("_reload", 3), ("_rld", 3)])
    .find(|(s, _)| f.ends_with(s))
    .map(|&(_, r)| r)
}

/// The weapons by their tuning key (`WEAPON_DBSHOTGUN_...`): the file and
/// the PC's ammunition picture.
const WEAPONS: [(&str, &str, &str); 7] = [
    ("PISTOL", "pistol", "bullet1.tga"),
    ("SHOTGUN", "shotgun", "bullet2.tga"),
    ("DBSHOTGUN", "dbsgun", "bullet2.tga"),
    ("XBOW", "x_bow", "bullet4.tga"),
    ("RIFLE", "rifle", "bullet3.tga"),
    ("SNIPER_RIFLE", "sniper", "bullet3.tga"),
    ("REVOLVER", "revolver", "bullet1.tga"),
];

fn cat(apk: &Apk, stem: &str) -> Option<Cat> {
    let t = apk.get(&format!("assets/characters/{stem}.cat"))?;
    parse_cat(&String::from_utf8_lossy(&t)).ok()
}

/// A number as `_RES.TXT` writes it (no trailing zeros).
fn num(v: f32) -> String {
    let s = format!("{:.3}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

struct Game<'a> {
    apk: &'a Apk,
    huntdat: PathBuf,
    chars: Characters,
    balance: BTreeMap<String, f64>,
    strings: BTreeMap<String, String>,
    res: String,
    built: Built,
}

impl Game<'_> {
    fn name(&self, s: &Stored, fallback: &str) -> String {
        s.name
            .as_ref()
            .and_then(|k| self.strings.get(k))
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }

    fn write_car(&self, file: &str, car: &Car) -> Result<(), ModError> {
        write(&self.huntdat.join(file), &c2_formats::car::write(car))
    }

    fn weapons(&mut self) -> Result<(), ModError> {
        let dir = self.huntdat.join("WEAPONS");
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        self.res.push_str("weapons {\n");
        let list: Vec<(i32, Stored)> = self
            .chars
            .weapons
            .iter()
            .map(|(i, s)| (*i, s.clone()))
            .collect();
        for (_, s) in list {
            // Its tuning key names it (the camera has none).
            let Some(key) = s.keys.first() else { continue };
            let Some(&(_, stem, pic)) = WEAPONS
                .iter()
                .find(|(k, _, _)| key == &format!("WEAPON_{k}_POWER"))
            else {
                continue;
            };
            let Some(c) = cat(self.apk, stem) else {
                self.built.notes.push(format!("{stem}: no descriptor"));
                continue;
            };
            let mut car = car_from_in(&c, self.apk, "weapons")?;
            let order: Vec<Option<usize>> =
                c.animations.iter().map(|(f, _)| weapon_role(f)).collect();
            if order.iter().all(Option::is_some) {
                let mut clips: Vec<_> = order.iter().flatten().zip(car.animations).collect();
                clips.sort_by_key(|(r, _)| **r);
                car.animations = clips.into_iter().map(|(_, a)| a).collect();
            }
            let file = format!("{stem}.car");
            write(
                &dir.join(file.to_ascii_uppercase()),
                &c2_formats::car::write(&car),
            )?;
            let w = s.weapon();
            let name = self.name(&s, stem);
            let mut b = format!(
                "{{\n name = '{name}'\n file = '{file}'\n pic = '{pic}'\n power = {}\n prec = {}\n loud = {}\n rate = {}\n shots = {}\n",
                num(w.power),
                num(w.prec),
                num(w.loud),
                num(w.rate),
                w.shots
            );
            if w.trace > 1 {
                let _ = writeln!(b, " trace = {}", w.trace);
            }
            if w.reload > 0 && w.reload < w.shots {
                let _ = writeln!(b, " reload = {}", w.reload);
            }
            if w.zoom > 0.0 && w.zoom < 45.0 {
                b.push_str(" optic = 1\n");
            }
            if stem == "dbsgun" {
                // Both barrels at once, as the PC's.
                b.push_str(" fall = 1\n");
            }
            b.push_str("}\n");
            self.res.push_str(&b);
            self.built.weapons.push(name);
        }
        self.res.push_str("}\n\n");
        Ok(())
    }

    fn animal_block(&self, name: &str, file: &str, ai: i32, s: &Stored) -> String {
        let a = s.animal();
        let mut b = format!(
            "{{\n name = '{name}'\n file = '{file}'\n ai = {ai}\n health = {}\n mass = {}\n",
            num(a.health),
            num(a.mass)
        );
        if ai >= 6 {
            for (k, v) in [
                ("length", a.length),
                ("radius", a.radius),
                ("basescore", a.score),
                ("smell", a.smell),
                ("hear", a.hear),
                ("look", a.look),
                ("shipdelta", a.ship_delta),
            ] {
                let _ = writeln!(b, " {k} = {}", num(v));
            }
            let _ = writeln!(b, " scale0 = {}", num(a.scale0 * 1000.0));
            let _ = writeln!(b, " scaleA = {}", num(a.scale_a * 1000.0));
        }
        if matches!(ai, 6 | 13 | 15 | 16 | 17 | 18) {
            b.push_str(" danger = TRUE\n");
        }
        b.push_str("}\n");
        b
    }

    fn animals(&mut self) -> Result<(), ModError> {
        self.res.push_str("characters {\n");
        if let Some(c) = cat(self.apk, "hunter1") {
            let car = car_from_in(&c, self.apk, "characters")?;
            self.write_car("HUNTER1.CAR", &car)?;
            self.res
                .push_str("{\n name = 'Hunter'\n file = 'hunter1.car'\n ai = 0\n health = 2\n}\n");
        }
        let list: Vec<(i32, Stored)> = self
            .chars
            .params
            .iter()
            .map(|(i, s)| (*i, s.clone()))
            .collect();
        let mut prices = Vec::new();
        for (index, s) in list {
            let stem = AMBIENTS
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, s)| (*s).to_string())
                .or_else(|| self.chars.files.get(&index).cloned());
            let Some(stem) = stem else { continue };
            let Some(c) = cat(self.apk, &stem) else {
                self.built.notes.push(format!("{stem}: no descriptor"));
                continue;
            };
            let car = car_from_in(&c, self.apk, "characters")?;
            let name = self.name(&s, &stem);
            let (ai, car) = if matches!(index, 7..=12 | 16..=24) {
                // The PC's own: the same clips in the same order.
                (index - 6, car)
            } else {
                let donor = DONORS
                    .iter()
                    .find(|(d, _)| *d == stem)
                    .and_then(|(_, names)| {
                        names
                            .iter()
                            .find_map(|n| PC_AI.iter().find(|(p, _)| p == n).map(|(_, ai)| *ai))
                    });
                let Some(ai) = donor else {
                    self.built
                        .notes
                        .push(format!("{name}: no PC kind to run it as (left out)"));
                    continue;
                };
                // Checked: the clip in the death's place falls.
                let die = layout(ai).iter().position(|r| *r == "die");
                let arranged = roles(&c, &stem, &car)
                    .and_then(|r| arrange(&car, &r, layout(ai)))
                    .filter(|a| die.is_some_and(|d| falls(a, d)));
                let Some(car) = arranged else {
                    self.built.notes.push(format!(
                        "{name}: its animations' roles are unknown (left out)"
                    ));
                    continue;
                };
                (ai, car)
            };
            let file = format!("{stem}.car");
            self.write_car(&file.to_ascii_uppercase(), &car)?;
            self.res.push_str(&self.animal_block(&name, &file, ai, &s));
            self.built.animals.push(format!("{name} ({file}, AI {ai})"));
            if ai >= 10 {
                let key = s.keys.first().map(|k| k.replace("_HEALTH", "_PRICE"));
                prices.push(
                    key.and_then(|k| self.balance.get(&k).copied())
                        .unwrap_or(0.0),
                );
            }
        }
        self.res.push_str("}\n\n");
        // Prices: the start, areas, animals (in the order listed), weapons.
        let mut p = String::from("prices {\n");
        let start = self
            .balance
            .get("DEFAULT_CREDITS")
            .copied()
            .unwrap_or(100.0);
        let _ = writeln!(p, " start = {start}");
        for n in 1.. {
            let Some(v) = self.balance.get(&format!("AREA{n}_PRICE")) else {
                break;
            };
            let _ = writeln!(p, " area = {v}");
        }
        for v in prices {
            let _ = writeln!(p, " dino = {v}");
        }
        for (k, _, _) in WEAPONS {
            if let Some(v) = self.balance.get(&format!("WEAPON_{k}_PRICE")) {
                let _ = writeln!(p, " weapon = {v}");
            }
        }
        p.push_str(" acces = 0\n acces = 0\n acces = 0\n acces = 0\n}\n\n.\n");
        self.res.push_str(&p);
        Ok(())
    }

    /// The ship and the supply drop.
    fn props(&mut self) -> Result<(), ModError> {
        for stem in ["ship2a", "sship1", "bag1"] {
            if let Some(c) = cat(self.apk, stem) {
                match car_from_in(&c, self.apk, "misc") {
                    Ok(car) => {
                        self.write_car(&format!("{}.CAR", stem.to_ascii_uppercase()), &car)?
                    }
                    Err(e) => self.built.notes.push(format!("{stem}: {e}")),
                }
            }
        }
        Ok(())
    }
}

/// Rebuilds what is done so far of `apk` as a game folder at `out`.
pub fn build(apk: &Path, out: &Path) -> Result<Built, ModError> {
    let zip = Apk(c2_vfs::ZipBackend::new(apk, "")?);
    let huntdat = out.join("HUNTDAT");
    let dir = huntdat.join("AREAS");
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let mut built = Built::default();
    for n in 1.. {
        if zip.get(&format!("assets/areas/area{n}.man")).is_none() {
            break;
        }
        built
            .areas
            .push(area(&zip, &format!("area{n}"), &dir, &format!("AREA{n}"))?);
    }
    if zip.get("assets/areas/trophy.man").is_some() {
        built.areas.push(area(&zip, "trophy", &dir, "TROPHY")?);
    }
    let Some((lib, balance)) = library(&zip) else {
        built
            .notes
            .push("no game library with its tables in the APK: no animals or weapons".into());
        return Ok(built);
    };
    let chars = mobile_chars::characters(&lib, &balance)?;
    // English names; Ice Age's own strings over the shared ones.
    let mut strings = BTreeMap::new();
    for f in [
        "assets/localizations/strings.csv",
        "assets/localizations/strings_ia.csv",
    ] {
        if let Some(d) = zip.get(f) {
            strings.extend(mobile_chars::strings(&d));
        }
    }
    let mut g = Game {
        apk: &zip,
        huntdat,
        chars,
        balance,
        strings,
        res: String::new(),
        built,
    };
    g.weapons()?;
    g.animals()?;
    g.props()?;
    let res = g.huntdat.join("_RES.TXT");
    write(&res, g.res.replace('\n', "\r\n").as_bytes())?;
    Ok(g.built)
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn weapon_clips_by_name() {
        let order: Vec<_> = [
            "revolver_down",
            "revolver_fire",
            "revolver_reload",
            "revolver_up",
        ]
        .iter()
        .map(|f| weapon_role(f))
        .collect();
        assert_eq!(order, [Some(2), Some(1), Some(3), Some(0)]);
        assert_eq!(weapon_role("pistol_animation_0"), None);
    }
}
