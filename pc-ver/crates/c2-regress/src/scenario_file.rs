//! Scenario files (`scenarios/*.toml`): one logical scenario, with a
//! section per game saying whether it runs there and what it places.

use std::path::Path;

use anyhow::{bail, Context, Result};
use c2_core::scenario::{KillPlan, Scenario, Script, SpeciesRef};
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
    /// `tour` (default) or `kills`.
    #[serde(default)]
    script: ScriptName,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScriptName {
    #[default]
    Tour,
    Kills,
}

/// `[kills]`: the kill script's distances and timings (`KillPlan`).
#[derive(Debug, Deserialize)]
struct Kills {
    start_distance: f32,
    timeout_ticks: u32,
    mid_ticks: u32,
    late_ticks: u32,
    min_moving_ticks: u32,
    body_reach: f32,
    min_body_reach: f32,
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
    kills: Option<Kills>,
    game: Vec<GameDef>,
}

pub struct ScenarioFile {
    raw: Raw,
    /// SHA-256 of the file's text with LF line endings, so a CRLF
    /// checkout (Windows) gives the same hash as the committed file.
    pub hash: String,
}

fn species_ref(s: &str) -> Result<SpeciesRef> {
    let (kind, n) = s
        .split_once(':')
        .with_context(|| format!("species {s:?}: expected ai:N, index:N or model:FILE"))?;
    match kind {
        "ai" => Ok(SpeciesRef::Ai(n.parse().context("species ai:N")?)),
        "index" => Ok(SpeciesRef::Index(n.parse().context("species index:N")?)),
        "model" if !n.is_empty() => Ok(SpeciesRef::Model(n.into())),
        _ => bail!("species {s:?}: expected ai:N, index:N or model:FILE"),
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
        if raw.scenario.script == ScriptName::Kills && raw.kills.is_none() {
            bail!("script = \"kills\" needs a [kills] section");
        }
        let hash = format!("{:x}", Sha256::digest(text.replace("\r\n", "\n")));
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
        // The tour places exactly one animal per offset; the kill script
        // takes the first offsets it needs (games have different killers).
        let fits = match h.script {
            ScriptName::Tour => species.len() == a.offsets.len(),
            ScriptName::Kills => species.len() <= a.offsets.len(),
        };
        if !fits || species.is_empty() {
            bail!(
                "{}: {} species for {} anchor offsets",
                game.id,
                species.len(),
                a.offsets.len()
            );
        }
        let script = match (h.script, &self.raw.kills) {
            (ScriptName::Kills, Some(k)) => Script::Kills(KillPlan {
                start_distance: k.start_distance,
                timeout_ticks: k.timeout_ticks,
                mid_ticks: k.mid_ticks,
                late_ticks: k.late_ticks,
                min_moving_ticks: k.min_moving_ticks,
                body_reach: k.body_reach,
                min_body_reach: k.min_body_reach,
            }),
            _ => Script::Tour,
        };
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
            offsets: a.offsets[..species.len()].to_vec(),
            species,
            fire_clearance: a.fire_clearance_deg.to_radians(),
            warmup_ticks: h.warmup_ticks,
            settle_ticks: h.settle_ticks,
            timeout_ticks: h.timeout_ticks,
            script,
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
    fn the_kill_scenario_parses() {
        let text = include_str!("../scenarios/kill_animations.toml");
        let f = ScenarioFile::parse(text).unwrap();
        assert_eq!(f.id(), "kill_animations");
        let ice = f.for_game(f.game("ice-age").unwrap()).unwrap();
        assert_eq!(ice.species.len(), 3);
        assert_eq!(ice.offsets.len(), 3, "the first offsets, one per killer");
        let Script::Kills(k) = &ice.script else {
            panic!("kill script expected")
        };
        assert_eq!((k.start_distance, k.timeout_ticks), (600.0, 1500));
        let addons = f.for_game(f.game("c2-addons").unwrap()).unwrap();
        assert_eq!(addons.species[0], SpeciesRef::Model("diablo.car".into()));
        // The tour stays exact, and a kill script needs its section.
        assert_eq!(f.for_game(f.game("c2").unwrap()).unwrap().species.len(), 2);
        assert!(ScenarioFile::parse(&text.replace("[kills]", "[other]")).is_err());
        assert!(
            ScenarioFile::parse(&text.replace("\"model:ta10f.car\"", "\"model:\""))
                .is_ok_and(|f| f.for_game(f.game("triassic-mee").unwrap()).is_err())
        );
    }

    #[test]
    fn the_hash_ignores_crlf_line_endings() {
        let crlf = FILE.replace('\n', "\r\n");
        let lf = ScenarioFile::parse(FILE).unwrap().hash;
        assert_eq!(ScenarioFile::parse(&crlf).unwrap().hash, lf);
        let plain = format!("{:x}", Sha256::digest(FILE.as_bytes()));
        assert_eq!(lf, plain, "an LF file keeps its published hash");
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
