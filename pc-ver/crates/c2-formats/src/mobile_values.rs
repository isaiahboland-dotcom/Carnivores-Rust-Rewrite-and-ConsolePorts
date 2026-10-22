//! The mobile games' tuning values (`DINO_T_REX_PRICE`, `WEAPON_SHOTGUN_SHOTS`
//! ...), read from the game's native library in the user's APK. They are
//! not in any data file: `BalanceManager_SetDefaultValues...` stores them
//! one by one into a table of `{ char key[32]; double value; }`. For each
//! entry the function puts the key's address in a register (`adrp` + `add`)
//! and then stores the value at offset 0x20 (`str`), the value made with
//! `mov`/`movk` (or `fmov`, or `xzr` for 0). This decodes just those
//! AArch64 instructions, enough to follow it.

use std::collections::BTreeMap;

use crate::error::{FormatError, FormatErrorKind};

fn bad(what: &str) -> FormatError {
    FormatError {
        offset: 0,
        context: what.into(),
        kind: FormatErrorKind::Syntax("not the expected library".into()),
    }
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
}

/// An ELF64 section: type, address, file offset, size, link.
struct Section {
    kind: u32,
    addr: u64,
    off: usize,
    size: usize,
    link: usize,
}

fn sections(d: &[u8]) -> Option<Vec<Section>> {
    if d.get(..5)? != b"\x7fELF\x02" {
        return None;
    }
    let shoff = usize::try_from(u64_at(d, 0x28)?).ok()?;
    let (entsize, num) = (usize::from(u16_at(d, 0x3A)?), usize::from(u16_at(d, 0x3C)?));
    (0..num)
        .map(|i| {
            let o = shoff + i * entsize;
            Some(Section {
                kind: u32_at(d, o + 4)?,
                addr: u64_at(d, o + 16)?,
                off: usize::try_from(u64_at(d, o + 24)?).ok()?,
                size: usize::try_from(u64_at(d, o + 32)?).ok()?,
                link: usize::try_from(u32_at(d, o + 40)?).ok()?,
            })
        })
        .collect()
}

/// The file bytes at a virtual address (not in a `NOBITS` section).
fn at_va<'a>(d: &'a [u8], secs: &[Section], va: u64) -> Option<&'a [u8]> {
    let s = secs
        .iter()
        .find(|s| s.addr != 0 && s.kind != 8 && va >= s.addr && va < s.addr + s.size as u64)?;
    d.get(s.off + usize::try_from(va - s.addr).ok()?..s.off + s.size)
}

/// `(name, address, size)` of every symbol.
fn symbols(d: &[u8], secs: &[Section]) -> Vec<(String, u64, u64)> {
    let mut out = Vec::new();
    for s in secs.iter().filter(|s| s.kind == 2 || s.kind == 11) {
        let Some(strs) = secs.get(s.link) else {
            continue;
        };
        for o in (s.off..s.off + s.size).step_by(24) {
            let (Some(name), Some(value), Some(size)) =
                (u32_at(d, o), u64_at(d, o + 8), u64_at(d, o + 16))
            else {
                continue;
            };
            let start = strs.off + name as usize;
            let Some(bytes) = d.get(start..strs.off + strs.size) else {
                continue;
            };
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            out.push((
                String::from_utf8_lossy(&bytes[..end]).into_owned(),
                value,
                size,
            ));
        }
    }
    out
}

