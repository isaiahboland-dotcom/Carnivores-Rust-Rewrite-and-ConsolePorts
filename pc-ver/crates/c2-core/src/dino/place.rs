//! `PlaceCharacters`: populating an area at the start of a hunt.

use super::{ai, check_place_collision_aquatic, check_place_collision_p, Character, Herd};
use crate::area::{Area, TimeOfDay};
use crate::rng::MsRand;
use c2_formats::mee::{SpawnGroup, SpawnLink, Tables};
use c2_formats::sav::Save;

/// Hunt options that affect placement.
#[derive(Debug, Clone, Copy)]
pub struct PlaceOptions {
    /// `OptDens` 0..=255 (128 default).
    pub density: i32,
    pub opt_agres: i32,
    /// Bit `ai` set for each target species chosen in the menu
    /// (`TargetDino`); 0 = none chosen.
    pub targets: u32,
    /// At most this many small ambient animals (`None`: the original's
    /// 5 + density / 80; the 3DS caps it, or turns them off with 0).
    pub max_ambient: Option<i32>,
}

/// Characters a Modder's Edition hunt may hold (`kCharacterCapacity`).
const MEE_MAX_CHARS: usize = 256;

impl Herd {
    fn by_ai(&self, ai_id: i32) -> Option<usize> {
        self.species.iter().position(|s| s.info.ai == ai_id)
    }

    /// Adds a character, unless its species has no model (a cooked pack
    /// with only some species, or a `.CAR` that failed to load).
    fn spawn(&mut self, ctype: usize, pos: [f32; 3], rng: &mut MsRand, opt_agres: i32) -> bool {
        // No model, or (dialect data) an AI family not implemented yet.
        if self
            .species
            .get(ctype)
            .is_some_and(|s| s.ani_time.is_empty() || s.info.ai < 0)
        {
            return false;
        }
        let mut c = Character {
            ctype,
            pos,
            tgx: pos[0],
            tgz: pos[2],
            tgtime: 0,
            ..Default::default()
        };
        if let Some(sp) = self.species.get(ctype) {
            c.reset(sp, rng, opt_agres);
        }
        self.chars.push(c);
        true
    }

    /// Places ambient animals near the hunter, brachiosaurs in shallow water
    /// and the main huntable population around the map centre.
    pub fn place(&mut self, area: &Area, hunter: (f32, f32), rng: &mut MsRand, o: PlaceOptions) {
        self.chars.clear();
        let (px, pz) = hunter;

        // Small ambient animals ("lohs"): CType 1..=5 in turn.
        let mut mc = 5 + o.density / 80;
        if area.time == TimeOfDay::Night {
            mc /= 2;
        }
        if let Some(max) = o.max_ambient {
            mc = mc.min(max.max(0));
        }
        let mut tr = 0;
        'lohs: for c in 0..mc {
            let ctype = 1 + (c % 5) as usize;
            let pos = loop {
                let mut p = [
                    px + rng.si_rand(10040) as f32,
                    0.0,
                    pz + rng.si_rand(10040) as f32,
                ];
                p[1] = area.land_height(p[0], p[2]);
                tr += 1;
                if tr > 10240 {
                    break 'lohs;
                }
                if !check_place_collision_p(area, &mut p) {
                    break p;
                }
            };
            if self.spawn(ctype, pos, rng, o.opt_agres) {
                if let Some(last) = self.chars.last_mut() {
                    if matches!(last.ai, ai::DIMET | ai::PTERA) {
                        last.pos[1] += 2048.0;
                    }
                }
            }
        }

