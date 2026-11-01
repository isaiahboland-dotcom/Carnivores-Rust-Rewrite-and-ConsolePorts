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
use c2_formats::mobile::{car_from_in, parse_cat, Assets, Cat, Rgba};
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
    let text = String::from_utf8_lossy(&rst);
    // Its snow (Ice Age's areas), as the engine reads it (`Area::set_snow`).
    let snow = text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "snow_density").then(|| v.trim().parse::<f32>().ok())?
    });
    if let Some(d) = snow.filter(|&d| d > 0.0) {
        write(
            &dir.join(format!("{stem}.SNOW")),
            format!("density {d}\n").as_bytes(),
        )?;
    }
    let rst = mobile_area::parse_rst(&text)?;
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

/// The phone game's interface: its sprites (in their atlases' pixels) and
/// the atlases they use.
struct Kit {
    sprites: BTreeMap<String, c2_formats::mobile_ui::Sprite>,
    atlases: BTreeMap<String, Rgba>,
}

impl Kit {
    /// The sprite configs (the other game's menu left out: both APKs carry
    /// both) and every atlas they name.
    fn load(apk: &Apk, ice_age: bool) -> Option<Self> {
        let skip = if ice_age { "dinhunter" } else { "iceage" };
        let mut raw = BTreeMap::new();
        let files = apk.0.files();
        for f in files
            .iter()
            .filter(|f| f.starts_with("assets/ui/") && f.ends_with(".cfg") && !f.contains(skip))
        {
            if let Some(d) = apk.get(f) {
                c2_formats::mobile_ui::parse(&String::from_utf8_lossy(&d), &mut raw);
            }
        }
        let mut atlases = BTreeMap::new();
        for t in raw.values().map(|s| s.texture.clone()) {
            if atlases.contains_key(&t) {
                continue;
            }
            let found = files.iter().find(|f| {
                let l = f.to_ascii_lowercase();
                l.starts_with("assets/textures/")
                    && l.rsplit('/')
                        .next()
                        .and_then(|n| n.rsplit_once('.'))
                        .is_some_and(|(n, e)| {
                            n == t.to_ascii_lowercase() && ["pkm", "crt", "tga", "png"].contains(&e)
                        })
            });
            let img = found.and_then(|f| {
                let d = apk.get(f)?;
                if f.to_ascii_lowercase().ends_with(".png") {
                    let (width, height, pixels) = crate::mobile::decode_png(&d)?;
                    Some(Rgba {
                        width,
                        height,
                        pixels,
                    })
                } else {
                    c2_formats::mobile::decode_texture(f, &d).ok()
                }
            });
            if let Some(img) = img {
                atlases.insert(t, img);
            }
        }
        let sprites = c2_formats::mobile_ui::scale(&raw, &|t| {
            atlases
                .get(t)
                .map(|i: &Rgba| (i.width as u32, i.height as u32))
        });
        (!sprites.is_empty()).then_some(Self { sprites, atlases })
    }

    /// A sprite's pixels.
    fn crop(&self, name: &str) -> Option<Rgba> {
        let s = self.sprites.get(name)?;
        let a = self.atlases.get(&s.texture)?;
        let (x0, y0) = (s.x.round() as usize, s.y.round() as usize);
        let (w, h) = (s.w.round() as usize, s.h.round() as usize);
        let mut pixels = Vec::with_capacity(w * h * 4);
        for y in y0..y0 + h {
            let i = (y.min(a.height.saturating_sub(1)) * a.width + x0) * 4;
            pixels.extend_from_slice(a.pixels.get(i..i + w * 4)?);
        }
        Some(Rgba {
            width: w,
            height: h,
            pixels,
        })
    }
}

