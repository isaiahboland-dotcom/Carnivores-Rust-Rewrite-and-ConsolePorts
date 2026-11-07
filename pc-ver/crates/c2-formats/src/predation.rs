//! `HUNTDAT/_PREDATION.TXT`: the "carnivores hunt herbivores" settings (not
//! in the original game). The launcher writes it from a recipe's
//! `[settings.predation]`; the engine reads it at the start of a hunt. When
//! the file is missing, predation is off.
//!
//! One setting per line, `key value`; `;` starts a comment. Animal names are
//! the rest of the line, as `_RES.TXT` names them (case ignored):
//!
//! ```text
//! priority prey          ; or hunter (the default)
//! fights 1               ; carnivores hunt cheaper carnivores
//! apex Tyrannosaurus     ; ignores prices: hunts every other animal
//! chain Tyrannosaurus    ; the food chain, top first (replaces prices)
//! chain Velociraptor
//! diet herbivore Ceratosaurus
//! scavenger Velociraptor
//! defensive Triceratops
//! ```

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

/// The path the settings are read from.
pub const PATH: &str = "HUNTDAT/_PREDATION.TXT";

/// Whom a carnivore that could chase either goes for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Priority {
    /// The hunter first: animals are hunted only while it is not after him.
    #[default]
    Hunter,
    /// Prey first: a sensed animal is chased even while hunting him.
    Prey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diet {
    Herbivore,
    Carnivore,
}

/// One animal's tags. `diet: None` keeps the one its AI implies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Animal {
    pub name: String,
    pub diet: Option<Diet>,
    /// Seeks out dead herbivores it senses, eats once and runs off.
    pub scavenger: bool,
    /// A herbivore that charges carnivores below it on the food chain.
    pub defensive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Predation {
    pub priority: Priority,
    /// Carnivores hunt carnivores priced at least 10% lower.
    pub carnivore_fights: bool,
    /// One animal above every other (the rest by price).
    pub apex: Option<String>,
    /// The whole food chain, top first; when given, it replaces prices.
    pub chain: Vec<String>,
    pub animals: Vec<Animal>,
}

impl Predation {
    /// The tags of the animal called `name`.
    pub fn animal(&self, name: &str) -> Option<&Animal> {
        let name = name.trim();
        self.animals
            .iter()
            .find(|a| a.name.trim().eq_ignore_ascii_case(name))
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
    /// file still loads.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut p = Self::default();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let (key, rest) = match line.split_once(char::is_whitespace) {
                Some((k, r)) => (k, r.trim()),
                None => (line, ""),
            };
            if rest.is_empty() {
                continue;
            }
            match key.to_ascii_lowercase().as_str() {
                "priority" => {
                    p.priority = if rest.eq_ignore_ascii_case("prey") {
                        Priority::Prey
                    } else {
                        Priority::Hunter
                    }
                }
                "fights" => p.carnivore_fights = matches!(rest, "1" | "on" | "true" | "TRUE"),
                "apex" => p.apex = Some(rest.into()),
                "chain" => p.chain.push(rest.into()),
                "diet" => {
                    let Some((d, name)) = rest.split_once(char::is_whitespace) else {
                        continue;
                    };
                    let diet = match d.to_ascii_lowercase().as_str() {
                        "herbivore" => Diet::Herbivore,
                        "carnivore" => Diet::Carnivore,
                        _ => continue,
                    };
                    p.animal_mut(name.trim()).diet = Some(diet);
                }
                "scavenger" => p.animal_mut(rest).scavenger = true,
                "defensive" => p.animal_mut(rest).defensive = true,
                _ => {}
            }
        }
        p
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut s =
            String::from("; Carnivores hunt herbivores (from the recipe's [settings.predation])\n");
        let prio = match self.priority {
            Priority::Hunter => "hunter",
            Priority::Prey => "prey",
        };
        s += &format!(
            "priority {prio}\nfights {}\n",
            u8::from(self.carnivore_fights)
        );
        if let Some(a) = &self.apex {
            s += &format!("apex {a}\n");
        }
        for c in &self.chain {
            s += &format!("chain {c}\n");
        }
        for a in &self.animals {
            match a.diet {
                Some(Diet::Herbivore) => s += &format!("diet herbivore {}\n", a.name),
                Some(Diet::Carnivore) => s += &format!("diet carnivore {}\n", a.name),
                None => {}
            }
            if a.scavenger {
                s += &format!("scavenger {}\n", a.name);
            }
            if a.defensive {
                s += &format!("defensive {}\n", a.name);
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let p = Predation {
            priority: Priority::Prey,
            carnivore_fights: true,
            apex: Some("Tyrannosaurus".into()),
            chain: vec!["Tyrannosaurus".into(), "Velociraptor".into()],
            animals: vec![
                Animal {
                    name: "Big Cat".into(),
                    diet: Some(Diet::Carnivore),
                    scavenger: true,
                    defensive: false,
                },
                Animal {
                    name: "Triceratops".into(),
                    diet: None,
                    scavenger: false,
                    defensive: true,
                },
            ],
        };
        assert_eq!(Predation::parse(p.to_text().as_bytes()), p);
    }

    #[test]
    fn names_are_matched_without_case_and_junk_is_skipped() {
        let p = Predation::parse(b"bogus 1\ndiet omnivore X\nscavenger  Raptor  ; note\n");
        assert!(p.animal("raptor").is_some_and(|a| a.scavenger));
        assert!(p.animal("X").is_none());
        assert_eq!(p.priority, Priority::Hunter);
    }
}
