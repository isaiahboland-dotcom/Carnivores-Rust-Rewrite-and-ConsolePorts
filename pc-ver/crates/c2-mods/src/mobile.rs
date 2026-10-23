//! Animals from the mobile games, imported from an APK the user supplies
//! (README M15): each animal the game being played doesn't have becomes a
//! loose add-on folder, the way community add-ons ship (a model, an entry
//! to paste, menu art, call sounds), so the usual add-on handling places
//! it. Nothing from the APK is kept anywhere but the user's own folder.
//!
//! - The model, texture, animations and sounds become one `.CAR`
//!   (`c2_formats::mobile`).
//! - The entry starts from a PC animal of the same kind (its AI, stats,
//!   spawns, kill setup; the mobile games give each new animal an AI of
//!   its own, approximated so), with the animal's own name, model and
//!   animations, picked by their names (`_walk`, `_die`...).
//! - Its menu picture (`Help/<name>.png`) and calls (the `callN` set whose
//!   first call is the animal's own call sound, else that sound) are named
//!   for the donor's menu slot, so the animal is added in a slot of its own
//!   next to it.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use c2_formats::menu_txt::MenuData;
use c2_formats::mobile::{car_from, decode_wav, parse_cat, Assets, Cat};
use c2_formats::res_txt::{write_block, Block, Entry, ResTxt, Sub};
use c2_vfs::{Backend, Vfs};

use crate::ModError;

/// The PC animals a mobile one takes its kind from, in order of preference
/// (the first the game has).
const DONORS: &[(&str, &[&str])] = &[
    ("amargasaurus", &["Stegosaurus"]),
    ("carnotaurus", &["Ceratosaurus", "Allosaurus"]),
    ("coelophysis", &["Velociraptor"]),
    ("dilophosaurus", &["Allosaurus"]),
    ("gigantoraptor", &["Allosaurus"]),
    ("iguanodon", &["Parasaurolophus"]),
    ("oviraptor", &["Parasaurolophus"]),
    ("pach", &["Chasmosaurus"]),
    ("troodon", &["Velociraptor"]),
    ("utahraptor", &["Velociraptor"]),
    ("andrewsarchus", &["Bear", "Allosaurus"]),
    ("doedicurus", &["Ankylosaurus"]),
    ("hyaenodon", &["Wolf", "Velociraptor"]),
    ("indricothere", &["Stegosaurus"]),
    ("titanis", &["Diatryma", "Velociraptor"]),
];

/// Names for animals whose descriptor has a working title (`Pach1`).
const NAMES: &[(&str, &str)] = &[("pach", "Pachycephalosaurus")];

/// One imported animal.
#[derive(Debug, Clone)]
pub struct Imported {
    pub name: String,
    /// Its add-on folder.
    pub dir: PathBuf,
    pub donor: String,
    pub notes: Vec<String>,
}

/// The APK, read as a zip.
struct Apk(c2_vfs::ZipBackend);

impl Assets for Apk {
    fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.0.read(path).ok()
    }
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> ModError + '_ {
    move |source| ModError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The animation keys an entry's role names give (`carnotaurus_run`).
fn role(file: &str) -> Option<&'static str> {
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

/// Each animation's role: from its name, or for 11 numbered ones the
/// usual order. `None`: numbered otherwise (a PC-style set, as its donor's).
fn roles(cat: &Cat) -> Option<Vec<Option<&'static str>>> {
    let named: Vec<_> = cat.animations.iter().map(|(f, _)| role(f)).collect();
    if named.contains(&Some("die")) {
        Some(named)
    } else if cat.animations.len() == ORDER.len() {
        Some(ORDER.iter().map(|r| Some(*r)).collect())
    } else {
        None
    }
}

/// Is this a mobile animal (it has a death animation, or numbered ones)?
fn is_animal(cat: &Cat) -> bool {
    cat.animations.len() >= 6
        && cat
            .animations
            .iter()
            .all(|(f, _)| role(f).is_some() || f.contains("_animation_"))
}

fn entry(key: &str, value: impl ToString) -> Entry {
    Entry {
        key: key.into(),
        value: value.to_string(),
    }
}

