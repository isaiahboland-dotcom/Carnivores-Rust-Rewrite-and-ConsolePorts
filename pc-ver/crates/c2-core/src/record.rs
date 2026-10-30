//! A hunter's record, beside the save (`trophy0N.rec`; the original's save
//! has no room for it): every kill, in hunts and in survival, and the best
//! survival wave. Statistics are worked out from it: the longest shot, the
//! highest-scoring kill, kills per mode.
//!
//! Plain text, a line each: `kill<TAB>mode<TAB>animal<TAB>score<TAB>range
//! <TAB>weapon<TAB>area<TAB>date` (range in metres, date as the trophies
//! keep it) and `best_wave<TAB>n`. Unknown lines are skipped.

#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

/// Hunt or survival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Hunt,
    Survival,
}

impl Mode {
    fn key(self) -> &'static str {
        match self {
            Mode::Hunt => "hunt",
            Mode::Survival => "survival",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Kill {
    pub mode: Mode,
    pub animal: String,
    pub score: i32,
    /// Metres (`TrophyItem::range`).
    pub range: f32,
    pub weapon: String,
    pub area: String,
    /// `(year << 20) + (month << 10) + day`.
    pub date: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Record {
    pub kills: Vec<Kill>,
    pub best_wave: u32,
}

/// Tabs and line breaks out of a name.
fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

impl Record {
    pub fn parse(text: &str) -> Self {
        let mut r = Record::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f.as_slice() {
                ["kill", mode, animal, score, range, weapon, area, date] => r.kills.push(Kill {
                    mode: if *mode == "survival" {
                        Mode::Survival
                    } else {
                        Mode::Hunt
                    },
                    animal: animal.to_string(),
                    score: score.parse().unwrap_or(0),
                    range: range.parse().unwrap_or(0.0),
                    weapon: weapon.to_string(),
                    area: area.to_string(),
                    date: date.parse().unwrap_or(0),
                }),
                ["best_wave", n] => r.best_wave = n.parse().unwrap_or(0),
                _ => {}
            }
        }
        r
    }

    pub fn write(&self) -> String {
        let mut s = format!("best_wave\t{}\n", self.best_wave);
        for k in &self.kills {
            s.push_str(&format!(
                "kill\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{}\n",
                k.mode.key(),
                clean(&k.animal),
                k.score,
                k.range,
                clean(&k.weapon),
                clean(&k.area),
                k.date
            ));
        }
        s
    }

    /// The longest shot that killed.
    pub fn longest(&self) -> Option<&Kill> {
        self.kills.iter().max_by(|a, b| a.range.total_cmp(&b.range))
    }

    /// The highest-scoring kill.
    pub fn best(&self) -> Option<&Kill> {
        self.kills.iter().max_by_key(|k| k.score)
    }

    pub fn count(&self, mode: Mode) -> usize {
        self.kills.iter().filter(|k| k.mode == mode).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_sums_up() {
        let k = |mode, animal: &str, score, range| Kill {
            mode,
            animal: animal.into(),
            score,
            range,
            weapon: "Rifle".into(),
            area: "Fort\tCiskin".into(),
            date: 7,
        };
        let r = Record {
            kills: vec![
                k(Mode::Hunt, "T-Rex", 25, 40.5),
                k(Mode::Survival, "Raptor", 6, 120.0),
                k(Mode::Hunt, "Stego", 3, 12.0),
            ],
            best_wave: 9,
        };
        let back = Record::parse(&r.write());
        assert_eq!(back.best_wave, 9);
        assert_eq!(back.kills.len(), 3);
        assert_eq!(back.kills[0].area, "Fort Ciskin");
        assert_eq!(back.longest().map(|k| k.animal.as_str()), Some("Raptor"));
        assert_eq!(back.best().map(|k| k.score), Some(25));
        assert_eq!((back.count(Mode::Hunt), back.count(Mode::Survival)), (2, 1));
        assert_eq!(Record::parse("junk\nbest_wave\tx\n"), Record::default());
    }
}
