//! Carnivores: Ice Age as a base game (README M14). Its engine is
//! Carnivores 2's with its own animals' AIs (`ai` 10..19 in its `_RES.TXT`
//! mean other animals than in C2's); the Modder's Edition re-implemented
//! those AIs under its own numbers (`c2_core::dino::mee`), which we port.
//! So an Ice Age folder plays as a dialect game: its `_RES.TXT` is rewritten
//! (a layer over the player's own files) with each animal's AI, animation
//! indices into Ice Age's own models, speeds, idles, deaths and kills.
//!
//! The per-AI values follow the Modder's Edition's own Ice Age rules
//! (`_RES ICE AGE.TXT`, which runs Ice Age in it), written out here as
//! ours; the animals' own numbers (health, mass, scores, senses) stay the
//! player's file's. Differences from that file: Ice Age's own models (its
//! Diatryma without the added swim animation), no trophy-room spots (Ice
//! Age's room places trophies as the original), and no halved score for an
//! animal that isn't a target (as every dialect game here).

use c2_formats::res_txt::{Block, ResTxt};
use c2_vfs::{MemBackend, Vfs};

use crate::ModError;

/// One Ice Age AI: its Modder's Edition AI, and the dialect lines for it.
struct Rule {
    ia: i32,
    ai: i32,
    lines: &'static str,
}

const CAM: &str = "CamYLand = 256\nCamYWater = 256\nCamBaseLand = 824\nCamBaseWater = 824\n";

