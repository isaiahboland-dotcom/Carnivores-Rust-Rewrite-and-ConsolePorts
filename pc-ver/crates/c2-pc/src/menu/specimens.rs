//! The field manual's specimen files on the PC and Android
//! (`c2_core::dhc::specimen`): each huntable's facts, read with the menu's
//! lists, and its model's height, measured when its file first opens.

use c2_core::dhc::specimen::{height_ratio, unpermitted, used_points, Specimen};
use c2_formats::res_txt::ResData;

use super::*;

/// The facts behind each huntable's file, by the animal column's item.
#[derive(Default)]
pub(super) struct Specimens {
    pub(super) facts: Vec<Specimen>,
    /// Each one's model (`_RES.TXT`'s `file`).
    pub(super) models: Vec<Option<String>>,
    /// Its description as the game's files give it (`DINOn.TXM`, `.TXU`).
    pub(super) texts: Vec<Vec<String>>,
    pub(super) texts_us: Vec<Vec<String>>,
    /// Whether its model's height was measured yet.
    measured: Vec<bool>,
}

/// What the facts are read from (the menu's lists as loaded).
pub(super) struct Sources<'a> {
    pub(super) huntdat: &'a Path,
    pub(super) data: &'a ResData,
    pub(super) menu: &'a MenuData,
    /// A Modder's Edition game's `_RES.TXT`.
    pub(super) dialect: Option<&'a ResTxt>,
    /// The menu's own animals (the roaming add-ons follow).
    pub(super) menu_animals: usize,
    pub(super) scores: &'a [Option<f32>],
    /// Per huntable, the areas it spawns on when picked (dialect games).
    pub(super) homes: Option<&'a [Vec<usize>]>,
    pub(super) areas: &'a [PathBuf],
    pub(super) texts: Vec<Vec<String>>,
    pub(super) texts_us: Vec<Vec<String>>,
}

impl Specimens {
    pub(super) fn read(src: Sources) -> Self {
        let Sources {
            huntdat,
            data,
            menu,
            dialect,
            menu_animals,
            scores,
            homes,
            areas,
            texts,
            texts_us,
        } = src;
        let predation = find_file(
            huntdat,
            c2_formats::predation::PATH.trim_start_matches("HUNTDAT/"),
        )
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| c2_formats::predation::Predation::parse(&b));
        let def_of = |name: &str| {
            data.characters.iter().find(|d| {
                d.name
                    .as_deref()
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim()))
            })
        };
        // Where each may turn up without a permit: a dialect game's spawn
        // tables, else the original's random first four (every map).
        let n = menu.huntables.len();
        let appears: Vec<Vec<usize>> = match dialect {
            Some(res) => {
                let roamers = c2_core::mee::roamers(res, menu_animals);
                roam_homes(res, n, areas, &roamers)
            }
            None => {
                let ais: Vec<i32> = data.characters.iter().map(|d| d.ai.unwrap_or(0)).collect();
                let free: Vec<&str> = unpermitted(&ais)
                    .into_iter()
                    .filter_map(|i| data.characters.get(i)?.name.as_deref())
                    .collect();
                let everywhere: Vec<usize> = (0..areas.len()).collect();
                menu.huntables
                    .iter()
                    .map(|h| {
                        if free
                            .iter()
                            .any(|f| f.trim().eq_ignore_ascii_case(h.name.trim()))
                        {
                            everywhere.clone()
                        } else {
                            Vec::new()
                        }
                    })
                    .collect()
            }
        };
        let mut facts = Vec::with_capacity(n);
        let mut models = Vec::with_capacity(n);
        for (k, h) in menu.huntables.iter().enumerate() {
            let def = def_of(&h.name);
            let ai = def.and_then(|d| d.ai).unwrap_or(h.ai);
            let scale0 = def.and_then(|d| d.scale0).unwrap_or(800) as f32 / 1000.0;
            let scale_a = def.and_then(|d| d.scale_a).unwrap_or(600).max(0) as f32 / 1000.0;
            let diet = c2_core::dino::predation::diet(
                predation.as_ref(),
                &h.name,
                dialect.map(|_| ai),
                ai,
            );
            // A roaming add-on is at home where it roams.
            let homes = match homes {
                Some(hs) if k < menu_animals => hs.get(k).cloned(),
                Some(_) => appears.get(k).cloned(),
                None => None,
            };
            facts.push(Specimen {
                name: h.name.clone(),
                permit: h.price,
                score: scores.get(k).copied().flatten().map(|s| s as i32),
                length: if h.length > 0.0 {
                    h.length
                } else {
                    def.and_then(|d| d.length).unwrap_or(0.0)
                },
                mass: if h.mass > 0.0 {
                    h.mass
                } else {
                    def.and_then(|d| d.mass).unwrap_or(0.0)
                },
                scales: (scale0, scale0 + scale_a),
                height_ratio: None,
                diet,
                homes,
                appears: appears.get(k).cloned().unwrap_or_default(),
            });
            models.push(def.and_then(|d| d.file.clone()));
        }
        Specimens {
            measured: vec![false; facts.len()],
            facts,
            models,
            texts,
            texts_us,
        }
    }

    /// Huntable `k`'s model, read from the game (none: no model, or a
    /// broken one).
    pub(super) fn model(&self, huntdat: &Path, k: usize) -> Option<c2_formats::car::Car> {
        let file = self.models.get(k)?.as_deref()?;
        let path = find_file(huntdat, file)?;
        match c2_formats::car::parse(&std::fs::read(&path).ok()?) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!("{}: {e}", path.display());
                None
            }
        }
    }

    /// Measures huntable `k`'s height from `car` (its first animation's
    /// first frame, else its mesh), once.
    pub(super) fn measure(&mut self, k: usize, car: Option<&c2_formats::car::Car>) {
        if self.measured.get(k).copied().unwrap_or(true) {
            return;
        }
        self.measured[k] = true;
        let Some(c) = car else { return };
        let points = c
            .animations
            .first()
            .and_then(|a| c2_core::model::pose_car(a, c.mesh.vertices.len(), 0))
            .unwrap_or_else(|| c.mesh.vertices.iter().map(|v| [v.x, v.y, v.z]).collect());
        if let Some(f) = self.facts.get_mut(k) {
            f.height_ratio = height_ratio(&used_points(&points, &c.mesh.faces));
        }
    }

    /// Whether huntable `k`'s height is still to be measured.
    pub(super) fn unmeasured(&self, k: usize) -> bool {
        self.measured.get(k).is_some_and(|m| !m)
    }
}
