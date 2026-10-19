//! What to play, shared by the front ends: which sound each game event
//! starts (`AddVoice*` call sites in the original) and how loud it is in
//! each ear (`CalcLRVolumes`). The front ends own the samples and the mixer.

use crate::dino::{hunt, DinoEvent};
use crate::game::{GameEvent, Hunt};
use crate::player::PlayerEvent;
use crate::ship::ShipEvent;

/// `MIN_RADIUS` in `Audio.h`.
pub const MIN_RADIUS: f32 = 512.0;
/// `MAX_CHANNEL`: voices playing at once; more are dropped.
pub const MAX_VOICES: usize = 16;
/// Every vanilla sound is 16-bit mono at this rate.
pub const SAMPLE_RATE: u32 = 22050;

/// A sound by where it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundId {
    /// A character's `.CAR` sound (by `ctype`).
    Species {
        ctype: usize,
        sound: usize,
    },
    /// A weapon's `.CAR` sound.
    Weapon {
        weapon: usize,
        slot: usize,
    },
    /// `SHIP2A.CAR` sound (`ship_sound::*`).
    Ship(usize),
    /// The area's random sounds (`.RSC`).
    Random(usize),
    /// `SOUNDFX/STEPS/HWALKn.WAV`, `FOOTWn.WAV` (n = 1..=3).
    Step(usize),
    StepWater(usize),
    /// `SOUNDFX/HUM_DIEn.WAV` (n = 1..=4).
    Scream(usize),
    /// `SOUNDFX/CALLS/CALL{ai-9}_{A,B,C}.WAV`.
    Call {
        ai: i32,
        variant: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundCmd {
    /// `AddVoice3dv` / `AddVoicev`: `pos = None` plays without position;
    /// `volume` 0..=256.
    Play {
        id: SoundId,
        pos: Option<[f32; 3]>,
        volume: i32,
    },
    /// `SetAmbient`: crossfade to the area's ambience `Some(zone)`, or the
    /// underwater loop (`A_UNDERW.WAV`) for `None`.
    Ambient { zone: Option<usize>, volume: i32 },
}

/// The sounds this tick's `events` start.
pub fn commands(events: &[GameEvent], h: &Hunt) -> Vec<SoundCmd> {
    let r3 = (h.real_time % 3) as usize;
    let play = |id, pos, volume| SoundCmd::Play { id, pos, volume };
    events
        .iter()
        .filter_map(|e| {
            Some(match e {
                GameEvent::WeaponSound { weapon, slot } => play(
                    SoundId::Weapon {
                        weapon: *weapon,
                        slot: *slot,
                    },
                    None,
                    256,
                ),
                GameEvent::Dino(DinoEvent::Sound { index, sound, pos }) => {
                    let ctype = h.herd.chars.get(*index)?.ctype;
                    play(
                        SoundId::Species {
                            ctype,
                            sound: *sound,
                        },
                        Some(*pos),
                        256,
                    )
                }
                GameEvent::Player(PlayerEvent::Step) => play(
                    SoundId::Step(r3),
                    None,
                    24 + (h.player.vspeed * 50.0) as i32,
                ),
                GameEvent::Player(PlayerEvent::StepWater) => play(
                    SoundId::StepWater(r3),
                    None,
                    64 + (h.player.vspeed * 30.0) as i32,
                ),
                GameEvent::Player(PlayerEvent::Land) => play(SoundId::Step(r3), None, 64),
                GameEvent::HunterDied { kind } if *kind != hunt::BREATH => {
                    play(SoundId::Scream((h.real_time % 4) as usize), None, 256)
                }
                GameEvent::Ship(ShipEvent::Sound { slot, at }) => {
                    play(SoundId::Ship(*slot), *at, 256)
                }
                GameEvent::RandomSound { sound, pos, volume } => {
                    play(SoundId::Random(*sound), Some(*pos), *volume)
                }
                GameEvent::Call { call, variant, pos } => play(
                    SoundId::Call {
                        ai: *call,
                        variant: *variant,
                    },
                    *pos,
                    256,
                ),
                GameEvent::Ambient { zone, volume } => SoundCmd::Ambient {
                    zone: *zone,
                    volume: *volume,
                },
                _ => return None,
            })
        })
        .collect()
}

/// `CalcLRVolumes` as linear (left, right) amplitudes for volume `v0`
/// (0..=256), heard from `cam` facing `alpha`. `pos = None` is a
/// non-positional voice.
pub fn lr_gains(v0: f32, pos: Option<[f32; 3]>, cam: [f32; 3], alpha: f32) -> (f32, f32) {
    let Some(p) = pos else {
        let g = v0 * 180.0 / 65536.0;
        return (g, g);
    };
    let v0 = v0 * 200.0;
    let (x, y, z) = (p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]);
    let (sa, ca) = alpha.sin_cos();
    let xx = x * ca + z * sa;
    let zz = (z * ca - x * sa).abs();
    let d = (xx * xx + y * y + zz * zz).sqrt() - MIN_RADIUS;
    let mut k = if d <= 0.0 { 1.0 } else { 1224.0 / (1224.0 + d) };
    if d > 6000.0 {
        k *= (4000.0 - (d - 6000.0)) / 4000.0;
    }
    let k = k.max(0.0);
    let fi = xx.abs().atan2(zz);
    let r = 0.7 + 0.3 * fi / (std::f32::consts::PI / 2.0);
    let l = 0.7 - 0.6 * fi / (std::f32::consts::PI / 2.0);
    let (near, far) = (v0 * r * k / 65536.0, v0 * l * k / 65536.0);
    if xx > 0.0 {
        (far, near)
    } else {
        (near, far)
    }
}

/// `SetAmbient`'s level for `volume` (0..=256), as a linear amplitude.
pub fn ambient_gain(volume: i32) -> f32 {
    32000.0 * volume as f32 / 256.0 / 65536.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gains_follow_the_original_curve() {
        let cam = [0.0; 3];
        // Non-positional: 180/65536 per unit of volume, both sides.
        assert_eq!(
            lr_gains(256.0, None, cam, 0.0),
            (256.0 * 180.0 / 65536.0, 256.0 * 180.0 / 65536.0)
        );
        // Close and dead ahead (-z): full k, equal gains (l = r = 0.7).
        let (l, r) = lr_gains(256.0, Some([0.0, 0.0, -100.0]), cam, 0.0);
        assert!((l - r).abs() < 1e-6 && (l - 256.0 * 200.0 * 0.7 / 65536.0).abs() < 1e-4);
        // To the right (+x): louder right.
        let (l, r) = lr_gains(256.0, Some([2000.0, 0.0, 0.0]), cam, 0.0);
        assert!(r > l);
        // Beyond 10000 + MIN_RADIUS: silent.
        assert_eq!(
            lr_gains(256.0, Some([0.0, 0.0, -11000.0]), cam, 0.0),
            (0.0, 0.0)
        );
    }
}
