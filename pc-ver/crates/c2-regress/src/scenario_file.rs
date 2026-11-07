//! Scenario files (`scenarios/*.toml`): one logical scenario, with a
//! section per game saying whether it runs there and what it places.

use std::path::Path;

use anyhow::{bail, Context, Result};
use c2_core::scenario::{Scenario, SpeciesRef};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Head {
    id: String,
    version: u32,
    seed: u32,
    warmup_ticks: u32,
    settle_ticks: u32,
    timeout_ticks: u32,
}

#[derive(Debug, Deserialize)]
struct View {
    #[serde(default)]
    landing: usize,
    cell: Option<[u16; 2]>,
    yaw_deg: f32,
    pitch_deg: f32,
}

#[derive(Debug, Deserialize)]
struct Anchor {
    distance: f32,
    bearing_deg: f32,
    fire_clearance_deg: f32,
    offsets: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameStatus {
    Supported,
    Unsupported,
    Planned,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameDef {
    pub id: String,
    pub name: String,
    pub status: GameStatus,
    #[serde(default)]
    pub reason: String,
    pub area: Option<String>,
    #[serde(default)]
    pub species: Vec<String>,
    /// The verified size of the game's weapon registry.
    pub weapons: Option<usize>,
    /// Overrides `[anchor] distance` (games with larger animals).
    pub anchor_distance: Option<f32>,
    /// Overrides `[view] landing` (areas where the first one is hidden).
    pub landing: Option<usize>,
    /// Overrides `[view] yaw_deg`.
    pub yaw_deg: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct Raw {
    scenario: Head,
    view: View,
    anchor: Anchor,
    game: Vec<GameDef>,
}

pub struct ScenarioFile {
    raw: Raw,
    /// SHA-256 of the file's text.
    pub hash: String,
}

fn species_ref(s: &str) -> Result<SpeciesRef> {
    let (kind, n) = s
        .split_once(':')
        .with_context(|| format!("species {s:?}: expected ai:N or index:N"))?;
    match kind {
        "ai" => Ok(SpeciesRef::Ai(n.parse().context("species ai:N")?)),
        "index" => Ok(SpeciesRef::Index(n.parse().context("species index:N")?)),
        _ => bail!("species {s:?}: expected ai:N or index:N"),
    }
}

impl ScenarioFile {
    pub fn parse(text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(text).context("reading the scenario")?;
        if raw.game.is_empty() {
            bail!("the scenario names no games");
        }
        if raw.anchor.offsets.is_empty() {
            bail!("the scenario places no animals");
        }
        let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
        Ok(Self { raw, hash })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| path.display().to_string())
    }

    pub fn id(&self) -> &str {
        &self.raw.scenario.id
    }

    pub fn version(&self) -> u32 {
        self.raw.scenario.version
    }

    pub fn games(&self) -> &[GameDef] {
        &self.raw.game
    }

    pub fn game(&self, id: &str) -> Result<&GameDef> {
        self.raw.game.iter().find(|g| g.id == id).with_context(|| {
            let ids: Vec<&str> = self.raw.game.iter().map(|g| g.id.as_str()).collect();
            format!("no game {id:?} in the scenario (it has {ids:?})")
        })
    }

    /// The scenario as `game` runs it. Fails for games it does not run on.
    pub fn for_game(&self, game: &GameDef) -> Result<Scenario> {
        if game.status != GameStatus::Supported {
            bail!(
                "{} is {:?} for this scenario: {}",
                game.name,
                game.status,
                game.reason
            );
        }
        let (h, v, a) = (&self.raw.scenario, &self.raw.view, &self.raw.anchor);
        let species = game
            .species
            .iter()
            .map(|s| species_ref(s))
            .collect::<Result<Vec<_>>>()?;
        if species.len() != a.offsets.len() {
            bail!(
                "{}: {} species for {} anchor offsets",
                game.id,
                species.len(),
                a.offsets.len()
            );
        }
        Ok(Scenario {
            id: h.id.clone(),
            version: h.version,
            seed: h.seed,
            landing: game.landing.unwrap_or(v.landing),
            cell: v.cell.map(|[x, z]| (x, z)),
            yaw: game.yaw_deg.unwrap_or(v.yaw_deg).to_radians(),
            pitch: v.pitch_deg.to_radians(),
            anchor_distance: game.anchor_distance.unwrap_or(a.distance),
            anchor_bearing: a.bearing_deg.to_radians(),
            offsets: a.offsets.clone(),
            species,
            fire_clearance: a.fire_clearance_deg.to_radians(),
            warmup_ticks: h.warmup_ticks,
            settle_ticks: h.settle_ticks,
            timeout_ticks: h.timeout_ticks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = include_str!("../scenarios/vanilla_map2_four_dinos.toml");

    #[test]
    fn the_canonical_scenario_parses() {
        let f = ScenarioFile::parse(FILE).unwrap();
        assert_eq!(f.id(), "vanilla_map2_four_dinos");
        let c2 = f.game("c2").unwrap();
        let s = f.for_game(c2).unwrap();
        assert_eq!(s.species.len(), 4);
        assert_eq!(s.species[0], SpeciesRef::Ai(18));
        assert_eq!(c2.weapons, Some(6));
        assert_eq!(s.anchor_distance, 3072.0);
        let ice = f.for_game(f.game("ice-age").unwrap()).unwrap();
        assert_eq!(ice.anchor_distance, 5120.0);
        assert_eq!(ice.landing, 0);
        assert_eq!(s.yaw, 0.0);
        assert!(f.for_game(f.game("fallen-kings").unwrap()).is_err());
        // Per-game view overrides and species by index.
        let extra = format!(
            "{FILE}\n[[game]]\nid = \"dialect\"\nname = \"D\"\nstatus = \"supported\"\n\
             area = \"AREA1\"\nlanding = 1\nyaw_deg = 270.0\n\
             species = [\"index:4\", \"index:5\", \"index:6\", \"index:7\"]\n"
        );
        let f = ScenarioFile::parse(&extra).unwrap();
        let d = f.for_game(f.game("dialect").unwrap()).unwrap();
        assert_eq!(d.species[0], SpeciesRef::Index(4));
        assert_eq!(d.landing, 1);
        assert_eq!(d.yaw, 270f32.to_radians());
        assert!(f.for_game(f.game("c1").unwrap()).is_err());
        assert!(f.game("c3").is_err());
    }

    #[test]
    fn broken_files_are_refused() {
        assert!(ScenarioFile::parse("").is_err());
        let wrong = FILE.replace("\"ai:14\"", "\"tri:1\"");
        let f = ScenarioFile::parse(&wrong).unwrap();
        assert!(f.for_game(f.game("c2").unwrap()).is_err());
        let short = FILE.replace(", \"ai:14\"]", "]");
        let f = ScenarioFile::parse(&short).unwrap();
        assert!(f.for_game(f.game("c2").unwrap()).is_err());
    }
}