/// The phone's trophy room is a walled square (`bound_west` .. in its
/// `.rst`, cells) round a raised middle; the PC's ring of stands
/// (`Area::set_trophy_mode`) lies outside it. Ours: six stands a side, four
/// cells in from the walls and facing in, in the ring's order (north west
/// to east, east north to south, south east to west, west south to north);
/// the hunter starts at the south end of the west walk (facing north, as he
/// starts: down the west row to the north one). The
/// engine reads it as `TROPHY.SPOT` (`Area::set_trophy_spots`).
fn trophy_spots(rst: &str) -> Option<String> {
    let bound = |k: &str| {
        rst.lines().find_map(|l| {
            let (key, v) = l.split_once('=')?;
            (key.trim() == k).then(|| v.trim().parse::<i32>().ok())?
        })
    };
    let (w, e) = (bound("bound_west")?, bound("bound_east")?);
    let (n, s) = (bound("bound_north")?, bound("bound_south")?);
    if e - w < 24 || s - n < 24 || !(0..1024).contains(&w) || !(0..1024).contains(&e) {
        return None;
    }
    let along = |a: i32, b: i32| (0..6).map(move |i| a + (b - a) * i / 5);
    let (x0, x1, z0, z1) = (w + 9, e - 9, n + 9, s - 9);
    let mut out = String::from("# c2rs: the phone game's trophy room (its walls), our stands\n");
    out += &format!("start {} {}\n", w + 12, s - 10);
    let sides = [
        along(x0, x1).map(|x| (x, n + 4)).collect::<Vec<_>>(),
        along(z0, z1).map(|z| (e - 4, z)).collect(),
        along(x1, x0).map(|x| (x, s - 4)).collect(),
        along(z1, z0).map(|z| (w + 4, z)).collect(),
    ];
    for (x, z) in sides.concat() {
        out += &format!("stand {x} {z}\n");
    }
    Some(out)
}

/// Makes the dark (or faint) pixels joined to the picture's edges
/// transparent.
fn clear_surround(img: &mut Rgba) {
    let (w, h) = (img.width, img.height);
    let dark = |p: &[u8]| p[3] < 128 || u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 40;
    let mut seen = vec![false; w * h];
    let mut todo: Vec<usize> = (0..w)
        .flat_map(|x| [x, (h.saturating_sub(1)) * w + x])
        .chain((0..h).flat_map(|y| [y * w, y * w + w.saturating_sub(1)]))
        .collect();
    while let Some(i) = todo.pop() {
        if i >= w * h || seen[i] || !dark(&img.pixels[i * 4..i * 4 + 4]) {
            continue;
        }
        seen[i] = true;
        img.pixels[i * 4 + 3] = 0;
        let (x, y) = (i % w, i / w);
        if x > 0 {
            todo.push(i - 1);
        }
        if x + 1 < w {
            todo.push(i + 1);
        }
        if y > 0 {
            todo.push(i - w);
        }
        if y + 1 < h {
            todo.push(i + w);
        }
    }
}

/// RGBA as a PNG.
fn png(img: &Rgba) -> Vec<u8> {
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, img.width as u32, img.height as u32);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = e.write_header() {
        let _ = w.write_image_data(&img.pixels);
    }
    out
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
    /// The menu's animals and weapons, in order.
    menu: Vec<MenuAnimal>,
    weapon_menu: Vec<(i32, Stored, String)>,
    kit: Option<Kit>,
    ice_age: bool,
}

