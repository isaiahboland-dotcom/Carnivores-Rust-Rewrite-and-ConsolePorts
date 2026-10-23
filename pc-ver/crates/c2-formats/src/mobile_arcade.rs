//! The mobile games' Survive mode ("arcade" in their code), read from the
//! native library in the user's APK, like [`crate::mobile_values`]: its
//! rules are code, not data.
//!
//! - `Game::ArcadeGameGenerateWave(int)`: a `switch` on the wave; each case
//!   fills an array of character types on the stack and sets how many, then
//!   all go on to place them at random in a box (two `float[2]` constants:
//!   its size and centre). The last case (past the table) is every later
//!   wave.
//! - `Game::ArcadeGameInit` / `ArcadeGameProcess`: the wait before the
//!   first wave and between waves (seconds, as float immediates).
//! - `Player::placeHunter`: the hunter's spot (the second `x`/`z` pair of
//!   float immediates; the first is another mode's).
//! - `CharactersManager::CharacterInfo_SetDefaultParams_DH(int)`: a
//!   `switch` on the type; each case looks up the animal's name string
//!   (`STR_ALLOSAURUS`), which tells the types apart.
//!
//! Positions are the mobile game's: its map is the PC map transposed (PC
//! `x` = mobile `-z`, PC `z` = mobile `x`).

use std::collections::BTreeMap;

use crate::error::FormatError;
use crate::mobile_values::{at_va, bad, sections, symbols};

/// The Survive mode's rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Arcade {
    /// Character types of each listed wave, then of every later one.
    pub waves: Vec<Vec<i32>>,
    pub after: Vec<i32>,
    /// Where animals come: centre `x`, `z` and half sizes (mobile units).
    pub spawn: [f32; 4],
    /// The hunter's spot (`x`, `z`).
    pub hunter: [f32; 2],
    /// Seconds before the first wave, and between waves.
    pub first_s: f32,
    pub rest_s: f32,
    /// The name key (`STR_ALLOSAURUS`) of each character type.
    pub names: BTreeMap<i32, String>,
}

fn word(code: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        code.get(i * 4..i * 4 + 4)?.try_into().ok()?,
    ))
}

fn adrp_target(ins: u32, pc: u64) -> u64 {
    let imm = (((ins >> 5) & 0x7FFFF) << 2 | ((ins >> 29) & 3)) as i64;
    let imm = (imm << 43) >> 43;
    (pc & !0xFFF).wrapping_add((imm << 12) as u64)
}

fn adr_target(ins: u32, pc: u64) -> u64 {
    let imm = (((ins >> 5) & 0x7FFFF) << 2 | ((ins >> 29) & 3)) as i64;
    pc.wrapping_add(((imm << 43) >> 43) as u64)
}

/// A function's code by symbol (a part of its name).
fn function<'a>(lib: &'a [u8], part: &str) -> Option<(&'a [u8], u64)> {
    let secs = sections(lib)?;
    let (_, va, size) = symbols(lib, &secs)
        .into_iter()
        .find(|(n, _, s)| n.contains(part) && *s > 16)?;
    let code = at_va(lib, &secs, va)?.get(..usize::try_from(size).ok()?)?;
    Some((code, va))
}

fn bytes_at(lib: &[u8], va: u64, n: usize) -> Option<&[u8]> {
    let secs = sections(lib)?;
    at_va(lib, &secs, va)?.get(..n)
}

