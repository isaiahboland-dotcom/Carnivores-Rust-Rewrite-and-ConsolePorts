//! The field manual's specimen database (DHC OS on PC and Android): what
//! the terminal shows about one huntable animal, worked out from the
//! pack's data and the hunter's record, and laid out as the lines of its
//! file. The platform supplies the facts it read ([`Specimen`]) and the
//! area names; this module decides what is said and how.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

pub use c2_formats::predation::Diet;

use super::{island_allows, Catalog, Rules, Tab};
use crate::record::{Kill, Mode, Record};

/// Seconds a file's retrieval sequence runs before the file shows.
pub const RETRIEVE_S: f32 = 1.4;

/// One huntable animal's facts, as the platform read them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Specimen {
    pub name: String,
    /// The permit's price: the points needed to hunt it.
    pub permit: i32,
    /// Its `basescore` as the hunt adds it (none: not known).
    pub score: Option<i32>,
    /// `_RES.TXT`'s `length` (metres) and `mass` (tonnes) at scale 1;
    /// 0 when not given.
    pub length: f32,
    pub mass: f32,
    /// The smallest and largest size the hunt gives it:
    /// `(scale0, scale0 + scaleA) / 1000` (`Character::reset`).
    pub scales: (f32, f32),
    /// Its model's height over its length at rest, once the model is read
    /// (none: not read, or no model).
    pub height_ratio: Option<f32>,
    pub diet: Option<Diet>,
    /// The areas (catalog indices) the game spawns it on when picked
    /// (a Modder's Edition game's spawn tables); none: every area.
    pub homes: Option<Vec<usize>>,
    /// The areas (catalog indices) where it may turn up without a permit.
    pub appears: Vec<usize>,
}

/// One line of a specimen's file.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A section's heading.
    Head(&'static str),
    /// A labelled value.
    Fact(&'static str, String),
    /// A list entry or a remark.
    Item(String),
    Blank,
}

/// The species the original places on every map without a permit
/// (`Characters.cpp: PlaceCharacters`, `dino::place`): the first four of
/// the main population are random among the seven species from the first
/// with AI 10. `ais`: each species' AI, in species order.
pub fn unpermitted(ais: &[i32]) -> Vec<usize> {
    match ais.iter().position(|&a| a == 10) {
        Some(f) => (f..(f + 7).min(ais.len())).collect(),
        None => Vec::new(),
    }
}

/// The areas animal `animal` (catalog index) can be hunted on: where the
/// island rules allow it and, when the game gives it homes, one of them.
/// None when that is every area (unrestricted).
pub fn native_areas(
    cat: &Catalog,
    rules: &Rules,
    animal: usize,
    homes: Option<&[usize]>,
) -> Option<Vec<usize>> {
    let name = cat.animals.get(animal).map_or("", |a| a.name.as_str());
    let v: Vec<usize> = (0..cat.areas.len())
        .filter(|&a| island_allows(rules.island(cat, a), Tab::Targets, name))
        .filter(|a| homes.is_none_or(|h| h.contains(a)))
        .collect();
    (v.len() != cat.areas.len()).then_some(v)
}

/// The points some face of the model uses: a model's stray vertices
/// (none drawn) would stretch its measure and its framing.
pub fn used_points(points: &[[f32; 3]], faces: &[c2_formats::model::Face]) -> Vec<[f32; 3]> {
    let mut used = vec![false; points.len()];
    for f in faces {
        for &v in &f.v {
            if let Some(u) = usize::try_from(v).ok().and_then(|v| used.get_mut(v)) {
                *u = true;
            }
        }
    }
    let kept: Vec<[f32; 3]> = points
        .iter()
        .zip(&used)
        .filter(|(_, u)| **u)
        .map(|(p, _)| *p)
        .collect();
    if kept.is_empty() {
        points.to_vec()
    } else {
        kept
    }
}

/// A model's height over its length, from its points (one pose): the
/// vertical extent over the longer horizontal one. None for a flat or
/// empty model.
pub fn height_ratio(points: &[[f32; 3]]) -> Option<f32> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let (h, len) = (hi[1] - lo[1], (hi[0] - lo[0]).max(hi[2] - lo[2]));
    (h.is_finite() && len.is_finite() && h > 0.0 && len > 0.0).then(|| h / len)
}

/// The retrieval sequence's log for the file of `name`; `model`: a 3D
/// specimen is reconstructed (else the photograph is retrieved).
pub fn retrieve_log(name: &str, model: bool) -> Vec<String> {
    vec![
        "> QUERYING DHC SPECIMEN DATABASE...".into(),
        format!("> FILE LOCATED: {}", name.trim().to_uppercase()),
        "> DECRYPTING FIELD NOTES...".into(),
        if model {
            "> RECONSTRUCTING 3D SPECIMEN...".into()
        } else {
            "> RETRIEVING PHOTOGRAPH...".into()
        },
        "> CROSS-CHECKING HUNTER RECORDS...".into(),
        "> ACCESS GRANTED".into(),
    ]
}

/// Lines of `log` shown `t` seconds into the retrieval.
pub fn retrieve_shown(log_len: usize, t: f32) -> usize {
    let step = RETRIEVE_S / log_len.max(1) as f32;
    ((t / step) as usize + 1).min(log_len)
}

/// A length in metres, or feet for `us` (the trophy plaque's units).
fn length(m: f32, us: bool) -> String {
    if us {
        format!("{:.1} ft", m * 3.281)
    } else {
        format!("{m:.1} m")
    }
}

