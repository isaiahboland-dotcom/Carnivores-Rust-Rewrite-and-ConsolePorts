//! 3DS-only settings, kept beside the save in `sdmc:/3ds/c2rs/settings.txt`
//! (`key = value` lines), so the save stays in the original's format.

use crate::DIR;

/// Small ambient animals around the hunter at most, when on: more than a
/// handful in view costs frames on an Old 3DS.
pub const AMBIENT_CAP: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The animals' running speed, percent (50..=100).
    pub run_pct: i32,
    /// Small ambient animals (the ones that score nothing) on or off.
    pub ambient: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            run_pct: 100,
            ambient: true,
        }
    }
}

fn path() -> String {
    format!("{DIR}/settings.txt")
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
                _ => {}
            }
        }
        s
    }

    pub fn store(&self) -> std::io::Result<()> {
        std::fs::write(
            path(),
            format!(
                "run_speed = {}\nambient = {}\n",
                self.run_pct,
                i32::from(self.ambient)
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