/// The donor's entry made the mobile animal's: name, model, animations
/// (the donor's own when the animal's are a PC-style numbered set); what
/// belongs to the donor alone (trophy poses, menu slot) dropped. Its slot
/// conditions (`overwrite char5`: radar, the picked spawn group) are kept
/// when the animal gets a slot of its own (`slot`): building moves them to
/// that slot.
fn make_entry(donor: &Block, name: &str, car: &str, cat: &Cat, slot: bool) -> Block {
    let char_cond = |n: &str| {
        !slot && (n.starts_with("overwrite") || n.starts_with("addition")) && n.contains("char")
    };
    let Some(roles) = roles(cat) else {
        let mut b = donor.clone();
        b.entries.retain(|e| {
            let k = e.key.to_ascii_lowercase();
            !["name", "file", "callno", "survivalindex"].contains(&k.as_str())
        });
        b.subs.retain(|s| {
            let n = s.name.trim().to_ascii_lowercase();
            !(n == "tropinfo" || char_cond(&n))
        });
        b.entries.insert(0, entry("name", format!("'{name}'")));
        b.entries.insert(1, entry("file", format!("'{car}'")));
        for s in &mut b.subs {
            s.at = (s.at + 2).min(b.entries.len());
        }
        return b;
    };
    let mut b = donor.clone();
    let drop_key = |k: &str| {
        let k = k.to_ascii_lowercase();
        k.ends_with("anim") || ["name", "file", "callno", "survivalindex"].contains(&k.as_str())
    };
    b.entries.retain(|e| !drop_key(&e.key));
    let killtype = b
        .subs
        .iter()
        .find(|s| s.name.trim().eq_ignore_ascii_case("killtype"))
        .cloned();
    b.subs.retain(|s| {
        let n = s.name.trim().to_ascii_lowercase();
        !([
            "idlegroup",
            "idle2group",
            "deathtype",
            "killtype",
            "tropinfo",
        ]
        .contains(&n.as_str())
            || char_cond(&n))
    });
    b.entries.insert(0, entry("name", format!("'{name}'")));
    b.entries.insert(1, entry("file", format!("'{car}'")));
    for s in &mut b.subs {
        s.at = (s.at + 2).min(b.entries.len());
    }
    let find = |r: &str| roles.iter().position(|x| *x == Some(r));
    for (r, key) in [
        ("run", "runAnim"),
        ("walk", "walkAnim"),
        ("swim", "swimAnim"),
        ("growl", "roarAnim"),
    ] {
        if let Some(i) = find(r) {
            b.entries.push(entry(key, i));
        }
    }
    // A jump: the animal's own, else its attack (a lunge). Without one the
    // dialect takes animation phase 0 (the run) as the jump, and the AIs
    // that jump (raptors, Allosaurus) run without stopping.
    if let Some(i) = find("jump").or(find("attack")) {
        b.entries.push(entry("jumpAnim", i));
    }
    let end = b.entries.len();
    let sub = |name: &str, entries: Vec<Entry>| Sub {
        name: name.into(),
        at: end,
        block: Block {
            entries,
            subs: Vec::new(),
        },
    };
    for (i, r) in roles.iter().enumerate() {
        if matches!(r, Some("idle" | "call")) {
            b.subs.push(sub(
                "idlegroup",
                vec![
                    entry("anim", i),
                    entry("startChance", 0.03),
                    entry("endChance", 1),
                    entry("instantRepeat", "TRUE"),
                ],
            ));
        }
    }
    let mut death = Vec::new();
    if let Some(i) = find("die") {
        death.push(entry("dieAnim", i));
    }
    if let Some(i) = find("sleep").or(find("die")) {
        death.push(entry("sleepAnim", i));
    }
    b.subs.push(sub("deathtype", death));
    // The donor's kill (the hunter's side), with this animal's eating.
    if let Some(mut k) = killtype {
        k.block
            .entries
            .retain(|e| !e.key.eq_ignore_ascii_case("eatAnim"));
        if let Some(i) = find("eat").or(find("attack")) {
            k.block.entries.push(entry("eatAnim", i));
        }
        k.at = end;
        b.subs.push(k);
    }
    b
}

/// An RGBA PNG (8-bit, any colour type).
fn decode_png(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut d = png::Decoder::new(Cursor::new(data));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = info.color_type.samples();
    let mut out = Vec::with_capacity(w * h * 4);
    for p in buf[..w * h * ch].chunks(ch) {
        out.extend_from_slice(&match ch {
            4 => [p[0], p[1], p[2], p[3]],
            3 => [p[0], p[1], p[2], 255],
            2 => [p[0], p[0], p[0], p[1]],
            _ => [p[0], p[0], p[0], 255],
        });
    }
    Some((w, h, out))
}

/// A menu picture (357x208, as the PC menus' `DINOn.TGA`) with the image
/// fitted to its height and centred on the menus' olive.
fn menu_picture(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let (cw, ch) = (357usize, 208usize);
    let k = (ch as f32 / h.max(1) as f32).min(cw as f32 / w.max(1) as f32);
    let (dw, dh) = ((w as f32 * k) as usize, (h as f32 * k) as usize);
    let (ox, oy) = ((cw - dw.min(cw)) / 2, (ch - dh.min(ch)) / 2);
    let bg = [32u32, 44, 30];
    let mut pixels = Vec::with_capacity(cw * ch);
    for y in 0..ch {
        for x in 0..cw {
            let mut c = bg;
            if (ox..ox + dw).contains(&x) && (oy..oy + dh).contains(&y) {
                let (sx, sy) = ((x - ox) * w / dw.max(1), (y - oy) * h / dh.max(1));
                let i = (sy.min(h - 1) * w + sx.min(w - 1)) * 4;
                if let Some(p) = rgba.get(i..i + 4) {
                    let a = u32::from(p[3]);
                    for k in 0..3 {
                        c[k] = (u32::from(p[k]) * a + bg[k] * (255 - a)) / 255;
                    }
                }
            }
            pixels
                .push(((c[0] as u16 >> 3) << 10) | ((c[1] as u16 >> 3) << 5) | (c[2] as u16 >> 3));
        }
    }
    c2_formats::tga::write(&c2_formats::tga::Picture {
        width: cw,
        height: ch,
        pixels,
    })
}

