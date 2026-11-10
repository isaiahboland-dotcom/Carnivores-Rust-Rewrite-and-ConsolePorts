//! Public configuration and tolerant comparison policy for private texture reviews.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Some death/hidden clips deliberately collapse the source mesh to a point.
/// Recognize this from geometry, never from an empty screenshot alone.
pub fn collapsed_pose(posed: &[[f32; 3]], reference: &[[f32; 3]]) -> bool {
    fn extent(points: &[[f32; 3]]) -> Option<f32> {
        if points.is_empty() || points.iter().flatten().any(|p| !p.is_finite()) {
            return None;
        }
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in points {
            for axis in 0..3 {
                lo[axis] = lo[axis].min(p[axis]);
                hi[axis] = hi[axis].max(p[axis]);
            }
        }
        Some((0..3).map(|a| hi[a] - lo[a]).fold(0., f32::max))
    }
    matches!((extent(posed), extent(reference)), (Some(p), Some(r)) if r > 0. && p < r * 0.001)
}

/// Versioned capture inputs, shared by baseline replay and renderer adapters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CaptureRecipe {
    pub schema: u32,
    pub animation: usize,
    pub time_ms: u32,
    pub center: [f32; 3],
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub vertical_half_angle_tangent: f32,
    pub near: f32,
    pub far: f32,
    pub background: [f64; 3],
    pub ambient: f32,
    pub instance_light: f32,
    pub brightness: f32,
    pub daylight: [f32; 3],
    pub fog: [f32; 2],
    pub resolution: [u32; 2],
}
impl CaptureRecipe {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported capture recipe version");
        ensure!(
            self.center
                .iter()
                .chain([
                    &self.distance,
                    &self.yaw,
                    &self.pitch,
                    &self.vertical_half_angle_tangent,
                    &self.near,
                    &self.far,
                    &self.ambient,
                    &self.instance_light,
                    &self.brightness
                ])
                .chain(self.daylight.iter())
                .chain(self.fog.iter())
                .all(|v| v.is_finite())
                && self.background.iter().all(|v| v.is_finite()),
            "non-finite capture recipe"
        );
        ensure!(
            self.distance > 0.
                && self.near > 0.
                && self.far > self.near
                && self.vertical_half_angle_tangent > 0.
                && self.pitch.abs() < std::f32::consts::FRAC_PI_2
                && self.fog[1] > self.fog[0]
                && self.resolution.iter().all(|v| (128..=4096).contains(v)),
            "invalid capture camera or resolution"
        );
        ensure!(
            self.background.iter().all(|v| (0.0..=1.0).contains(v))
                && [self.ambient, self.instance_light, self.brightness]
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v)),
            "invalid capture lighting"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub asset_type: Option<String>,
    pub width: u32,
    pub height: u32,
    pub phases: Vec<f64>,
    /// Empty means all clips; explicit indices allow compact texture sweeps.
    pub animations: Vec<usize>,
    pub keep_models: bool,
    pub channel_tolerance: u8,
    pub changed_fraction: f64,
    pub source: Vec<Source>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            asset_type: None,
            width: 960,
            height: 720,
            phases: vec![0., 0.33, 0.67],
            animations: Vec::new(),
            keep_models: false,
            channel_tolerance: 12,
            changed_fraction: 0.005,
            source: Vec::new(),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub kind: String,
    pub path: PathBuf,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub yaw_offset_degrees: f32,
    #[serde(default)]
    pub pitch_degrees: f32,
}
impl Config {
    pub fn asset_type(&self) -> &str {
        self.asset_type
            .as_deref()
            .unwrap_or(if self.animations.is_empty() {
                "animation-archive"
            } else {
                "texture-sweep"
            })
    }
    pub fn parse(text: &str) -> Result<Self> {
        let mut config: Self = toml::from_str(text)?;
        ensure!(
            ["texture-sweep", "animation-archive"].contains(&config.asset_type()),
            "Rust reviews support texture-sweep or animation-archive assets"
        );
        ensure!(
            config.asset_type() != "animation-archive" || config.animations.is_empty(),
            "animation-archive must select every clip (animations = [])"
        );
        // An animation archive must preserve the actual clips, not only sampled PNGs.
        if config.asset_type() == "animation-archive" {
            config.keep_models = true;
        }
        ensure!(
            (128..=4096).contains(&config.width) && (128..=4096).contains(&config.height),
            "resolution must be 128..4096"
        );
        ensure!(
            config.changed_fraction.is_finite() && (0.0..=1.0).contains(&config.changed_fraction),
            "changed_fraction must be in [0,1]"
        );
        ensure!(
            !config.phases.is_empty()
                && config
                    .phases
                    .iter()
                    .all(|p| p.is_finite() && (0.0..1.0).contains(p)),
            "phases must be in [0,1)"
        );
        ensure!(!config.source.is_empty(), "no [[source]] entries");
        let mut ids = BTreeSet::new();
        for source in &config.source {
            ensure!(
                source.yaw_offset_degrees.is_finite()
                    && source.pitch_degrees.is_finite()
                    && source.yaw_offset_degrees.abs() <= 45.
                    && source.pitch_degrees.abs() <= 45.,
                "review camera offsets must be within 45 degrees"
            );
            ensure!(
                !source.id.is_empty()
                    && source
                        .id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                    && ids.insert(&source.id),
                "source ids must be unique safe identifiers"
            );
            ensure!(
                ["apk", "game", "cars"].contains(&source.kind.as_str()),
                "kind must be apk, game or cars"
            );
            ensure!(!source.path.as_os_str().is_empty(), "empty source path");
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn texture_review_recipe_roundtrip_and_validation() {
        let recipe = super::CaptureRecipe {
            schema: 1,
            animation: 2,
            time_ms: 330,
            center: [1., 2., 3.],
            distance: 1000.,
            yaw: 1.57,
            pitch: 0.,
            vertical_half_angle_tangent: 0.6,
            near: 16.,
            far: 11000.,
            background: [0.18, 0.2, 0.24],
            ambient: 1.,
            instance_light: 1.,
            brightness: 1.,
            daylight: [0., 1., 0.],
            fog: [1e9, 2e9],
            resolution: [960, 720],
        };
        recipe.validate().unwrap();
        let encoded = serde_json::to_vec(&recipe).unwrap();
        let decoded: super::CaptureRecipe = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(recipe, decoded);
        let mut invalid = recipe.clone();
        invalid.distance = f32::NAN;
        assert!(invalid.validate().is_err());
        invalid = recipe.clone();
        invalid.far = invalid.near;
        assert!(invalid.validate().is_err());
        invalid = recipe;
        invalid.schema += 1;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn texture_review_distinguishes_hidden_source_geometry_from_render_failure() {
        let normal = [[0., 0., 0.], [1000., 500., 200.]];
        let hidden = [[0., 0., 0.], [0., 0.25, 0.25]];
        assert!(super::collapsed_pose(&hidden, &normal));
        assert!(!super::collapsed_pose(&normal, &normal));
        assert!(!super::collapsed_pose(&[], &normal));
        assert!(!super::collapsed_pose(&[[f32::NAN; 3]], &normal));
    }
    use crate::image_diff::{diff, Rgba};

    #[test]
    fn texture_review_tolerates_noise_but_detects_a_small_hole() {
        let policy = Config::default();
        let image = |data| Rgba {
            width: 100,
            height: 100,
            data,
        };
        let base = image(vec![255; 100 * 100 * 4]);
        let mut noise = base.data.clone();
        for p in noise.as_chunks_mut::<4>().0 {
            p[0] = 244;
        }
        assert_eq!(
            diff(&base, &image(noise), policy.channel_tolerance, &[])
                .unwrap()
                .ratio(),
            0.0
        );
        let mut hole = base.data.clone();
        for p in hole.as_chunks_mut::<4>().0.iter_mut().take(60) {
            p[..3].fill(0);
        }
        let difference = diff(&base, &image(hole), policy.channel_tolerance, &[]).unwrap();
        assert!(
            difference.ratio() > policy.changed_fraction,
            "a 0.6% hole exceeds 0.5%"
        );
        assert!(difference.ratio() <= 0.01, "a looser policy can accept it");
    }

    #[test]
    fn texture_review_rejects_silent_policy_typos_and_unsafe_or_duplicate_ids() {
        let source = "\n[[source]]\nid = 'animals'\nkind = 'cars'\npath = 'local'\n";
        assert!(Config::parse(source).unwrap().keep_models);
        assert!(
            !Config::parse(&format!("animations = [0]{source}"))
                .unwrap()
                .keep_models
        );
        for invalid in [
            "asset_type = 'original-parity'",
            "asset_type = 'typo'",
            "asset_type = 'animation-archive'\nanimations = [0]",
            "changed_faction = 0.1",
            "changed_fraction = -0.1",
            "channel_tolerance = 256",
            "phases = [1.0]",
            "width = 0",
            "channel_tolerance = '12'",
        ] {
            assert!(
                Config::parse(&format!("{invalid}{source}")).is_err(),
                "{invalid}"
            );
        }
        assert!(Config::parse(&format!("{source}{source}")).is_err());
        assert!(Config::parse(&source.replace("'animals'", "'../private'")).is_err());
    }
}
