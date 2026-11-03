//! Frontend events and hunt kill records.

#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::dino::DinoEvent;
use crate::player::PlayerEvent;
use crate::ship::ShipEvent;
use crate::trace::Hit;

/// Things the frontend should present (sounds, effects, messages).
#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    /// A sound of weapon `weapon`'s model (its number there).
    WeaponSound {
        weapon: usize,
        sound: usize,
    },
    /// The dialect's empty click (`CLICKn.WAV`, 0..3) and a round picked
    /// up (`COLLECTn.WAV`).
    Click(usize),
    Collect(usize),
    Player(PlayerEvent),
    Dino(DinoEvent),
    /// A bullet impact (`AddElements`).
    Impact {
        hit: Hit,
        pos: [f32; 3],
    },
    /// An animal died; `trophy` = huntable species (`AI >= 10`).
    Killed {
        index: usize,
        trophy: bool,
    },
    HunterDied {
        kind: i32,
        /// The hunter screams (a dialect kill type may say not to).
        scream: bool,
    },
    Ship(ShipEvent),
    /// The looping ambience changed: `zone` = RSC ambient index, `None` =
    /// underwater loop. `volume` is `AVolume` (0..=256).
    Ambient {
        zone: Option<usize>,
        volume: i32,
    },
    /// A one-off RSC random sound (`RandSound[sound]`) near the hunter.
    /// A hunting call: `call` is the target AI (10..), `variant` 0..=2 picks
    /// `CALL{call-9}_A/B/C.WAV`; `pos = None` is the hunter's own call.
    Call {
        call: i32,
        variant: usize,
        pos: Option<[f32; 3]>,
    },
    RandomSound {
        sound: usize,
        pos: [f32; 3],
        volume: i32,
    },
    /// A trophy was registered (slot in the trophy room).
    Trophy {
        slot: usize,
        score: i32,
    },
    /// A kill added `score` to the account (species `ctype`); also with the
    /// tranquilizer, where no trophy is kept.
    Scored {
        ctype: usize,
        score: i32,
        /// Distance of the kill, metres (the trophy's "range").
        range: f32,
        tranq: bool,
        /// Night bonus actually awarded, in percent (0, 10 or 20).
        night_bonus: u8,
    },
    Message(String),
}

/// A kill this hunt (the frontends name its parts for the record).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntKill {
    pub ctype: usize,
    pub score: i32,
    /// Metres.
    pub range: f32,
    pub weapon: usize,
}