/// A `switch`'s jump table at the start of `code`: the value it subtracts
/// first (`sub wN, w1, #k`), the case count, each case's address and the
/// default's (`b.hi`).
fn switch(lib: &[u8], code: &[u8], va: u64) -> Option<(i32, Vec<u64>, Option<u64>)> {
    let (mut sub, mut count, mut base, mut default) = (0, None, None, None);
    let mut x = [0u64; 32];
    let mut table = None;
    let mut half = true;
    for i in 0..80 {
        let ins = word(code, i)?;
        let pc = va + i as u64 * 4;
        let (rd, rn) = ((ins & 31) as usize, ((ins >> 5) & 31) as usize);
        if ins & 0xFF80_0000 == 0x5100_0000 {
            sub = ((ins >> 10) & 0xFFF) as i32;
        } else if ins & 0xFF80_001F == 0x7100_001F {
            count = Some(((ins >> 10) & 0xFFF) as usize + 1);
        } else if ins & 0xFF00_001F == 0x5400_0008 {
            // B.HI
            let imm = ((ins >> 5) & 0x7FFFF) as i64;
            default = Some(pc.wrapping_add((((imm << 45) >> 45) * 4) as u64));
        } else if ins & 0x9F00_0000 == 0x9000_0000 {
            x[rd] = adrp_target(ins, pc);
        } else if ins & 0xFF80_0000 == 0x9100_0000 {
            x[rd] = x[rn].wrapping_add(u64::from((ins >> 10) & 0xFFF));
        } else if ins & 0x9F00_0000 == 0x1000_0000 {
            base = Some(adr_target(ins, pc));
        } else if ins & 0xFFE0_FC00 == 0x7860_7800 || ins & 0xFFE0_FC00 == 0x3860_7800 {
            // LDRH / LDRB Wt, [Xn, Xm, LSL ..]: the table.
            half = ins & 0xFFE0_FC00 == 0x7860_7800;
            table = Some(x[rn]);
        } else if ins & 0xFFFF_FC1F == 0xD61F_0000 {
            // BR: the switch is set up.
            break;
        }
    }
    let (count, base, table) = (count?, base?, table?);
    let size = if half { 2 } else { 1 };
    let t = bytes_at(lib, table, count * size)?;
    let cases = (0..count)
        .map(|i| {
            let e = if half {
                u64::from(u16::from_le_bytes([t[i * 2], t[i * 2 + 1]]))
            } else {
                u64::from(t[i])
            };
            base + e * 4
        })
        .collect();
    Some((sub, cases, default))
}

/// Just enough of an AArch64 machine to run one wave's case: immediates,
/// vector immediates and constant loads into the stack, branches; it stops
/// at the count's store (`str wN, [x0, #..]`).
struct Machine {
    x: [u64; 32],
    v: [[u32; 4]; 32],
    sp: u64,
    mem: BTreeMap<u64, u32>,
}

const SP: u64 = 0x7000_0000;

impl Machine {
    fn new() -> Self {
        Self {
            x: [0; 32],
            v: [[0; 4]; 32],
            sp: SP,
            mem: BTreeMap::new(),
        }
    }

    fn base(&self, rn: usize) -> u64 {
        if rn == 31 {
            self.sp
        } else {
            self.x[rn]
        }
    }

