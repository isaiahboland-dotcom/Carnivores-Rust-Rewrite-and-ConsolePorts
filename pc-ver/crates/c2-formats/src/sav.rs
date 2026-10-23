//! `trophy0N.sav`: the trophy room plus game options.
//! Written by `Game.cpp: SaveTrophy`, read by `LoadTrophy`. 1660 bytes.

use crate::error::FormatError;
use crate::reader::{c_str, Reader, WriteLe};

/// `TTrophyItem`, 64 bytes: one stuffed animal in the trophy room.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TrophyItem {
    /// Character type; 0 = empty slot.
    pub ctype: i32,
    pub weapon: i32,
    pub phase: i32,
    pub height: i32,
    pub weight: i32,
    pub score: i32,
    /// `(year << 20) + (month << 10) + day`.
    pub date: i32,
    /// `(hour << 10) + minute`.
    pub time: i32,
    pub scale: f32,
    /// Distance of the shot / 64.
    pub range: f32,
    pub reserved: [i32; 4],
}

impl TrophyItem {
    /// Ours, in `reserved` (the original never reads it): the area it was
    /// won in (1-based map number; 0 unknown).
    pub fn area(&self) -> i32 {
        self.reserved[0]
    }

    pub fn set_area(&mut self, n: i32) {
        self.reserved[0] = n;
    }

    /// Ours, in `reserved`: locked (can't be removed).
    pub fn locked(&self) -> bool {
        self.reserved[1] & 1 != 0
    }

    pub fn set_locked(&mut self, on: bool) {
        self.reserved[1] = (self.reserved[1] & !1) | i32::from(on);
    }
}

/// `TStats`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    pub shots_made: i32,
    pub success: i32,
    pub path: f32,
    pub time: f32,
}

/// `KeyMap`: Windows virtual-key codes (1 = left mouse button).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyMap {
    pub forward: i32,
    pub backward: i32,
    pub up: i32,
    pub down: i32,
    pub left: i32,
    pub right: i32,
    pub fire: i32,
    pub show: i32,
    pub strafe_left: i32,
    pub strafe_right: i32,
    pub strafe: i32,
    pub jump: i32,
    pub run: i32,
    pub crouch: i32,
    pub call: i32,
    pub change_call: i32,
    pub binoculars: i32,
}

/// Options stored after the trophy room, in file order.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Options {
    pub agres: i32,
    pub dens: i32,
    pub sens: i32,
    pub res: i32,
    pub fog: i32,
    pub text: i32,
    pub view_r: i32,
    pub shadows: i32,
    pub mouse_sens: i32,
    pub brightness: i32,
    pub keys: KeyMap,
    pub reverse_mouse: i32,
    pub scent: i32,
    pub camo: i32,
    pub radar: i32,
    pub tranq: i32,
    pub alpha_colorkey: i32,
    pub sys: i32,
    pub sound: i32,
    pub render: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Save {
    pub player_name_raw: [u8; 128],
    pub reg_number: i32,
    pub score: i32,
    pub rank: i32,
    pub last: Stats,
    pub total: Stats,
    pub bodies: [TrophyItem; 24],
    pub options: Options,
}

pub const FILE_SIZE: usize = 1660;

impl Save {
    pub fn player_name(&self) -> String {
        c_str(&self.player_name_raw)
    }
}

fn stats(r: &mut Reader<'_>) -> Result<Stats, FormatError> {
    Ok(Stats {
        shots_made: r.i32("sav.stats")?,
        success: r.i32("sav.stats")?,
        path: r.f32("sav.stats")?,
        time: r.f32("sav.stats")?,
    })
}

