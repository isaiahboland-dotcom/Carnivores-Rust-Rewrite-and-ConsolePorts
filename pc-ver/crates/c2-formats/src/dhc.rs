//! `HUNTDAT/_DHC.TXT`: the DinoHunt Corp terminal's rules (not in the
//! original game). The launcher writes it from a recipe's `[settings.dhc]`;
//! the menus read it when they load the game. When the file is missing,
//! the terminal offers what the original menu offers.
//!
//! One setting per line, `key value`; `;` starts a comment. Animal,
//! weapon and equipment names are the rest of the line, as the menus name
//! them (case ignored). Areas are map names (`AREA1`, a `.c2map`'s stem).
//! `island` starts an island; the `animal`, `weapon`, `equipment` and
//! `time` lines after it are its allow-lists (none of a kind: every one).
//!
//! ```text
//! discovery sighting       ; default: off, sighting, scan or kill
//! cost 200                 ; licence cost in score points, default
//! discover kill Velociraptor
//! price 500 Velociraptor
//! island AREA1
//! animal Parasaurolophus
//! weapon Pistol
//! equipment Radar
//! time day
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

/// The path the rules are read from.
pub const PATH: &str = "HUNTDAT/_DHC.TXT";

/// The licence cost when the recipe sets none.
pub const DEFAULT_LICENCE_COST: u32 = 200;

/// The licence costs the launcher accepts (outside: clamped).
pub const LICENCE_RANGE: (u32, u32) = (0, 100_000);

/// The times of day an island can allow, in the menus' order.
pub const TIMES: [&str; 3] = ["dawn", "day", "night"];

/// How a secret animal becomes known to the terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Discovery {
    /// Never hidden: listed from the start.
    #[default]
    Off,
    /// Watched within binocular range for a while.
    Sighting,
    /// Picked up by the radar or binoculars.
    Scan,
    /// Killed (or tranquilized).
    Kill,
}

impl Discovery {
    pub const ALL: [Discovery; 4] = [
        Discovery::Off,
        Discovery::Sighting,
        Discovery::Scan,
        Discovery::Kill,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Discovery::Off => "off",
            Discovery::Sighting => "sighting",
            Discovery::Scan => "scan",
            Discovery::Kill => "kill",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|d| d.key().eq_ignore_ascii_case(k.trim()))
    }
}

/// One animal's own settings (unset: the defaults).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Animal {
    pub name: String,
    pub discovery: Option<Discovery>,
    pub licence_cost: Option<u32>,
}

/// What one area offers. An empty list allows every item of its kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Island {
    /// Map name (`AREA1`).
    pub area: String,
    pub animals: Vec<String>,
    pub weapons: Vec<String>,
    pub equipment: Vec<String>,
    /// Bit per [`TIMES`] entry; 0: every time.
    pub times: u8,
}

fn listed(list: &[String], name: &str) -> bool {
    list.is_empty()
        || list
            .iter()
            .any(|n| n.trim().eq_ignore_ascii_case(name.trim()))
}

impl Island {
    pub fn allows_animal(&self, name: &str) -> bool {
        listed(&self.animals, name)
    }

    pub fn allows_weapon(&self, name: &str) -> bool {
        listed(&self.weapons, name)
    }

    pub fn allows_equipment(&self, name: &str) -> bool {
        listed(&self.equipment, name)
    }

    /// Time `t` (0 dawn, 1 day, 2 night).
    pub fn allows_time(&self, t: usize) -> bool {
        self.times == 0 || (t < 8 && self.times & (1 << t) != 0)
    }