    /// Runs from `pc` until the count is stored (`str wN, [x0, #..]`): the
    /// count.
    fn run(&mut self, lib: &[u8], fva: u64, fcode: &[u8], mut pc: u64) -> Option<usize> {
        for _ in 0..600 {
            let ins = word(fcode, usize::try_from((pc - fva) / 4).ok()?)?;
            let (rd, rn) = ((ins & 31) as usize, ((ins >> 5) & 31) as usize);
            let mut next = pc + 4;
            if ins & 0x7F80_0000 == 0x5280_0000 || ins & 0x7F80_0000 == 0x1280_0000 {
                let wide = ins >> 31 == 1;
                let mut v = u64::from((ins >> 5) & 0xFFFF) << (16 * ((ins >> 21) & 3));
                if ins & 0x7F80_0000 == 0x1280_0000 {
                    v = !v;
                }
                if rd != 31 {
                    self.x[rd] = if wide { v } else { v & 0xFFFF_FFFF };
                }
            } else if ins & 0x7F80_0000 == 0x7280_0000 {
                let sh = 16 * ((ins >> 21) & 3);
                let v = u64::from((ins >> 5) & 0xFFFF);
                self.x[rd] = (self.x[rd] & !(0xFFFFu64 << sh)) | (v << sh);
            } else if ins & 0x9FF8_9C00 == 0x0F00_0400 {
                // MOVI Vd.{2,4}S, #imm8{, LSL #8*k}
                let imm8 = ((ins >> 16) & 7) << 5 | ((ins >> 5) & 31);
                let sh = 8 * ((ins >> 13) & 3);
                let lanes = if ins & (1 << 30) != 0 { 4 } else { 2 };
                let mut v = [0; 4];
                for l in v.iter_mut().take(lanes) {
                    *l = imm8 << sh;
                }
                self.v[rd] = v;
            } else if ins & 0xBFF8_FC00 == 0x2F00_E400 {
                // MOVI Vd.2D, #mask (here: zero)
                let imm8 = ((ins >> 16) & 7) << 5 | ((ins >> 5) & 31);
                let byte = |b: u32| if imm8 & (1 << b) != 0 { 0xFFu32 } else { 0 };
                let lo = (0..4).fold(0, |a, b| a | byte(b) << (8 * b));
                let hi = (4..8).fold(0, |a, b| a | byte(b) << (8 * (b - 4)));
                self.v[rd] = [lo, hi, lo, hi];
            } else if ins & 0x9F00_0000 == 0x9000_0000 {
                self.x[rd] = adrp_target(ins, pc);
            } else if ins & 0xFF80_0000 == 0x9100_0000 {
                let imm = u64::from((ins >> 10) & 0xFFF) << (12 * ((ins >> 22) & 1));
                let v = self.base(rn).wrapping_add(imm);
                if rd == 31 {
                    self.sp = v;
                } else {
                    self.x[rd] = v;
                }
            } else if ins & 0xFFC0_0000 == 0x3DC0_0000 || ins & 0xFFC0_0000 == 0xFD40_0000 {
                // LDR Qt / Dt, [Xn, #imm]
                let q = ins & 0xFFC0_0000 == 0x3DC0_0000;
                let n = if q { 16 } else { 8 };
                let a = self.base(rn) + u64::from((ins >> 10) & 0xFFF) * n;
                let mut v = [0; 4];
                if a < SP {
                    let b = bytes_at(lib, a, n as usize)?;
                    for (k, l) in v.iter_mut().enumerate().take(n as usize / 4) {
                        *l = u32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().ok()?);
                    }
                }
                self.v[rd] = v;
            } else if ins & 0xFFC0_0000 == 0xB900_0000 {
                // STR Wt, [Xn, #imm]
                let a = self.base(rn) + u64::from((ins >> 10) & 0xFFF) * 4;
                if rn == 0 {
                    return usize::try_from(self.x[rd] & 0xFFFF_FFFF).ok();
                }
                self.mem.insert(a, (self.x[rd] & 0xFFFF_FFFF) as u32);
            } else if ins & 0xFFC0_0000 == 0xFD00_0000 || ins & 0xFFC0_0000 == 0x3D80_0000 {
                // STR Dt / Qt, [Xn, #imm]
                let q = ins & 0xFFC0_0000 == 0x3D80_0000;
                let n = if q { 16 } else { 8 };
                let a = self.base(rn) + u64::from((ins >> 10) & 0xFFF) * n;
                for k in 0..n / 4 {
                    self.mem.insert(a + k * 4, self.v[rd][k as usize]);
                }
            } else if ins & 0xFFE0_FC00 == 0xB820_7800 {
                // STR Wt, [Xn, Xm, LSL #2]
                let m = ((ins >> 16) & 31) as usize;
                let a = self.base(rn) + (self.x[m] << 2);
                self.mem.insert(a, (self.x[rd] & 0xFFFF_FFFF) as u32);
            } else if ins & 0xFC00_0000 == 0x1400_0000 {
                let imm = (ins & 0x03FF_FFFF) as i64;
                next = pc.wrapping_add((((imm << 38) >> 38) * 4) as u64);
            } else if ins & 0xFC00_0000 == 0x9400_0000 {
                // BL: past the part we follow.
                return None;
            }
            pc = next;
        }
        None
    }
}

