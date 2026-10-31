//! The mobile games' character and weapon tables, read from the game's
//! native library in the user's APK. The PC keeps these in `_RES.TXT`; the
//! mobile games set them in code: `CharactersManager::CharacterInfo_Init`
//! calls `CharacterInfo_SetDefaultParams_DH` (or `_IA`) and
//! `CharacterInfo_SetDefaultWeaponsParams` for each character index, each a
//! `switch` that fills a 5768-byte `CharacterInfo` with constants, a name
//! from the localization (`STR_T_REX`) and values from the balance table
//! (`DINO_T_REX_HEALTH`, looked up by `strcmp`). `Game::LoadCharacters`
//! names each index's files (24: `tirex`).
//!
//! This runs those functions on just enough of an AArch64 machine to see
//! what they store; calls are stood in for by name (the localization
//! returns its key, a `strcmp` finds the balance entry).

use alloc::collections::BTreeMap;
use alloc::{string::String, string::ToString, vec::Vec};

use crate::error::FormatError;
use crate::mobile_values::{at_va, bad, fmov_imm, sections, symbols, u32_at, u64_at, Section};

/// Size of one `CharacterInfo`.
pub const INFO_SIZE: u64 = 0x1688;

/// What a character's setup stored.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stored {
    /// Its name's localization key (`STR_T_REX`).
    pub name: Option<String>,
    /// Bytes stored, by offset in its `CharacterInfo` (4-byte words).
    pub words: BTreeMap<u64, u32>,
    /// The balance keys it read, in order.
    pub keys: Vec<String>,
}

impl Stored {
    pub fn f32(&self, off: u64) -> Option<f32> {
        self.words.get(&off).map(|&w| f32::from_bits(w))
    }

    pub fn i32(&self, off: u64) -> Option<i32> {
        self.words.get(&off).map(|&w| w as i32)
    }
}

/// A library's characters, by index: what each setup function stored, and
/// the file `Game::LoadCharacters` loads for it.
#[derive(Debug, Clone, Default)]
pub struct Characters {
    pub params: BTreeMap<i32, Stored>,
    pub weapons: BTreeMap<i32, Stored>,
    pub files: BTreeMap<i32, String>,
}

struct Lib<'a> {
    d: &'a [u8],
    secs: Vec<Section>,
    /// PLT stub address -> imported name.
    plt: BTreeMap<u64, String>,
}

impl<'a> Lib<'a> {
    fn new(d: &'a [u8]) -> Option<Self> {
        let secs = sections(d)?;
        let mut lib = Self {
            d,
            secs,
            plt: BTreeMap::new(),
        };
        lib.plt = lib.plt_names();
        Some(lib)
    }

    fn bytes(&self, va: u64, n: usize) -> Option<&'a [u8]> {
        at_va(self.d, &self.secs, va)?.get(..n)
    }

    fn word(&self, va: u64) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(va, 4)?.try_into().ok()?))
    }

    fn cstr(&self, va: u64) -> Option<String> {
        let b = at_va(self.d, &self.secs, va)?;
        let end = b.iter().take(128).position(|&c| c == 0)?;
        core::str::from_utf8(&b[..end]).ok().map(str::to_string)
    }

    fn function(&self, part: &str) -> Option<(u64, u64)> {
        symbols(self.d, &self.secs)
            .into_iter()
            .filter(|(n, _, s)| n.contains(part) && *s > 16)
            .map(|(_, va, size)| (va, size))
            .next()
    }

    /// Each PLT stub's import: `.rela.plt` gives each GOT slot's symbol;
    /// a stub is `adrp x16, page; ldr x17, [x16, #off]`.
    fn plt_names(&self) -> BTreeMap<u64, String> {
        let d = self.d;
        let mut slot_name = BTreeMap::new();
        for rel in self.secs.iter().filter(|s| s.kind == 4) {
            let Some(dynsym) = self.secs.get(rel.link).filter(|s| s.kind == 11) else {
                continue;
            };
            let Some(strs) = self.secs.get(dynsym.link) else {
                continue;
            };
            for o in (rel.off..rel.off + rel.size).step_by(24) {
                let (Some(slot), Some(info)) = (u64_at(d, o), u64_at(d, o + 8)) else {
                    continue;
                };
                let sym = dynsym.off + (info >> 32) as usize * 24;
                let Some(name) = u32_at(d, sym) else { continue };
                let start = strs.off + name as usize;
                let Some(b) = d.get(start..strs.off + strs.size) else {
                    continue;
                };
                let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                slot_name.insert(slot, String::from_utf8_lossy(&b[..end]).into_owned());
            }
        }
        let mut out = BTreeMap::new();
        for s in self.secs.iter().filter(|s| s.kind == 1 && s.addr != 0) {
            // Executable sections only hold stubs worth reading; any
            // `adrp x16` + `ldr x17, [x16]` pair is one.
            for va in (s.addr..s.addr + s.size as u64).step_by(4) {
                let (Some(a), Some(l)) = (self.word(va), self.word(va + 4)) else {
                    continue;
                };
                if a & 0x9F00_001F != 0x9000_0010 || l & 0xFFC0_03FF != 0xF940_0211 {
                    continue;
                }
                let slot = adrp(a, va) + u64::from((l >> 10) & 0xFFF) * 8;
                if let Some(n) = slot_name.get(&slot) {
                    out.insert(va, n.clone());
                }
            }
        }
        out
    }
}