pub fn parse(data: &[u8]) -> Result<Save, FormatError> {
    let mut r = Reader::new(data);
    let player_name_raw = r.array("sav.name")?;
    let reg_number = r.i32("sav.reg")?;
    let score = r.i32("sav.score")?;
    let rank = r.i32("sav.rank")?;
    let last = stats(&mut r)?;
    let total = stats(&mut r)?;
    let mut bodies = [TrophyItem::default(); 24];
    for b in &mut bodies {
        let mut i = || r.i32("sav.body");
        *b = TrophyItem {
            ctype: i()?,
            weapon: i()?,
            phase: i()?,
            height: i()?,
            weight: i()?,
            score: i()?,
            date: i()?,
            time: i()?,
            ..Default::default()
        };
        b.scale = r.f32("sav.body.scale")?;
        b.range = r.f32("sav.body.range")?;
        for v in &mut b.reserved {
            *v = r.i32("sav.body.r")?;
        }
    }
    let mut i = |c: &str| r.i32(c);
    let o = Options {
        agres: i("sav.opt")?,
        dens: i("sav.opt")?,
        sens: i("sav.opt")?,
        res: i("sav.opt")?,
        fog: i("sav.opt")?,
        text: i("sav.opt")?,
        view_r: i("sav.opt")?,
        shadows: i("sav.opt")?,
        mouse_sens: i("sav.opt")?,
        brightness: i("sav.opt")?,
        keys: KeyMap {
            forward: i("sav.keys")?,
            backward: i("sav.keys")?,
            up: i("sav.keys")?,
            down: i("sav.keys")?,
            left: i("sav.keys")?,
            right: i("sav.keys")?,
            fire: i("sav.keys")?,
            show: i("sav.keys")?,
            strafe_left: i("sav.keys")?,
            strafe_right: i("sav.keys")?,
            strafe: i("sav.keys")?,
            jump: i("sav.keys")?,
            run: i("sav.keys")?,
            crouch: i("sav.keys")?,
            call: i("sav.keys")?,
            change_call: i("sav.keys")?,
            binoculars: i("sav.keys")?,
        },
        reverse_mouse: i("sav.opt")?,
        scent: i("sav.opt")?,
        camo: i("sav.opt")?,
        radar: i("sav.opt")?,
        tranq: i("sav.opt")?,
        alpha_colorkey: i("sav.opt")?,
        sys: i("sav.opt")?,
        sound: i("sav.opt")?,
        render: i("sav.opt")?,
    };
    r.finish("sav")?;
    Ok(Save {
        player_name_raw,
        reg_number,
        score,
        rank,
        last,
        total,
        bodies,
        options: o,
    })
}

pub fn write(s: &Save) -> Vec<u8> {
    let mut w = Vec::with_capacity(FILE_SIZE);
    w.extend_from_slice(&s.player_name_raw);
    for v in [s.reg_number, s.score, s.rank] {
        w.put_i32(v);
    }
    for st in [s.last, s.total] {
        w.put_i32(st.shots_made);
        w.put_i32(st.success);
        w.put_f32(st.path);
        w.put_f32(st.time);
    }
    for b in &s.bodies {
        for v in [
            b.ctype, b.weapon, b.phase, b.height, b.weight, b.score, b.date, b.time,
        ] {
            w.put_i32(v);
        }
        w.put_f32(b.scale);
        w.put_f32(b.range);
        for v in b.reserved {
            w.put_i32(v);
        }
    }
    let o = &s.options;
    let k = &o.keys;
    for v in [
        o.agres,
        o.dens,
        o.sens,
        o.res,
        o.fog,
        o.text,
        o.view_r,
        o.shadows,
        o.mouse_sens,
        o.brightness,
        k.forward,
        k.backward,
        k.up,
        k.down,
        k.left,
        k.right,
        k.fire,
        k.show,
        k.strafe_left,
        k.strafe_right,
        k.strafe,
        k.jump,
        k.run,
        k.crouch,
        k.call,
        k.change_call,
        k.binoculars,
        o.reverse_mouse,
        o.scent,
        o.camo,
        o.radar,
        o.tranq,
        o.alpha_colorkey,
        o.sys,
        o.sound,
        o.render,
    ] {
        w.put_i32(v);
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_synthetic() {
        let mut name = [0u8; 128];
        name[..4].copy_from_slice(b"Test");
        let mut bodies = [TrophyItem::default(); 24];
        bodies[3] = TrophyItem {
            ctype: 7,
            score: 12,
            scale: 1.1,
            range: 30.0,
            ..Default::default()
        };
        let s = Save {
            player_name_raw: name,
            reg_number: 1,
            score: 120,
            rank: 1,
            last: Stats {
                shots_made: 3,
                success: 1,
                path: 10.0,
                time: 60.0,
            },
            total: Stats::default(),
            bodies,
            options: Options {
                agres: 128,
                view_r: 128,
                ..Default::default()
            },
        };
        let bytes = write(&s);
        assert_eq!(bytes.len(), FILE_SIZE);
        assert_eq!(parse(&bytes).ok(), Some(s));
    }
}