const RULES: &[Rule] = &[
    // Ambients: the pig, the archaeopteryx.
    Rule {
        ia: 1,
        ai: 6,
        lines: "hearK = 0.5\nfearCall = 2\nfearCall = 4\nfearCall = 6\nfearCall = 8\n\
                runspd = 0.6\nwlkspd = 0.2\nrunAnim = 1\nwalkAnim = 0\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.0625\nendChance = 0.25\n}\n\
                dontWeave = TRUE\nwaterLevel = 60\n",
    },
    Rule {
        ia: 2,
        ai: 3,
        lines: "maxalt = 2800\nminalt = 1800\nwaterLevel = 40\nflyspd = 1.5\ngldspd = 1.3\n\
                flyAnim = 0\nglideAnim = 1\ndeathtype{\nfallAnim = 2\ndieAnim = 3\n}\n",
    },
    // Brontotherium.
    Rule {
        ia: 10,
        ai: 20,
        lines: "fearCall = 2\nfearCall = 6\nfearCall = 8\nrunspd = 1.536\nwlkspd = 0.336\n\
                aggress = 16\nkilldist = 300\nrunAnim = 1\nwalkAnim = 0\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                killtype{\nhunterAnim = 3\neatAnim = 7\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 160\nweaveRange = 3072\n",
    },
    // Wild boar.
    Rule {
        ia: 11,
        ai: 21,
        lines: "fearCall = 2\nfearCall = 4\nfearCall = 6\nfearCall = 8\nrunspd = 1.152\n\
                wlkspd = 0.224\naggress = 20\nkilldist = 300\nrunAnim = 1\nwalkAnim = 0\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                killtype{\nhunterAnim = 4\neatAnim = 7\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 100\nweaveRange = 3072\n",
    },
    // Wolf.
    Rule {
        ia: 12,
        ai: 22,
        lines: "fearCall = 6\nfearCall = 8\nrunspd = 2.048\njmpspd = 2.048\nwlkspd = 0.32\n\
                aggress = 140\nkilldist = 300\nrunAnim = 1\nwalkAnim = 0\njumpAnim = 7\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.0625\nendChance = 0.5\n\
                randomStarting = TRUE\nrandomEnding = TRUE\n}\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                killtype{\nhunterAnim = 5\neatAnim = 8\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 120\nweaveRange = 3072\n",
    },
    // Woolly rhinoceros.
    Rule {
        ia: 13,
        ai: 23,
        lines: "fearCall = 2\nfearCall = 6\nfearCall = 8\nrunspd = 1.536\nwlkspd = 0.32\n\
                aggress = 18\nkilldist = 300\nrunAnim = 1\nwalkAnim = 0\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                killtype{\nhunterAnim = 6\neatAnim = 7\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 100\nweaveRange = 3072\n",
    },
    // Diatryma: the dialect runs it as a spinosaur (jump, slide).
    Rule {
        ia: 14,
        ai: 16,
        lines: "fearCall = 2\nfearCall = 6\nfearCall = 8\nrunspd = 1.28\nwlkspd = 0.448\n\
                jmpspd = 1.2\naggress = 140\nkilldist = 300\nrunAnim = 1\nwalkAnim = 0\n\
                slideAnim = 5\njumpAnim = 4\n\
                deathtype{\ndieAnim = 6\nsleepAnim = 7\n}\n\
                idlegroup{\nanim = 2\nstartChance = 0.0703125\nendChance = 1\ninstantRepeat = TRUE\n}\n\
                idlegroup{\nanim = 3\nstartChance = 0.0703125\nendChance = 1\ninstantRepeat = TRUE\n}\n\
                killtype{\nhunterAnim = 7\nhunterOffset = 200\neatAnim = 8\nscream = TRUE\n}\n\
                waterLevel = 140\nweaveRange = 1648\n",
    },
    // Giant deer.
    Rule {
        ia: 15,
        ai: 24,
        lines: "fearCall = 2\nfearCall = 4\nfearCall = 6\nfearCall = 8\nrunspd = 1.536\n\
                wlkspd = 0.32\naggress = 20\nkilldist = 300\nrunAnim = 1\nwalkAnim = 0\n\
                deathtype{\ndieAnim = 4\nsleepAnim = 5\n}\n\
                idlegroup{\nanim = 2\nanim = 3\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                killtype{\nhunterAnim = 8\neatAnim = 6\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 230\nweaveRange = 0\n",
    },
    // Smilodon.
    Rule {
        ia: 16,
        ai: 25,
        lines: "fearCall = 8\nrunspd = 2.048\nwlkspd = 0.64\njmpspd = 2.048\naggress = 40\n\
                killdist = 300\nrunAnim = 1\nwalkAnim = 0\njumpAnim = 7\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.0625\nendChance = 1\n\
                randomStarting = TRUE\nrandomEnding = TRUE\n}\n\
                killtype{\nhunterAnim = 9\neatAnim = 8\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 160\nweaveRange = 3072\n",
    },
    // Mammoth.
    Rule {
        ia: 17,
        ai: 26,
        lines: "fearCall = 8\nrunspd = 1.28\nwlkspd = 0.64\naggress = 20\nkilldist = 300\n\
                runAnim = 1\nwalkAnim = 0\n\
                deathtype{\ndieAnim = 4\nsleepAnim = 5\n}\n\
                idlegroup{\nanim = 2\nanim = 3\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                killtype{\nhunterAnim = 10\neatAnim = 6\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 340\nweaveRange = 0\n",
    },
    // Bear.
    Rule {
        ia: 18,
        ai: 27,
        lines: "runspd = 1.792\nwlkspd = 0.48\nkilldist = 300\naggress = 20\n\
                runAnim = 1\nwalkAnim = 0\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                killtype{\nhunterAnim = 11\neatAnim = 7\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 140\nweaveRange = 0\n",
    },
    // Yeti: the dialect runs it as a bear.
    Rule {
        ia: 19,
        ai: 27,
        lines: "runspd = 2.688\nwlkspd = 0.896\nkilldist = 300\naggress = 35\n\
                runAnim = 1\nwalkAnim = 0\n\
                idlegroup{\nanim = 2\nanim = 3\nanim = 4\nstartChance = 0.03125\nendChance = 0.5\n}\n\
                deathtype{\ndieAnim = 5\nsleepAnim = 6\n}\n\
                killtype{\nhunterAnim = 12\neatAnim = 7\nhunterSync = TRUE\nscream = TRUE\n}\n\
                waterLevel = 180\nweaveRange = 0\n",
    },
];