/// A weight in tonnes, or short tons for `us` (the plaque's).
fn weight(t: f32, us: bool) -> String {
    if us {
        format!("{:.2} t", t / 0.907)
    } else {
        format!("{t:.2} T")
    }
}

/// `lo - hi` in `unit`, or one value when they round alike.
fn range(lo: f32, hi: f32, unit: impl Fn(f32) -> String) -> String {
    let (a, b) = (unit(lo), unit(hi));
    if a == b {
        a
    } else {
        format!("{a} - {b}")
    }
}

/// A date as the trophies keep it, in the plaque's order.
fn date(d: i32, us: bool) -> String {
    let (day, month, year) = (d & 255, (d >> 10) & 255, d >> 20);
    if us {
        format!("{month}.{day}.{year}")
    } else {
        format!("{day}.{month}.{year}")
    }
}

impl Specimen {
    /// Its weight at size `s` (the trophy plaque's `mass * s * s`).
    pub fn weight_at(&self, s: f32) -> f32 {
        self.mass * s * s
    }

    /// The file's lines. `areas`: the area names by catalog index;
    /// `native`: [`native_areas`]; `rec`: the hunter's record (none: no
    /// hunter signed in); `us`: US units.
    pub fn lines(
        &self,
        areas: &[String],
        native: Option<&[usize]>,
        rec: Option<&Record>,
        us: bool,
    ) -> Vec<Line> {
        let mut v = vec![Line::Head("CLASSIFICATION")];
        let diet = match self.diet {
            Some(Diet::Carnivore) => "CARNIVORE",
            Some(Diet::Herbivore) => "HERBIVORE",
            None => "UNCLASSIFIED",
        };
        v.push(Line::Fact("DIET", diet.into()));
        let (lo, hi) = self.scales;
        if self.length > 0.0 {
            let l = self.length;
            v.push(Line::Fact(
                "LENGTH",
                range(l * lo, l * hi, |x| length(x, us)),
            ));
            if let Some(r) = self.height_ratio.filter(|r| *r > 0.0) {
                let h = range(l * r * lo, l * r * hi, |x| length(x, us));
                v.push(Line::Fact("HEIGHT", format!("{h} (EST.)")));
            }
        }
        if self.mass > 0.0 {
            v.push(Line::Fact(
                "WEIGHT",
                range(self.weight_at(lo), self.weight_at(hi), |x| weight(x, us)),
            ));
        }
        v.push(Line::Blank);
        v.push(Line::Head("LICENSING"));
        v.push(Line::Fact("PERMIT", format!("{} PTS", self.permit)));
        if let Some(s) = self.score {
            // Whole points, as the hunt adds them (tranquilized x1.25).
            let tranq = (s as f32 * 1.25) as i32;
            v.push(Line::Fact("BOUNTY", format!("{s} PTS (TRANQ {tranq})")));
        }
        let name = |a: &usize| {
            areas
                .get(*a)
                .cloned()
                .unwrap_or_else(|| format!("AREA {}", a + 1))
        };
        if let Some(n) = native {
            v.push(Line::Blank);
            v.push(Line::Head("NATIVE TO"));
            if n.is_empty() {
                v.push(Line::Item("NO AUTHORISED AREA".into()));
            }
            v.extend(n.iter().map(|a| Line::Item(name(a))));
        }
        v.push(Line::Blank);
        v.push(Line::Head("MAY APPEAR ON"));
        if self.appears.is_empty() {
            v.push(Line::Item("PERMIT HUNTS ONLY".into()));
        } else if self.appears.len() == areas.len() && !areas.is_empty() {
            v.push(Line::Item("EVERY AREA".into()));
        } else {
            v.extend(self.appears.iter().map(|a| Line::Item(name(a))));
        }
        v.push(Line::Blank);
        v.push(Line::Head("HUNTER RECORDS"));
        let Some(rec) = rec else {
            v.push(Line::Item("NO HUNTER SIGNED IN".into()));
            return v;
        };
        let kills = rec.count_of(&self.name);
        if kills == 0 {
            v.push(Line::Item("NO KILLS ON FILE".into()));
            return v;
        }
        v.push(Line::Fact("KILLS", kills.to_string()));
        match rec.biggest(&self.name) {
            Some(k) => {
                let mut s = weight(self.weight_at(k.scale), us);
                if self.length > 0.0 {
                    s = format!("{s} / {}", length(self.length * k.scale, us));
                }
                v.push(Line::Fact("LARGEST", s));
                v.push(Line::Item(self.kill_note(k, us)));
            }
            // Kills recorded before sizes were kept.
            None => v.push(Line::Fact("LARGEST", "SIZE NOT RECORDED".into())),
        }
        if let Some(k) = rec.best_of(&self.name) {
            v.push(Line::Fact("TOP SCORE", format!("{} PTS", k.score)));
            v.push(Line::Item(self.kill_note(k, us)));
        }
        v.push(Line::Item(
            "ARCHIVED: KEPT AFTER A TROPHY LEAVES THE ROOM".into(),
        ));
        v
    }

    /// Where and how a kill was made, under its record.
    fn kill_note(&self, k: &Kill, us: bool) -> String {
        let mut parts = Vec::new();
        for s in [&k.weapon, &k.area] {
            if !s.trim().is_empty() {
                parts.push(s.trim().to_uppercase());
            }
        }
        if k.mode == Mode::Survival {
            parts.push("SURVIVAL".into());
        }
        if k.date != 0 {
            parts.push(date(k.date, us));
        }
        format!("  {}", parts.join(", "))
    }
}