/// 16-bit PCM bytes as a mono WAV.
fn wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let mut w = b"RIFF".to_vec();
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    for v in [1u16, 1] {
        w.extend_from_slice(&v.to_le_bytes());
    }
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    for v in [2u16, 16] {
        w.extend_from_slice(&v.to_le_bytes());
    }
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

/// The PCM after a WAV's `data` header.
fn wav_data(d: &[u8]) -> &[u8] {
    d.windows(4)
        .position(|w| w == b"data")
        .and_then(|i| d.get(i + 8..))
        .unwrap_or_default()
}

/// The APK's `callN` set (`_a`, `_b`, `_c`) holding one of `sounds` (the
/// animal's call or growl).
fn call_set(apk: &Apk, sounds: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
    for k in 1..=64 {
        let p = |l: char| format!("assets/sounds/calls/call{k}_{l}.wav");
        let set: Vec<Vec<u8>> = ['a', 'b', 'c']
            .iter()
            .filter_map(|&l| apk.get(&p(l)))
            .collect();
        if set.is_empty() {
            continue;
        }
        let same = |a: &[u8], b: &[u8]| {
            let (a, b) = (wav_data(a), wav_data(b));
            let n = a.len().min(b.len()).min(2000);
            n > 200 && a[..n] == b[..n]
        };
        if set.iter().any(|c| sounds.iter().any(|s| same(c, s))) {
            return Some(set);
        }
    }
    None
}

/// Whether `dir` is an add-on [`import`] made (its README says so).
pub fn is_import(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("README.txt"))
        .is_ok_and(|t| t.contains("from your own copy of the mobile game"))
}

/// The mobile game's tuning values (`DINO_T_REX_PRICE` ...), read from
/// its native library (`c2_formats::mobile_values`), largest first.
pub fn values(apk: &Path) -> Result<std::collections::BTreeMap<String, f64>, ModError> {
    let zip = c2_vfs::ZipBackend::new(apk, "")?;
    let mut libs: Vec<(String, Vec<u8>)> = zip
        .files()
        .into_iter()
        .filter(|f| f.starts_with("lib/arm64-v8a/") && f.ends_with(".so"))
        .filter_map(|f| zip.read(&f).ok().map(|d| (f, d)))
        .collect();
    libs.sort_by_key(|l| std::cmp::Reverse(l.1.len()));
    for (_, d) in &libs {
        if let Ok(v) = c2_formats::mobile_values::defaults(d) {
            if !v.is_empty() {
                return Ok(v);
            }
        }
    }
    Err(ModError::Recipe(
        "no tuning values found in the APK's arm64 libraries".into(),
    ))
}

/// The file `import` writes beside the add-ons for the Carnivores 2 APK:
/// its Survive mode (`c2_formats::mobile_arcade`), played on the PC's
/// Mount Ravan.
pub const MOBILE_SURVIVAL: &str = "mobile-survival.txt";

/// The Survive mode of a Carnivores 2 (Dinosaur Hunter) APK: its rules from
/// the native library, its animals' English names from its strings. `None`
/// for another game's APK (Ice Age's is on an Ice Age map).
pub fn survive(apk: &Path) -> Result<Option<c2_formats::mobile_arcade::Survive>, ModError> {
    let zip = c2_vfs::ZipBackend::new(apk, "")?;
    let Some(lib) = zip
        .files()
        .into_iter()
        .find(|f| f.starts_with("lib/arm64-v8a/") && f.contains("dinHunter"))
    else {
        return Ok(None);
    };
    let arcade = c2_formats::mobile_arcade::arcade(&zip.read(&lib)?)?;
    let csv = zip
        .read("assets/localizations/strings.csv")
        .map(|d| String::from_utf8_lossy(&d).into_owned())
        .unwrap_or_default();
    let english = |key: &str| {
        csv.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(",\""))
            .and_then(|r| r.split('"').next())
            .map(|n| {
                // "ALLOSAURUS" or "Allosaurus": as a name.
                let mut c = n.chars();
                c.next().map_or_else(String::new, |f| {
                    f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()
                })
            })
            .unwrap_or_else(|| key.trim_start_matches("STR_").to_string())
    };
    // The mobile Survive map: `area5`, Mount Ravan (`Menu::StartPlayingTheGame`).
    Ok(Some(arcade.survive(5, &english)))
}