/// The spawn groups: 0 ambients, 1 the roster at random, 2 the picked
/// animals; and every area snowing.
const TABLES: &str = "spawntable {\n\
spawngroup{\nspawnmax = 6\nspawnmin = 0\nspawnrate = 1\ndensityMulti = 2\nmoveForward = TRUE\n}\n\
spawngroup{\nspawnmax = 4\nspawnmin = 0\nspawnrate = 1\nrandomise = TRUE\n}\n\
spawngroup{\nspawnmax = 13\nspawnmin = 12\nspawnrate = 0.5\ndensityMulti = 4\n}\n\
}\n\n";

const SNOW_AREA: &str = "{\nmapShadowDir = 1\nmapShadowMax = 20\nsnow{\nhSpd = 4\nvSpd = 192\n\
dens = 2000\nred = 240\ngre = 240\nblu = 240\nalp = 255\nrad = 1\n}\n}\n";

/// Keys of Ice Age's character blocks and their dialect names.
fn key(k: &str) -> &str {
    match k.to_ascii_lowercase().as_str() {
        "smell" => "smellK",
        "hear" => "hearK",
        "look" => "lookK",
        _ => k,
    }
}

/// Whether `res` is Carnivores: Ice Age's own (original-format) rules.
pub fn is_ice_age(res: &ResTxt) -> bool {
    !c2_formats::mee::is_dialect(res)
        && res.section("characters").is_some_and(|s| {
            s.blocks.iter().any(|b| {
                b.get(&["file"])
                    .is_some_and(|f| f.trim_matches('\'').eq_ignore_ascii_case("mamm.car"))
            })
        })
}

fn character(b: &Block, slot: &mut usize) -> String {
    let ia = b.get(&["ai"]).map_or(0, c2_formats::res_txt::c_atoi);
    let rule = RULES.iter().find(|r| r.ia == ia);
    let mut t = String::from("{\n");
    for e in &b.entries {
        let k = e.key.as_str();
        match k.to_ascii_lowercase().as_str() {
            "ai" => t += &format!("ai = {}\n", rule.map_or(ia, |r| r.ai)),
            // `danger` is the menu's word; the dialect's AIs know.
            "danger" => {}
            _ => t += &format!("{} = {}\n", key(k), e.value),
        }
    }
    if let Some(r) = rule {
        if ia >= 10 {
            // A menu animal: its call and slot; picked, it shows on the
            // radar and comes in numbers (group 2), else one of the roster.
            t += &format!(
                "callNo = {slot}\noverwrite char{slot} {{\nradar = TRUE\nradR = 0\nradG = 240\nradB = 0\n\
                 spawninfo{{\nspawngroup = 2\nspawnratio = 1\n}}\n}}\n\
                 spawninfo{{\nspawngroup = 1\nspawnratio = 1\n}}\ndogSmell = TRUE\n"
            );
            if let Some(r) = b.get(&["radius"]) {
                t += &format!("collectRad = {r}\n");
            }
            t += CAM;
            *slot += 1;
        } else {
            t += &format!(
                "spawninfo{{\nspawngroup = 0\nspawnratio = {}\n}}\n",
                if ia == 1 { 2 } else { 1 }
            );
        }
        t += r.lines;
        t += "bloR = 112\nbloG = 0\nbloB = 0\n";
    }
    t + "}\n"
}

/// Ice Age's `_RES.TXT` in the dialect (its weapons and prices as they
/// are).
pub fn to_dialect(res: &ResTxt) -> String {
    let mut t =
        String::from("// Carnivores: Ice Age, as c2rs plays it (written from your copy).\n\n");
    t += "areatable {\n";
    for _ in 0..5 {
        t += SNOW_AREA;
    }
    t += "}\n\n";
    t += TABLES;
    let mut slot = 0;
    for s in &res.sections {
        if s.name.eq_ignore_ascii_case("characters") {
            t += "characters {\n";
            for b in &s.blocks {
                t += &character(b, &mut slot);
                t += "\n";
            }
            t += "}\n\n";
        } else {
            let one = ResTxt {
                sections: vec![s.clone()],
                warnings: Vec::new(),
            };
            // Without the end mark (`.`): one goes at the very end.
            let w = one.write();
            let w = w.trim_end();
            t += w.strip_suffix('.').unwrap_or(w);
            t += "\n";
        }
    }
    t + ".\n"
}