    /// Whether it restricts nothing.
    pub fn is_open(&self) -> bool {
        self.animals.is_empty()
            && self.weapons.is_empty()
            && self.equipment.is_empty()
            && self.times == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dhc {
    /// The default for animals without their own.
    pub discovery: Discovery,
    /// The default licence cost, in score points.
    pub licence_cost: u32,
    pub animals: Vec<Animal>,
    pub islands: Vec<Island>,
}

impl Default for Dhc {
    fn default() -> Self {
        Self {
            discovery: Discovery::Off,
            licence_cost: DEFAULT_LICENCE_COST,
            animals: Vec::new(),
            islands: Vec::new(),
        }
    }
}

impl Dhc {
    /// The settings of the animal called `name`.
    pub fn animal(&self, name: &str) -> Option<&Animal> {
        let name = name.trim();
        self.animals
            .iter()
            .find(|a| a.name.trim().eq_ignore_ascii_case(name))
    }

    /// How animal `name` is discovered.
    pub fn discovery_of(&self, name: &str) -> Discovery {
        self.animal(name)
            .and_then(|a| a.discovery)
            .unwrap_or(self.discovery)
    }

    /// What animal `name`'s licence costs.
    pub fn licence_cost_of(&self, name: &str) -> u32 {
        self.animal(name)
            .and_then(|a| a.licence_cost)
            .unwrap_or(self.licence_cost)
    }

    /// The island of map `stem` (none: everything is allowed there).
    pub fn island(&self, stem: &str) -> Option<&Island> {
        let stem = stem.trim();
        self.islands
            .iter()
            .find(|i| i.area.trim().eq_ignore_ascii_case(stem))
    }

    fn animal_mut(&mut self, name: &str) -> &mut Animal {
        let i = match self
            .animals
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
        {
            Some(i) => i,
            None => {
                self.animals.push(Animal {
                    name: name.into(),
                    ..Animal::default()
                });
                self.animals.len() - 1
            }
        };
        &mut self.animals[i]
    }

    /// Reads the file. Unknown keys and bad values are skipped, so a newer
    /// file still loads; island lines before any `island` are skipped too.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut d = Self::default();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let Some((key, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let rest = rest.trim();
            let cost = |v: &str| {
                v.trim()
                    .parse::<u32>()
                    .ok()
                    .map(|c| c.clamp(LICENCE_RANGE.0, LICENCE_RANGE.1))
            };
            match key.to_ascii_lowercase().as_str() {
                "discovery" => {
                    if let Some(m) = Discovery::from_key(rest) {
                        d.discovery = m;
                    }
                }
                "cost" => {
                    if let Some(c) = cost(rest) {
                        d.licence_cost = c;
                    }
                }
                "discover" => {
                    let Some((m, name)) = rest.split_once(char::is_whitespace) else {
                        continue;
                    };
                    if let Some(m) = Discovery::from_key(m) {
                        d.animal_mut(name.trim()).discovery = Some(m);
                    }
                }
                "price" => {
                    let Some((c, name)) = rest.split_once(char::is_whitespace) else {
                        continue;
                    };
                    if let Some(c) = cost(c) {
                        d.animal_mut(name.trim()).licence_cost = Some(c);
                    }
                }
                "island" => d.islands.push(Island {
                    area: rest.into(),
                    ..Island::default()
                }),
                k @ ("animal" | "weapon" | "equipment" | "time") => {
                    let Some(island) = d.islands.last_mut() else {
                        continue;
                    };
                    match k {
                        "animal" => island.animals.push(rest.into()),
                        "weapon" => island.weapons.push(rest.into()),
                        "equipment" => island.equipment.push(rest.into()),
                        _ => {
                            if let Some(t) = TIMES.iter().position(|t| t.eq_ignore_ascii_case(rest))
                            {
                                island.times |= 1 << t;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        d
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut s = String::from("; DinoHunt Corp terminal (from the recipe's [settings.dhc])\n");
        s += &format!(
            "discovery {}\ncost {}\n",
            self.discovery.key(),
            self.licence_cost
        );
        for a in &self.animals {
            if let Some(m) = a.discovery {
                s += &format!("discover {} {}\n", m.key(), a.name);
            }
            if let Some(c) = a.licence_cost {
                s += &format!("price {c} {}\n", a.name);
            }
        }
        for i in &self.islands {
            s += &format!("island {}\n", i.area);
            for (key, list) in [
                ("animal", &i.animals),
                ("weapon", &i.weapons),
                ("equipment", &i.equipment),
            ] {
                for n in list {
                    s += &format!("{key} {n}\n");
                }
            }
            for (t, name) in TIMES.iter().enumerate() {
                if i.times & (1 << t) != 0 {
                    s += &format!("time {name}\n");
                }
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Dhc {
        Dhc {
            discovery: Discovery::Sighting,
            licence_cost: 350,
            animals: vec![
                Animal {
                    name: "Velociraptor".into(),
                    discovery: Some(Discovery::Kill),
                    licence_cost: Some(500),
                },
                Animal {
                    name: "Big Cat".into(),
                    discovery: None,
                    licence_cost: Some(0),
                },
            ],
            islands: vec![
                Island {
                    area: "AREA1".into(),
                    animals: vec!["Parasaurolophus".into()],
                    weapons: vec!["Pistol".into(), "Shotgun".into()],
                    equipment: vec!["Radar".into()],
                    times: 0b011,
                },
                Island {
                    area: "AREA2".into(),
                    ..Island::default()
                },
            ],
        }
    }

    #[test]
    fn round_trips_through_text() {
        let d = sample();
        assert_eq!(Dhc::parse(d.to_text().as_bytes()), d);
        assert_eq!(
            Dhc::parse(Dhc::default().to_text().as_bytes()),
            Dhc::default()
        );
    }

    #[test]
    fn unset_costs_are_200_and_an_animal_override_wins() {
        let d = Dhc::default();
        assert_eq!(d.licence_cost_of("Anything"), 200);
        let d = sample();
        assert_eq!(d.licence_cost_of("velociraptor"), 500);
        assert_eq!(d.licence_cost_of("Big Cat"), 0);
        assert_eq!(d.licence_cost_of("Dimorphodon"), 350);
        assert_eq!(d.discovery_of("VELOCIRAPTOR"), Discovery::Kill);
        assert_eq!(d.discovery_of("Big Cat"), Discovery::Sighting);
    }

    #[test]
    fn islands_allow_listed_items_and_empty_lists_allow_all() {
        let d = sample();
        let a1 = d.island("area1").unwrap_or_else(|| panic!("AREA1"));
        assert!(a1.allows_animal("parasaurolophus") && !a1.allows_animal("Velociraptor"));
        assert!(a1.allows_weapon("Shotgun") && !a1.allows_weapon("X-Bow"));
        assert!(a1.allows_time(0) && a1.allows_time(1) && !a1.allows_time(2));
        let a2 = d.island("AREA2").unwrap_or_else(|| panic!("AREA2"));
        assert!(a2.is_open() && a2.allows_animal("Anything") && a2.allows_time(2));
        assert!(d.island("AREA9").is_none());
    }

    #[test]
    fn junk_is_skipped_and_costs_are_clamped() {
        let d = Dhc::parse(
            b"bogus 1\ndiscovery maybe\ncost lots\nanimal Orphan\n\
              price 9999999 Rex ; note\ndiscover never Rex\nisland AREA3\ntime noon\n",
        );
        assert_eq!(d.discovery, Discovery::Off);
        assert_eq!(d.licence_cost, DEFAULT_LICENCE_COST);
        assert_eq!(d.licence_cost_of("rex"), LICENCE_RANGE.1);
        assert_eq!(d.discovery_of("rex"), Discovery::Off);
        assert_eq!(d.islands.len(), 1);
        assert!(d.islands[0].is_open(), "{:?}", d.islands[0]);
    }
}