/// A huntable as the menus show it.
struct MenuAnimal {
    /// Its character index.
    index: i32,
    stem: String,
    name: String,
    stored: Stored,
    cat: Cat,
    car: Car,
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
        for (index, s) in list {
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
            self.weapon_menu.push((index, s.clone(), name.clone()));
            self.built.weapons.push(name);
        }
        self.res.push_str("}\n\n");
        Ok(())
    }

    /// A `characters` entry; `runs_as`: the PC AI an extra AI number runs
    /// as.
    fn animal_block(
        &self,
        name: &str,
        file: &str,
        (ai, runs_as): (i32, Option<i32>),
        s: &Stored,
    ) -> String {
        let a = s.animal();
        let mut b = format!(
            "{{\n name = '{name}'\n file = '{file}'\n ai = {ai}\n health = {}\n mass = {}\n",
            num(a.health),
            num(a.mass)
        );
        if let Some(r) = runs_as {
            let _ = writeln!(b, " runsAs = {r}");
        }
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
        if matches!(runs_as.unwrap_or(ai), 6 | 13 | 15 | 16 | 17 | 18) {
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
        // The PC's own first, then the new ones: a huntable's menu slot is
        // its place among them, and each new one takes the next AI number
        // from 19 (a slot of its own; `runsAs` its kind's AI), as a patched
        // exe's extra slots.
        let pc = |i: i32| matches!(i, 7..=12 | 16..=24);
        let mut list: Vec<(i32, Stored)> = self
            .chars
            .params
            .iter()
            .map(|(i, s)| (*i, s.clone()))
            .collect();
        list.sort_by_key(|(i, _)| (!pc(*i), *i));
        let mut extra_ai = 19;
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
            let (ai, car) = if pc(index) {
                // The PC's own: the same clips in the same order.
                ((index - 6, None), car)
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
                extra_ai += 1;
                ((extra_ai - 1, Some(ai)), car)
            };
            let file = format!("{stem}.car");
            self.write_car(&file.to_ascii_uppercase(), &car)?;
            self.res.push_str(&self.animal_block(&name, &file, ai, &s));
            let (ai, runs_as) = ai;
            self.built.animals.push(match runs_as {
                Some(r) => format!("{name} ({file}, AI {ai} as {r})"),
                None => format!("{name} ({file}, AI {ai})"),
            });
            if ai >= 10 {
                self.menu.push(MenuAnimal {
                    index,
                    stem: stem.clone(),
                    name: name.clone(),
                    stored: s.clone(),
                    cat: c.clone(),
                    car: car.clone(),
                });
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

    /// A string with the mobile text's escapes (`\n`) as lines, and its
    /// `%d` as `value` (`%d%%`: a percentage).
    fn text(&self, key: &str, value: Option<f64>) -> Option<String> {
        let t = self.strings.get(key)?;
        let v = value.map_or(String::new(), |v| format!("{}", v.abs().round() as i64));
        Some(
            t.replace("%d%%", &format!("{v}%"))
                .replace("%d", &v)
                .replace("\\n", "\n")
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\r\n"),
        )
    }

    /// The menus' texts, pictures and calls (`MENU/TXT`, `MENU/PICS`,
    /// `SOUNDFX`), from the APK's strings, help pictures and sprites.
    fn menu_files(&mut self) -> Result<(), ModError> {
        let txt = self.huntdat.join("MENU").join("TXT");
        let pics = self.huntdat.join("MENU").join("PICS");
        let calls = self.huntdat.join("SOUNDFX").join("CALLS");
        for d in [&txt, &pics, &calls] {
            std::fs::create_dir_all(d).map_err(io(d))?;
        }
        let areas = self.built.areas.len().saturating_sub(1);
        for n in 1..=areas {
            let name = self.text(&format!("STR_AREA{n}"), None);
            let info = self.text(&format!("STR_AREA{n}_INFO"), None);
            if let Some(name) = name {
                let t = format!("{name}\r\n\r\n{}\r\n", info.unwrap_or_default());
                write(&txt.join(format!("AREA{n}.TXT")), t.as_bytes())?;
            }
            if let Some(img) = self
                .kit
                .as_ref()
                .and_then(|k| k.crop(&format!("button_area_active_{n}")))
            {
                let tga = crate::mobile::menu_picture(img.width, img.height, &img.pixels);
                write(&pics.join(format!("AREA{n}.TGA")), &tga)?;
            }
        }
        for (i, a) in self.menu.iter().enumerate() {
            let n = i + 1;
            let key = a.stored.name.clone().unwrap_or_default();
            let score = Some(f64::from(a.stored.animal().score));
            let info = self.text(&format!("{key}_INFO"), score).unwrap_or_default();
            let t = format!("Name   : {}\r\n{info}\r\n", a.name);
            // The mobile texts give both units.
            write(&txt.join(format!("DINO{n}.TXM")), t.as_bytes())?;
            write(&txt.join(format!("DINO{n}.TXU")), t.as_bytes())?;
            let pic = crate::mobile::help_picture(self.apk, &[a.name.clone(), a.stem.clone()])
                .or_else(|| crate::mobile::model_picture(&a.car));
            if let Some((w, h, px)) = pic {
                write(
                    &pics.join(format!("DINO{n}.TGA")),
                    &crate::mobile::menu_picture(w, h, &px),
                )?;
            }
            // Its calls: the `callN` set holding its call or growl; else
            // the set its index gives (`call1` is index 16's, the
            // Parasaurolophus: the sets go by index, checked on the four
            // whose calls are named).
            let snd = |f: &str| self.apk.get(&format!("assets/sounds/characters/{f}.wav"));
            let mine: Vec<Vec<u8>> = a
                .cat
                .sounds
                .iter()
                .filter(|f| matches!(crate::mobile::role(f), Some("call" | "growl")))
                .filter_map(|f| snd(f))
                .chain(snd(&format!("{}_call", a.stem)))
                .collect();
            let by_index = || {
                let k = a.index - 15;
                let set: Vec<Vec<u8>> = ['a', 'b', 'c']
                    .iter()
                    .filter_map(|l| {
                        self.apk
                            .get(&format!("assets/sounds/calls/call{k}_{l}.wav"))
                    })
                    .collect();
                (k > 0 && !set.is_empty()).then_some(set)
            };
            match crate::mobile::call_set(self.apk, &mine).or_else(by_index) {
                Some(set) => {
                    for (l, d) in ['A', 'B', 'C'].iter().zip(set) {
                        let (pcm, rate) = c2_formats::mobile::decode_wav(&d)?;
                        let w = crate::mobile::wav(&pcm, rate);
                        write(&calls.join(format!("CALL{n}_{l}.WAV")), &w)?;
                    }
                }
                None => self.built.notes.push(format!("{}: no call", a.name)),
            }
        }
        for (i, (index, w, name)) in self.weapon_menu.iter().enumerate() {
            let n = i + 1;
            let key = w.name.clone().unwrap_or_default();
            let info = self.text(&format!("{key}_INFO"), None).unwrap_or_default();
            let t = format!("Weapon : {name}\r\n\r\n{info}\r\n");
            write(&txt.join(format!("WEAPON{n}.TXT")), t.as_bytes())?;
            // The mobile menu's weapon pictures: 1-6 in order, the camera 7th,
            // the revolver 8th.
            let slot = if *index == 41 { 8 } else { index + 1 };
            if let Some(img) = self
                .kit
                .as_ref()
                .and_then(|k| k.crop(&format!("button_weapon_active_{slot}")))
            {
                let tga = crate::mobile::menu_picture(img.width, img.height, &img.pixels);
                write(&pics.join(format!("WEAPON{n}.TGA")), &tga)?;
            }
        }
        // Hunting times, observer mode and the accessories.
        let score = |k: &str| self.balance.get(k).copied();
        for (file, key, head, value) in [
            ("DAY1.NFO", "DAWN", "Hunting time", None),
            ("DAY2.NFO", "DAY", "Hunting time", None),
            ("DAY3.NFO", "NIGHT", "Hunting time", None),
            ("OBSERVE.NFO", "OBSERVER_MODE", "Mode", None),
            (
                "CAMOFLAG.NFO",
                "CAMOUFLAGE",
                "Accessory",
                score("SCORE_BONUS_CAMOUFLAGE"),
            ),
            (
                "SCENT.NFO",
                "COVER_SCENT",
                "Accessory",
                score("SCORE_BONUS_COVER_SCENT"),
            ),
            (
                "RADAR.NFO",
                "RADAR",
                "Accessory",
                score("SCORE_BONUS_NO_RADAR"),
            ),
            (
                "TRANQ.NFO",
                "TRANQUILIZER",
                "Accessory",
                score("SCORE_BONUS_TRANQUILIZER"),
            ),
        ] {
            let (Some(full), Some(info)) = (
                self.text(&format!("STR_{key}_FULL"), None),
                self.text(&format!("STR_{key}_INFO"), value),
            ) else {
                continue;
            };
            let t = format!("{head} : {full}\r\n\r\n{info}\r\n");
            write(&txt.join(file), t.as_bytes())?;
        }
        // Double ammo has no text in the phone game: ours.
        write(
            &txt.join("DOUBLE.NFO"),
            b"Accessory : Double Ammo\r\n\r\nTwice the ammunition for every weapon you carry.\r\n",
        )?;
        // The hunter's and the menus' sounds, by the PC's names.
        let fx = self.huntdat.join("SOUNDFX");
        std::fs::create_dir_all(fx.join("STEPS")).map_err(io(&fx))?;
        // (APK file, PC file): the phone keeps the PC's names, lower case.
        let mut sounds = vec![
            ("misc/a_underw".to_string(), "A_UNDERW".to_string()),
            ("menu/menugo".into(), "MENUGO".into()),
            ("menu/menumov".into(), "MENUMOV".into()),
            ("menu/menumusic_cmpr".into(), "MENUAMB".into()),
        ];
        for n in 1..=4 {
            sounds.push((format!("hunter/hum_die{n}"), format!("HUM_DIE{n}")));
        }
        for n in 1..=3 {
            for kind in ["HWALK", "FOOTW"] {
                let from = format!("steps/{}{n}", kind.to_ascii_lowercase());
                sounds.push((from, format!("STEPS/{kind}{n}")));
            }
        }
        for (from, to) in sounds {
            if let Some(d) = self.apk.get(&format!("assets/sounds/{from}.wav")) {
                let (pcm, rate) = c2_formats::mobile::decode_wav(&d)?;
                write(
                    &fx.join(format!("{to}.WAV")),
                    &crate::mobile::wav(&pcm, rate),
                )?;
            }
        }
        Ok(())
    }

    /// The mobile interface's art (`MOBILE/*.PNG`, `MOBILE/SPRITES.CFG`),
    /// for menus drawn as the phone game's.
    fn ui_kit(&mut self) -> Result<(), ModError> {
        let Some(kit) = &self.kit else { return Ok(()) };
        let dir = self.huntdat.join("MOBILE");
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        for (name, img) in &kit.atlases {
            write(
                &dir.join(format!("{}.PNG", name.to_ascii_uppercase())),
                &png(img),
            )?;
        }
        // The interface's bitmap fonts (BMFont): each descriptor with its
        // atlas as a PNG beside it.
        let fonts: Vec<String> = self
            .apk
            .0
            .files()
            .into_iter()
            .filter(|f| f.starts_with("assets/fonts/") && f.contains(".fnt"))
            .collect();
        for f in fonts {
            let Some(d) = self.apk.get(&f) else { continue };
            let text = String::from_utf8_lossy(&d).into_owned();
            let font = c2_formats::bmfont::parse(&text);
            let page = format!("assets/fonts/{}", font.page);
            let Some(img) = self
                .apk
                .get(&page)
                .and_then(|p| c2_formats::mobile::decode_texture(&page, &p).ok())
            else {
                continue;
            };
            let atlas = font
                .page
                .rsplit_once('.')
                .map_or(font.page.as_str(), |(n, _)| n)
                .to_ascii_uppercase();
            write(&dir.join(format!("{atlas}.PNG")), &png(&img))?;
            let stem = f
                .rsplit('/')
                .next()
                .and_then(|n| n.split_once('.'))
                .map_or("FONT", |(n, _)| n)
                .to_ascii_uppercase();
            let text = text.replace(&format!("\"{}\"", font.page), &format!("\"{atlas}.PNG\""));
            write(&dir.join(format!("{stem}.FNT")), text.as_bytes())?;
        }
        // The hunt's area maps (each area at each time of day, the grid
        // painted in) and the compass's face.
        let maps = dir.join("MAPS");
        std::fs::create_dir_all(&maps).map_err(io(&maps))?;
        for f in self.apk.0.files() {
            let l = f.to_ascii_lowercase();
            let Some(name) = l.strip_prefix("assets/textures/") else {
                continue;
            };
            let stem = name.rsplit('/').next().and_then(|n| n.split_once('.'));
            let out = match stem {
                Some((n, _)) if name.starts_with("maps/") && n.ends_with("_map") => maps.join(
                    format!("{}.PNG", n.trim_end_matches("_map").to_ascii_uppercase()),
                ),
                Some(("compas", _)) => dir.join("COMPAS.PNG"),
                _ => continue,
            };
            if let Some(mut img) = self
                .apk
                .get(&f)
                .and_then(|d| c2_formats::mobile::decode_texture(&f, &d).ok())
            {
                // The compass's face is cut out by its model; the black
                // around it goes (filled in from the edges, so its marks
                // stay).
                if out.ends_with("COMPAS.PNG") {
                    clear_surround(&mut img);
                }
                write(&out, &png(&img))?;
            }
        }
        let title = if self.ice_age {
            "Carnivores: Ice Age"
        } else {
            "Carnivores: Dinosaur Hunter"
        };
        write(&dir.join("GAME.TXT"), title.as_bytes())?;
        write(
            &dir.join("SPRITES.CFG"),
            c2_formats::mobile_ui::write(&kit.sprites).as_bytes(),
        )?;
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
        // The phone game has no water rings: ours.
        self.write_car("WCIRCLE2.CAR", &water_ring())
    }
}

/// A water ring (`WCIRCLE2.CAR`, which the hunt spreads where something
/// wades): a flat band of 16 segments, faint white towards its outer edge,
/// growing over two seconds from 24 to 128 units across as the original's.
fn water_ring() -> Car {
    use c2_formats::car::{Animation, ANIFX_SLOTS};
    use c2_formats::model::{face_flags, Face, Mesh, Vertex};
    const SEG: usize = 16;
    const FRAMES: usize = 41;
    // Outer and inner radius at `t` (0..=1).
    let radii = |t: f32| {
        let outer = 24.0 + 104.0 * t;
        [outer, (outer - 22.0).max(2.0)]
    };
    let at = |r: f32, i: usize| {
        let a = i as f32 / SEG as f32 * std::f32::consts::TAU;
        (r * a.cos(), r * a.sin())
    };
    // Vertices: the outer circle, then the inner.
    let ring = |t: f32| -> Vec<(f32, f32)> {
        radii(t)
            .iter()
            .flat_map(|&r| (0..SEG).map(move |i| at(r, i)))
            .collect()
    };
    let vertices = ring(0.0)
        .into_iter()
        .enumerate()
        .map(|(i, (x, z))| Vertex {
            x,
            y: 0.0,
            z,
            owner: (i / SEG) as i16,
            hide: 0,
        })
        .collect();
    // Across the band: u 0 (inner) to 63 (outer).
    let face = |v: [usize; 3], tu: [i32; 3]| Face {
        v: v.map(|v| v as i32),
        tu,
        tv: [8; 3],
        flags: face_flags::DOUBLE_SIDE,
        dmask: 0,
        distant: 0,
        next: 0,
        group: 0,
        reserved: [0; 12],
    };
    let mut faces = Vec::new();
    for i in 0..SEG {
        let j = (i + 1) % SEG;
        faces.push(face([i, j, SEG + i], [63, 63, 0]));
        faces.push(face([j, SEG + j, SEG + i], [63, 0, 0]));
    }
    // A soft band, brightest three quarters of the way out (5-5-5 grey).
    let mut texture = vec![0u16; 256 * 256];
    for (i, p) in texture.iter_mut().enumerate() {
        let u = (i % 256) as f32 / 63.0;
        let v = if u > 1.0 {
            0.0
        } else if u < 0.75 {
            u / 0.75
        } else {
            (1.0 - u) / 0.25
        };
        let c = (v * 26.0) as u16;
        *p = c << 10 | c << 5 | c;
    }
    let mut data = Vec::with_capacity(FRAMES * SEG * 2 * 3);
    for f in 0..FRAMES {
        for (x, z) in ring(f as f32 / (FRAMES - 1) as f32) {
            data.extend([(x * 8.0) as i16, 0, (-z * 8.0) as i16]);
        }
    }
    let mut name_raw = [0u8; 32];
    name_raw[..8].copy_from_slice(b"Wcircle2");
    let mut anim_name = [0u8; 32];
    anim_name[..5].copy_from_slice(b"Water");
    Car {
        name_raw,
        mesh: Mesh {
            vertices,
            faces,
            texture,
        },
        animations: vec![Animation {
            name_raw: anim_name,
            kps: 20,
            frames: FRAMES,
            data,
        }],
        sounds: Vec::new(),
        anim_sounds: Some([-1; ANIFX_SLOTS]),
        trailing: Vec::new(),
    }
}

/// Beside the game's `HUNTDAT`: `sha256:<hex>` of the APK it was built
/// from, for recipes whose base is an APK (`recipe::check_apk`).
pub const SOURCE_APK: &str = "from-apk.txt";

/// Rebuilds what is done so far of `apk` as a game folder at `out`.
pub fn build(apk: &Path, out: &Path) -> Result<Built, ModError> {
    let zip = Apk(c2_vfs::ZipBackend::new(apk, "")?);
    let huntdat = out.join("HUNTDAT");
    let dir = huntdat.join("AREAS");
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let sum = crate::recipe::sha256_file(apk).map_err(io(apk))?;
    let p = out.join(SOURCE_APK);
    write(&p, format!("{sum}\n").as_bytes())?;
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
        // Its walls aren't the PC room's: our layout of its stands.
        let rst = zip.get("assets/areas/trophy.rst").unwrap_or_default();
        match trophy_spots(&String::from_utf8_lossy(&rst)) {
            Some(t) => {
                let p = dir.join("TROPHY.SPOT");
                write(&p, t.as_bytes())?;
            }
            None => built
                .notes
                .push("trophy room: no bounds in trophy.rst, the PC's ring kept".into()),
        }
    }
    let Some((lib, balance)) = library(&zip) else {
        built
            .notes
            .push("no game library with its tables in the APK: no animals or weapons".into());
        return Ok(built);
    };
    // Its Survive mode (Dinosaur Hunter's), offered by the game beside it.
    if let Ok(Some(sv)) = crate::mobile::survive(apk) {
        let p = out.join(crate::mobile::MOBILE_SURVIVAL);
        std::fs::write(&p, sv.write()).map_err(io(&p))?;
    }
    let chars = mobile_chars::characters(&lib, &balance)?;
    let ice_age = zip.0.files().iter().any(|f| f.ends_with("/libiceAge.so"));
    // English names; Ice Age's own strings over the shared ones (a
    // Dinosaur Hunter APK carries them too, with Ice Age's areas).
    let mut strings = BTreeMap::new();
    let ia = ice_age.then_some("assets/localizations/strings_ia.csv");
    for f in std::iter::once("assets/localizations/strings.csv").chain(ia) {
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
        menu: Vec::new(),
        weapon_menu: Vec::new(),
        kit: Kit::load(&zip, ice_age),
        ice_age,
    };
    g.weapons()?;
    g.animals()?;
    g.props()?;
    g.menu_files()?;
    g.ui_kit()?;
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
    fn trophy_room_stands_inside_its_walls() {
        let rst =
            "[general]\nbound_west = 131\nbound_east = 178\nbound_north = 131\nbound_south = 178\n";
        let t = trophy_spots(rst).unwrap_or_default();
        let cells = |k: &str| -> Vec<(i32, i32)> {
            t.lines()
                .filter_map(|l| {
                    let mut w = l.split_whitespace();
                    (w.next()? == k).then_some(())?;
                    Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?))
                })
                .collect()
        };
        let stands = cells("stand");
        assert_eq!(cells("start"), vec![(143, 168)]);
        assert_eq!(stands.len(), 24);
        assert_eq!((stands[0], stands[5]), ((140, 135), (169, 135)));
        assert_eq!((stands[6], stands[23]), ((174, 140), (135, 140)));
        assert!(stands
            .iter()
            .all(|&(x, z)| (131..=178).contains(&x) && (131..=178).contains(&z)));
        assert!(trophy_spots("bound_west = 1").is_none());
    }

    #[test]
    fn surround_cleared_marks_kept() {
        // 5x5: black all round, a lit middle with a black mark in it.
        let mut px = vec![0u8; 5 * 5 * 4];
        for (i, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (i % 5, i / 5);
            let lit = (1..4).contains(&x) && (1..4).contains(&y) && (x, y) != (2, 2);
            p.copy_from_slice(&if lit {
                [200, 100, 50, 255]
            } else {
                [0, 0, 0, 255]
            });
        }
        let mut img = Rgba {
            width: 5,
            height: 5,
            pixels: px,
        };
        clear_surround(&mut img);
        let a = |x: usize, y: usize| img.pixels[(y * 5 + x) * 4 + 3];
        assert_eq!((a(0, 0), a(4, 2), a(1, 1), a(2, 2)), (0, 0, 255, 255));
    }

    #[test]
    fn water_ring_round_trips() {
        let car = water_ring();
        let back = c2_formats::car::parse(&c2_formats::car::write(&car)).expect("parses");
        assert_eq!(back.mesh.vertices.len(), 32);
        assert_eq!(back.mesh.faces.len(), 32);
        assert_eq!(back.animations[0].frames, 41);
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