fn adrp(ins: u32, pc: u64) -> u64 {
    let imm = (((ins >> 5) & 0x7FFFF) << 2 | ((ins >> 29) & 3)) as i64;
    let imm = (imm << 43) >> 43;
    (pc & !0xFFF).wrapping_add((imm << 12) as u64)
}

fn adr(ins: u32, pc: u64) -> u64 {
    let imm = (((ins >> 5) & 0x7FFFF) << 2 | ((ins >> 29) & 3)) as i64;
    pc.wrapping_add(((imm << 43) >> 43) as u64)
}

fn sext(v: u64, bits: u32) -> i64 {
    ((v << (64 - bits)) as i64) >> (64 - bits)
}

/// The `this` the functions get; each index's info is `INFO_SIZE` on.
const THIS: u64 = 0x4000_0000;
const SP: u64 = 0x7000_0000;

struct Machine<'l, 'a> {
    lib: &'l Lib<'a>,
    x: [u64; 32],
    v: [[u8; 16]; 32],
    sp: u64,
    mem: BTreeMap<u64, u8>,
    balance: &'l BTreeMap<String, f64>,
    name: Option<String>,
    keys: Vec<String>,
}

impl<'l, 'a> Machine<'l, 'a> {
    fn new(lib: &'l Lib<'a>, balance: &'l BTreeMap<String, f64>) -> Self {
        Self {
            lib,
            x: [0; 32],
            v: [[0; 16]; 32],
            sp: SP,
            mem: BTreeMap::new(),
            balance,
            name: None,
            keys: Vec::new(),
        }
    }

    /// Register `r` as a base (31 is `sp`).
    fn base(&self, r: usize) -> u64 {
        if r == 31 {
            self.sp
        } else {
            self.x[r]
        }
    }

    /// Register `r` as an operand (31 is zero).
    fn reg(&self, r: usize) -> u64 {
        if r == 31 {
            0
        } else {
            self.x[r]
        }
    }

    fn set(&mut self, r: usize, v: u64, wide: bool) {
        if r != 31 {
            self.x[r] = if wide { v } else { v & 0xFFFF_FFFF };
        }
    }

    fn set_base(&mut self, r: usize, v: u64) {
        if r == 31 {
            self.sp = v;
        } else {
            self.x[r] = v;
        }
    }

    fn load(&self, a: u64, n: usize) -> [u8; 16] {
        let mut out = [0u8; 16];
        let from_lib = a < THIS && !self.mem.contains_key(&a);
        let lib = if from_lib { self.lib.bytes(a, n) } else { None };
        for (k, b) in out.iter_mut().enumerate().take(n) {
            *b = match lib {
                Some(l) => l[k],
                None => self.mem.get(&(a + k as u64)).copied().unwrap_or(0),
            };
        }
        out
    }