/// Whether the game folder `dir` is Carnivores: Ice Age.
pub fn is_ice_age_dir(dir: &std::path::Path) -> bool {
    crate::vanilla(&[dir.to_path_buf()]).is_ok_and(|v| v.layers().iter().any(|l| l.name == LAYER))
}

/// The name of the layer [`layer`] makes (in [`crate::vanilla`]).
pub const LAYER: &str = "ice age rules";

/// A layer that makes an Ice Age game folder a dialect game (`None` for
/// any other game).
pub fn layer(vfs: &Vfs) -> Result<Option<MemBackend>, ModError> {
    let Ok(data) = vfs.read("HUNTDAT/_RES.TXT") else {
        return Ok(None);
    };
    let res = ResTxt::parse(&String::from_utf8_lossy(&data))?;
    if !is_ice_age(&res) {
        return Ok(None);
    }
    let mut m = MemBackend::new();
    m.insert("HUNTDAT/_RES.TXT", to_dialect(&res).into_bytes());
    Ok(Some(m))
}

/// What [`import`] made: add-on folders, and what it left out.
#[derive(Debug, Clone, Default)]
pub struct Import {
    pub added: Vec<std::path::PathBuf>,
    pub skipped: Vec<String>,
}

fn io(path: &std::path::Path) -> impl Fn(std::io::Error) -> ModError + '_ {
    move |source| ModError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The `prices` section's `key` lines (`area`, `dino`), in order.
fn prices_of(res: &ResTxt, key: &str) -> Vec<i32> {
    res.section("prices")
        .map(|p| {
            p.entries
                .iter()
                .filter(|e| e.key.eq_ignore_ascii_case(key))
                .map(|e| c2_formats::res_txt::c_atoi(&e.value))
                .collect()
        })
        .unwrap_or_default()
}

/// A block's `key = value` lines (for a small sub-block written by hand).
fn entries_text(b: &Block) -> String {
    b.entries
        .iter()
        .map(|e| format!(" {} = {}\r\n", e.key, e.value))
        .collect()
}