        // Brachiosaurs, wading in water 220..380 deep.
        if let Some(brach) = self.by_ai(ai::BRACH) {
            let mut tr = 0;
            'brach: for _ in 0..3 {
                let pos = loop {
                    let x = 512.0 * 256.0 + rng.si_rand(50 * 256) as f32 * 10.0;
                    let z = 512.0 * 256.0 + rng.si_rand(50 * 256) as f32 * 10.0;
                    let y = area.land_height(x, z);
                    let wy = area.land_up_h(x, z) - y;
                    tr += 1;
                    if tr > 10240 {
                        break 'brach;
                    }
                    if (220.0..=380.0).contains(&wy) {
                        break [x, y, z];
                    }
                };
                self.spawn(brach, pos, rng, o.opt_agres);
            }
        }

        // Main population: the first 4 (or all, with no targets chosen) are
        // random among the 7 species from AI 10 on; the rest cycle the targets.
        let targets: Vec<i32> = (10..20).filter(|a| o.targets & (1 << a) != 0).collect();
        let first_main = self.by_ai(10);
        let mc = 8 + o.density / 30 + rng.r_rand(6);
        let mut tr = 0;
        'main: for c in 0..mc {
            let ctype = if c < 4 || targets.is_empty() {
                first_main.map(|f| f + rng.r_rand(6) as usize)
            } else {
                self.by_ai(targets[c as usize % targets.len()])
            };
            let pos = loop {
                let mut p = [
                    512.0 * 256.0 + (rng.si_rand(50 * 256) * 10) as f32,
                    0.0,
                    512.0 * 256.0 + (rng.si_rand(50 * 256) * 10) as f32,
                ];
                p[1] = area.land_height(p[0], p[2]);
                tr += 1;
                if tr > 10240 {
                    break 'main;
                }
                if (p[0] - px).abs() + (p[2] - pz).abs() < 256.0 * 40.0 {
                    continue;
                }
                if !check_place_collision_p(area, &mut p) {
                    break p;
                }
            };
            if let Some(ct) = ctype.filter(|&t| t < self.species.len()) {
                self.spawn(ct, pos, rng, o.opt_agres);
            }
        }
    }

    /// `PlaceCharacters` of the Modder's Edition Engine
    /// (`Game/CharacterSpawn.cpp` in the modernized fork): every pack type
    /// (the pack table's, plus one per character with its own spawn info)
    /// is linked into the spawn groups it names; each group spawns
    /// `min..=max` (with `rate`, and the density option) leaders, picked
    /// by ratio, in turn or at random, then each pack's followers at the
    /// leader's spot. Species whose model didn't load, or whose AI family
    /// isn't implemented yet, are skipped.
    pub fn place_mee(
        &mut self,
        area: &Area,
        hunter: (f32, f32),
        rng: &mut MsRand,
        t: &Tables,
        density: i32,
    ) {
        self.chars.clear();
        struct Pack {
            members: Vec<(usize, f32)>,
            min: i32,
            max: i32,
            spawn: Vec<SpawnLink>,
        }
        let mut packs: Vec<Pack> = t
            .pack_groups
            .iter()
            .map(|g| Pack {
                members: Vec::new(),
                min: g.min,
                max: g.max,
                spawn: g.spawn.clone(),
            })
            .collect();
        for (ct, c) in t.characters.iter().enumerate() {
            for l in &c.packs {
                if let Some(p) = packs.get_mut(l.group) {
                    p.members.push((ct, l.ratio));
                }
            }
        }
        for (ct, c) in t.characters.iter().enumerate() {
            if !c.spawn.is_empty() {
                packs.push(Pack {
                    members: vec![(ct, 1.0)],
                    min: 1,
                    max: 1,
                    spawn: c.spawn.clone(),
                });
            }
        }
        // Spawn group -> (pack, its spawn link) in link order.
        let mut links: Vec<Vec<(usize, usize)>> = vec![Vec::new(); t.spawn_groups.len()];
        for (p, pack) in packs.iter().enumerate() {
            if pack.members.is_empty() {
                continue;
            }
            for (si, l) in pack.spawn.iter().enumerate() {
                if let Some(v) = links.get_mut(l.group) {
                    v.push((p, si));
                }
            }
        }
        let mut tr = 0;
        for (sg, group) in t.spawn_groups.iter().enumerate() {
            let link = &links[sg];
            if link.is_empty() {
                continue;
            }
            let mut n = group.min;
            for _ in 0..group.max - group.min {
                if group.rate * 30000.0 > rng.r_rand(30000) as f32 {
                    n += 1;
                }
            }
            if group.density_multi != 0 {
                n += ((density - 128) as f32 / 128.0 * group.density_multi as f32) as i32;
                n = n.max(0);
            }
            let ratio = |l: &(usize, usize)| packs[l.0].spawn[l.1].ratio.max(0.0);
            let total: f32 = link.iter().map(ratio).sum();
            let mut scores = vec![0.0f32; link.len()];
            let mut posi = 0usize;
            for _ in 0..n {
                // `kCharacterCapacity`: the engine halts beyond it; stop.
                if self.chars.len() >= MEE_MAX_CHARS {
                    return;
                }
                let pick = if total <= 0.0 {
                    link[0].0
                } else if group.randomise {
                    let mut sel = rng.r_rand(30000) as f32 / 30000.0 * total;
                    let mut last = 0;
                    let mut found = None;
                    for (i, l) in link.iter().enumerate() {
                        let r = ratio(l);
                        if r <= 0.0 {
                            continue;
                        }
                        last = i;
                        if sel <= r {
                            found = Some(l.0);
                            break;
                        }
                        sel -= r;
                    }
                    found.unwrap_or(link[last].0)
                } else {
                    loop {
                        let post = posi % link.len();
                        if scores[post] >= 1.0 {
                            scores[post] -= 1.0;
                            break link[post].0;
                        }
                        scores[post] += ratio(&link[post]);
                        posi += 1;
                    }
                };
                let pack = &packs[pick];
                let leader = pack.members[0].0;
                let Some(pos) = self.mee_spot(area, hunter, rng, &mut tr, group, leader, None)
                else {
                    break;
                };
                self.spawn(leader, pos, rng, 128);
                if !group.move_forward {
                    let mut k = 1;
                    if pack.max > 1 {
                        k = pack.min;
                        for _ in 0..pack.max - pack.min {
                            if rng.r_rand(2) == 1 {
                                k += 1;
                            }
                        }
                    }
                    let total_m: f32 = pack.members.iter().map(|m| m.1.max(0.0)).sum();
                    for _ in 1..k {
                        if self.chars.len() >= MEE_MAX_CHARS {
                            return;
                        }
                        let mut sel = rng.r_rand(30000) as f32 / 30000.0 * total_m;
                        let mut ct = pack.members[0].0;
                        for &(c, r) in &pack.members {
                            let r = r.max(0.0);
                            if r <= 0.0 {
                                continue;
                            }
                            ct = c;
                            if sel <= r {
                                break;
                            }
                            sel -= r;
                        }
                        let Some(p) =
                            self.mee_spot(area, hunter, rng, &mut tr, group, ct, Some(pos))
                        else {
                            break;
                        };
                        self.spawn(ct, p, rng, 128);
                    }
                }
                if tr > 10500 {
                    break;
                }
            }
        }
    }

    /// `spawnMapAmbient`'s search for a spot for character type `ct`:
    /// around the hunter (`moveForward` groups), at a pack leader's spot, or
    /// in one of the group's regions; at least 40 cells from the hunter,
    /// outside avoided regions, in water of the right depth for waders and
    /// fish, clear of obstacles. `None` after too many tries.
    #[allow(clippy::too_many_arguments)]
    fn mee_spot(
        &self,
        area: &Area,
        hunter: (f32, f32),
        rng: &mut MsRand,
        tr: &mut i32,
        g: &SpawnGroup,
        ct: usize,
        leader: Option<[f32; 3]>,
    ) -> Option<[f32; 3]> {
        let (px, pz) = hunter;
        let t = self.species.get(ct).map(|s| &s.tuning);
        let mee_ai = t.and_then(|t| t.mee_ai).unwrap_or(-1);
        // BRACH, BRACHDANGER, ICTH wade; FISH, MOSA swim.
        let wader = matches!(mee_ai, 30 | 31 | 34);
        let swimmer = matches!(mee_ai, 32 | 33);
        // (xmin, xmax, ymin, ymax) in cells; strictly inside.
        let inside = |x: f32, z: f32, r: &(i32, i32, i32, i32)| {
            x > r.0 as f32 * 256.0
                && x < r.1 as f32 * 256.0
                && z > r.2 as f32 * 256.0
                && z < r.3 as f32 * 256.0
        };
        let rect = |r: &c2_formats::mee::Region| (r.xmin, r.xmax, r.ymin, r.ymax);
        loop {
            let mut p = [0.0f32; 3];
            if g.move_forward && *tr < 1024 {
                p[0] = px + rng.si_rand(10040) as f32;
                p[2] = pz + rng.si_rand(10040) as f32;
                if !g.regions.iter().any(|r| inside(p[0], p[2], &rect(r))) {
                    *tr += 1;
                    continue;
                }
            } else if let Some(l) = leader {
                p[0] = l[0];
                p[2] = l[2];
            } else {
                let r = g.regions[rng.r_rand(g.regions.len() as i32 - 1) as usize];
                p[0] = (r.xmin * 256 + (rng.r_rand(r.xmax - r.xmin) * 256).abs()) as f32;
                p[2] = (r.ymin * 256 + (rng.r_rand(r.ymax - r.ymin) * 256).abs()) as f32;
            }
            p[1] = area.land_height(p[0], p[2]);
            let wy = area.land_up_h(p[0], p[2]) - p[1];
            *tr += 1;
            if *tr > 10240 {
                return None;
            }
            let far = (p[0] - px).abs() + (p[2] - pz).abs() >= 256.0 * 40.0;
            if !far {
                continue;
            }
            if wader && !(220.0..=380.0).contains(&wy) {
                continue;
            }
            if swimmer {
                let (lo, hi) = t.and_then(|t| t.depth).unwrap_or((0, 0));
                if wy < lo as f32 || wy > hi as f32 {
                    continue;
                }
            }
            if g.avoid.iter().any(|r| inside(p[0], p[2], &rect(r))) {
                continue;
            }
            let blocked = if wader || swimmer {
                check_place_collision_aquatic(area, &mut p)
            } else {
                check_place_collision_p(area, &mut p)
            };
            if !blocked {
                return Some(p);
            }
        }
    }

    /// `PlaceTrophy`: each saved trophy stands on its slot of the trophy
    /// room's landing ring (`Area::set_trophy_mode`), facing inwards.
    /// `state` holds the slot.
    pub fn place_trophies(&mut self, area: &Area, room: &Save, rng: &mut MsRand) {
        use std::f32::consts::PI;
        self.chars.clear();
        for (slot, b) in room.bodies.iter().enumerate().take(24) {
            let ctype = b.ctype as usize;
            if b.ctype <= 0 || ctype >= self.species.len() {
                continue;
            }
            let Some(&(cx, cz)) = area.landings.get(slot) else {
                continue;
            };
            let alpha = match slot {
                0..6 => PI / 2.0,
                6..12 => PI,
                12..18 => PI * 3.0 / 2.0,
                _ => 0.0,
            };
            let (x, z) = (f32::from(cx) * 256.0 + 128.0, f32::from(cz) * 256.0 + 128.0);
            let mut c = Character {
                ctype,
                alpha,
                pos: [x, area.land_height(x, z), z],
                ..Default::default()
            };
            c.reset(&self.species[ctype], rng, 128);
            c.state = slot as i32;
            c.scale = b.scale;
            self.chars.push(c);
        }
    }
}