    fn store(&mut self, a: u64, b: &[u8]) {
        for (k, &v) in b.iter().enumerate() {
            self.mem.insert(a + k as u64, v);
        }
    }

    fn load_x(&self, a: u64, n: usize) -> u64 {
        let b = self.load(a, n);
        let mut v = [0u8; 8];
        v[..n.min(8)].copy_from_slice(&b[..n.min(8)]);
        u64::from_le_bytes(v)
    }

    fn f(&self, r: usize, double: bool) -> f64 {
        if double {
            f64::from_le_bytes(self.v[r][..8].try_into().unwrap_or([0; 8]))
        } else {
            f64::from(f32::from_le_bytes(
                self.v[r][..4].try_into().unwrap_or([0; 4]),
            ))
        }
    }

    fn set_f(&mut self, r: usize, v: f64, double: bool) {
        let mut b = [0u8; 16];
        if double {
            b[..8].copy_from_slice(&v.to_le_bytes());
        } else {
            b[..4].copy_from_slice(&(v as f32).to_le_bytes());
        }
        self.v[r] = b;
    }

    /// A call: the imports these functions make, stood in for.
    fn call(&mut self, target: u64) -> bool {
        let name = self.lib.plt.get(&target).cloned().unwrap_or_default();
        if name.contains("LocalizationManager") && name.contains("get") {
            self.name = self.lib.cstr(self.x[1]);
            self.x[0] = self.x[1];
        } else if name == "strcmp" {
            // The balance entry the loop looks for: found, here.
            let key = self.lib.cstr(self.x[1]).unwrap_or_default();
            let v = self.balance.get(&key).copied().unwrap_or(-1.0);
            self.store(self.x[0] + 0x20, &v.to_le_bytes());
            self.keys.push(key);
            self.x[0] = 0;
        } else if name.contains("stack_chk_fail") {
            return false;
        } else if !name.contains("strcpy") && !name.contains("memcpy") {
            self.x[0] = 0;
        }
        true
    }