/// A key name (`DINO_T_REX_PRICE`): capitals, digits and `_`.
fn is_key(s: &str) -> bool {
    s.len() >= 4
        && s.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// `DecodeBitMasks` for a 64-bit logical immediate.
fn bitmask(n: u32, immr: u32, imms: u32) -> Option<u64> {
    let combined = (n << 6) | (!imms & 0x3F);
    let len = 31 - combined.leading_zeros();
    if combined == 0 || len < 1 {
        return None;
    }
    let size = 1u32 << len;
    let levels = size - 1;
    let (s, r) = (imms & levels, immr & levels);
    if s == levels {
        return None;
    }
    let ones: u64 = (1u64 << (s + 1)) - 1;
    let mask = if size == 64 {
        u64::MAX
    } else {
        (1u64 << size) - 1
    };
    let elem = if r == 0 {
        ones
    } else {
        ((ones >> r) | (ones << (size - r))) & mask
    };
    let mut v = 0u64;
    let mut i = 0;
    while i < 64 {
        v |= elem << i;
        i += size;
    }
    Some(v)
}

/// `FMOV Dd, #imm8` as a double.
fn fmov_imm(imm8: u32) -> f64 {
    let sign = if imm8 & 0x80 != 0 { -1.0 } else { 1.0 };
    let exp = (((imm8 >> 4) & 7) ^ 4) as i32 - 3;
    sign * (1.0 + f64::from(imm8 & 15) / 16.0) * 2f64.powi(exp)
}

/// Follows the code at `va` (`code`): each store at offset 0x20 records
/// the value for the key whose address was last made. `name` reads the
/// string at an address.
pub fn scan(code: &[u8], va: u64, name: &dyn Fn(u64) -> Option<String>) -> BTreeMap<String, f64> {
    let mut x = [0u64; 32];
    let mut d = [0f64; 32];
    let mut key: Option<String> = None;
    let mut out = BTreeMap::new();
    for (i, w) in code.as_chunks::<4>().0.iter().enumerate() {
        let ins = u32::from_le_bytes(*w);
        let pc = va + i as u64 * 4;
        let rd = (ins & 31) as usize;
        let rn = ((ins >> 5) & 31) as usize;
        if ins & 0x9F00_0000 == 0x9000_0000 {
            // ADRP
            let imm = (((ins >> 5) & 0x7FFFF) << 2 | ((ins >> 29) & 3)) as i64;
            let imm = (imm << 43) >> 43;
            x[rd] = (pc & !0xFFF).wrapping_add((imm << 12) as u64);
        } else if ins & 0xFF80_0000 == 0x9100_0000 {
            // ADD Xd, Xn, #imm{, LSL #12}
            let imm = u64::from((ins >> 10) & 0xFFF) << (12 * ((ins >> 22) & 1));
            x[rd] = x[rn].wrapping_add(imm);
            if rd != 31 {
                if let Some(s) = name(x[rd]).filter(|s| is_key(s)) {
                    key = Some(s);
                }
            }
        } else if ins & 0x7F80_0000 == 0x5280_0000 || ins & 0x7F80_0000 == 0x1280_0000 {
            // MOVZ / MOVN (32 or 64-bit)
            let wide = ins >> 31 == 1;
            let v = u64::from((ins >> 5) & 0xFFFF) << (16 * ((ins >> 21) & 3));
            let v = if ins & 0x7F80_0000 == 0x1280_0000 {
                !v
            } else {
                v
            };
            x[rd] = if wide { v } else { v & 0xFFFF_FFFF };
        } else if ins & 0x7F80_0000 == 0x7280_0000 {
            // MOVK
            let sh = 16 * ((ins >> 21) & 3);
            let v = u64::from((ins >> 5) & 0xFFFF);
            x[rd] = (x[rd] & !(0xFFFFu64 << sh)) | (v << sh);
        } else if ins & 0xFF80_0000 == 0xB200_0000 && rn == 31 {
            // ORR Xd, XZR, #bitmask (MOV)
            if let Some(v) = bitmask((ins >> 22) & 1, (ins >> 16) & 63, (ins >> 10) & 63) {
                x[rd] = v;
            }
        } else if ins & 0xFFE0_1FE0 == 0x1E60_1000 {
            // FMOV Dd, #imm
            d[rd] = fmov_imm((ins >> 13) & 0xFF);
        } else if ins & 0xFFFF_FC00 == 0x9E67_0000 {
            // FMOV Dd, Xn
            d[rd] = f64::from_bits(x[rn]);
        } else if ins & 0xFFC0_0000 == 0xF900_0000 || ins & 0xFFC0_0000 == 0xFD00_0000 {
            // STR Xt / Dt, [Xn, #imm*8]
            if (ins >> 10) & 0xFFF == 4 {
                if let Some(k) = key.take() {
                    let v = if ins & 0xFFC0_0000 == 0xFD00_0000 {
                        d[rd]
                    } else if rd == 31 {
                        0.0
                    } else {
                        f64::from_bits(x[rd])
                    };
                    out.entry(k).or_insert(v);
                }
            }
        }
        if rd == 31 {
            x[31] = 0;
        }
    }
    out
}

/// The default values in a mobile game's native library (an ELF64
/// AArch64 `.so`): its largest `BalanceManager_SetDefaultValues...`.
pub fn defaults(lib: &[u8]) -> Result<BTreeMap<String, f64>, FormatError> {
    let secs = sections(lib).ok_or_else(|| bad("elf"))?;
    let (_, va, size) = symbols(lib, &secs)
        .into_iter()
        .filter(|(n, _, s)| n.contains("BalanceManager_SetDefaultValues") && *s > 16)
        .max_by_key(|s| s.2)
        .ok_or_else(|| bad("BalanceManager_SetDefaultValues"))?;
    let code = at_va(lib, &secs, va)
        .and_then(|c| c.get(..usize::try_from(size).ok()?))
        .ok_or_else(|| bad("code"))?;
    let name = |a: u64| {
        let b = at_va(lib, &secs, a)?;
        let end = b.iter().take(64).position(|&c| c == 0)?;
        std::str::from_utf8(&b[..end]).ok().map(str::to_string)
    };
    Ok(scan(code, va, &name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adrp(rd: u32, pc: u64, target: u64) -> u32 {
        let imm = ((target & !0xFFF) as i64 - (pc & !0xFFF) as i64) >> 12;
        let imm = imm as u32;
        0x9000_0000 | ((imm & 3) << 29) | (((imm >> 2) & 0x7FFFF) << 5) | rd
    }

    #[test]
    fn follows_keys_and_their_stored_values() {
        let base = 0x10000u64;
        let names = |a: u64| match a {
            0x20ef5 => Some("DINO_T_REX_PRICE".to_string()),
            0x20f10 => Some("DINO_T_REX_SCORE".to_string()),
            0x20f40 => Some("WEAPON_PISTOL_RATE".to_string()),
            0x20f60 => Some("STR_PLAY".to_string()),
            _ => None,
        };
        let code = [
            adrp(1, base, 0x20ef5),
            0x9100_0000 | (0xef5 << 10) | (1 << 5) | 1, // add x1, x1, #0xef5
            0xD280_0000 | (3 << 21) | (0x4097 << 5) | 9, // movz x9, #0x4097, lsl 48
            0xF280_0000 | (2 << 21) | (0x7000 << 5) | 9, // movk x9, #0x7000, lsl 32
            0xF900_0000 | (4 << 10) | (19 << 5) | 9,    // str x9, [x19, #0x20]
            adrp(2, base + 20, 0x20f10),
            0x9100_0000 | (0xf10 << 10) | (2 << 5) | 2,
            0x1E60_1000 | (0x70 << 13) | 3,         // fmov d3, #1.0
            0xFD00_0000 | (4 << 10) | (8 << 5) | 3, // str d3, [x8, #0x20]
            adrp(4, base + 36, 0x20f40),
            0x9100_0000 | (0xf40 << 10) | (4 << 5) | 4,
            0xF900_0000 | (4 << 10) | (19 << 5) | 31, // str xzr, [x19, #0x20]
        ];
        let bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
        let v = scan(&bytes, base, &names);
        assert_eq!(v.get("DINO_T_REX_PRICE"), Some(&1500.0));
        assert_eq!(v.get("DINO_T_REX_SCORE"), Some(&1.0));
        assert_eq!(v.get("WEAPON_PISTOL_RATE"), Some(&0.0));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn immediates() {
        assert_eq!(fmov_imm(0x70), 1.0);
        assert_eq!(fmov_imm(0x00), 2.0);
        assert_eq!(fmov_imm(0x60), 0.5);
        // ORR x, xzr, #0x4000000000000000 (2.0): N=1, immr=2, imms=0.
        assert_eq!(bitmask(1, 2, 0), Some(0x4000_0000_0000_0000));
        assert!(defaults(b"not an elf").is_err());
    }
}