/// The file `import` writes beside the add-ons: the mobile game's values
/// for the game's own areas, animals and weapons (a balance file; the
/// launcher offers each where it differs).
pub const MOBILE_VALUES: &str = "mobile-values.toml";

/// Letters and digits, lower case (`T_REX`, `T-Rex` -> `trex`).
fn norm(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Whether a mobile key's name and a game's name are the same animal or
/// weapon (`PACHYCEPHALLOSAURUS` / `Pachycephalosaurus`, `RHINOCEROS` /
/// `Rhino`): equal, or a long common start.
fn same(a: &str, b: &str) -> bool {
    let (a, b) = (norm(a), norm(b));
    let common = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    a == b || common >= 5 && common * 10 >= a.len().min(b.len()) * 6
}

/// `DINO_T_REX_PRICE` -> ("T_REX", "PRICE") for `prefix` `DINO_`.
fn split_key<'a>(key: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = key.strip_prefix(prefix)?;
    [
        "HEALTH",
        "PRICE",
        "SCORE",
        "POWER",
        "PRECISION",
        "RATE",
        "SHOTS",
    ]
    .iter()
    .find_map(|f| Some((rest.strip_suffix(&format!("_{f}"))?, *f)))
}

/// One mobile animal's or weapon's values, by field.
type Fields = std::collections::BTreeMap<&'static str, f64>;

fn grouped(
    values: &std::collections::BTreeMap<String, f64>,
    prefix: &str,
) -> Vec<(String, Fields)> {
    let mut out: Vec<(String, Fields)> = Vec::new();
    for (k, v) in values {
        let Some((name, field)) = split_key(k, prefix) else {
            continue;
        };
        let field: &'static str = match field {
            "HEALTH" => "health",
            "PRICE" => "price",
            "SCORE" => "score",
            "POWER" => "power",
            "PRECISION" => "prec",
            "RATE" => "rate",
            _ => "shots",
        };
        match out.iter_mut().find(|(n, _)| n == name) {
            Some((_, f)) => {
                f.insert(field, *v);
            }
            None => out.push((name.to_string(), Fields::from([(field, *v)]))),
        }
    }
    out
}

/// The mobile games' prices and scores are on their own scale (a T-Rex
/// costs 1500 there, 500 on PC): the median ratio over the animals both
/// have, for prices and for scores.
fn scales(game: &crate::balance::Balance, dinos: &[(String, Fields)]) -> (f64, f64) {
    let ratio = |field: &str, ours: &dyn Fn(&crate::balance::Item) -> Option<f64>| {
        let mut r: Vec<f64> = game
            .animals
            .iter()
            .filter_map(|it| {
                let (_, f) = dinos.iter().find(|(n, _)| same(n, &it.name))?;
                let (a, b) = (ours(it)?, *f.get(field)?);
                (a > 0.0 && b > 0.0).then_some(a / b)
            })
            .collect();
        r.sort_by(f64::total_cmp);
        r.get(r.len() / 2).copied().unwrap_or(1.0 / 3.0)
    };
    (
        ratio("price", &|i| i.price.map(f64::from)),
        ratio("score", &|i| i.score.map(f64::from)),
    )
}

fn round_price(v: f64) -> i32 {
    ((v / 5.0).round() * 5.0) as i32
}

fn round_score(v: f64) -> f32 {
    ((v * 2.0).round() / 2.0) as f32
}

/// The mobile values for the game's own entries, on the game's scale.
fn mobile_balance(
    values: &std::collections::BTreeMap<String, f64>,
    game: &crate::balance::Balance,
) -> crate::balance::Balance {
    use crate::balance::Item;
    let dinos = grouped(values, "DINO_");
    let weapons = grouped(values, "WEAPON_");
    let (pk, sk) = scales(game, &dinos);
    let get = |f: &Fields, k: &str| f.get(k).copied();
    let animals = game
        .animals
        .iter()
        .filter_map(|it| {
            let (_, f) = dinos.iter().find(|(n, _)| same(n, &it.name))?;
            Some(Item {
                name: it.name.clone(),
                price: it.price.and(get(f, "price").map(|p| round_price(p * pk))),
                score: get(f, "score").map(|s| round_score(s * sk)),
                health: get(f, "health").map(|h| h as f32),
                ..Default::default()
            })
        })
        .collect();
    let weapons = game
        .weapons
        .iter()
        .filter_map(|it| {
            let (_, f) = weapons.iter().find(|(n, _)| same(n, &it.name))?;
            let v = |k| get(f, k).map(|v| v as f32);
            Some(Item {
                name: it.name.clone(),
                price: get(f, "price").map(|p| round_price(p * pk)),
                power: v("power"),
                prec: v("prec"),
                rate: v("rate"),
                shots: v("shots"),
                ..Default::default()
            })
        })
        .collect();
    let areas = game
        .areas
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let p = values.get(&format!("AREA{}_PRICE", i + 1))?;
            Some(Item {
                name: it.name.clone(),
                price: Some(round_price(p * pk)),
                ..Default::default()
            })
        })
        .collect();
    crate::balance::Balance {
        start: None,
        areas,
        animals,
        weapons,
    }
}