    /// Runs from `pc` to the function's `ret`; `cases` is the switch's
    /// targets by value (`br` goes to `case`).
    fn run(&mut self, mut pc: u64, case: u64) -> Option<()> {
        for _ in 0..20_000 {
            let ins = self.lib.word(pc)?;
            let (rd, rn) = ((ins & 31) as usize, ((ins >> 5) & 31) as usize);
            let rm = ((ins >> 16) & 31) as usize;
            let wide = ins >> 31 == 1;
            let mut next = pc + 4;
            match ins {
                // MOVZ / MOVN / MOVK
                _ if ins & 0x7F80_0000 == 0x5280_0000 || ins & 0x7F80_0000 == 0x1280_0000 => {
                    let mut v = u64::from((ins >> 5) & 0xFFFF) << (16 * ((ins >> 21) & 3));
                    if ins & 0x7F80_0000 == 0x1280_0000 {
                        v = !v;
                    }
                    self.set(rd, v, wide);
                }
                _ if ins & 0x7F80_0000 == 0x7280_0000 => {
                    let sh = 16 * ((ins >> 21) & 3);
                    let v = u64::from((ins >> 5) & 0xFFFF);
                    let old = self.reg(rd);
                    self.set(rd, (old & !(0xFFFFu64 << sh)) | (v << sh), wide);
                }
                // MOV (ORR, shifted register, from zero)
                _ if ins & 0x7F20_FFE0 == 0x2A00_03E0 => self.set(rd, self.reg(rm), wide),
                // ORR (immediate)
                _ if ins & 0x7F80_0000 == 0x3200_0000 => {
                    let n = (ins >> 22) & 1;
                    let v =
                        crate::mobile_values::bitmask(n, (ins >> 16) & 0x3F, (ins >> 10) & 0x3F)
                            .unwrap_or(0);
                    self.set(rd, self.reg(rn) | v, wide);
                }
                // ADD / SUB (immediate), not setting flags
                _ if ins & 0x7F00_0000 == 0x1100_0000 || ins & 0x7F00_0000 == 0x5100_0000 => {
                    let imm = u64::from((ins >> 10) & 0xFFF) << (12 * ((ins >> 22) & 1));
                    let a = self.base(rn);
                    let v = if ins & 0x4000_0000 != 0 {
                        a.wrapping_sub(imm)
                    } else {
                        a.wrapping_add(imm)
                    };
                    let v = if wide { v } else { v & 0xFFFF_FFFF };
                    self.set_base(rd, v);
                }
                // ADD (shifted register)
                _ if ins & 0x7F20_0000 == 0x0B00_0000 => {
                    let amt = (ins >> 10) & 0x3F;
                    let v = self.reg(rn).wrapping_add(self.reg(rm) << amt);
                    self.set(rd, v, wide);
                }
                // SMADDL
                _ if ins & 0xFFE0_8000 == 0x9B20_0000 => {
                    let ra = ((ins >> 10) & 31) as usize;
                    let p = sext(self.reg(rn), 32).wrapping_mul(sext(self.reg(rm), 32));
                    self.set(rd, self.reg(ra).wrapping_add(p as u64), true);
                }
                // ADRP / ADR
                _ if ins & 0x9F00_0000 == 0x9000_0000 => self.set(rd, adrp(ins, pc), true),
                _ if ins & 0x9F00_0000 == 0x1000_0000 => self.set(rd, adr(ins, pc), true),
                // LDR / STR (unsigned offset)
                _ if ins & 0x3B00_0000 == 0x3900_0000 => {
                    let size = ins >> 30;
                    let simd = ins & (1 << 26) != 0;
                    let opc = (ins >> 22) & 3;
                    let n = if simd && size == 0 && opc >= 2 {
                        16
                    } else {
                        1usize << size
                    };
                    let a = self.base(rn) + u64::from((ins >> 10) & 0xFFF) * n as u64;
                    self.ldst(simd, opc & 1 == 1 || (simd && opc == 3), rd, a, n, ins);
                }
                // LDUR / STUR, and pre/post-indexed
                _ if ins & 0x3B20_0000 == 0x3800_0000 && (ins >> 10) & 3 != 2 => {
                    let size = ins >> 30;
                    let simd = ins & (1 << 26) != 0;
                    let opc = (ins >> 22) & 3;
                    let n = if simd && size == 0 && opc >= 2 {
                        16
                    } else {
                        1usize << size
                    };
                    let imm = sext(u64::from((ins >> 12) & 0x1FF), 9) as u64;
                    let mode = (ins >> 10) & 3;
                    let a = if mode == 1 {
                        self.base(rn)
                    } else {
                        self.base(rn).wrapping_add(imm)
                    };
                    self.ldst(simd, opc & 1 == 1 || (simd && opc == 3), rd, a, n, ins);
                    if mode != 0 {
                        let b = self.base(rn).wrapping_add(imm);
                        self.set_base(rn, b);
                    }
                }
                // LDR / STR (register offset)
                _ if ins & 0x3B20_0C00 == 0x3820_0800 => {
                    let size = ins >> 30;
                    let simd = ins & (1 << 26) != 0;
                    let opc = (ins >> 22) & 3;
                    let n = if simd && size == 0 && opc >= 2 {
                        16
                    } else {
                        1usize << size
                    };
                    let sh = if ins & (1 << 12) != 0 { size } else { 0 };
                    let a = self.base(rn).wrapping_add(self.reg(rm) << sh);
                    self.ldst(simd, opc & 1 == 1 || (simd && opc == 3), rd, a, n, ins);
                }
                // LDP / STP
                _ if ins & 0x3A00_0000 == 0x2800_0000 => {
                    let opc = ins >> 30;
                    let simd = ins & (1 << 26) != 0;
                    let n: usize = if simd {
                        4 << opc
                    } else if opc == 2 {
                        8
                    } else {
                        4
                    };
                    let imm = sext(u64::from((ins >> 15) & 0x7F), 7) * n as i64;
                    let mode = (ins >> 23) & 3;
                    let a = if mode == 1 {
                        self.base(rn)
                    } else {
                        self.base(rn).wrapping_add(imm as u64)
                    };
                    let rt2 = ((ins >> 10) & 31) as usize;
                    let load = ins & (1 << 22) != 0;
                    self.ldst(simd, load, rd, a, n, ins);
                    self.ldst(simd, load, rt2, a + n as u64, n, ins);
                    if mode != 2 {
                        let b = self.base(rn).wrapping_add(imm as u64);
                        self.set_base(rn, b);
                    }
                }
                // LDAR* and other exclusive/acquire loads: unknown, 0.
                _ if ins & 0x3F00_0000 == 0x0800_0000 => self.set(rd, 0, true),
                // FMOV (immediate)
                _ if ins & 0xFF20_1FE0 == 0x1E20_1000 => {
                    let double = (ins >> 22) & 3 == 1;
                    self.set_f(rd, fmov_imm((ins >> 13) & 0xFF), double);
                }
                // FMOV (general to/from FP), FMOV (register)
                _ if ins & 0x7F3F_FC00 == 0x1E27_0000 => {
                    let mut b = [0u8; 16];
                    b[..8].copy_from_slice(&self.reg(rn).to_le_bytes());
                    if !wide {
                        b[4..8].fill(0);
                    }
                    self.v[rd] = b;
                }
                _ if ins & 0x7F3F_FC00 == 0x1E26_0000 => {
                    let v = u64::from_le_bytes(self.v[rn][..8].try_into().unwrap_or([0; 8]));
                    self.set(rd, v, wide);
                }
                _ if ins & 0xFF3F_FC00 == 0x1E20_4000 => {
                    let double = (ins >> 22) & 3 == 1;
                    let v = self.f(rn, double);
                    self.set_f(rd, v, double);
                }
                // FCVT
                _ if ins & 0xFFFF_FC00 == 0x1E62_4000 => {
                    let v = self.f(rn, true);
                    self.set_f(rd, v, false);
                }
                _ if ins & 0xFFFF_FC00 == 0x1E22_C000 => {
                    let v = self.f(rn, false);
                    self.set_f(rd, v, true);
                }
                // SCVTF / UCVTF (from a general register)
                _ if ins & 0x7F3E_FC00 == 0x1E22_0000 => {
                    let double = (ins >> 22) & 3 == 1;
                    let r = self.reg(rn);
                    let v = if ins & (1 << 16) != 0 {
                        if wide {
                            r as f64
                        } else {
                            f64::from(r as u32)
                        }
                    } else if wide {
                        r as i64 as f64
                    } else {
                        f64::from(r as u32 as i32)
                    };
                    self.set_f(rd, v, double);
                }
                // FCVTZS / FCVTZU (to a general register)
                _ if ins & 0x7F3E_FC00 == 0x1E38_0000 => {
                    let v = self.f(rn, (ins >> 22) & 3 == 1) as i64;
                    self.set(rd, v as u64, wide);
                }
                // FMUL / FDIV / FADD / FSUB (scalar)
                _ if ins & 0xFF20_0C00 == 0x1E20_0800 && (ins >> 12) & 0xF < 4 => {
                    let double = (ins >> 22) & 3 == 1;
                    let (a, b) = (self.f(rn, double), self.f(rm, double));
                    let v = match (ins >> 12) & 0xF {
                        0 => a * b,
                        1 => a / b,
                        2 => a + b,
                        _ => a - b,
                    };
                    self.set_f(rd, v, double);
                }
                // MOVI (32-bit lanes) and MOVI .2D
                _ if ins & 0x9FF8_9C00 == 0x0F00_0400 => {
                    let imm8 = ((ins >> 16) & 7) << 5 | ((ins >> 5) & 31);
                    let sh = 8 * ((ins >> 13) & 3);
                    let lanes = if ins & (1 << 30) != 0 { 4 } else { 2 };
                    let mut b = [0u8; 16];
                    for l in 0..lanes {
                        b[l * 4..l * 4 + 4].copy_from_slice(&(imm8 << sh).to_le_bytes());
                    }
                    self.v[rd] = b;
                }
                _ if ins & 0xBFF8_FC00 == 0x2F00_E400 => {
                    let imm8 = ((ins >> 16) & 7) << 5 | ((ins >> 5) & 31);
                    let mut b = [0u8; 16];
                    for (k, x) in b.iter_mut().enumerate() {
                        if imm8 & (1 << (k % 8)) != 0 {
                            *x = 0xFF;
                        }
                    }
                    if ins & (1 << 30) == 0 {
                        b[8..].fill(0);
                    }
                    self.v[rd] = b;
                }
                // B, BL, BR, RET
                _ if ins & 0xFC00_0000 == 0x1400_0000 => {
                    next = pc.wrapping_add((sext(u64::from(ins & 0x03FF_FFFF), 26) * 4) as u64);
                }
                _ if ins & 0xFC00_0000 == 0x9400_0000 => {
                    let t = pc.wrapping_add((sext(u64::from(ins & 0x03FF_FFFF), 26) * 4) as u64);
                    if !self.call(t) {
                        return None;
                    }
                }
                _ if ins & 0xFFFF_FC1F == 0xD61F_0000 => next = case,
                _ if ins & 0xFFFF_FC1F == 0xD65F_0000 => return Some(()),
                // CBZ / CBNZ: as the register says.
                _ if ins & 0x7E00_0000 == 0x3400_0000 => {
                    let v = if wide {
                        self.reg(rd)
                    } else {
                        self.reg(rd) & 0xFFFF_FFFF
                    };
                    if (v == 0) != (ins & (1 << 24) != 0) {
                        next =
                            pc.wrapping_add((sext(u64::from((ins >> 5) & 0x7FFFF), 19) * 4) as u64);
                    }
                }
                // Anything else (compares, conditional branches, TBZ, MRS,
                // hints): no effect; branches not taken.
                _ => {}
            }
            pc = next;
        }
        None
    }

