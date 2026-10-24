//! Game folders made for the Modder's Edition Engine (README M9): their
//! `_RES.TXT` resolved for a hunt, and their species mapped onto the AIs
//! this engine has. Format side: `c2_formats::mee`.
//!
//! The dialect's AI families are ported (`crate::dino::huntable`, `mee_*`):
//! the shared huntable AI, T-Rex, ambients, flyers (the wary one too),
//! brachiosaurs, fish and Mosasaurus, Ichthyornis. [`family`] runs the rest
//! as the closest original AI: Titan (unfinished upstream) and Micro
//! (switched off upstream); poacher and dog (multiplayer only) are not
//! placed.

use c2_formats::mee::{flag, Hunt, Tables};
use c2_formats::res_txt::{c_atof, c_atoi, Block, CharacterDef, Entry, ResData, ResTxt, WeaponDef};

use crate::dino::mee::{is_ported, DeathType, IdleGroup, KillType, MeeInfo};
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
/// `land_fall` is a bullet's drop, not the original's crossbow flag: it
/// stays for [`crate::game::MeeWeapon`].
fn vanilla_weapon_keys(b: &Block) -> Block {
    let mut b = b.clone();
    for e in &mut b.entries {
        let k = match e.key.to_ascii_lowercase().as_str() {
            "land_power" => "power",
            "land_prec" => "prec",
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

#[cfg(test)]
pub(crate) fn vanilla_keys_for_test(b: &Block) -> Block {
    vanilla_weapon_keys(b)
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

/// Picked animals come on any map (owner's rule; the dialect spawns them
/// only where their tables say, so an animal from another game, or a
/// roaming add-on, picked for this map wouldn't come). A picked character
/// (its `callNo` slot in `h.picked`, or in `extra` by index) not in the
/// area's target group (the spawn group this map's own menu animals use)
/// joins it; its own spawns stay. Ambient animals (AI under 10) keep to
/// their tables.
pub fn spawn_picked_anywhere(res: &ResTxt, h: &Hunt, tables: &mut Tables, extra: &[usize]) {
    let raw = res.section("characters").map_or(&[][..], |s| &s.blocks);
    let slot = |i: usize, b: &Block| c2_formats::mee::menu_slot(raw.get(i), b);
    let all = Tables::read(
        res,
        &Hunt {
            picked: c2_formats::bits::Bits::ALL,
            ..*h
        },
    );
    let mut uses = vec![0usize; all.spawn_groups.len()];
    for (_, c) in all
        .characters
        .iter()
        .enumerate()
        .filter(|(i, c)| slot(*i, &c.block).is_some())
    {
        for l in &c.spawn {
            if let Some(n) = uses.get_mut(l.group) {
                *n += 1;
            }
        }
    }
    let Some(target) = (0..uses.len())
        .filter(|&g| uses[g] > 0)
        .max_by_key(|&g| uses[g])
    else {
        return;
    };
    let packs_in: Vec<bool> = tables
        .pack_groups
        .iter()
        .map(|p| p.spawn.iter().any(|l| l.group == target))
        .collect();
    for (i, c) in tables.characters.iter_mut().enumerate() {
        let ambient = c.block.get(&["ai"]).map(c_atoi).is_some_and(|a| a < 10);
        let by_slot = slot(i, &c.block).is_some_and(|k| h.picked.has(k));
        let picked = by_slot || extra.contains(&i);
        // Already among the targets (alone or in a pack)?
        let in_target = c.spawn.iter().any(|l| l.group == target)
            || c.packs
                .iter()
                .any(|l| packs_in.get(l.group).copied().unwrap_or(false));
        if picked && !ambient && !in_target {
            c.spawn.push(c2_formats::mee::SpawnLink {
                group: target,
                ratio: 1.0,
            });
            // Its "when picked" block may be tied to its own game's maps
            // (a combined game's): still a target on the radar here.
            if by_slot && c.block.get(&["radar"]).is_none() {
                c.block.entries.push(Entry {
                    key: "radar".into(),
                    value: "TRUE".into(),
                });
            }
        }
    }
}

/// [`read`] then [`spawn_picked_anywhere`]; a picked animal whose model is
/// only given per map (a placeholder, `blank.car`, elsewhere) takes the
/// first of its maps' models where it has none.
pub fn read_picked_anywhere(res: &ResTxt, h: &Hunt, extra: &[usize]) -> MeeHunt {
    let mut m = read(res, h);
    spawn_picked_anywhere(res, h, &mut m.tables, extra);
    let raw = res.section("characters").map_or(&[][..], |s| &s.blocks);
    for (i, c) in m.tables.characters.iter().enumerate() {
        let picked = c2_formats::mee::menu_slot(raw.get(i), &c.block)
            .is_some_and(|k| h.picked.has(k))
            || extra.contains(&i);
        let Some(def) = m.res.characters.get_mut(i) else {
            continue;
        };
        let blank = def
            .file
            .as_deref()
            .is_some_and(|f| f.to_ascii_lowercase().ends_with("blank.car"));
        if picked && blank {
            if let Some(f) = raw.get(i).and_then(map_model) {
                def.file = Some(f);
            }
        }
    }
    m
}

/// The first model a raw entry's area blocks give it (not a placeholder).
fn map_model(b: &Block) -> Option<String> {
    b.subs.iter().find_map(|s| {
        let own = s
            .name
            .contains("area")
            .then(|| s.block.get(&["file"]))
            .flatten()
            .map(|f| f.trim().trim_matches('\'').to_string())
            .filter(|f| !f.to_ascii_lowercase().ends_with("blank.car"));
        own.or_else(|| map_model(&s.block))
    })
}

/// Whether a roaming entry is one to offer for picking: named (one game
/// keeps hidden creatures as `?`) and with a model (not its `blank.car`
/// placeholder). The others still roam where their tables put them.
pub fn offered(b: &Block) -> bool {
    let name = b.get(&["name"]).unwrap_or_default();
    let file = b.get(&["file"]).unwrap_or_default().to_ascii_lowercase();
    name.chars().any(char::is_alphabetic) && !file.trim().trim_matches('\'').ends_with("blank.car")
}

/// The roaming add-ons of a dialect game (a huntable AI, 10-29, with no
/// menu slot, [`c2_formats::mee::menu_slot`]: they only roam): their
/// character indices, each with the menu slot it is picked by, from
/// `first_slot` on (after the menu's own).
pub fn roamers(res: &ResTxt, first_slot: usize) -> Vec<(usize, usize)> {
    let raw = res.section("characters").map_or(&[][..], |s| &s.blocks);
    let all = Tables::read(
        res,
        &Hunt {
            area: None,
            time: 1,
            picked: c2_formats::bits::Bits::ALL,
        },
    );
    all.characters
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            let ai = c.block.get(&["ai"]).map(c_atoi);
            let slot = c2_formats::mee::menu_slot(raw.get(*i), &c.block);
            slot.is_none() && ai.is_some_and(|a| (10..30).contains(&a)) && offered(&c.block)
        })
        .map(|(i, _)| i)
        .zip(first_slot..c2_formats::bits::Bits::LEN - 10)
        .collect()
}

/// The picked roamers' characters (`roamers`: (character, slot); their slot
/// in `h.picked`), for [`read_picked_anywhere`]'s `extra`.
pub fn picked_roamers(h: &Hunt, roamers: &[(usize, usize)]) -> Vec<usize> {
    roamers
        .iter()
        .filter(|&&(_, slot)| h.picked.has(slot))
        .map(|&(i, _)| i)
        .collect()
}

/// Picked roamers take their slot as `callNo`, so they hunt as menu
/// animals (after [`read_picked_anywhere`] brought them to this map).
pub fn pick_roamers(h: &Hunt, tables: &mut Tables, roamers: &[(usize, usize)]) {
    for &(i, slot) in roamers.iter().filter(|&&(_, slot)| h.picked.has(slot)) {
        if let Some(c) = tables.characters.get_mut(i) {
            c.block.entries.push(Entry {
                key: "callNo".into(),
                value: slot.to_string(),
            });
        }
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

fn truthy(v: &str) -> bool {
    v.contains("TRUE")
}

/// One `key = value` line of a character (`ReadCharacterLine`). Every
/// `xxxAnim` takes a logical phase, used or not, so the numbering (and the
/// phase 0 that a missing key falls back to) matches the original.
fn info_line(m: &mut MeeInfo, key: &str, v: &str) {
    let k = key.to_ascii_lowercase();
    let anim = c_atoi(v);
    match k.as_str() {
        "runanim" => m.run = m.add_anim(anim),
        "walkanim" => m.walk = m.add_anim(anim),
        "swimanim" => m.swim = m.add_anim(anim),
        "jumpanim" => m.jump = m.add_anim(anim),
        "slideanim" => m.slide = m.add_anim(anim),
        "lookanim" | "fishidleanim" => {
            let p = m.add_anim(anim);
            m.looks.push(p);
        }
        "smellanim" => {
            let p = m.add_anim(anim);
            m.smells.push(p);
        }
        "roaranim" => {
            let p = m.add_anim(anim);
            m.roars.push(p);
        }
        "flyanim" => m.fly = m.add_anim(anim),
        "glideanim" => m.glide = m.add_anim(anim),
        "waterdanim" => {
            let p = m.add_anim(anim);
            m.water_dies.push(p);
        }
        "takeoffanim" => m.takeoff = m.add_anim(anim),
        "landanim" => m.land = m.add_anim(anim),
        "shakelanim" => m.shake_land = m.add_anim(anim),
        "shakewanim" => m.shake_water = m.add_anim(anim),
        "tkfspd" => m.takeoff_speed = c_atof(v),
        "lndspd" => m.land_speed = c_atof(v),
        "diveanim" | "climbanim" | "fireanim" | "pauseanim" | "reloadanim" => {
            m.add_anim(anim);
        }
        "flyspd" => m.fly_speed = c_atof(v),
        "gldspd" => m.glide_speed = c_atof(v),
        "mindepth" | "minalt" => m.min_depth = c_atoi(v),
        "maxdepth" | "maxalt" => m.max_depth = c_atoi(v),
        "runspd" => m.run_speed = c_atof(v),
        "wlkspd" => m.walk_speed = c_atof(v),
        "swmspd" => m.swim_speed = c_atof(v),
        "jmpspd" => m.jump_speed = c_atof(v),
        "killdist" => m.kill_dist = c_atoi(v),
        "aggress" => m.aggress = c_atoi(v),
        "defensive" => m.defensive = truthy(v),
        "fearshothear" => m.fear_hear_shot = truthy(v),
        "fearshothit" => m.fear_shot = truthy(v),
        "weaverange" => m.weave_range = c_atof(v),
        "alarmcall" => m.alarm_call = usize::try_from(c_atoi(v)).ok(),
        "dontweave" => m.dont_weave = truthy(v),
        "canswim" => m.can_swim = truthy(v),
        "waterlevel" => m.water_level = c_atoi(v),
        "morphtime" => m.morph_time = c_atof(v),
        "hpa" => m.health_a = c_atoi(v),
        "trexobjcollide" => m.trex_obj_collide = truthy(v),
        "dash" => m.dash = truthy(v),
        "collisiondist" => m.collision_dist = c_atoi(v),
        "runrotatespeed" => m.rot_speed_multi = c_atof(v),
        "dangerfish" => m.danger_fish = truthy(v),
        "dontswimaway" => m.dont_swim_away = truthy(v),
        "jumprange" => m.jump_range = c_atoi(v),
        "spcdepth" => m.spacing_depth = c_atoi(v),
        "callno" => m.call_no = c_atoi(v),
        "fearcall" => {
            let n = c_atoi(v);
            if !m.fear_calls.contains(&n) {
                m.fear_calls.push(n);
            }
        }
        "dontfear" => {
            let n = c_atoi(v);
            m.fear_calls.retain(|&f| f != n);
        }
        _ => {}
    }
}

/// A nested block of a character (`ReadKillTypeInfo`, `ReadDeathTypeInfo`,
/// `ReadIdleGroupInfo`, `ReadIdle2GroupInfo`).
fn info_sub(m: &mut MeeInfo, name: &str, b: &Block) {
    let name = name.to_ascii_lowercase();
    let lines = || {
        b.entries
            .iter()
            .map(|e| (e.key.to_ascii_lowercase(), &e.value))
    };
    if name.contains("killtype") {
        let mut k = KillType::default();
        for (key, v) in lines() {
            match key.as_str() {
                "hunteranim" => k.hunter_anim = c_atoi(v),
                "hunteroffset" => k.offset = c_atoi(v) as f32,
                "eatanim" => k.anim = m.add_anim(c_atoi(v)),
                "carrycorpse" => k.carry_corpse = truthy(v),
                "dontloop" => k.dont_loop = truthy(v),
                "scream" => k.scream = truthy(v),
                _ => {}
            }
        }
        m.kill_types.push(k);
    } else if name.contains("deathtype") {
        let mut d = DeathType::default();
        for (key, v) in lines() {
            match key.as_str() {
                "dieanim" => d.die = m.add_anim(c_atoi(v)),
                "sleepanim" => d.sleep = m.add_anim(c_atoi(v)),
                "fallanim" => d.fall = m.add_anim(c_atoi(v)),
                "nosleep" => d.no_sleep = truthy(v),
                _ => {}
            }
        }
        m.death_types.push(d);
    } else if name.contains("idlegroup") || name.contains("waterigroup") {
        let mut g = IdleGroup::default();
        for (key, v) in lines() {
            match key.as_str() {
                "startchance" => g.start = c_atof(v),
                "endchance" => g.end = c_atof(v),
                "randomstarting" => g.start_on_any = truthy(v),
                "randomending" => g.end_on_any = truthy(v),
                "instantrepeat" => g.instant_repeat = truthy(v),
                "anim" => g.anims.push(m.add_anim(c_atoi(v))),
                _ => {}
            }
        }
        if name.contains("idlegroup") {
            m.idle_groups.push(g);
        } else {
            m.idle2_groups.push(g);
        }
    }
}

/// A resolved character block as the dialect's species data.
pub fn info(b: &Block, mee_ai: i32) -> MeeInfo {
    let mut m = MeeInfo::new(mee_ai);
    let mut subs = b.subs.iter().peekable();
    for (i, e) in b.entries.iter().enumerate() {
        while let Some(s) = subs.next_if(|s| s.at <= i) {
            info_sub(&mut m, &s.name, &s.block);
        }
        info_line(&mut m, &e.key, &e.value);
    }
    for s in subs {
        info_sub(&mut m, &s.name, &s.block);
    }
    m
}

/// Gives loaded species their dialect data. AIs with the dialect's own
/// behaviour ported ([`is_ported`]) run it; the rest run as their
/// [`family`] with [`Tuning`] overrides (AI -1, never placed, when there is
/// none).
pub fn tune(species: &mut [Species], tables: &Tables) {
    for (s, c) in species.iter_mut().zip(&tables.characters) {
        s.tuning = tuning(&c.block);
        // The dialect reads the score as a decimal (`basescore = 7.5`).
        if let Some(v) = c.block.get(&["basescore"]) {
            s.info.base_score = c_atof(v);
        }
        let mee_ai = s.tuning.mee_ai.unwrap_or(s.info.ai);
        if is_ported(mee_ai) {
            s.info.ai = mee_ai;
            s.mee = Some(Box::new(info(&c.block, mee_ai)));
        } else {
            s.info.ai = family(mee_ai).unwrap_or(-1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RES: &str = "characters {\n{\n name = 'Wolf'\n ai = 22\n runAnim = 3\n walkAnim = 1\n\
 runspd = 1.5\n killdist = 300\n fearShotHit = TRUE\n\
 killtype {\n  hunterAnim = 3\n  hunterOffset = 150\n  eatAnim = 7\n  scream = FALSE\n }\n\
 deathtype {\n  dieAnim = 5\n  sleepAnim = 6\n }\n\
 idlegroup {\n  startChance = 0.5\n  anim = 2\n  anim = 4\n }\n\
 jumpAnim = 8\n}\n}\n.\n";

    #[test]
    fn an_animal_picked_by_its_overwrite_comes_on_another_games_map() {
        // As one game writes its menu animals (no call, its "when picked"
        // block and its model tied to its own map), here on map 2.
        let res = ResTxt::parse(
            "spawntable {\n spawngroup {\n  spawnmax = 4\n }\n}\n\
             characters {\n{\n name = 'Hunter'\n ai = 0\n}\n\
             {\n name = 'Local'\n ai = 12\n callNo = 1\n spawninfo {\n  spawngroup = 0\n  spawnratio = 1\n }\n}\n\
             {\n name = 'Visitor'\n file = 'blank.car'\n ai = 14\n callNo = -5 1\n\
             overwrite char0 area1 {\n  radar = TRUE\n  spawninfo {\n   spawngroup = 0\n   spawnratio = 1\n  }\n }\n\
             overwrite area1 {\n  file = 'own/visitor.car'\n }\n}\n}\n.\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let h = Hunt {
            area: Some(2),
            time: 1,
            picked: c2_formats::bits::Bits::bit(0),
        };
        let m = read_picked_anywhere(&res, &h, &[]);
        let v = &m.tables.characters[2];
        assert_eq!(v.spawn.len(), 1, "joins this map's targets");
        assert_eq!(v.block.get(&["radar"]), Some("TRUE"));
        assert_eq!(m.res.characters[2].file.as_deref(), Some("own/visitor.car"));
        // Not picked: stays away, as its game has it.
        let off = Hunt {
            picked: c2_formats::bits::Bits::bit(1),
            ..h
        };
        assert!(read_picked_anywhere(&res, &off, &[]).tables.characters[2]
            .spawn
            .is_empty());
        assert!(roamers(&res, 2).is_empty(), "a menu animal, not a roamer");
    }

    #[test]
    fn roaming_add_ons_get_the_slots_after_the_menu_and_hunt_by_them() {
        let res = ResTxt::parse(
            "characters {\n{\n name = 'Hunter'\n ai = 0\n}\n\
             {\n name = 'Raptor'\n ai = 12\n callNo = 0\n}\n\
             {\n name = 'Pig'\n ai = 1\n}\n\
             {\n name = 'Roamer'\n ai = 14\n}\n}\n.\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let roamers = roamers(&res, 1);
        assert_eq!(roamers, [(3, 1)]);
        let h = Hunt {
            area: Some(1),
            time: 1,
            picked: c2_formats::bits::Bits::bit(1),
        };
        let mut t = Tables::read(&res, &h);
        pick_roamers(&h, &mut t, &roamers);
        assert_eq!(t.characters[3].block.get(&["callNo"]), Some("1"));
        let unpicked = Hunt {
            picked: c2_formats::bits::Bits::bit(0),
            ..h
        };
        let mut t = Tables::read(&res, &unpicked);
        pick_roamers(&unpicked, &mut t, &roamers);
        assert_eq!(t.characters[3].block.get(&["callNo"]), None);
    }

    #[test]
    fn info_numbers_phases_in_reading_order() {
        let res = ResTxt::parse(RES).unwrap_or_else(|e| panic!("{e}"));
        let b = &res
            .section("characters")
            .map(|s| &s.blocks[0])
            .cloned()
            .unwrap_or_default();
        let m = info(b, 22);
        // runAnim, walkAnim, then the blocks in place, then jumpAnim.
        assert_eq!(m.anim_index, vec![3, 1, 7, 5, 6, 2, 4, 8]);
        assert_eq!((m.run, m.walk, m.jump, m.swim), (0, 1, 7, 0));
        assert_eq!(m.kill_types.len(), 1);
        let k = m.kill_types[0];
        assert_eq!(
            (k.hunter_anim, k.offset, k.anim, k.scream),
            (3, 150.0, 2, false)
        );
        assert_eq!((m.death(0).die, m.death(0).sleep), (3, 4));
        assert_eq!(m.idle_groups[0].anims, vec![5, 6]);
        assert_eq!(m.idle_groups[0].start, 0.5);
        assert_eq!((m.run_speed, m.kill_dist, m.fear_shot), (1.5, 300, true));
        assert_eq!(m.morph_time, 1.0);
    }
}
