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
/// Removes every trophy not locked; how many went.
pub fn clear_unlocked(s: &mut Save) -> usize {
    let mut n = 0;
    for b in s.bodies.iter_mut().filter(|b| b.ctype != 0 && !b.locked()) {
        b.ctype = 0;
        n += 1;
    }
    n
}

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

#[cfg(test)]
mod clear_tests {
    use super::*;

    #[test]
    fn clearing_keeps_locked_trophies() {
        let mut s = new_player("a", 0, 0);
        s.bodies[2].ctype = 3;
        s.bodies[5].ctype = 4;
        s.bodies[5].set_locked(true);
        assert_eq!(clear_unlocked(&mut s), 1);
        assert_eq!((s.bodies[2].ctype, s.bodies[5].ctype), (0, 4));
    }
}