    fn ldst(&mut self, simd: bool, load: bool, rt: usize, a: u64, n: usize, ins: u32) {
        if load {
            if simd {
                self.v[rt] = self.load(a, n);
            } else {
                let mut v = self.load_x(a, n);
                // LDRSW / LDRSH / LDRSB
                let opc = (ins >> 22) & 3;
                if opc >= 2 && n < 8 {
                    v = sext(v, n as u32 * 8) as u64;
                }
                self.set(rt, v, n == 8 || opc == 2);
            }
        } else if simd {
            let b = self.v[rt];
            self.store(a, &b[..n]);
        } else {
            let b = self.reg(rt).to_le_bytes();
            self.store(a, &b[..n.min(8)]);
        }
    }
}

/// A `switch (index)` at a function's start: its first value and its
/// cases' targets.
fn cases(lib: &Lib<'_>, va: u64) -> Option<(i64, Vec<u64>, u64)> {
    let (mut sub, mut count, mut base, mut table, mut default) = (0, None, None, None, None);
    let mut x = [0u64; 32];
    let mut half = true;
    for i in 0..120 {
        let pc = va + i * 4;
        let ins = lib.word(pc)?;
        let (rd, rn) = ((ins & 31) as usize, ((ins >> 5) & 31) as usize);
        if ins & 0xFF80_0000 == 0x5100_0000 && rn == 1 {
            sub = i64::from((ins >> 10) & 0xFFF);
        } else if ins & 0xFF80_001F == 0x7100_001F && count.is_none() {
            count = Some(((ins >> 10) & 0xFFF) as usize + 1);
        } else if ins & 0xFF00_001F == 0x5400_0008 && default.is_none() {
            default = Some(pc.wrapping_add((sext(u64::from((ins >> 5) & 0x7FFFF), 19) * 4) as u64));
        } else if ins & 0x9F00_0000 == 0x9000_0000 {
            x[rd] = adrp(ins, pc);
        } else if ins & 0xFF80_0000 == 0x9100_0000 {
            x[rd] = x[rn].wrapping_add(u64::from((ins >> 10) & 0xFFF));
        } else if ins & 0x9F00_0000 == 0x1000_0000 {
            base = Some(adr(ins, pc));
        } else if ins & 0xFFE0_FC00 == 0x7860_7800 || ins & 0xFFE0_FC00 == 0x3860_7800 {
            half = ins & 0xFFE0_FC00 == 0x7860_7800;
            table = Some(x[rn]);
        } else if ins & 0xFFFF_FC1F == 0xD61F_0000 {
            break;
        }
    }
    let (count, base, table) = (count?, base?, table?);
    let size = if half { 2 } else { 1 };
    let t = lib.bytes(table, count * size)?;
    let targets = (0..count)
        .map(|i| {
            let e = if half {
                u64::from(u16::from_le_bytes([t[i * 2], t[i * 2 + 1]]))
            } else {
                u64::from(t[i])
            };
            base + e * 4
        })
        .collect();
    Some((sub, targets, default?))
}

