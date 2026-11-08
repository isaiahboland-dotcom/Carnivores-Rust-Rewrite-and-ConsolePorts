//! The original hunt executable's globals that a kill run needs, found in
//! its own code (both shipped software renderers, Carnivores 2's and Ice
//! Age's, are built from the released source with the same layout).
//!
//! - `MyHealth`: the variable most often set to `MAX_HEALTH` (128000) with
//!   `mov dword [abs], 128000` (`ProcessSyncro` and the hunt's start).
//! - `AddDeadBody` starts `mov eax, [MyHealth]; sub esp, 12` (`if
//!   (!MyHealth) return;`). In it: the first `mov ecx, [abs]` is `ChCount`,
//!   the next `mov edx, [abs]` is `CameraAlpha`, the first `lea ecx,
//!   [eax+abs]` is `&Characters[0]` (`Characters[ChCount].CType`), and the
//!   body's `pos` (TCharacter offsets 0x5C, 0x60, 0x64) is stored from
//!   registers loaded from `PlayerX`, `PlayerY` and `PlayerZ` (no killer).
//!
//! The results are checked: `alpha` (0x78) is stored from `CameraAlpha`,
//! and the three player globals lie within 16 bytes of each other.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

/// `TCharacter` offsets (Hunt.h).
const POS: u32 = 0x5C;
const ALPHA: u32 = 0x78;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Addrs {
    pub health: u32,
    pub characters: u32,
    pub count: u32,
    pub player: [u32; 3],
    pub camera_alpha: u32,
}