/// The float at the second pair of `str wN, [x19]` / `str wN, [x19, #8]`
/// immediates in `Player::placeHunter`.
fn hunter(code: &[u8]) -> Option<[f32; 2]> {
    let mut x = [0u64; 32];
    let mut pairs = Vec::new();
    let mut first: Option<f32> = None;
    for i in 0..code.len() / 4 {
        let ins = word(code, i)?;
        let (rd, rn) = ((ins & 31) as usize, ((ins >> 5) & 31) as usize);
        if ins & 0x7F80_0000 == 0x5280_0000 || ins & 0x7F80_0000 == 0x1280_0000 {
            let mut v = u64::from((ins >> 5) & 0xFFFF) << (16 * ((ins >> 21) & 3));
            if ins & 0x7F80_0000 == 0x1280_0000 {
                v = !v;
            }
            x[rd] = v & 0xFFFF_FFFF;
        } else if ins & 0x7F80_0000 == 0x7280_0000 {
            let sh = 16 * ((ins >> 21) & 3);
            x[rd] = (x[rd] & !(0xFFFFu64 << sh)) | (u64::from((ins >> 5) & 0xFFFF) << sh);
        } else if ins & 0xFFC0_0000 == 0xB900_0000 && rn == 19 && rd != 31 {
            let f = f32::from_bits(x[rd] as u32);
            match (ins >> 10) & 0xFFF {
                0 => first = Some(f),
                2 => {
                    if let Some(a) = first.take() {
                        pairs.push([a, f]);
                    }
                }
                _ => {}
            }
        }
    }
    pairs.get(1).copied()
}

/// The first float immediate made in `code`: a `movz wN, #hi, lsl #16`
/// (32-bit) or the top half of a 64-bit one (`movz`/`orr` into an `x`).
fn first_float(code: &[u8]) -> Option<f32> {
    for i in 0..code.len() / 4 {
        let ins = word(code, i)?;
        if ins & 0xFFE0_0000 == 0x52A0_0000 {
            return Some(f32::from_bits(((ins >> 5) & 0xFFFF) << 16));
        }
        if ins & 0xFFE0_0000 == 0xD2E0_0000 {
            return Some(f32::from_bits(((ins >> 5) & 0xFFFF) << 16));
        }
        if ins & 0xFF80_03E0 == 0xB200_03E0 {
            // ORR Xd, XZR, #bitmask
            let v =
                crate::mobile_values::bitmask((ins >> 22) & 1, (ins >> 16) & 63, (ins >> 10) & 63)?;
            return Some(f32::from_bits((v >> 32) as u32));
        }
    }
    None
}

/// The name key of each type in `SetDefaultParams_DH`'s switch.
fn names(lib: &[u8], code: &[u8], va: u64) -> Option<BTreeMap<i32, String>> {
    let (sub, cases, _) = switch(lib, code, va)?;
    let mut out = BTreeMap::new();
    for (k, &target) in cases.iter().enumerate() {
        let start = usize::try_from((target - va) / 4).ok()?;
        let mut page = None;
        for i in start..start + 40 {
            let ins = word(code, i)?;
            let pc = va + i as u64 * 4;
            if ins & 0x9F00_001F == 0x9000_0001 {
                page = Some(adrp_target(ins, pc));
            } else if ins & 0xFF80_03FF == 0x9100_0021 {
                if let Some(p) = page {
                    let a = p + u64::from((ins >> 10) & 0xFFF);
                    let secs = sections(lib)?;
                    let s = at_va(lib, &secs, a)?;
                    let end = s.iter().take(64).position(|&c| c == 0)?;
                    let name = String::from_utf8_lossy(&s[..end]).into_owned();
                    if name.starts_with("STR_") {
                        out.insert(sub + k as i32, name);
                    }
                    break;
                }
            }
        }
    }
    Some(out)
}

/// The Survive mode as our games play it (`mobile-survival.txt`, written
/// by the APK import beside the add-ons): animals by name, positions in PC
/// units on the PC area.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Survive {
    /// `AREAn.MAP` (the mobile game's survive map is the PC one: the same
    /// heights).
    pub area: u32,
    pub hunter: [f32; 2],
    /// Centre `x`, `z`, half sizes along `x`, `z`.
    pub spawn: [f32; 4],
    pub first_ms: i32,
    pub rest_ms: i32,
    pub waves: Vec<Vec<String>>,
    pub after: Vec<String>,
}

