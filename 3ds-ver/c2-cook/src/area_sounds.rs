//! An area's sounds fitted to the 3DS: its random sounds and ambient loops
//! together take at most a set share of linear memory.
//!
//! The originals' areas carry up to 15 MiB of sound (AREA13: three 45 s
//! loops of 1.9 MiB and 7 MiB of randoms, one 2.3 MiB), against the 22 MiB
//! of linear memory a hunt has for everything. Loops get at most half the
//! share; randoms get what the loops leave. Neither is cut unless the area
//! is over its share.

/// An area's ambient loop at most this long on the 3DS. The originals'
/// are seconds long; one standalone (Fallen Kings) loops minutes of music,
/// 13 MiB of the 22 MiB of video memory a hunt has, so that no hunt on
/// those areas fit ("Too much for this console", owner 2026-11-01).
const MAX_AMBIENT_SECS: u32 = 45;

/// A 16-bit mono loop of `rate` Hz cut to `secs`, its first half second
/// cross-faded from what followed the cut, so the loop runs on without a
/// click. Shorter ones as they are.
fn loop_cut(pcm: &[u8], rate: u32, secs: u32) -> Vec<u8> {
    let s: Vec<i16> = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    let len = (rate * secs) as usize;
    let fade = (rate / 2) as usize;
    if s.len() <= len + fade {
        return pcm.to_vec();
    }
    let mut out: Vec<i16> = s[..len].to_vec();
    for (i, o) in out.iter_mut().take(fade).enumerate() {
        // At the seam the loop goes from s[len - 1] to out[0] = s[len].
        let a = i as f32 / fade as f32;
        *o = (f32::from(s[i]) * a + f32::from(s[len + i]) * (1.0 - a)) as i16;
    }
    out.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// No loop is cut shorter than this to fit (a shorter one repeats audibly).
const MIN_LOOP_SECS: u32 = 8;
/// No random sound is cut shorter than this to fit.
const MIN_RANDOM_SECS: f32 = 0.5;
/// A random sound at most this long, however much room there is.
const MAX_RANDOM_SECS: f32 = 4.0;
/// A cut random sound fades out over this long, so it ends without a click.
const FADE_OUT_SECS: f32 = 0.1;

/// Bytes `loop_cut` leaves of a loop of `len` bytes cut to `secs`.
fn loop_bytes(len: usize, rate: u32, secs: u32) -> usize {
    let cut = (rate * secs) as usize * 2;
    if len <= cut + rate as usize {
        len
    } else {
        cut
    }
}

/// `pcm` (16-bit mono) cut to `secs`, fading out over its last 0.1 s.
fn clip_cut(pcm: &[u8], rate: u32, secs: f32) -> Vec<u8> {
    let len = ((rate as f32 * secs) as usize) * 2;
    if pcm.len() <= len {
        return pcm.to_vec();
    }
    let mut out = pcm[..len].to_vec();
    let fade = ((rate as f32 * FADE_OUT_SECS) as usize).min(len / 2);
    for i in 0..fade {
        let at = len - (fade - i) * 2;
        let v = i16::from_le_bytes([out[at], out[at + 1]]);
        let k = 1.0 - (i + 1) as f32 / fade as f32;
        out[at..at + 2].copy_from_slice(&((f32::from(v) * k) as i16).to_le_bytes());
    }
    out
}

/// The random sounds and loops (16-bit mono PCM at `rate`) cut so that
/// together they take at most `budget` bytes where that is possible
/// (loops down to 8 s each, randoms down to half a second).
pub fn fit(
    randoms: Vec<Vec<u8>>,
    loops: Vec<Vec<u8>>,
    rate: u32,
    budget: usize,
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    // Loops: the longest cut (at most the standing 45 s) within half.
    let mut secs = MAX_AMBIENT_SECS;
    let loops_at = |s: u32| -> usize { loops.iter().map(|l| loop_bytes(l.len(), rate, s)).sum() };
    while secs > MIN_LOOP_SECS && loops_at(secs) > budget / 2 {
        secs -= 1;
    }
    let loops_used = loops_at(secs);
    let loops: Vec<Vec<u8>> = loops.iter().map(|l| loop_cut(l, rate, secs)).collect();
    // Randoms: each at most 4 s, shortened together until they fit.
    let left = budget.saturating_sub(loops_used);
    let mut cap = MAX_RANDOM_SECS;
    let cap_bytes = |c: f32| -> usize {
        randoms
            .iter()
            .map(|r| r.len().min(((rate as f32 * c) as usize) * 2))
            .sum()
    };
    while cap > MIN_RANDOM_SECS && cap_bytes(cap) > left {
        cap -= 0.25;
    }
    let randoms = randoms.iter().map(|r| clip_cut(r, rate, cap)).collect();
    (randoms, loops)
}

/// For each sound, the first earlier one with the same samples (`None`:
/// the first of its kind, or silence). Areas reuse a sound for several
/// slots; the pack holds it once, with `.SAME` files naming it for the
/// others, and the 3DS loads it once.
pub fn same_as(sounds: &[&[u8]]) -> Vec<Option<usize>> {
    (0..sounds.len())
        .map(|i| {
            if sounds[i].is_empty() {
                return None;
            }
            (0..i).find(|&j| sounds[j] == sounds[i])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_sounds_point_at_their_first_copy() {
        let (a, b, none): (&[u8], &[u8], &[u8]) = (&[1, 2, 3, 4], &[1, 2, 3, 5], &[]);
        let same = same_as(&[a, b, a, none, b, a, none]);
        assert_eq!(
            same,
            vec![None, None, Some(0), None, Some(1), Some(0), None]
        );
    }

    #[test]
    fn long_loops_are_cut_without_a_seam() {
        let pcm = |v: &[i16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        // 10 Hz, 3 s cap, half a second (5 samples) of fade.
        let short: Vec<i16> = (0..35).collect();
        assert_eq!(loop_cut(&pcm(&short), 10, 3), pcm(&short));
        let long: Vec<i16> = (0..100).map(|i| i * 10).collect();
        let out = loop_cut(&pcm(&long), 10, 3);
        assert_eq!(out.len(), 30 * 2);
        let first = i16::from_le_bytes([out[0], out[1]]);
        // The loop's first sample follows its last as the source did.
        assert_eq!(first, long[30]);
        assert_eq!(&out[10..], &pcm(&long[5..30])[..]);
    }

    fn pcm(secs: f32, rate: u32) -> Vec<u8> {
        vec![7u8; ((rate as f32 * secs) as usize) * 2]
    }

    #[test]
    fn an_area_under_its_share_is_untouched() {
        let (r, l) = fit(vec![pcm(1.0, 100)], vec![pcm(10.0, 100)], 100, 1 << 20);
        assert_eq!(r[0].len(), pcm(1.0, 100).len());
        assert_eq!(l[0].len(), pcm(10.0, 100).len());
    }

    #[test]
    fn an_area_over_its_share_is_cut_to_fit() {
        // AREA13-like: three 45 s loops, randoms up to 20 s, at 100 Hz.
        let loops = vec![pcm(45.0, 100); 3];
        let randoms = vec![pcm(20.0, 100); 8];
        let budget = 5000 * 2; // 50 s of audio
        let (r, l) = fit(randoms, loops, 100, budget);
        let total: usize = r.iter().chain(&l).map(Vec::len).sum();
        assert!(total <= budget, "{total} > {budget}");
        // Loops took at most half; each still at least 8 s.
        assert!(l.iter().all(|x| x.len() >= 8 * 100 * 2));
        assert!(l.iter().map(Vec::len).sum::<usize>() <= budget / 2);
        // Randoms are cut, not dropped.
        assert_eq!(r.len(), 8);
        assert!(r.iter().all(|x| !x.is_empty()));
    }

    #[test]
    fn random_sounds_are_capped_and_fade_out() {
        let (r, _) = fit(vec![vec![100u8; 2000]], Vec::new(), 100, 1 << 20);
        assert_eq!(r[0].len(), 4 * 100 * 2);
        // Last sample silent, the fade reaches zero.
        assert_eq!(&r[0][r[0].len() - 2..], &[0, 0]);
    }
}
