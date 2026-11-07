//! `HUNTDAT/_RULES.TXT`: an instance's own rules (not in the original
//! game). The launcher writes it from a recipe's `[settings.rules]` (its
//! Advanced tab); every platform's menu and hunt read it. When the file is
//! missing, nothing differs from the game.
//!
//! One rule per line, `key value...`; `;` starts a comment. Areas are named
//! by their map's file name without extension (`AREA3`, or a `.c2map`'s);
//! `*` is every area. Animal names are the rest of the line, as `_RES.TXT`
//! names them (case ignored; `Raptor #2` is the second one named Raptor):
//!
//! ```text
//! times day night            ; times of day offered: dawn, day, night
//! lock dynamic_time 0        ; setting fixed and hidden from the menus
//! lock secret_dinos 4
//! area AREA2 huntable 3      ; at most 3 of the picked animals on AREA2
//! secret 2 AREA1,AREA3 Tyrannosaurus   ; 2 more on AREA1 and AREA3
//! secret 1 * Velociraptor
//! hide Tyrannosaurus         ; in the game, but not in the hunt menu
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

/// The path the rules are read from.
pub const PATH: &str = "HUNTDAT/_RULES.TXT";

/// The times of day, in the menus' order (`0` dawn, `1` day, `2` night).
pub const TIMES: [&str; 3] = ["dawn", "day", "night"];

/// The settings an instance can fix (and hide from the in-game menus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lock {
    DynamicTime,
    AdvancedObserver,
    MultiAmmo,
    MobileRadar,
    Teleport,
    AllHuntable,
    /// A count (0..=255): unpicked huntable animals per map.
    SecretDinos,
    /// Points (0: from the menu's prices).
    SurvivalBudget,
}

impl Lock {
    pub const ALL: [Lock; 8] = [
        Lock::DynamicTime,
        Lock::AdvancedObserver,
        Lock::MultiAmmo,
        Lock::MobileRadar,
        Lock::Teleport,
        Lock::AllHuntable,
        Lock::SecretDinos,
        Lock::SurvivalBudget,
    ];

    /// The file's (and the recipe's) name for it.
    pub fn key(self) -> &'static str {
        match self {
            Lock::DynamicTime => "dynamic_time",
            Lock::AdvancedObserver => "advanced_observer",
            Lock::MultiAmmo => "multi_ammo",
            Lock::MobileRadar => "mobile_radar",
            Lock::Teleport => "teleport",
            Lock::AllHuntable => "all_huntable",
            Lock::SecretDinos => "secret_dinos",
            Lock::SurvivalBudget => "survival_budget",
        }
    }

    pub fn from_key(k: &str) -> Option<Lock> {
        Lock::ALL
            .into_iter()
            .find(|l| l.key().eq_ignore_ascii_case(k))
    }

    /// An on/off setting (the rest are numbers).
    pub fn is_switch(self) -> bool {
        !matches!(self, Lock::SecretDinos | Lock::SurvivalBudget)
    }
}

/// Extra unpicked animals of one kind, placed at random on some areas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Secret {
    pub name: String,
    pub count: u32,
    /// Map names; empty: every area.
    pub areas: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    /// Bit per time of day offered ([`TIMES`]); 0 (or a file without the
    /// line): all three.
    pub times: u8,
    /// Settings fixed by the instance, sorted, one per setting.
    pub locks: Vec<(Lock, i32)>,
    /// Per area (map name): at most this many of the picked animals.
    pub huntable: Vec<(String, u32)>,
    pub secrets: Vec<Secret>,
    /// Animals kept off the hunt menu (they still roam, as secrets).
    pub hidden: Vec<String>,
}

/// Whether map `stem` is one of `areas` (empty: every area).
fn names_area(areas: &[String], stem: &str) -> bool {
    areas.is_empty()
        || areas
            .iter()
            .any(|a| a == "*" || a.eq_ignore_ascii_case(stem))
}