/// A picture of the model (side view), for what the APK has none of.
fn model_picture(car: &c2_formats::car::Car) -> Option<(usize, usize, Vec<u8>)> {
    let (w, h) = (357, 208);
    c2_formats::portrait::render(&car.mesh, w, h, 0.8).map(|px| (w, h, px))
}

/// What a weapon import needs of the APK and the game.
struct Weapon<'a> {
    zip: &'a Apk,
    res: &'a ResTxt,
    menu: &'a MenuData,
    values: &'a std::collections::BTreeMap<String, f64>,
    price_k: f64,
    apk: &'a Path,
}

impl Weapon<'_> {
    /// A weapon the game doesn't have (the revolver) as an add-on: its
    /// model, the Pistol's entry with the mobile game's stats, a menu
    /// picture drawn from the model for the Pistol's slot (a slot of its
    /// own next to it).
    fn import(&self, cat: &Cat, stem: &str, out: &Path) -> Result<Option<Imported>, ModError> {
        let weapons = self
            .res
            .section("weapons")
            .map(|s| s.blocks.as_slice())
            .unwrap_or_default();
        let named = |n: &str| {
            weapons.iter().find(|b| {
                crate::res_merge::block_id(b).is_some_and(|id| id.eq_ignore_ascii_case(n))
            })
        };
        let Some(donor) = named("Pistol").or(weapons.first()) else {
            return Ok(None);
        };
        let donor_name = donor
            .get(&["name"])
            .map(|n| n.trim().trim_matches('\'').trim().to_string())
            .unwrap_or_default();
        let name = if cat.title.chars().any(|c| c.is_ascii_digit()) || cat.title.is_empty() {
            let mut c = stem.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        } else {
            cat.title.clone()
        };
        let car = c2_formats::mobile::car_from_in(cat, self.zip, "weapons")?;
        let car_file = format!("{}.CAR", stem.to_ascii_uppercase());
        let dir = out.join(&name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(io(&dir))?;
        }
        std::fs::create_dir_all(dir.join("WEAPONS")).map_err(io(&dir))?;
        let write = |file: &str, data: &[u8]| -> Result<(), ModError> {
            let p = dir.join(file);
            std::fs::write(&p, data).map_err(io(&p))
        };
        write(
            &format!("WEAPONS/{car_file}"),
            &c2_formats::car::write(&car),
        )?;
        let mut b = donor.clone();
        set_key(&mut b, "name", format!("'{name}'"));
        set_key(&mut b, "file", format!("'{car_file}'"));
        let mut notes = Vec::new();
        let stats = grouped(self.values, "WEAPON_");
        match stats.iter().find(|(n, _)| same(n, &name) || same(n, stem)) {
            Some((_, f)) => {
                let dialect = c2_formats::mee::is_dialect(self.res);
                for (field, keys) in [
                    ("power", &["land_power", "power"][..]),
                    ("prec", &["land_prec", "prec"][..]),
                    ("rate", &["rate"][..]),
                    ("shots", &["shots"][..]),
                ] {
                    let Some(v) = f.get(field) else { continue };
                    let have: Vec<&str> = keys
                        .iter()
                        .copied()
                        .filter(|k| b.get(&[k]).is_some())
                        .collect();
                    let fallback = if dialect {
                        keys[0]
                    } else {
                        keys[keys.len() - 1]
                    };
                    for k in if have.is_empty() {
                        vec![fallback]
                    } else {
                        have
                    } {
                        set_key(&mut b, k, format!("{v}"));
                    }
                }
                if let Some(p) = f.get("price") {
                    set_key(&mut b, "price", round_price(p * self.price_k).to_string());
                }
            }
            None => notes.push(format!("no mobile values: the {donor_name}'s stats")),
        }
        write(
            &format!("{name} _RES.txt"),
            format!(
                "// {name}, imported from your copy of the mobile game ({}).\r\n\
                 // Modder's Edition weapon entry, from the {donor_name}'s.\r\n\r\n{}",
                self.apk
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                write_block(&b)
            )
            .as_bytes(),
        )?;
        let slot = self
            .menu
            .weapons
            .iter()
            .position(|w| w.name.eq_ignore_ascii_case(&donor_name))
            .map(|i| i + 1);
        match (slot, model_picture(&car)) {
            (Some(n), Some((w, h, px))) => {
                write(&format!("WEAPON{n}.TGA"), &menu_picture(w, h, &px))?;
            }
            _ => notes.push("no menu slot: it isn't offered".into()),
        }
        write(
            "README.txt",
            format!(
                "{name}: imported by c2rs from your own copy of the mobile game.\r\n\
                 For your own use only: don't share these files.\r\n"
            )
            .as_bytes(),
        )?;
        Ok(Some(Imported {
            name,
            dir,
            donor: donor_name,
            notes,
        }))
    }
}

