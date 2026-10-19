//! Game folders made for the Modder's Edition Engine (README M9): their
//! `_RES.TXT` resolved for a hunt, and their species mapped onto the AIs
//! this engine has. Format side: `c2_formats::mee`.
//!
//! **Interim:** the dialect has AI families the original doesn't (Ice Age
//! animals, Pachycephalosaurus, Titan, Micro, fish, Mosasaurus, ...). Until
//! each is implemented, [`family`] runs it as the closest original AI, and
//! families with nothing close (aquatic, poacher, dog) are not placed.

use c2_formats::mee::{flag, Hunt, Tables};
use c2_formats::res_txt::{c_atof, c_atoi, Block, CharacterDef, Entry, ResData, ResTxt, WeaponDef};

use crate::dino::{ai, Species, Tuning};

/// The original AI a dialect AI number runs as for now, `None` if none fits
/// (`Core/Constants.h` lists the dialect's numbers).
pub fn family(mee_ai: i32) -> Option<i32> {
    Some(match mee_ai {
        0..=5 => mee_ai,
        // PIG: an ambient grazer, like Moschops.
        6 => ai::MOSH,
        10..=18 => mee_ai,
        // PACH, HOG, DEER: herbivores that flee.
        19 | 21 | 24 => ai::PARA,
        // BRONT, MAMM, LANDBRACH: big herbivores.
        20 | 26 | 35 => ai::STEGO,
        // RHINO.
        23 => ai::ANKY,
        // WOLF, MICRO: small pack hunters.
        22 | 29 => ai::VELO,
        // SMILO, BEAR.
        25 | 27 => ai::ALLO,
        // TITAN.
        28 => ai::CERAT,
        // BRACH, BRACHDANGER.
        30 | 34 => ai::BRACH,
        // ICTH: a sea bird.
        31 => ai::PTERA,
        // FISH, MOSA, POACHER, HUNTDOG and anything else.
        _ => return None,
    })
}

/// A hunt's data from a dialect `_RES.TXT`.
pub struct MeeHunt {
    /// Characters and weapons as the engine would load them for this hunt.
    pub res: ResData,
    pub tables: Tables,
}

/// The dialect renamed some weapon keys (manual, "Note: As of 1.1.0").
fn vanilla_weapon_keys(b: &Block) -> Block {
    let mut b = b.clone();
    for e in &mut b.entries {
        let k = match e.key.to_ascii_lowercase().as_str() {
            "land_power" => "power",
            "land_prec" => "prec",
            "land_fall" => "fall",
            "pic1" => "pic",
            _ => continue,
        };
        *e = Entry {
            key: k.into(),
            value: e.value.clone(),
        };
    }
    b
}

/// `_RES.TXT` resolved for hunt `h`.
pub fn read(res: &ResTxt, h: &Hunt) -> MeeHunt {
    let tables = Tables::read(res, h);
    let weapons = res
        .section("weapons")
        .map(|s| {
            s.blocks
                .iter()
                .map(|b| {
                    WeaponDef::from_block(&vanilla_weapon_keys(&c2_formats::mee::resolve(b, h)))
                })
                .collect()
        })
        .unwrap_or_default();
    let characters = tables
        .characters
        .iter()
        .map(|c| CharacterDef::from_block(&c.block))
        .collect();
    MeeHunt {
        res: ResData {
            weapons,
            characters,
            prices: None,
        },
        tables,
    }
}

fn index(b: &Block, key: &str) -> Option<usize> {
    b.get(&[key])
        .map(c_atoi)
        .filter(|&v| v >= 0)
        .map(|v| v as usize)
}

/// A character's overrides (`ScriptParser.cpp: ReadCharacterLine`).
pub fn tuning(b: &Block) -> Tuning {
    let speed = |k: &str| b.get(&[k]).map(c_atof);
    let death = b.subs_named("deathtype").next();
    let mee_ai = b.get(&["ai"]).map(c_atoi);
    let lo = b.get(&["mindepth"]).map(c_atoi);
    let hi = b.get(&["maxdepth"]).map(c_atoi);
    Tuning {
        run: index(b, "runAnim"),
        walk: index(b, "walkAnim"),
        swim: index(b, "swimAnim"),
        jump: index(b, "jumpAnim"),
        slide: index(b, "slideAnim"),
        die: death.and_then(|d| index(d, "dieAnim")),
        sleep: death.and_then(|d| index(d, "sleepAnim")),
        idles: b
            .subs_named("idlegroup")
            .next()
            .map(|g| {
                g.all("anim")
                    .map(c_atoi)
                    .filter(|&a| a >= 0)
                    .map(|a| a as usize)
                    .collect()
            })
            .unwrap_or_default(),
        run_speed: speed("runspd"),
        walk_speed: speed("wlkspd"),
        swim_speed: speed("swmspd"),
        jump_speed: speed("jmpspd"),
        radar: flag(b, "radar"),
        mee_ai,
        depth: lo.zip(hi),
    }
}

/// Gives loaded species their dialect overrides and runs each as its
/// [`family`] (AI -1, never placed, when there is none).
pub fn tune(species: &mut [Species], tables: &Tables) {
    for (s, c) in species.iter_mut().zip(&tables.characters) {
        s.tuning = tuning(&c.block);
        let mee_ai = s.tuning.mee_ai.unwrap_or(s.info.ai);
        s.info.ai = family(mee_ai).unwrap_or(-1);
    }
}
