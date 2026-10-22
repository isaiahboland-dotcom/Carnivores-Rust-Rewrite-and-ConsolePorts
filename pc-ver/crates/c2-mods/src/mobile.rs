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
/// belongs to the donor alone (its slot conditions, trophy poses, menu
/// slot) dropped.
fn make_entry(donor: &Block, name: &str, car: &str, cat: &Cat) -> Block {
    let Some(roles) = roles(cat) else {
        let mut b = donor.clone();
        b.entries.retain(|e| {
            let k = e.key.to_ascii_lowercase();
            !["name", "file", "callno", "survivalindex"].contains(&k.as_str())
        });
        b.subs.retain(|s| {
            let n = s.name.trim().to_ascii_lowercase();
            !(n == "tropinfo"
                || (n.starts_with("overwrite") || n.starts_with("addition")) && n.contains("char"))
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
            || (n.starts_with("overwrite") || n.starts_with("addition")) && n.contains("char"))
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
        ("jump", "jumpAnim"),
        ("growl", "roarAnim"),
    ] {
        if let Some(i) = find(r) {
            b.entries.push(entry(key, i));
        }
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
    for v in [16u32] {
        w.extend_from_slice(&v.to_le_bytes());
    }
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

/// Imports the animals of the APK at `apk` that the game in `base` (a
/// Modder's Edition game: the entries are in its dialect) doesn't have,
/// one add-on folder each under `out`.
pub fn import(apk: &Path, base: &Vfs, out: &Path) -> Result<Vec<Imported>, ModError> {
    let zip = Apk(c2_vfs::ZipBackend::new(apk, "")?);
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
        let has = base.resolve(&format!("HUNTDAT/{stem}.car")).is_some();
        if !is_animal(&cat) || has {
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
        let b = make_entry(donor, &name, &car_file, &cat);
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
        // Menu picture and calls for the donor's slot: a slot of its own.
        let slot = menu
            .huntables
            .iter()
            .position(|h| h.name.eq_ignore_ascii_case(&donor_name))
            .map(|i| i + 1);
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
                match pic {
                    Some((w, h, px)) => write(&format!("DINO{n}.TGA"), &menu_picture(w, h, &px))?,
                    None => notes.push("no menu picture in the APK".into()),
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
    Ok(done)
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
        let b = make_entry(donor, "X", "X.CAR", &cat());
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
