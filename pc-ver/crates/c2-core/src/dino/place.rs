//! `PlaceCharacters`: populating an area at the start of a hunt.

use super::{ai, check_place_collision_p, Character, Herd};
use crate::area::{Area, TimeOfDay};
use crate::rng::MsRand;
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
}

impl Herd {
    fn by_ai(&self, ai_id: i32) -> Option<usize> {
        self.species.iter().position(|s| s.info.ai == ai_id)
    }

    fn spawn(&mut self, ctype: usize, pos: [f32; 3], rng: &mut MsRand, opt_agres: i32) {
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
            self.spawn(ctype, pos, rng, o.opt_agres);
            let last = self.chars.len() - 1;
            if matches!(self.chars[last].ai, ai::DIMET | ai::PTERA) {
                self.chars[last].pos[1] += 2048.0;
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