impl Arcade {
    /// As our games play it, on PC area `area`; `name` gives an animal's
    /// name for its key (`STR_ALLOSAURUS`).
    pub fn survive(&self, area: u32, name: &dyn Fn(&str) -> String) -> Survive {
        let named = |w: &Vec<i32>| {
            w.iter()
                .map(|t| {
                    self.names
                        .get(t)
                        .map_or_else(|| format!("type {t}"), |k| name(k))
                })
                .collect()
        };
        // The mobile map is the PC one transposed: PC x = -z, PC z = x.
        let [cx, cz, hx, hz] = self.spawn;
        Survive {
            area,
            hunter: [-self.hunter[1], self.hunter[0]],
            spawn: [-cz, cx, hz, hx],
            first_ms: (self.first_s * 1000.0).round() as i32,
            rest_ms: (self.rest_s * 1000.0).round() as i32,
            waves: self.waves.iter().map(named).collect(),
            after: named(&self.after),
        }
    }
}

impl Survive {
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        let nums = |v: &str| -> Vec<f32> {
            v.split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect()
        };
        let names = |v: &str| -> Vec<String> {
            v.split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect()
        };
        for line in text.lines().map(str::trim) {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "area" => s.area = v.parse().unwrap_or(0),
                "hunter" => {
                    if let [x, z] = nums(v)[..] {
                        s.hunter = [x, z];
                    }
                }
                "spawn" => {
                    if let [x, z, a, b] = nums(v)[..] {
                        s.spawn = [x, z, a, b];
                    }
                }
                "first_ms" => s.first_ms = v.parse().unwrap_or(500),
                "rest_ms" => s.rest_ms = v.parse().unwrap_or(2000),
                "wave" => s.waves.push(names(v)),
                "after" => s.after = names(v),
                _ => {}
            }
        }
        s
    }

    pub fn write(&self) -> String {
        let mut t = String::from(
            "# The mobile game's Survive mode, read from your own APK (keep it local).\n",
        );
        t += &format!("area = {}\n", self.area);
        t += &format!("hunter = {} {}\n", self.hunter[0], self.hunter[1]);
        let [a, b, c, d] = self.spawn;
        t += &format!("spawn = {a} {b} {c} {d}\n");
        t += &format!("first_ms = {}\nrest_ms = {}\n", self.first_ms, self.rest_ms);
        for w in &self.waves {
            t += &format!("wave = {}\n", w.join(", "));
        }
        t += &format!("after = {}\n", self.after.join(", "));
        t
    }

    /// Every animal it sends.
    pub fn animals(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .waves
            .iter()
            .flatten()
            .chain(&self.after)
            .cloned()
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// The Survive mode of a mobile game's native library.
pub fn arcade(lib: &[u8]) -> Result<Arcade, FormatError> {
    let (wcode, wva) =
        function(lib, "ArcadeGameGenerateWave").ok_or_else(|| bad("ArcadeGameGenerateWave"))?;
    let (_, cases, default) = switch(lib, wcode, wva).ok_or_else(|| bad("wave switch"))?;
    let mut spawn = None;
    let mut wave = |pc: u64| -> Option<Vec<i32>> {
        let mut m = Machine::new();
        let n = m.run(lib, wva, wcode, pc)?;
        // The array the count goes with: `add x24, sp, #..`, the register
        // the placing loop reads the types from; it is the lowest stack
        // slot written.
        let lo = *m.mem.keys().next()?;
        spawn = Some([
            f32::from_bits(m.v[11][0]),
            f32::from_bits(m.v[11][1]),
            f32::from_bits(m.v[10][0]) / 2.0,
            f32::from_bits(m.v[10][1]) / 2.0,
        ]);
        (0..n)
            .map(|k| m.mem.get(&(lo + 4 * k as u64)).map(|&t| t as i32))
            .collect()
    };
    let waves = cases
        .iter()
        .map(|&pc| wave(pc))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| bad("waves"))?;
    let after = default.and_then(&mut wave).unwrap_or_default();
    let (hcode, _) = function(lib, "placeHunter").ok_or_else(|| bad("placeHunter"))?;
    let hunter = hunter(hcode).ok_or_else(|| bad("hunter"))?;
    let first_s = function(lib, "ArcadeGameInit")
        .and_then(|(c, _)| first_float(c))
        .unwrap_or(0.5);
    let rest_s = function(lib, "ArcadeGameProcess")
        .and_then(|(c, _)| first_float(c))
        .unwrap_or(2.0);
    let (ncode, nva) = function(lib, "CharacterInfo_SetDefaultParams_DH")
        .ok_or_else(|| bad("SetDefaultParams_DH"))?;
    let names = names(lib, ncode, nva).ok_or_else(|| bad("names"))?;
    Ok(Arcade {
        waves,
        after,
        spawn: spawn.ok_or_else(|| bad("spawn"))?,
        hunter,
        first_s,
        rest_s,
        names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movz(rd: u32, imm: u32, hw: u32) -> u32 {
        0x5280_0000 | (hw << 21) | (imm << 5) | rd
    }
    fn movk(rd: u32, imm: u32, hw: u32) -> u32 {
        0x7280_0000 | (hw << 21) | (imm << 5) | rd
    }
    fn str19(rt: u32, off: u32) -> u32 {
        0xB900_0000 | ((off / 4) << 10) | (19 << 5) | rt
    }
    fn bytes(code: &[u32]) -> Vec<u8> {
        code.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn hunter_is_the_second_pair() {
        let code = [
            movz(8, 0x4719, 1),
            movz(10, 0xc70d, 1),
            str19(8, 0),
            str19(10, 8),
            movz(9, 0x6c9e, 0),
            movz(10, 0x7911, 0),
            movk(9, 0x477f, 1),
            movk(10, 0xc7ae, 1),
            str19(9, 0),
            str19(10, 8),
        ];
        let h = hunter(&bytes(&code)).unwrap();
        assert!(
            (h[0] - 65388.6).abs() < 0.1 && (h[1] + 89330.1).abs() < 0.1,
            "{h:?}"
        );
    }

    #[test]
    fn a_wave_case_runs_to_its_count() {
        // mov w9, #20; movi v0.2s, #19; str d0, [sp, #0x30];
        // add x24, sp, #0x30; mov w10, #2; mov x? ..; str w9, [x24, x10, lsl 2];
        // mov w8, #3; str w8, [x0, #0x8a0]
        let code = [
            movz(9, 20, 0),
            0x0F00_0400 | (19 << 5), // movi v0.2s, #19
            0xFD00_0000 | ((0x30 / 8) << 10) | (31 << 5), // str d0, [sp, #0x30]
            0x9100_0000 | (0x30 << 10) | (31 << 5) | 24, // add x24, sp, #0x30
            movz(10, 2, 0),
            0xB820_7800 | (10 << 16) | (24 << 5) | 9, // str w9, [x24, x10, lsl #2]
            movz(8, 3, 0),
            0xB900_0000 | ((0x8a0 / 4) << 10) | 8, // str w8, [x0, #0x8a0]
        ];
        let b = bytes(&code);
        let mut m = Machine::new();
        let n = m.run(&[], 0x1000, &b, 0x1000).unwrap();
        let lo = *m.mem.keys().next().unwrap();
        let t: Vec<u32> = (0..n).map(|k| m.mem[&(lo + 4 * k as u64)]).collect();
        assert_eq!(t, [19, 19, 20]);
    }

    #[test]
    fn survive_is_on_the_pc_map_and_round_trips() {
        let a = Arcade {
            waves: vec![vec![20, 20], vec![19]],
            after: vec![19, 20],
            spawn: [45230.0, -84069.0, 2000.0, 3000.0],
            hunter: [65388.0, -89330.0],
            first_s: 0.5,
            rest_s: 2.0,
            names: [
                (19, "STR_ALLOSAURUS".into()),
                (20, "STR_CHASMOSAURUS".into()),
            ]
            .into(),
        };
        let s = a.survive(5, &|k| k[4..5].to_string() + &k[5..].to_ascii_lowercase());
        assert_eq!(s.hunter, [89330.0, 65388.0]);
        assert_eq!(s.spawn, [84069.0, 45230.0, 3000.0, 2000.0]);
        assert_eq!(s.waves[0], ["Chasmosaurus", "Chasmosaurus"]);
        assert_eq!((s.first_ms, s.rest_ms), (500, 2000));
        assert_eq!(Survive::parse(&s.write()), s);
        assert_eq!(s.animals(), ["Allosaurus", "Chasmosaurus"]);
    }

    #[test]
    fn not_a_library() {
        assert!(arcade(b"nothing").is_err());
    }
}