/// The first non-empty line of a menu text.
fn first_line(d: &[u8]) -> Option<String> {
    String::from_utf8_lossy(d)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// The combined game (README M14): Carnivores: Ice Age's animals and areas
/// as add-ons for the Carnivores 2 game in `base` (a Modder's Edition game:
/// the entries are in its dialect), from the player's Ice Age folder `ia`,
/// under `out`:
///
/// - one folder per animal: its model, its entry (made for its Ice Age menu
///   slot, so it gets a slot of its own beside C2's), menu picture and
///   texts, call icon and sounds, and Ice Age's hunter model (its kill
///   animations, merged into the game's hunter);
/// - "Ice Age areas": its maps as `AREAn` after the base game's last, with
///   `.c2map` descriptors (name, price, picture, text, and `snow = TRUE`,
///   ours: they snow).
pub fn import(ia: &std::path::Path, base: &Vfs, out: &std::path::Path) -> Result<Import, ModError> {
    let game = ResTxt::parse(&String::from_utf8_lossy(&base.read("HUNTDAT/_RES.TXT")?))?;
    if !c2_formats::mee::is_dialect(&game) {
        return Err(ModError::Recipe(
            "Ice Age's animals beside C2's need a Modder's Edition game: add its folder as an overlay"
                .into(),
        ));
    }
    let v = crate::vanilla(&[ia.to_path_buf()])?;
    if !v.layers().iter().any(|l| l.name == LAYER) {
        return Err(ModError::Recipe(format!(
            "{} isn't Carnivores: Ice Age",
            ia.display()
        )));
    }
    let res = ResTxt::parse(&String::from_utf8_lossy(&v.read("HUNTDAT/_RES.TXT")?))?;
    let mut done = Import::default();
    let write = |p: std::path::PathBuf, d: &[u8]| -> Result<(), ModError> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(io(dir))?;
        }
        std::fs::write(&p, d).map_err(io(&p))
    };
    let hunter = v.read("HUNTDAT/HUNTER1.CAR").ok();
    // The base game's menu slots: each entry is made for one of them (Ice
    // Age's slot, wrapped round), so each is added in a new slot of its
    // own beside that slot's animal (`crate::loose::SlotAdd`), not left in
    // a slot past the base's that another add-on takes.
    let menu = match base.read("HUNTDAT/_MENU.TXT") {
        Ok(d) => {
            c2_formats::menu_txt::MenuData::from_res(&ResTxt::parse(&String::from_utf8_lossy(&d))?)
        }
        Err(_) => c2_formats::menu_txt::MenuData::from_res(&game),
    };
    let base_slots = menu.huntables.len().max(1) as i32;
    let dino_prices = prices_of(&res, "dino");
    let note = "imported from your copy of Carnivores: Ice Age";
    // Animals: the menu ones (`callNo` = their Ice Age slot).
    for b in res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default()
    {
        let (Some(name), Some(file)) = (b.get(&["name"]), b.get(&["file"])) else {
            continue;
        };
        let Some(slot) = b.get(&["callNo"]).map(c2_formats::res_txt::c_atoi) else {
            continue;
        };
        let (name, file) = (
            name.trim_matches('\'').to_string(),
            file.trim_matches('\'').to_string(),
        );
        let Ok(car) = v.read(&format!("HUNTDAT/{file}")) else {
            done.skipped.push(format!("{name}: no {file}"));
            continue;
        };
        let dir = out.join(&name);
        let t = slot % base_slots;
        let n = t + 1;
        write(dir.join(&file), &car)?;
        let mut b = b.clone();
        // Ice Age's price for it (else the slot it is drawn for gives its own).
        if let Some(p) = dino_prices.get(slot as usize) {
            b.entries.push(c2_formats::res_txt::Entry {
                key: "price".into(),
                value: p.to_string(),
            });
        }
        for e in b
            .entries
            .iter_mut()
            .filter(|e| e.key.eq_ignore_ascii_case("callNo"))
        {
            e.value = t.to_string();
        }
        for sub in b.subs.iter_mut() {
            if sub
                .name
                .eq_ignore_ascii_case(&format!("overwrite char{slot}"))
            {
                sub.name = format!("overwrite char{t}");
            }
        }
        let entry = format!(
            "// {name}, {note}.\r\n// Modder's Edition entry (menu slot {n}; Ice Age's {}).\r\n\r\n{}",
            slot + 1,
            c2_formats::res_txt::write_block(&b)
        );
        let ia_n = slot + 1;
        write(dir.join(format!("{name} _RES.txt")), entry.as_bytes())?;
        let copy = |from: &str, to: &str| -> Result<(), ModError> {
            if let Ok(d) = v.read(from) {
                write(dir.join(to), &d)?;
            }
            Ok(())
        };
        copy(
            &format!("HUNTDAT/MENU/PICS/DINO{ia_n}.TGA"),
            &format!("DINO{n}.TGA"),
        )?;
        copy(
            &format!("HUNTDAT/MENU/TXT/DINO{ia_n}.TXM"),
            &format!("DINO{n}.TXM"),
        )?;
        copy(
            &format!("HUNTDAT/MENU/TXT/DINO{ia_n}.TXU"),
            &format!("DINO{n}.TXU"),
        )?;
        copy(
            &format!("HUNTDAT/MENU/PICS/CALL{ia_n}.TGA"),
            &format!("CALL{n}.TGA"),
        )?;
        for l in ['A', 'B', 'C'] {
            copy(
                &format!("HUNTDAT/SOUNDFX/CALLS/CALL{ia_n}_{l}.WAV"),
                &format!("CALL{n}_{l}.WAV"),
            )?;
        }
        if let Some(h) = &hunter {
            write(dir.join("hunter_ice_age.car"), h)?;
        }
        write(
            dir.join("README.txt"),
            format!("{name}, {note} (c2rs). Keep it local: it is game data.\r\n").as_bytes(),
        )?;
        done.added.push(dir);
    }
    // Ambients (the pig, the archaeopteryx): only on Ice Age's areas (they
    // spawn there through `addition areaN`; C2's maps keep their own).
    let first = (1..=99)
        .rev()
        .find(|n| {
            base.resolve(&format!("HUNTDAT/AREAS/AREA{n}.MAP"))
                .is_some()
        })
        .unwrap_or(0)
        + 1;
    let ia_areas = (1..=99u32)
        .take_while(|i| v.resolve(&format!("HUNTDAT/AREAS/AREA{i}.MAP")).is_some())
        .count() as u32;
    let header: Vec<String> = (first..first + ia_areas)
        .map(|n| format!("area{n}"))
        .collect();
    for b in res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default()
    {
        let ai = b.get(&["ai"]).map_or(0, c2_formats::res_txt::c_atoi);
        let (Some(name), Some(file)) = (b.get(&["name"]), b.get(&["file"])) else {
            continue;
        };
        if !(1..10).contains(&ai) || header.is_empty() {
            continue;
        }
        let (name, file) = (
            name.trim_matches('\'').to_string(),
            file.trim_matches('\'').to_string(),
        );
        let Ok(car) = v.read(&format!("HUNTDAT/{file}")) else {
            done.skipped.push(format!("{name}: no {file}"));
            continue;
        };
        let dir = out.join(&name);
        write(dir.join(&file), &car)?;
        let mut b = b.clone();
        let spawn: Vec<String> = b
            .subs
            .iter()
            .filter(|s| s.name.eq_ignore_ascii_case("spawninfo"))
            .map(|s| format!("spawninfo{{\r\n{}}}\r\n", entries_text(&s.block)))
            .collect();
        b.subs.retain(|s| !s.name.eq_ignore_ascii_case("spawninfo"));
        let mut entry = c2_formats::res_txt::write_block(&b);
        let close = entry.rfind('}').unwrap_or(entry.len());
        entry.insert_str(
            close,
            &format!(
                "addition {} {{\r\n{}}}\r\n",
                header.join(" "),
                spawn.concat()
            ),
        );
        write(
            dir.join(format!("{name} _RES.txt")),
            format!("// {name}, {note}: on Ice Age's areas only.\r\n\r\n{entry}").as_bytes(),
        )?;
        write(
            dir.join("README.txt"),
            format!("{name}, {note} (c2rs). Keep it local: it is game data.\r\n").as_bytes(),
        )?;
        done.added.push(dir);
    }
    // Areas: after the base game's last `AREAn`.
    let last = (1..=99)
        .rev()
        .find(|n| {
            base.resolve(&format!("HUNTDAT/AREAS/AREA{n}.MAP"))
                .is_some()
        })
        .unwrap_or(0);
    let prices = prices_of(&res, "area");
    let dir = out.join("Ice Age areas");
    let mut areas = 0;
    for i in 1..=99u32 {
        let (Ok(map), Ok(rsc)) = (
            v.read(&format!("HUNTDAT/AREAS/AREA{i}.MAP")),
            v.read(&format!("HUNTDAT/AREAS/AREA{i}.RSC")),
        ) else {
            break;
        };
        let n = last + i;
        write(dir.join(format!("AREAS/AREA{n}.MAP")), &map)?;
        write(dir.join(format!("AREAS/AREA{n}.RSC")), &rsc)?;
        let text = v.read(&format!("HUNTDAT/MENU/TXT/AREA{i}.TXT")).ok();
        if let Some(t) = &text {
            write(dir.join(format!("MENU/TXT/AREA{n}.TXT")), t)?;
        }
        if let Ok(p) = v.read(&format!("HUNTDAT/MENU/PICS/AREA{i}.TGA")) {
            write(dir.join(format!("MENU/PICS/AREA{n}.TGA")), &p)?;
        }
        let name = text
            .as_deref()
            .and_then(first_line)
            .unwrap_or_else(|| format!("Ice Age area {i}"));
        let price = prices.get(i as usize - 1).copied().unwrap_or(0);
        let c2map = format!(
            "info\r\n{{\r\n name    = '{name}'\r\n mapfile = 'huntdat\\areas\\area{n}'\r\n \
             rscfile = 'huntdat\\areas\\area{n}.rsc'\r\n pic     = 'huntdat\\menu\\pics\\area{n}.tga'\r\n \
             text    = 'huntdat\\menu\\txt\\area{n}.txt'\r\n price   = {price}\r\n snow    = TRUE\r\n}}\r\n.\r\n"
        );
        write(dir.join(format!("AREAS/AREA{n}.c2map")), c2map.as_bytes())?;
        areas += 1;
    }
    if areas > 0 {
        write(
            dir.join("README.txt"),
            format!("Ice Age's areas, {note} (c2rs). Keep it local: it is game data.\r\n")
                .as_bytes(),
        )?;
        done.added.push(dir);
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IA: &str = "weapons {\n{\n name = 'Pistol'\n file = 'pistol.car'\n shots = 8\n}\n}\n\
        characters {\n{\n name = 'Hunter'\n file = 'hunter1.car'\n ai = 0\n}\n\
        {\n name = 'Pig'\n file = 'pig1.car'\n ai = 1\n health = 2\n}\n\
        {\n name = 'Wolf'\n file = 'wolf.car'\n ai = 12\n health = 7\n smell = 0.85\n radius = 418\n danger = TRUE\n}\n\
        {\n name = 'Mammoth'\n file = 'mamm.car'\n ai = 17\n health = 16\n}\n}\n\
        prices {\n start = 100\n dino = 10\n dino = 15\n}\n";

    #[test]
    fn ice_age_rules_become_the_dialect() {
        let res = ResTxt::parse(IA).unwrap();
        assert!(is_ice_age(&res));
        let d = ResTxt::parse(&to_dialect(&res)).unwrap();
        assert!(c2_formats::mee::is_dialect(&d) && !is_ice_age(&d));
        let chars = &d.section("characters").unwrap().blocks;
        let wolf = &chars[2];
        assert_eq!(wolf.get(&["ai"]), Some("22"));
        assert_eq!(wolf.get(&["callNo"]), Some("0"));
        assert_eq!(wolf.get(&["smellK"]), Some("0.85"));
        assert_eq!(wolf.get(&["jumpAnim"]), Some("7"));
        assert_eq!(wolf.get(&["collectRad"]), Some("418"));
        assert!(wolf.get(&["danger"]).is_none());
        assert_eq!(chars[3].get(&["callNo"]), Some("1"));
        assert_eq!(chars[1].get(&["ai"]), Some("6"));
        assert_eq!(chars[0].get(&["ai"]), Some("0"));
        // Weapons and prices as they were.
        assert!(d.section("weapons").is_some() && d.section("prices").is_some());
        let menu = c2_formats::menu_txt::MenuData::from_res(&d);
        assert_eq!(menu.huntables.len(), 2);
        assert_eq!(menu.huntables[1].price, 15);
        // Every area snows; the trophy room doesn't.
        let h = |area| c2_formats::mee::Hunt {
            area,
            time: 1,
            picked: c2_formats::bits::Bits::NONE,
        };
        let t = c2_formats::mee::Tables::read(&d, &h(Some(3)));
        assert_eq!(t.snow.map(|s| (s.dens, s.v_spd)), Some((2000, 192)));
        assert!(c2_formats::mee::Tables::read(&d, &h(None)).snow.is_none());
    }
}