impl Rules {
    pub fn is_empty(&self) -> bool {
        *self == Rules::default()
    }

    /// Whether time `t` (0 dawn, 1 day, 2 night) is offered.
    pub fn time_allowed(&self, t: usize) -> bool {
        t < 3 && (self.times & 7 == 0 || self.times & (1 << t) != 0)
    }

    /// `t` if it is offered, else the first time that is (day first).
    pub fn allowed_time(&self, t: usize) -> usize {
        if self.time_allowed(t) {
            return t;
        }
        [1, 0, 2]
            .into_iter()
            .find(|&t| self.time_allowed(t))
            .unwrap_or(1)
    }

    /// The value `l` is fixed at, if the instance fixes it.
    pub fn lock(&self, l: Lock) -> Option<i32> {
        self.locks.iter().find(|(k, _)| *k == l).map(|&(_, v)| v)
    }

    pub fn is_locked(&self, l: Lock) -> bool {
        self.lock(l).is_some()
    }

    /// `l`'s value: the locked one, else `own` (the player's).
    pub fn value(&self, l: Lock, own: i32) -> i32 {
        self.lock(l).unwrap_or(own)
    }

    /// At most how many picked animals map `stem` gets.
    pub fn max_huntable(&self, stem: &str) -> Option<u32> {
        self.huntable
            .iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(stem))
            .map(|&(_, n)| n)
    }

    /// The extra animals for map `stem`.
    pub fn secrets_on<'a>(&'a self, stem: &'a str) -> impl Iterator<Item = &'a Secret> + 'a {
        self.secrets
            .iter()
            .filter(move |s| s.count > 0 && names_area(&s.areas, stem))
    }

    /// Whether the hunt menu's `i`-th animal of `names` is kept off it
    /// (the second "Raptor" is "Raptor #2").
    pub fn hides(&self, names: &[String], i: usize) -> bool {
        let Some(name) = names.get(i).map(|n| n.trim()) else {
            return false;
        };
        let n = 1 + names[..i]
            .iter()
            .filter(|m| m.trim().eq_ignore_ascii_case(name))
            .count();
        self.hidden.iter().any(|h| {
            let h = h.trim();
            match h
                .rsplit_once(" #")
                .map(|(a, k)| (a.trim(), k.parse::<usize>()))
            {
                Some((a, Ok(k))) if a.eq_ignore_ascii_case(name) => k == n,
                _ => n == 1 && h.eq_ignore_ascii_case(name),
            }
        })
    }

    /// Reads the file. Unknown keys and bad values are skipped, so a newer
    /// file still loads.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut r = Self::default();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let mut words = line.split_whitespace();
            let Some(key) = words.next() else { continue };
            match key.to_ascii_lowercase().as_str() {
                "times" => {
                    for w in words {
                        if let Some(i) = TIMES.iter().position(|t| t.eq_ignore_ascii_case(w)) {
                            r.times |= 1 << i;
                        }
                    }
                }
                "lock" => {
                    let (Some(l), Some(v)) = (
                        words.next().and_then(Lock::from_key),
                        words.next().and_then(|v| v.parse::<i32>().ok()),
                    ) else {
                        continue;
                    };
                    r.locks.retain(|(k, _)| *k != l);
                    r.locks.push((l, v));
                }
                "area" => {
                    let (Some(a), Some(k), Some(n)) = (words.next(), words.next(), words.next())
                    else {
                        continue;
                    };
                    if let (true, Ok(n)) = (k.eq_ignore_ascii_case("huntable"), n.parse()) {
                        r.huntable.retain(|(b, _)| !b.eq_ignore_ascii_case(a));
                        r.huntable.push((a.into(), n));
                    }
                }
                "secret" => {
                    let (Some(Ok(count)), Some(areas)) =
                        (words.next().map(str::parse::<u32>), words.next())
                    else {
                        continue;
                    };
                    let name = words.collect::<Vec<_>>().join(" ");
                    if name.is_empty() {
                        continue;
                    }
                    let areas = areas
                        .split(',')
                        .map(str::trim)
                        .filter(|a| !a.is_empty() && *a != "*")
                        .map(String::from)
                        .collect();
                    r.secrets.push(Secret { name, count, areas });
                }
                "hide" => {
                    let name = words.collect::<Vec<_>>().join(" ");
                    if !name.is_empty() {
                        r.hidden.push(name);
                    }
                }
                _ => {}
            }
        }
        r.locks.sort();
        r
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut s = String::from("; Instance rules (from the recipe's [settings.rules])\n");
        if self.times & 7 != 0 && self.times & 7 != 7 {
            let t: Vec<&str> = (0..3)
                .filter(|&i| self.times & (1 << i) != 0)
                .map(|i| TIMES[i])
                .collect();
            s += &format!("times {}\n", t.join(" "));
        }
        for (l, v) in &self.locks {
            s += &format!("lock {} {v}\n", l.key());
        }
        for (a, n) in &self.huntable {
            s += &format!("area {a} huntable {n}\n");
        }
        for x in &self.secrets {
            let areas = if x.areas.is_empty() {
                String::from("*")
            } else {
                x.areas.join(",")
            };
            s += &format!("secret {} {areas} {}\n", x.count, x.name);
        }
        for h in &self.hidden {
            s += &format!("hide {h}\n");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Rules {
        Rules {
            times: 0b110,
            locks: vec![(Lock::DynamicTime, 0), (Lock::SecretDinos, 4)],
            huntable: vec![("AREA2".into(), 3)],
            secrets: vec![
                Secret {
                    name: "Tyrannosaurus Rex".into(),
                    count: 2,
                    areas: vec!["AREA1".into(), "AREA3".into()],
                },
                Secret {
                    name: "Velociraptor".into(),
                    count: 1,
                    areas: vec![],
                },
            ],
            hidden: vec!["Raptor #2".into(), "Rex".into()],
        }
    }

    #[test]
    fn round_trips_through_text() {
        let r = sample();
        assert_eq!(Rules::parse(r.to_text().as_bytes()), r);
        assert!(Rules::parse(Rules::default().to_text().as_bytes()).is_empty());
    }

    #[test]
    fn times_fall_back_to_one_offered() {
        let r = sample();
        assert!(!r.time_allowed(0));
        assert_eq!(r.allowed_time(0), 1);
        let night = Rules {
            times: 0b100,
            ..Rules::default()
        };
        assert_eq!(night.allowed_time(1), 2);
        assert!(Rules::default().time_allowed(0), "no line: all offered");
    }

    #[test]
    fn locks_areas_and_secrets_are_looked_up_by_name() {
        let r = sample();
        assert_eq!(r.value(Lock::DynamicTime, 1), 0);
        assert_eq!(r.value(Lock::Teleport, 1), 1);
        assert_eq!(r.max_huntable("area2"), Some(3));
        assert_eq!(r.max_huntable("AREA1"), None);
        let on1: Vec<&str> = r.secrets_on("AREA1").map(|s| s.name.as_str()).collect();
        assert_eq!(on1, ["Tyrannosaurus Rex", "Velociraptor"]);
        assert_eq!(r.secrets_on("AREA2").count(), 1);
    }

    #[test]
    fn hidden_animals_are_matched_by_name_and_number() {
        let r = sample();
        let names: Vec<String> = ["Raptor", "rex", "Raptor", "Trike"]
            .map(String::from)
            .to_vec();
        let hidden: Vec<usize> = (0..5).filter(|&i| r.hides(&names, i)).collect();
        assert_eq!(hidden, [1, 2]);
    }

    #[test]
    fn unknown_and_bad_lines_are_skipped() {
        let r = Rules::parse(b"lock nonsense 1\nlock teleport x\nsecret two * Rex\nfoo\n");
        assert!(r.is_empty());
    }
}