fn u32_at(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

/// Offsets where `pat` matches, `None` in `pat` matching any byte.
fn find(d: &[u8], pat: &[Option<u8>]) -> Vec<usize> {
    (0..d.len().saturating_sub(pat.len()))
        .filter(|&i| {
            pat.iter()
                .enumerate()
                .all(|(j, p)| p.is_none_or(|b| d[i + j] == b))
        })
        .collect()
}

fn bytes(b: &[u8]) -> Vec<Option<u8>> {
    b.iter().copied().map(Some).collect()
}

impl Addrs {
    pub fn scan(d: &[u8]) -> Result<Self> {
        let mut counts: HashMap<u32, usize> = HashMap::new();
        let mut pat = bytes(&[0xC7, 0x05]);
        pat.extend([None; 4]);
        pat.extend(bytes(&[0x00, 0xF4, 0x01, 0x00]));
        for i in find(d, &pat) {
            *counts.entry(u32_at(d, i + 2).unwrap_or(0)).or_default() += 1;
        }
        let health = counts
            .into_iter()
            .max_by_key(|&(a, n)| (n, a))
            .map(|(a, _)| a)
            .context("no `mov [abs], 128000`: not a Carnivores hunt executable")?;
        let mut start = bytes(&[0xA1]);
        start.extend(bytes(&health.to_le_bytes()));
        start.extend(bytes(&[0x83, 0xEC, 0x0C]));
        let f = match find(d, &start).as_slice() {
            [f] => *f,
            other => bail!("AddDeadBody: {} candidates", other.len()),
        };
        let w = &d[f..d.len().min(f + 0x300)];
        let at = |p: &[u8]| find(w, &bytes(p)).first().copied();
        let count_at = at(&[0x8B, 0x0D]).context("AddDeadBody: no ChCount load")?;
        let count = u32_at(w, count_at + 2).context("truncated")?;
        let camera_alpha = match w.get(count_at + 6..count_at + 8) {
            Some([0x8B, 0x15]) => u32_at(w, count_at + 8).context("truncated")?,
            _ => bail!("AddDeadBody: no CameraAlpha load after ChCount"),
        };
        let chars_at = at(&[0x8D, 0x88]).context("AddDeadBody: no &Characters[ChCount]")?;
        let characters = u32_at(w, chars_at + 2).context("truncated")?;
        let player = player_globals(w, characters)?;
        let stores = stores_from_globals(w);
        if !stores.contains(&(characters + ALPHA, camera_alpha)) {
            bail!("AddDeadBody: the body's alpha is not set from {camera_alpha:#x}");
        }
        Ok(Self {
            health,
            characters,
            count,
            player,
            camera_alpha,
        })
    }

    /// The hook script's `addr` lines (`original/kill.c`).
    pub fn script_lines(&self) -> String {
        let [x, y, z] = self.player;
        format!(
            "0 addr health {:#x}\n0 addr characters {:#x}\n0 addr count {:#x}\n\
             0 addr player_x {x:#x}\n0 addr player_y {y:#x}\n0 addr player_z {z:#x}\n\
             0 addr camera_alpha {:#x}\n",
            self.health, self.characters, self.count, self.camera_alpha
        )
    }
}

/// `(destination, source global)` for each `mov [reg+disp32], reg` whose
/// register was last loaded with `mov reg, [abs]` (or `mov eax, [abs]`).
fn stores_from_globals(w: &[u8]) -> Vec<(u32, u32)> {
    let mut regs: [Option<u32>; 8] = [None; 8];
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 <= w.len() {
        let (op, m) = (w[i], w[i + 1]);
        let reg = usize::from((m >> 3) & 7);
        if op == 0xA1 {
            regs[0] = u32_at(w, i + 1);
            i += 5;
        } else if op == 0x8B && m & 0xC7 == 0x05 {
            regs[reg] = u32_at(w, i + 2);
            i += 6;
        } else if op == 0x89 && m >> 6 == 2 && m & 7 != 4 {
            if let (Some(src), Some(dst)) = (regs[reg], u32_at(w, i + 2)) {
                out.push((dst, src));
            }
            i += 6;
        } else {
            i += 1;
        }
    }
    out
}

/// `PlayerX`, `PlayerY`, `PlayerZ`: the sources of the body's `pos` stores.
fn player_globals(w: &[u8], characters: u32) -> Result<[u32; 3]> {
    let stores = stores_from_globals(w);
    let mut p = [0u32; 3];
    for (k, slot) in p.iter_mut().enumerate() {
        let dst = characters + POS + 4 * k as u32;
        *slot = stores
            .iter()
            .find(|s| s.0 == dst)
            .map(|s| s.1)
            .with_context(|| format!("AddDeadBody: no store of the body's pos[{k}]"))?;
    }
    let (lo, hi) = (p.iter().min().unwrap_or(&0), p.iter().max().unwrap_or(&0));
    if hi - lo > 16 || p[0] == p[1] || p[1] == p[2] || p[0] == p[2] {
        bail!("AddDeadBody: unlikely player globals {p:#x?}");
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    /// A made-up executable with the shapes the scan looks for (not the
    /// game's code): the health resets, then `AddDeadBody`.
    fn image(order: [usize; 3]) -> Vec<u8> {
        let (health, count, cam, chars) = (0x0180_0000u32, 0x0140_0000, 0x0160_0000, 0x0044_0000);
        let player = [0x0150_0008u32, 0x0150_0004, 0x0150_0000];
        let mut d = vec![0x90u8; 64];
        for _ in 0..3 {
            d.extend([0xC7, 0x05]);
            d.extend(le(health));
            d.extend([0x00, 0xF4, 0x01, 0x00]);
        }
        // A decoy reset of something else.
        d.extend([0xC7, 0x05]);
        d.extend(le(0x0170_0000));
        d.extend([0x00, 0xF4, 0x01, 0x00]);
        d.push(0xA1);
        d.extend(le(health));
        d.extend([0x83, 0xEC, 0x0C, 0x8B, 0x0D]);
        d.extend(le(count));
        d.extend([0x8B, 0x15]);
        d.extend(le(cam));
        d.extend([0x8D, 0x88]);
        d.extend(le(chars));
        d.extend([0x89, 0x90]);
        d.extend(le(chars + ALPHA));
        // The body's pos from the player globals, through edx (2) and
        // eax (0), in the order given.
        for k in order {
            d.extend([0x8B, 0x15]);
            d.extend(le(player[k]));
            d.extend([0x89, 0x96]);
            d.extend(le(chars + POS + 4 * k as u32));
        }
        d.push(0xA1);
        d.extend(le(0x0190_0000));
        d.extend([0xC3; 32]);
        d
    }

    #[test]
    fn the_globals_are_found() {
        for order in [[0, 1, 2], [2, 0, 1]] {
            let a = Addrs::scan(&image(order)).unwrap();
            assert_eq!(a.health, 0x0180_0000);
            assert_eq!((a.count, a.camera_alpha), (0x0140_0000, 0x0160_0000));
            assert_eq!(a.characters, 0x0044_0000);
            assert_eq!(a.player, [0x0150_0008, 0x0150_0004, 0x0150_0000]);
        }
        let lines = Addrs::scan(&image([0, 1, 2])).unwrap().script_lines();
        assert!(lines.contains("0 addr player_z 0x1500000\n"));
    }

    #[test]
    fn other_files_are_refused() {
        assert!(Addrs::scan(&[0u8; 256]).is_err());
        // AddDeadBody without the body's alpha from the camera.
        let mut d = image([0, 1, 2]);
        let i = find(&d, &bytes(&[0x89, 0x90]))[0];
        d[i + 2] ^= 0x04;
        assert!(Addrs::scan(&d).is_err());
    }
}
