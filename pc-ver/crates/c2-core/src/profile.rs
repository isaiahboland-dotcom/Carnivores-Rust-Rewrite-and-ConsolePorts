//! A new player's profile (`trophy0N.sav`) with the original's defaults
//! (`Menu.cpp`: new player registration, `SetDefaults`).

use c2_formats::sav::{KeyMap, Options, Save, TrophyItem};

/// Default key bindings, as Windows virtual-key codes.
pub fn default_keys() -> KeyMap {
    KeyMap {
        forward: 87,
        backward: 83,
        up: 38,
        down: 40,
        left: 37,
        right: 39,
        fire: 1,
        show: 69,
        strafe_left: 65,
        strafe_right: 68,
        strafe: 17,
        jump: 32,
        run: 16,
        crouch: 88,
        call: 18,
        change_call: 67,
        binoculars: 66,
    }
}

pub fn default_options() -> Options {
    Options {
        agres: 128,
        dens: 128,
        sens: 128,
        res: 1,
        fog: 1,
        text: 1,
        view_r: 128,
        shadows: 1,
        mouse_sens: 128,
        brightness: 128,
        keys: default_keys(),
        ..Default::default()
    }
}

/// A newly registered hunter with `score` points to spend.
pub fn new_player(name: &str, reg: i32, score: i32) -> Save {
    let mut raw = [0u8; 128];
    for (d, b) in raw.iter_mut().zip(name.bytes().take(127)) {
        *d = b;
    }
    Save {
        player_name_raw: raw,
        reg_number: reg,
        score,
        rank: 0,
        last: Default::default(),
        total: Default::default(),
        bodies: [TrophyItem::default(); 24],
        options: default_options(),
    }
}
