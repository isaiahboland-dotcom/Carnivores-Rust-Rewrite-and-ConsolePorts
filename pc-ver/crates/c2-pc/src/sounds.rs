//! Every sound a hunt can play, loaded once (`Hunt2.cpp` WinMain loads,
//! character/weapon/ship `.CAR` sounds, the area's `.RSC` sounds).

use std::path::Path;

use c2_formats::car::Car;
use c2_formats::rsc::Rsc;

use crate::area_view::find_file;
use crate::audio::{Clip, SAMPLE_RATE};

pub struct SoundBank {
    pub species: Vec<Vec<Option<Clip>>>,
    pub weapons: Vec<Vec<Option<Clip>>>,
    pub ship: Vec<Option<Clip>>,
    pub random: Vec<Option<Clip>>,
    pub ambients: Vec<Option<Clip>>,
    pub underwater: Option<Clip>,
    pub steps: Vec<Option<Clip>>,
    pub steps_water: Vec<Option<Clip>>,
    pub screams: Vec<Option<Clip>>,
    /// Hunting calls by AI 10.. : `CALLS/CALL{n}_A/B/C.WAV`.
    pub calls: Vec<Vec<Option<Clip>>>,
}

fn car_sounds(car: Option<&Car>) -> Vec<Option<Clip>> {
    car.map_or_else(Vec::new, |c| {
        c.sounds
            .iter()
            .map(|s| Clip::from_pcm(&s.pcm, SAMPLE_RATE, 1))
            .collect()
    })
}

fn wav(dir: Option<&Path>, name: &str) -> Option<Clip> {
    dir.and_then(|d| find_file(d, name))
        .and_then(|p| Clip::from_wav(&p))
}

impl SoundBank {
    pub fn load(
        huntdat: &Path,
        species: &[Option<&Car>],
        weapons: &[Option<&Car>],
        ship: Option<&Car>,
        rsc: &Rsc,
    ) -> Self {
        let fx = find_file(huntdat, "SOUNDFX");
        let steps_dir = fx.as_deref().and_then(|d| find_file(d, "STEPS"));
        let many = |dir: Option<&Path>, names: &[&str]| {
            names.iter().map(|n| wav(dir, n)).collect::<Vec<_>>()
        };
        let bank = Self {
            species: species.iter().map(|c| car_sounds(*c)).collect(),
            weapons: weapons.iter().map(|c| car_sounds(*c)).collect(),
            ship: car_sounds(ship),
            random: rsc
                .random_sounds
                .iter()
                .map(|p| Clip::from_pcm(p, SAMPLE_RATE, 1))
                .collect(),
            ambients: rsc
                .ambients
                .iter()
                .map(|a| Clip::from_pcm(&a.pcm, SAMPLE_RATE, 1))
                .collect(),
            underwater: wav(fx.as_deref(), "A_UNDERW.WAV"),
            steps: many(
                steps_dir.as_deref(),
                &["HWALK1.WAV", "HWALK2.WAV", "HWALK3.WAV"],
            ),
            steps_water: many(
                steps_dir.as_deref(),
                &["FOOTW1.WAV", "FOOTW2.WAV", "FOOTW3.WAV"],
            ),
            screams: many(
                fx.as_deref(),
                &[
                    "HUM_DIE1.WAV",
                    "HUM_DIE2.WAV",
                    "HUM_DIE3.WAV",
                    "HUM_DIE4.WAV",
                ],
            ),
            calls: {
                let dir = fx.as_deref().and_then(|d| find_file(d, "CALLS"));
                (1..=10)
                    .map(|n| {
                        ["A", "B", "C"]
                            .iter()
                            .map(|v| wav(dir.as_deref(), &format!("CALL{n}_{v}.WAV")))
                            .collect()
                    })
                    .collect()
            },
        };
        let n = |v: &[Option<Clip>]| v.iter().filter(|c| c.is_some()).count();
        let nn = |v: &[Vec<Option<Clip>>]| v.iter().map(|x| n(x)).sum::<usize>();
        tracing::info!(
            "sounds: {} species, {} weapon, {} ship, {} random, {} ambient, {} steps, {} screams",
            nn(&bank.species),
            nn(&bank.weapons),
            n(&bank.ship),
            n(&bank.random),
            n(&bank.ambients),
            n(&bank.steps) + n(&bank.steps_water),
            n(&bank.screams)
        );
        bank
    }
}

/// `clips[i]`, if loaded.
pub fn get(clips: &[Option<Clip>], i: usize) -> Option<&Clip> {
    clips.get(i).and_then(Option::as_ref)
}