/// Runs a setup function for each index its `switch` knows.
fn setup(lib: &Lib<'_>, part: &str, balance: &BTreeMap<String, f64>) -> BTreeMap<i32, Stored> {
    let mut out = BTreeMap::new();
    let Some((va, _)) = lib.function(part) else {
        return out;
    };
    let Some((first, targets, default)) = cases(lib, va) else {
        return out;
    };
    for (k, &t) in targets.iter().enumerate() {
        if t == default {
            continue;
        }
        let index = first + k as i64;
        let mut m = Machine::new(lib, balance);
        let this = THIS;
        m.x[0] = this;
        m.x[1] = index as u64;
        if m.run(va, t).is_none() {
            continue;
        }
        let at = this + index as u64 * INFO_SIZE;
        let mut words = BTreeMap::new();
        for off in (0..INFO_SIZE).step_by(4) {
            if (0..4).any(|b| m.mem.contains_key(&(at + off + b))) {
                let b = m.load(at + off, 4);
                words.insert(off, u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            }
        }
        out.insert(
            index as i32,
            Stored {
                name: m.name,
                words,
                keys: m.keys,
            },
        );
    }
    out
}

/// `Game::LoadCharacters`: each `CharacterInfo_Load(index, "file")`, set up
/// as `adrp/add x2, "file"; mov w1, #index` before the shared call.
fn files(lib: &Lib<'_>) -> BTreeMap<i32, String> {
    let mut out = BTreeMap::new();
    let Some((va, size)) = lib.function("Game14LoadCharacters") else {
        return out;
    };
    let (mut x2, mut w1) = (None, None);
    for pc in (va..va + size).step_by(4) {
        let Some(ins) = lib.word(pc) else { break };
        let (rd, rn) = ((ins & 31) as usize, ((ins >> 5) & 31) as usize);
        if ins & 0x9F00_001F == 0x9000_0002 {
            x2 = Some(adrp(ins, pc));
        } else if ins & 0xFF80_03FF == 0x9100_0042 && rd == 2 && rn == 2 {
            x2 = x2.map(|p| p + u64::from((ins >> 10) & 0xFFF));
        } else if ins & 0xFFE0_001F == 0x5280_0001 {
            w1 = Some(((ins >> 5) & 0xFFFF) as i32);
        } else if ins & 0xFC00_0000 == 0x1400_0000 || ins & 0xFC00_0000 == 0x9400_0000 {
            if let (Some(i), Some(f)) = (w1.take(), x2.take().and_then(|a| lib.cstr(a))) {
                out.entry(i).or_insert(f);
            }
        }
    }
    out
}

/// The characters of a mobile game's library (`libdinHunter.so`,
/// `libiceAge.so`), with `balance` its tuning values
/// ([`crate::mobile_values::defaults`]).
pub fn characters(lib: &[u8], balance: &BTreeMap<String, f64>) -> Result<Characters, FormatError> {
    let lib = Lib::new(lib).ok_or_else(|| bad("elf"))?;
    // The game's own set: whichever `CharacterInfo_Init` calls.
    let init = lib
        .function("CharacterInfo_Init")
        .ok_or_else(|| bad("CharacterInfo_Init"))?;
    let called: Vec<String> = (init.0..init.0 + init.1)
        .step_by(4)
        .filter_map(|pc| {
            let ins = lib.word(pc)?;
            (ins & 0xFC00_0000 == 0x9400_0000).then(|| {
                let t = pc.wrapping_add((sext(u64::from(ins & 0x03FF_FFFF), 26) * 4) as u64);
                lib.plt.get(&t).cloned().unwrap_or_default()
            })
        })
        .collect();
    let params = called
        .iter()
        .find(|n| n.contains("SetDefaultParams"))
        .map_or("SetDefaultParams_DH", |n| {
            if n.contains("_IA") {
                "SetDefaultParams_IA"
            } else {
                "SetDefaultParams_DH"
            }
        });
    Ok(Characters {
        params: setup(&lib, params, balance),
        weapons: setup(&lib, "SetDefaultWeaponsParams", balance),
        files: files(&lib),
    })
}

/// A character's `_RES.TXT` values, from where `CharacterInfo` keeps them
/// (matched field by field against the PC's for the animals both have).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Animal {
    pub health: f32,
    pub mass: f32,
    pub length: f32,
    pub radius: f32,
    pub score: f32,
    pub smell: f32,
    pub hear: f32,
    pub look: f32,
    pub ship_delta: f32,
    /// `scale0` and `scaleA` over 1000.
    pub scale0: f32,
    pub scale_a: f32,
}

