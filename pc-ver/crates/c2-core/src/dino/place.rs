//! `PlaceCharacters`: populating an area at the start of a hunt.

use super::{
    ai, check_place_collision_aquatic, check_place_collision_p, in_region, Character, Herd, Pack,
};
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
    pub targets: c2_formats::bits::Bits,
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

    /// [`Self::spawn`] for a dialect hunt: the character keeps to spawn
    /// group `group`, packs at `density`. Its index, if added.
    fn spawn_in(
        &mut self,
        area: &Area,
        ctype: usize,
        pos: [f32; 3],
        rng: &mut MsRand,
        group: usize,
        density: f32,
    ) -> Option<usize> {
        if !self.spawn(ctype, pos, rng, 128) {
            return None;
        }
        let i = self.chars.len() - 1;
        let fish = self.species[ctype]
            .mee
            .as_ref()
            .is_some_and(|m| super::mee::is_fish(m.ai));
        let c = &mut self.chars[i];
        c.spawn_group = Some(group);
        c.pack_density = density;
        let wader = self.species[ctype].mee.as_ref().is_some_and(|m| {
            use super::mee::mee_ai::*;
            matches!(m.ai, BRACH | BRACHDANGER | LANDBRACH | ICTH)
        });
        if wader {
            c.spawn_alt = area.land_up_h(c.pos[0], c.pos[2]);
        }
        if fish {
            // `spawnMapAmbient`; the clamp to the water happens at once.
            let (x, z) = (c.tgx, c.tgz);
            c.depth =
                area.land_height(x, z) + (area.land_up_h(x, z) - area.land_height(x, z) / 2.0);
            c.tdepth = c.depth;
        }
        Some(i)
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
        let targets: Vec<i32> = (10..20).filter(|&a| o.targets.has(a as usize)).collect();
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
        self.packs.clear();
        self.groups = t.spawn_groups.clone();
        struct PackType {
            members: Vec<(usize, f32)>,
            min: i32,
            max: i32,
            density: f32,
            spawn: Vec<SpawnLink>,
        }
        let mut packs: Vec<PackType> = t
            .pack_groups
            .iter()
            .map(|g| PackType {
                members: Vec::new(),
                min: g.min,
                max: g.max,
                density: g.density,
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
                packs.push(PackType {
                    members: vec![(ct, 1.0)],
                    min: 1,
                    max: 1,
                    density: 0.0,
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
        // Packs whose leader isn't loaded (the 3DS loads only the picked
        // animals) can't spawn; leaving them out gives their share to the
        // others. With everything loaded, as on PC, nothing changes.
        let loaded = |ct: usize| {
            self.species
                .get(ct)
                .is_some_and(|s| !s.ani_time.is_empty() && s.info.ai >= 0)
        };
        for l in &mut links {
            l.retain(|&(p, _)| packs[p].members.first().is_some_and(|m| loaded(m.0)));
        }
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
                let Some(li) = self.spawn_in(area, leader, pos, rng, sg, pack.density) else {
                    continue;
                };
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
                    let pid = self.packs.len();
                    if k > 1 {
                        self.packs.push(Pack {
                            leader: li,
                            ..Default::default()
                        });
                        self.chars[li].pack = Some(pid);
                    }
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
                        if let Some(m) = self.spawn_in(area, ct, p, rng, sg, pack.density) {
                            self.chars[m].pack = Some(pid);
                        }
                    }
                }
                if tr > 10500 {
                    break;
                }
            }
        }
    }

    /// The dialect's `Characters_AddSecondaryOne`: a killed animal of a
    /// `moveForward` spawn group is replaced by a new one of its kind,
    /// 40-80 cells from the hunter, inside the group's regions.
    pub fn add_secondary_in(
        &mut self,
        area: &Area,
        index: usize,
        hunter: (f32, f32),
        rng: &mut MsRand,
        opt_agres: i32,
    ) {
        let Some(src) = self.chars.get(index) else {
            return;
        };
        let (ctype, sg) = (src.ctype, src.spawn_group);
        let Some(g) = sg
            .and_then(|g| self.groups.get(g))
            .filter(|g| g.move_forward)
        else {
            return;
        };
        if self.chars.len() > 250 {
            return;
        }
        let (px, pz) = hunter;
        for _ in 0..128 {
            let mut p = [
                px + rng.si_rand(20040) as f32,
                0.0,
                pz + rng.si_rand(20040) as f32,
            ];
            p[1] = area.land_height(p[0], p[2]);
            if !g.regions.iter().any(|r| in_region(r, p[0], p[2]))
                || g.avoid.iter().any(|r| in_region(r, p[0], p[2]))
                || check_place_collision_p(area, &mut p)
                || (p[0] - px).abs() + (p[2] - pz).abs() < 256.0 * 40.0
            {
                continue;
            }
            let mut c = Character {
                ctype,
                pos: p,
                tgx: p[0],
                tgz: p[2],
                spawn_group: sg,
                ..Default::default()
            };
            if let Some(sp) = self.species.get(ctype) {
                c.reset(sp, rng, opt_agres);
            }
            self.chars.push(c);
            return;
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
        if !self.trophy_spots.is_empty() {
            self.place_trophy_spots(area, room, rng);
            return;
        }
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

    /// The trophy room of a dialect game (`CharacterSpawn.cpp`, placing
    /// trophies): trophy n at spot n's landing point plus its offsets (more
    /// for a bigger than usual animal), facing and pose.
    fn place_trophy_spots(&mut self, area: &Area, room: &Save, rng: &mut MsRand) {
        let deg = |d: f32| d * std::f32::consts::PI / 180.0;
        // Each trophy on its own spot when it is made for that animal; the
        // others (saved before, or by the original's stands) on the first
        // free spot that is. The save keeps its slots.
        let fits = |t: usize, ctype: usize| {
            self.trophy_spots
                .get(t)
                .is_some_and(|s| s.chars.contains(&ctype))
        };
        let mut used: Vec<bool> = (0..self.trophy_spots.len())
            .map(|t| {
                room.bodies
                    .get(t)
                    .is_some_and(|b| b.ctype > 0 && fits(t, b.ctype as usize))
            })
            .collect();
        let mut at = vec![None; room.bodies.len()];
        for (slot, b) in room.bodies.iter().enumerate() {
            if b.ctype <= 0 {
                continue;
            }
            let ctype = b.ctype as usize;
            at[slot] = if fits(slot, ctype) {
                Some(slot)
            } else if (0..used.len()).any(|t| fits(t, ctype)) {
                let free = (0..used.len()).find(|&t| !used[t] && fits(t, ctype));
                if let Some(t) = free {
                    used[t] = true;
                }
                free
            } else {
                // No spot made for it (an add-on without `tropinfo`): its
                // own slot's spot (where a kill put it).
                (slot < used.len()).then_some(slot)
            };
        }
        for (slot, b) in room.bodies.iter().enumerate() {
            let ctype = b.ctype as usize;
            let Some(spot) = at[slot].and_then(|t| self.trophy_spots.get(t)).cloned() else {
                continue;
            };
            let Some(&(lx, lz)) = area.landings.get(spot.pos) else {
                continue;
            };
            let Some(sp) = self.species.get(ctype) else {
                continue;
            };
            // How much bigger than usual, in `scaleA`s.
            let (s0, sa) = (
                sp.info.scale0 as f32 / 1000.0,
                sp.info.scale_a as f32 / 1000.0,
            );
            let k = if sa != 0.0 { (b.scale - s0) / sa } else { 0.0 };
            let x =
                f32::from(lx) * 256.0 + 128.0 + spot.offset[0] as f32 + k * spot.scale[0] as f32;
            let z =
                f32::from(lz) * 256.0 + 128.0 + spot.offset[2] as f32 + k * spot.scale[2] as f32;
            let y = area.land_height(x, z) + spot.offset[1] as f32 + k * spot.scale[1] as f32;
            let mut c = Character {
                ctype,
                pos: [x, y, z],
                ..Default::default()
            };
            c.reset(sp, rng, 128);
            c.pos = [x, y, z];
            c.alpha = deg(spot.alpha);
            c.beta = deg(spot.beta);
            c.gamma = deg(spot.gamma);
            c.state = slot as i32;
            c.scale = b.scale;
            // The pose: a model animation, shown by a phase of its own
            // (only for the animals the spot is made for).
            let own = spot.chars.contains(&ctype);
            if let (Some(a), Some(m), true) = (spot.anim, self.species[ctype].mee.as_mut(), own) {
                c.phase = m
                    .anim_index
                    .iter()
                    .position(|&x| x == a)
                    .unwrap_or_else(|| m.add_anim(a as i32));
                c.prev_phase = c.phase;
                c.ftime = 0;
            }
            self.chars.push(c);
        }
    }
}
