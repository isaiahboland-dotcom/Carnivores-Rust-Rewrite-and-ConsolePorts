//! 3DS-only settings, kept beside the save in `sdmc:/3ds/c2rs/settings.txt`
//! (`key = value` lines), so the save stays in the original's format.

use crate::dir;

/// Small ambient animals around the hunter at most, when on: more than a
/// handful in view costs frames on an Old 3DS.
pub const AMBIENT_CAP: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The animals' running speed, percent (50..=100).
    pub run_pct: i32,
    /// Small ambient animals (the ones that score nothing) on or off.
    pub ambient: bool,
    /// Blood sprays and trails.
    pub blood: bool,
    /// The death camera; off: a black screen and the prompt.
    pub death_anim: bool,
    /// Hunt options: "3DS friendly" (animals notice the hunter from a
    /// shorter range), stealth crouch, random ambients (modded games).
    pub friendly: bool,
    pub stealth: bool,
    pub random_ambients: bool,
    /// The crouch button (the Old 3DS has no ZL).
    pub crouch_key: CrouchKey,
    /// Stealth gyro aim: degrees the view turns per degree the console
    /// turns, percent (`GYRO_STEPS`).
    pub gyro_pct: i32,
    /// The hunter last played (save slot).
    pub hunter: i32,
    /// The map as the mobile game draws it (a yellow frame and view cone);
    /// teleporting by tapping the map.
    pub map_mobile: bool,
    pub teleport: bool,
    /// The controls along the bottom of the top screen in a hunt.
    pub hints: bool,
}

/// How the hunter crouches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrouchKey {
    /// Hold ZL (New 3DS); L jumps.
    Zl,
    /// Hold L to crouch, tap L to jump.
    L,
    /// D-Pad Up cycles walk, run, crouch; L jumps.
    DpadUp,
}

impl CrouchKey {
    pub fn name(self) -> &'static str {
        match self {
            CrouchKey::Zl => "ZL",
            CrouchKey::L => "Hold L",
            CrouchKey::DpadUp => "D-Pad Up",
        }
    }

    pub fn next(self) -> Self {
        match self {
            CrouchKey::Zl => CrouchKey::L,
            CrouchKey::L => CrouchKey::DpadUp,
            CrouchKey::DpadUp => CrouchKey::Zl,
        }
    }

    fn key(self) -> &'static str {
        match self {
            CrouchKey::Zl => "zl",
            CrouchKey::L => "l",
            CrouchKey::DpadUp => "dpad",
        }
    }

    /// ZL on a New 3DS, L on the others.
    fn default_for_console() -> Self {
        let mut new = false;
        if unsafe { ctru_sys::APT_CheckNew3DS(&mut new) } >= 0 && new {
            CrouchKey::Zl
        } else {
            CrouchKey::L
        }
    }
}

/// The gyro aim steps the setting cycles through.
pub const GYRO_STEPS: [i32; 4] = [25, 40, 60, 100];

/// The next gyro aim step after `pct`.
pub fn next_gyro(pct: i32) -> i32 {
    GYRO_STEPS
        .iter()
        .copied()
        .find(|&s| s > pct)
        .unwrap_or(GYRO_STEPS[0])
}

/// How far animals notice the hunter with "3DS friendly" on, as a share
/// of the original's distance.
pub const FRIENDLY_NOTICE: f32 = 0.6;

impl Default for Settings {
    fn default() -> Self {
        Self {
            run_pct: 100,
            ambient: true,
            blood: true,
            death_anim: true,
            friendly: false,
            stealth: false,
            random_ambients: false,
            crouch_key: CrouchKey::default_for_console(),
            gyro_pct: 40,
            hunter: 0,
            map_mobile: false,
            teleport: false,
            hints: false,
        }
    }
}

fn path() -> String {
    format!("{}/settings.txt", dir())
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Ok(text) = std::fs::read_to_string(path()) else {
            return s;
        };
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            match (k.trim(), v.trim()) {
                ("run_speed", v) => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.run_pct = n.clamp(50, 100);
                    }
                }
                ("ambient", v) => s.ambient = v != "0",
                ("blood", v) => s.blood = v != "0",
                ("death_animation", v) => s.death_anim = v != "0",
                ("friendly", v) => s.friendly = v != "0",
                ("stealth_crouch", v) => s.stealth = v != "0",
                ("random_ambients", v) => s.random_ambients = v != "0",
                ("hunter", v) => s.hunter = v.parse().unwrap_or(0),
                ("map_mobile", v) => s.map_mobile = v != "0",
                ("teleport", v) => s.teleport = v != "0",
                ("button_hints", v) => s.hints = v != "0",
                ("gyro_aim", v) => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.gyro_pct = n.clamp(GYRO_STEPS[0], 100);
                    }
                }
                ("crouch_button", v) => {
                    s.crouch_key = match v {
                        "zl" => CrouchKey::Zl,
                        "dpad" => CrouchKey::DpadUp,
                        _ => CrouchKey::L,
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub fn store(&self) -> std::io::Result<()> {
        std::fs::write(
            path(),
            format!(
                "run_speed = {}\nambient = {}\nblood = {}\ndeath_animation = {}\n\
                 friendly = {}\nstealth_crouch = {}\nrandom_ambients = {}\ncrouch_button = {}\ngyro_aim = {}\n\
                 hunter = {}\nmap_mobile = {}\nteleport = {}\nbutton_hints = {}\n",
                self.run_pct,
                i32::from(self.ambient),
                i32::from(self.blood),
                i32::from(self.death_anim),
                i32::from(self.friendly),
                i32::from(self.stealth),
                i32::from(self.random_ambients),
                self.crouch_key.key(),
                self.gyro_pct,
                self.hunter,
                i32::from(self.map_mobile),
                i32::from(self.teleport),
                i32::from(self.hints)
            ),
        )
    }

    /// `PlaceOptions::max_ambient`.
    pub fn max_ambient(&self) -> i32 {
        if self.ambient {
            AMBIENT_CAP
        } else {
            0
        }
    }
}
