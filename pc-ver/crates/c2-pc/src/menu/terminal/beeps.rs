//! The terminal's UI beeps on the PC (`c2_core::dhc::beep`, synthesized).
//! The menu has no sound of its own, so a sound output is opened the
//! first time a beep plays and closed when a hunt starts, which opens its
//! own. Off with the "DHC OS sounds" option.

use c2_core::dhc::beep::Beep;

use crate::audio::{Audio, Clip, SAMPLE_RATE};

/// Unpositioned, at the original's full voice volume (`AddVoicev`).
const VOLUME: i32 = 256;

pub(crate) struct Beeper {
    audio: Audio,
    /// One per [`Beep::ALL`].
    clips: Vec<Option<Clip>>,
}

impl Beeper {
    fn new() -> Self {
        let clips = Beep::ALL
            .iter()
            .map(|b| {
                let pcm: Vec<u8> = b
                    .pcm(SAMPLE_RATE)
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
                Clip::from_pcm(&pcm, SAMPLE_RATE, 1)
            })
            .collect();
        Beeper {
            audio: Audio::new(),
            clips,
        }
    }

    fn play(&mut self, b: Beep) {
        let clip = Beep::ALL
            .iter()
            .position(|&x| x == b)
            .and_then(|i| self.clips.get(i))
            .and_then(Option::as_ref);
        self.audio.play(clip, None, VOLUME);
    }
}

impl super::Menu {
    /// Plays beep `b`, when there is one and the option is on.
    pub(in crate::menu) fn beep(&mut self, b: Option<Beep>) {
        let Some(b) = b.filter(|_| self.pc.dhc_beeps != 0) else {
            return;
        };
        self.beeper.get_or_insert_with(Beeper::new).play(b);
    }
}
