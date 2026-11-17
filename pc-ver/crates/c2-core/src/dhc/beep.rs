//! The terminal's UI beeps: which one an action makes, and the sound
//! itself, synthesized (no game file). Each platform plays the PCM through
//! its own sound path.

use alloc::vec::Vec;

use super::terminal::{Action, Screen, Terminal};

/// A UI sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Beep {
    /// The focus moved (keys or pad, not the pointer's hover).
    Tick,
    /// Something was picked, opened or switched.
    Select,
    /// Back to the screen before.
    Back,
    /// Refused: a notice came up.
    Refuse,
    /// The deploy sequence started.
    Deploy,
}

impl Beep {
    pub const ALL: [Beep; 5] = [
        Beep::Tick,
        Beep::Select,
        Beep::Back,
        Beep::Refuse,
        Beep::Deploy,
    ];

    /// The tones: (frequency in Hz, seconds), played one after another.
    fn tones(self) -> &'static [(f32, f32)] {
        match self {
            Beep::Tick => &[(1760.0, 0.018)],
            Beep::Select => &[(880.0, 0.045), (1320.0, 0.04)],
            Beep::Back => &[(990.0, 0.04), (660.0, 0.05)],
            Beep::Refuse => &[(196.0, 0.16)],
            Beep::Deploy => &[(660.0, 0.06), (880.0, 0.06), (1320.0, 0.12)],
        }
    }

    /// Mono 16-bit PCM at `rate` Hz: square-ish tones (a sine with its
    /// third harmonic, as an old terminal's speaker), each with a short
    /// attack and release so there are no clicks.
    pub fn pcm(self, rate: u32) -> Vec<i16> {
        let rate = rate.max(1) as f32;
        let mut out = Vec::new();
        for &(f, secs) in self.tones() {
            let n = (secs * rate) as usize;
            let edge = (0.004 * rate).max(1.0);
            for i in 0..n {
                let t = i as f32 / rate;
                let ph = 2.0 * core::f32::consts::PI * f * t;
                let s = libm::sinf(ph) + libm::sinf(3.0 * ph) / 3.0;
                let env = (i as f32 / edge).min((n - i) as f32 / edge).min(1.0);
                out.push((s * env * 0.25 * f32::from(i16::MAX)) as i16);
            }
        }
        out
    }
}

/// What a beep is decided from: the terminal's state an action may change.
#[derive(PartialEq)]
struct Seen {
    screen: Screen,
    focus: (super::Tab, [usize; 4], usize, usize, usize),
    loadout: super::Loadout,
    refusals: u32,
}

fn seen(t: &Terminal) -> Seen {
    Seen {
        screen: t.screen,
        focus: (
            t.tab,
            t.rows,
            t.main_row,
            t.survival_row(),
            t.specimen_row(),
        ),
        loadout: t.loadout(),
        refusals: t.refusals(),
    }
}

impl Terminal {
    /// [`Terminal::act`], and the beep it makes (none when nothing
    /// changed, or the pointer only hovered). An outcome left for the
    /// platform counts as a pick; [`Terminal::take_outcome`] still has it.
    pub fn act_beep(&mut self, a: Action) -> Option<Beep> {
        let before = seen(self);
        let had_outcome = self.has_outcome();
        self.act(a);
        let after = seen(self);
        if after.refusals != before.refusals {
            return Some(Beep::Refuse);
        }
        if after.screen != before.screen {
            return Some(match after.screen {
                Screen::Deploy => Beep::Deploy,
                Screen::Main if before.screen != Screen::Boot => Beep::Back,
                _ if before.screen == Screen::Specimen => Beep::Back,
                Screen::Setup if before.screen == Screen::Deploy && !self.has_outcome() => {
                    Beep::Back
                }
                _ => Beep::Select,
            });
        }
        if self.has_outcome() && !had_outcome || after.loadout != before.loadout {
            return Some(Beep::Select);
        }
        if after.focus != before.focus && !matches!(a, Action::Focus(_)) {
            return Some(Beep::Tick);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_beep_is_short_and_not_silent() {
        for b in Beep::ALL {
            let p = b.pcm(22050);
            assert!(!p.is_empty() && p.len() < 22050 / 2, "{b:?}: {}", p.len());
            assert!(
                p.iter().any(|&s| s.unsigned_abs() > 4000),
                "{b:?} too quiet"
            );
            // Soft edges: no click at either end.
            assert!(p[0].unsigned_abs() < 2000 && p[p.len() - 1].unsigned_abs() < 2000);
        }
        assert_eq!(
            Beep::Tick.pcm(22050),
            Beep::Tick.pcm(22050),
            "the same each time"
        );
    }
}