/// Sets `key` in an entry (replacing every value it had).
fn set_key(b: &mut Block, key: &str, value: String) {
    b.entries.retain(|e| !e.key.eq_ignore_ascii_case(key));
    b.entries.push(entry(key, value));
}

/// Imports the animals of the APK at `apk` that the game in `base` (a
/// Modder's Edition game: the entries are in its dialect) doesn't have,
/// one add-on folder each under `out`.
pub fn import(apk: &Path, base: &Vfs, out: &Path) -> Result<Import, ModError> {
    let zip = Apk(c2_vfs::ZipBackend::new(apk, "")?);
    // The game's tuning in the mobile game (none: the donors' stand).
    let values = values(apk).unwrap_or_default();
    let game = crate::balance::read(base)?;
    let dinos = grouped(&values, "DINO_");
    let (pk, sk) = scales(&game, &dinos);
    std::fs::create_dir_all(out).map_err(io(out))?;
    if !values.is_empty() {
        let p = out.join(MOBILE_VALUES);
        std::fs::write(&p, mobile_balance(&values, &game).to_toml()).map_err(io(&p))?;
    }
    if let Ok(Some(sv)) = survive(apk) {
        let p = out.join(MOBILE_SURVIVAL);
        std::fs::write(&p, sv.write()).map_err(io(&p))?;
    }
    let res_text = String::from_utf8_lossy(&base.read("HUNTDAT/_RES.TXT")?).into_owned();
    let res = ResTxt::parse(&res_text)?;
    if !c2_formats::mee::is_dialect(&res) {
        return Err(ModError::Recipe(
            "importing mobile animals needs a Modder's Edition game: add its folder as an overlay"
                .into(),
        ));
    }
    let menu = match base.read("HUNTDAT/_MENU.TXT") {
        Ok(d) => MenuData::from_res(&ResTxt::parse(&String::from_utf8_lossy(&d))?),
        Err(_) => MenuData::from_res(&res),
    };
    let chars = res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default();
    let by_name = |n: &str| {
        chars
            .iter()
            .find(|b| crate::res_merge::block_id(b).is_some_and(|id| id.eq_ignore_ascii_case(n)))
    };
    let mut cats: Vec<String> = zip
        .0
        .files()
        .into_iter()
        .filter(|f| {
            let l = f.to_ascii_lowercase();
            l.starts_with("assets/characters/") && l.ends_with(".cat")
        })
        .collect();
    cats.sort();
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    // What other imports beside this one made (both mobile games have the
    // revolver): not made twice.
    let elsewhere: Vec<String> = out
        .parent()
        .and_then(|p| std::fs::read_dir(p).ok())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p != out)
                .filter_map(|p| std::fs::read_dir(p).ok())
                .flat_map(|rd| {
                    rd.flatten()
                        .map(|e| e.file_name().to_string_lossy().to_lowercase())
                })
                .collect()
        })
        .unwrap_or_default();
    let imported = |name: &str| elsewhere.iter().any(|n| *n == name.to_lowercase());
    for path in cats {
        let Some(text) = zip.get(&path) else { continue };
        let Ok(cat) = parse_cat(&String::from_utf8_lossy(&text)) else {
            continue;
        };
        let stem = cat
            .model
            .trim_end_matches(".3dn")
            .trim_end_matches("_model")
            .to_ascii_lowercase();
        // A weapon (its model in `weapons`): new when the game has no
        // weapon model of that name (the revolver).
        if zip
            .get(&format!("assets/models/weapons/{}", cat.model))
            .is_some()
        {
            let title = if cat.title.chars().any(|c| c.is_ascii_digit()) {
                let mut c = stem.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            } else {
                cat.title.clone()
            };
            if imported(&title) {
                skipped.push(format!(
                    "{title}: already imported from the other mobile game"
                ));
            } else if base
                .resolve(&format!("HUNTDAT/WEAPONS/{stem}.car"))
                .is_none()
            {
                let w = Weapon {
                    zip: &zip,
                    res: &res,
                    menu: &menu,
                    values: &values,
                    price_k: pk,
                    apk,
                };
                done.extend(w.import(&cat, &stem, out)?);
            }
            continue;
        }
        // The game has it: a model of that name, or an animal of that name
        // (the Modder's Edition renames models: `diatr_s.car`). People
        // (the poacher) aren't animals to hunt.
        let named = chars.iter().any(|b| {
            b.get(&["name"]).is_some_and(|n| {
                let n = n.trim().trim_matches('\'');
                same(n, &stem) || same(n, &cat.title)
            })
        });
        let has = base.resolve(&format!("HUNTDAT/{stem}.car")).is_some() || named;
        if !is_animal(&cat) || has || ["poacher", "hunter"].iter().any(|p| stem.starts_with(p)) {
            continue;
        }
        let mut notes = Vec::new();
        let prefs = DONORS
            .iter()
            .find(|(s, _)| *s == stem)
            .map_or(&[][..], |d| d.1);
        let donor = prefs.iter().find_map(|n| by_name(n)).or_else(|| {
            chars.iter().find(|b| {
                b.get(&["ai"])
                    .map(c2_formats::res_txt::c_atoi)
                    .is_some_and(|a| (10..30).contains(&a))
            })
        });
        let Some(donor) = donor else {
            notes.push("no animal in the game to take its kind from: skipped".into());
            continue;
        };
        let donor_name = donor
            .get(&["name"])
            .map(|n| n.trim().trim_matches('\'').trim().to_string())
            .unwrap_or_default();
        // Numbered PC-style animations play as the donor's: only when it
        // has as many (else they'd play the wrong moves).
        if roles(&cat).is_none() {
            let donor_anims = donor
                .get(&["file"])
                .map(|f| f.trim().trim_matches('\'').trim().to_string())
                .and_then(|f| base.read(&format!("HUNTDAT/{f}")).ok())
                .and_then(|d| c2_formats::car::parse(&d).ok())
                .map(|c| c.animations.len());
            if donor_anims != Some(cat.animations.len()) {
                skipped.push(format!(
                    "{}: its {} animations don't match the {donor_name}'s: not imported",
                    cat.title,
                    cat.animations.len()
                ));
                continue;
            }
        }
        let shown = NAMES
            .iter()
            .find(|(s, _)| *s == stem)
            .map_or_else(|| cat.title.clone(), |(_, n)| (*n).to_string());
        if imported(&shown) {
            skipped.push(format!(
                "{shown}: already imported from the other mobile game"
            ));
            continue;
        }
        let name = NAMES
            .iter()
            .find(|(s, _)| *s == stem)
            .map(|(_, n)| (*n).to_string())
            .or_else(|| Some(cat.title.clone()).filter(|t| !t.is_empty()))
            .unwrap_or_else(|| stem.clone());
        let car = car_from(&cat, &zip)?;
        let car_file = format!("{}.car", stem.to_ascii_uppercase());
        let dir = out.join(&name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(io(&dir))?;
        }
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        let write = |file: &str, data: &[u8]| -> Result<(), ModError> {
            let p = dir.join(file);
            std::fs::write(&p, data).map_err(io(&p))
        };
        write(&car_file, &c2_formats::car::write(&car))?;
        // Menu picture and calls for the donor's slot: a slot of its own.
        let slot = menu
            .huntables
            .iter()
            .position(|h| h.name.eq_ignore_ascii_case(&donor_name))
            .map(|i| i + 1);
        let mut b = make_entry(donor, &name, &car_file, &cat, slot.is_some());
        // Its own health, score and price from the mobile game, on the
        // game's scale.
        match dinos.iter().find(|(n, _)| same(n, &name) || same(n, &stem)) {
            Some((_, f)) => {
                if let Some(h) = f.get("health") {
                    set_key(&mut b, "health", format!("{h}"));
                }
                if let Some(s) = f.get("score") {
                    crate::balance::set_score(&mut b, round_score(s * sk));
                }
                if let Some(p) = f.get("price") {
                    set_key(&mut b, "price", round_price(p * pk).to_string());
                }
            }
            None => notes.push(format!("no mobile values: the {donor_name}'s stats")),
        }
        write(
            &format!("{name} _RES.txt"),
            format!(
                "// {name}, imported from your copy of the mobile game ({}).\r\n\
                 // Modder's Edition entry, kind taken from the {donor_name}.\r\n\r\n{}",
                apk.file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                write_block(&b)
            )
            .as_bytes(),
        )?;
        match slot {
            Some(n) => {
                // `Help/<name>.png`, or one named longer (`pach` ->
                // `pachycephalosaurus.png`).
                let help: Vec<String> = zip
                    .0
                    .files()
                    .into_iter()
                    .filter(|f| f.to_ascii_lowercase().starts_with("assets/help/"))
                    .collect();
                let pic = [name.to_ascii_lowercase().replace(' ', ""), stem.clone()]
                    .iter()
                    .find_map(|s| {
                        help.iter().find(|f| {
                            let n = f.to_ascii_lowercase();
                            n.ends_with(&format!("/{s}.png"))
                                || n.starts_with(&format!("assets/help/{s}")) && n.ends_with(".png")
                        })
                    })
                    .and_then(|f| zip.get(f))
                    .and_then(|d| decode_png(&d));
                // None in the APK: one drawn from the model.
                let pic = pic.or_else(|| model_picture(&car));
                match pic {
                    Some((w, h, px)) => write(&format!("DINO{n}.TGA"), &menu_picture(w, h, &px))?,
                    None => notes.push("no menu picture".into()),
                }
                // Its calls: the `callN` set with its call (or growl) in
                // it, else its own `<name>_call.wav` (not always listed).
                let snd = |s: &str| zip.get(&format!("assets/sounds/characters/{s}.wav"));
                let own = snd(&format!("{stem}_call"));
                let mine: Vec<Vec<u8>> = cat
                    .sounds
                    .iter()
                    .filter(|s| matches!(role(s), Some("call" | "growl")))
                    .filter_map(|s| snd(s))
                    .chain(own.clone())
                    .collect();
                let set = call_set(&zip, &mine).or_else(|| own.map(|o| vec![o; 3]));
                match set {
                    Some(set) => {
                        for (l, d) in ['A', 'B', 'C'].iter().zip(set) {
                            let (pcm, rate) = decode_wav(&d)?;
                            write(&format!("CALL{n}_{l}.WAV"), &wav(&pcm, rate))?;
                        }
                    }
                    None => notes.push("no call sound: it can't be called".into()),
                }
            }
            None => notes.push(format!("the {donor_name} has no menu slot: it roams")),
        }
        write(
            "README.txt",
            format!(
                "{name}: imported by c2rs from your own copy of the mobile game.\r\n\
                 For your own use only: don't share these files.\r\n"
            )
            .as_bytes(),
        )?;
        done.push(Imported {
            name,
            dir,
            donor: donor_name,
            notes,
        });
    }
    Ok(Import {
        added: done,
        skipped,
    })
}