impl Stored {
    pub fn animal(&self) -> Animal {
        let f = |o: u64| self.f32(o).unwrap_or(0.0);
        Animal {
            health: f(0x1630),
            mass: f(0x1634),
            length: f(0x1638),
            radius: f(0x163c),
            score: f(0x1640),
            smell: f(0x1644),
            hear: f(0x1648),
            look: f(0x164c),
            ship_delta: f(0x1650),
            scale0: f(0x1654),
            scale_a: f(0x1658),
        }
    }

    pub fn weapon(&self) -> Weapon {
        let f = |o: u64| self.f32(o).unwrap_or(0.0);
        let i = |o: u64| self.i32(o).unwrap_or(0);
        Weapon {
            power: f(0x1664),
            prec: f(0x1668),
            trace: i(0x166c),
            loud: f(0x1670),
            rate: f(0x1674),
            shots: i(0x1678),
            reload: i(0x167c),
            zoom: f(0x1680),
        }
    }
}

/// A weapon's `_RES.TXT` values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Weapon {
    pub power: f32,
    pub prec: f32,
    /// Pellets a shot fires.
    pub trace: i32,
    pub loud: f32,
    pub rate: f32,
    pub shots: i32,
    /// Rounds a reload loads (the magazine).
    pub reload: i32,
    /// Its aim's field of view, degrees (the sniper rifle's narrower: a scope).
    pub zoom: f32,
}

