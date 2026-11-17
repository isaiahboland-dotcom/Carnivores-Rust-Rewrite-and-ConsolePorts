//! A hunter's record, beside the save (`trophy0N.rec`; the original's save
//! has no room for it): every kill, in hunts and in survival, and the best
//! survival wave. Statistics are worked out from it: the longest shot, the
//! highest-scoring kill, kills per mode, and per species the biggest and
//! the best-scoring kill (the DHC OS field manual's records, kept when the
//! trophy is removed from the room).
//!
//! Plain text, a line each: `kill<TAB>mode<TAB>animal<TAB>score<TAB>range
//! <TAB>weapon<TAB>area<TAB>date[<TAB>scale]` (range in metres, date as the
//! trophies keep it, scale as the animal's `scale`; records written before
//! it was kept have no scale) and `best_wave<TAB>n`. Unknown lines are
//! skipped.

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
    /// The animal's size (`Character::scale`); 0: not recorded.
    pub scale: f32,
}

impl Kill {
    /// Whether this kill was of the species called `animal` (names
    /// compared without case or surrounding spaces).
    pub fn is(&self, animal: &str) -> bool {
        self.animal.trim().eq_ignore_ascii_case(animal.trim())
    }
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
                ["kill", mode, animal, score, range, weapon, area, date, rest @ ..]
                    if rest.len() <= 1 =>
                {
                    r.kills.push(Kill {
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
                        scale: rest
                            .first()
                            .and_then(|s| s.parse::<f32>().ok())
                            .filter(|s| s.is_finite() && *s > 0.0)
                            .unwrap_or(0.0),
                    })
                }
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
                "kill\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{}\t{:.3}\n",
                k.mode.key(),
                clean(&k.animal),
                k.score,
                k.range,
                clean(&k.weapon),
                clean(&k.area),
                k.date,
                k.scale
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

    /// How many kills of the species called `animal` (names compared
    /// without case or surrounding spaces).
    pub fn count_of(&self, animal: &str) -> usize {
        self.kills.iter().filter(|k| k.is(animal)).count()
    }

    /// The biggest kill of `animal` whose size was recorded (the first of
    /// equals).
    pub fn biggest(&self, animal: &str) -> Option<&Kill> {
        self.kills
            .iter()
            .filter(|k| k.is(animal) && k.scale > 0.0)
            .fold(None, |best: Option<&Kill>, k| match best {
                Some(b) if b.scale >= k.scale => Some(b),
                _ => Some(k),
            })
    }

    /// The highest-scoring kill of `animal` (the first of equals); none
    /// when it never scored (survival kills score nothing).
    pub fn best_of(&self, animal: &str) -> Option<&Kill> {
        self.kills
            .iter()
            .filter(|k| k.is(animal) && k.score > 0)
            .fold(None, |best: Option<&Kill>, k| match best {
                Some(b) if b.score >= k.score => Some(b),
                _ => Some(k),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

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
            scale: 0.0,
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

    #[test]
    fn per_species_records_keep_size_and_read_old_lines() {
        // A line written before the size was kept, and two with it.
        let text = "kill\thunt\tT-Rex\t25\t40.5\tRifle\tArea\t7\n\
                    kill\thunt\tt-rex \t31\t12.0\tRifle\tArea\t8\t1.150\n\
                    kill\tsurvival\tT-Rex\t0\t9.0\tRifle\tArea\t9\t1.400\n\
                    kill\thunt\tRaptor\t12\t9.0\tRifle\tArea\t9\t1.900\n";
        let r = Record::parse(text);
        assert_eq!(r.kills.len(), 4);
        assert_eq!(r.kills[0].scale, 0.0);
        assert_eq!(r.count_of("T-REX"), 3);
        assert_eq!(r.biggest("T-Rex").map(|k| k.date), Some(9));
        assert_eq!(r.best_of("T-Rex").map(|k| k.score), Some(31));
        assert!(r.biggest("Stego").is_none() && r.best_of("Stego").is_none());
        let back = Record::parse(&r.write());
        assert_eq!(back.biggest("t-rex").map(|k| k.scale), Some(1.4));
        // An unrecorded size never wins, nor does a malformed one.
        let odd = Record::parse("kill\thunt\tA\t5\t1\tW\tX\t1\tNaN\n");
        assert_eq!(odd.kills.len(), 1);
        assert!(odd.biggest("A").is_none());
        // Too many fields: not a kill line this version knows.
        assert!(Record::parse("kill\thunt\tA\t5\t1\tW\tX\t1\t1\tz\n")
            .kills
            .is_empty());
    }
}