/// What an import made, and what it left out (and why).
#[derive(Debug, Clone, Default)]
pub struct Import {
    pub added: Vec<Imported>,
    pub skipped: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Cat {
        let anims = [
            "x_walk",
            "x_run",
            "x_idle_active",
            "x_die",
            "x_sleep",
            "x_eat",
            "x_call",
        ];
        Cat {
            title: "X".into(),
            model: "x.3dn".into(),
            animations: anims.iter().map(|a| (a.to_string(), None)).collect(),
            sounds: vec!["x_call".into()],
        }
    }

    #[test]
    fn entry_takes_the_donors_kind_and_its_own_animations() {
        let res = ResTxt::parse(
            "characters {\n{\n name = 'Velociraptor'\n file = 'velo.car'\n ai = 15\n callNo = 5\n\
             runAnim = 0\n walkAnim = 1\n health = 6\n overwrite char5 {\n  radar = TRUE\n }\n\
             spawninfo{\n  spawngroup = 1\n }\n killtype{\n  hunterAnim = 0\n  eatAnim = 6\n }\n\
             tropinfo{\n  tropAnim = 12\n }\n}\n}\n.\n",
        )
        .unwrap_or_default();
        let donor = &res.sections[0].blocks[0];
        let b = make_entry(donor, "X", "X.CAR", &cat(), false);
        let get = |k: &str| b.get(&[k]).map(str::to_string);
        assert_eq!(get("name").as_deref(), Some("'X'"));
        assert_eq!(get("file").as_deref(), Some("'X.CAR'"));
        assert_eq!(get("ai").as_deref(), Some("15"));
        assert_eq!(get("health").as_deref(), Some("6"));
        assert_eq!(
            (get("walkAnim"), get("runAnim")),
            (Some("0".into()), Some("1".into()))
        );
        assert!(get("callNo").is_none());
        let subs: Vec<&str> = b.subs.iter().map(|s| s.name.trim()).collect();
        assert!(subs.contains(&"spawninfo"));
        assert!(!subs
            .iter()
            .any(|s| s.starts_with("overwrite") || *s == "tropinfo"));
        assert_eq!(subs.iter().filter(|s| **s == "idlegroup").count(), 2);
        let kill = b.subs_named("killtype").next().cloned().unwrap_or_default();
        assert_eq!(kill.get(&["hunterAnim"]), Some("0"));
        assert_eq!(kill.get(&["eatAnim"]), Some("5"));
        let death = b
            .subs_named("deathtype")
            .next()
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            (death.get(&["dieAnim"]), death.get(&["sleepAnim"])),
            (Some("3"), Some("4"))
        );
        // It reads back as an entry.
        let text = format!("characters {{\n{}}}\n.\n", write_block(&b));
        let again = ResTxt::parse(&text).unwrap_or_default();
        assert_eq!(again.sections[0].blocks.len(), 1);
    }

    #[test]
    fn menu_picture_is_menu_sized() {
        let px = vec![255u8; 4 * 4 * 4];
        let t = c2_formats::tga::parse(&menu_picture(4, 4, &px)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((t.width, t.height), (357, 208));
        // White in the middle, olive at the edge.
        assert_eq!(t.pixels[104 * 357 + 178] & 0x7FFF, 0x7FFF);
        assert_ne!(t.pixels[0] & 0x7FFF, 0x7FFF);
    }
}