/// `localizations/strings*.csv` (`key,en,de,fr,es`; Latin-1): each key's
/// English text.
pub fn strings(csv: &[u8]) -> BTreeMap<String, String> {
    let text: String = csv.iter().map(|&b| char::from(b)).collect();
    let mut out = BTreeMap::new();
    for line in text.lines().skip(1) {
        let Some((key, rest)) = line.split_once(',') else {
            continue;
        };
        let en = match rest.strip_prefix('"') {
            Some(q) => {
                // Up to the closing quote; `""` is a quote.
                let mut s = String::new();
                let mut it = q.chars().peekable();
                while let Some(c) = it.next() {
                    if c == '"' {
                        if it.peek() == Some(&'"') {
                            it.next();
                        } else {
                            break;
                        }
                    }
                    s.push(c);
                }
                s
            }
            None => rest.split(',').next().unwrap_or_default().to_string(),
        };
        out.insert(key.trim().to_string(), en);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_take_english() {
        let csv = b"key,en,de\r\nSTR_A,\"Say \"\"hi\"\", T-Rex\",\"x\"\r\nSTR_B,Plain,y\r\n";
        let s = strings(csv);
        assert_eq!(
            s.get("STR_A").map(String::as_str),
            Some("Say \"hi\", T-Rex")
        );
        assert_eq!(s.get("STR_B").map(String::as_str), Some("Plain"));
    }

    #[test]
    fn stored_fields() {
        let mut s = Stored::default();
        s.words.insert(0x1630, 1024.0f32.to_bits());
        s.words.insert(0x1678, 16);
        assert_eq!(s.animal().health, 1024.0);
        assert_eq!(s.weapon().shots, 16);
    }
}
