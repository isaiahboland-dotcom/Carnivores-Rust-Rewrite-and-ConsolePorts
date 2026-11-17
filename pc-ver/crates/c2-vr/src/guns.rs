//! Guns held in VR (GAMEPLAN "Hands-on hunting"): each weapon's `.CAR`
//! drawn without the hunter's arms, which the tracked hands replace. The
//! arms are found by `c2_core::hands` (the original hand rig, or the
//! game's `_HANDS.TXT` overrides) and left out of a copy of the mesh; the
//! game's file and the flat game's first-person weapon are untouched. A
//! gun whose arms aren't found is not holdable: drawn whole, its arms would
//! float in front of the player's own hands.
//!
//! The hunt does not draw guns yet; the roster prepares them at load
//! ([`load`]) and reports which are holdable.

use std::path::{Path, PathBuf};

use c2_core::game::WeaponInfo;
use c2_core::hands::{HandMask, Source};
use c2_core::model::PreparedModel;
use c2_formats::car::Car;
use c2_formats::hands::Hands;
use c2_formats::res_txt::ResData;
use glam::Vec3;

/// One weapon as held in VR.
pub struct HeldGun {
    pub name: String,
    /// The gun without the arms, in the prepared models' units
    /// (`PreparedModel::from_mesh`). Its vertices are the model's, so the
    /// model's animations pose it.
    pub model: PreparedModel,
    /// The weapon's animations (none drawn yet: VR has no reloading).
    pub car: Car,
    /// Where the hand that holds the grip rests (prepared units); none
    /// when no hand was recognised.
    pub grip: Option<Vec3>,
    /// What decided which parts were left out.
    pub source: Source,
}

/// A point in the model's file units as the prepared models place it
/// (`x * 2, y * 2, z * -2`, `model.rs: from_mesh`).
fn prepared(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0] * 2.0, p[1] * 2.0, -p[2] * 2.0)
}

impl HeldGun {
    /// `car` (the weapon called `name`, whose file is `id`, `sha256:<hex>`)
    /// prepared to hold, under `overrides`.
    pub fn new(name: &str, car: Car, id: &str, overrides: &Hands) -> Self {
        let mask = HandMask::new(&car.mesh, id, overrides);
        let shown = mask.apply(&car.mesh);
        // The grip: of the recognised hands, the one nearest the gun's
        // middle (the pistol's other hand supports it from the side).
        let gun: Vec<[f32; 3]> = c2_core::dhc::specimen::used_points(
            &shown
                .vertices
                .iter()
                .map(|v| [v.x, v.y, v.z])
                .collect::<Vec<_>>(),
            &shown.faces,
        );
        let mid = gun.iter().fold(Vec3::ZERO, |s, p| s + Vec3::from(*p)) / gun.len().max(1) as f32;
        let grip = mask
            .hand_centres(&car.mesh)
            .into_iter()
            .map(Vec3::from)
            .min_by(|a, b| a.distance(mid).total_cmp(&b.distance(mid)))
            .map(|c| prepared(c.into()));
        HeldGun {
            name: name.to_string(),
            model: PreparedModel::from_mesh(&shown),
            car,
            grip,
            source: mask.source,
        }
    }

    /// Whether it can be held: its arms were found and left out.
    pub fn holdable(&self) -> bool {
        self.source != Source::Unrecognised
    }
}

/// The game's weapons (`res`'s, in order) as held guns, read from the
/// first of `huntdat` (overlays first) that has each file; the overrides
/// from the first `_HANDS.TXT`. None where a weapon's model doesn't load.
pub fn load(huntdat: &[PathBuf], res: &ResData) -> Vec<Option<HeldGun>> {
    let find = |name: &str| huntdat.iter().find_map(|d| crate::hunt::find_file(d, name));
    let overrides = find(c2_formats::hands::PATH.trim_start_matches("HUNTDAT/"))
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| Hands::parse(&b))
        .unwrap_or_default();
    res.weapons
        .iter()
        .map(|d| {
            let info = WeaponInfo::from_def(d);
            let path = huntdat.iter().find_map(|h| {
                let dir = crate::hunt::find_file(h, "WEAPONS")?;
                crate::hunt::find_file(&dir, &info.file)
            })?;
            read(&info.name, &path, &overrides)
        })
        .collect()
}

fn read(name: &str, path: &Path, overrides: &Hands) -> Option<HeldGun> {
    let bytes = std::fs::read(path).ok()?;
    match c2_formats::car::parse(&bytes) {
        Ok(car) => Some(HeldGun::new(
            name,
            car,
            &c2_mods::recipe::sha256(&bytes),
            overrides,
        )),
        Err(e) => {
            println!("{}: {e}", path.display());
            None
        }
    }
}

/// One line on what was prepared: each gun, holdable or not.
pub fn summary(guns: &[Option<HeldGun>]) -> String {
    let parts: Vec<String> = guns
        .iter()
        .flatten()
        .map(|g| {
            let how = match g.source {
                Source::VanillaRig(n) => format!("{n} hand(s) left out"),
                Source::Override => "parts left out by _HANDS.TXT".to_string(),
                Source::Unrecognised => "arms not found, not holdable".to_string(),
            };
            format!("{} ({how}, {} faces)", g.name.trim(), g.model.faces.len())
        })
        .collect();
    format!("VR guns: {}", parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_core::hands::VANILLA_RIG;
    use c2_formats::model::{Face, Mesh, Vertex};

    /// Bones `first..` with `counts` vertices, one strip of faces, at `x`.
    fn part(m: &mut Mesh, first: i16, counts: &[usize], x: f32) {
        let start = m.vertices.len();
        for (k, &n) in counts.iter().enumerate() {
            for _ in 0..n {
                let i = m.vertices.len() as f32;
                m.vertices.push(Vertex {
                    x,
                    y: i,
                    z: 1.0,
                    owner: first + k as i16,
                    hide: 0,
                });
            }
        }
        for i in start..m.vertices.len() - 2 {
            let i = i as i32;
            m.faces.push(Face {
                v: [i, i + 1, i + 2],
                tu: [0; 3],
                tv: [0; 3],
                flags: 0,
                dmask: 0,
                distant: 0,
                next: 0,
                group: 0,
                reserved: [0; 12],
            });
        }
    }

    fn car(mesh: Mesh) -> Car {
        Car {
            name_raw: [0; 32],
            mesh,
            animations: vec![],
            sounds: vec![],
            anim_sounds: None,
            trailing: Vec::new(),
        }
    }

    #[test]
    fn the_pistol_is_held_by_the_hand_on_its_grip() {
        // The original pistol's layout: a hand on the grip (bones 0-22),
        // the gun (23-25), the supporting hand off to the side (26-48).
        let mut m = Mesh::default();
        part(&mut m, 0, &VANILLA_RIG, 1.0);
        part(&mut m, 23, &[136, 52, 8], 0.0);
        part(&mut m, 26, &VANILLA_RIG, -30.0);
        let g = HeldGun::new("Pistol", car(m.clone()), "sha256:aa", &Hands::default());
        assert!(g.holdable());
        assert_eq!(g.source, Source::VanillaRig(2));
        assert_eq!(g.model.faces.len(), 136 + 52 + 8 - 2);
        assert_eq!(
            g.grip.map(|p| p.x),
            Some(2.0),
            "the grip hand, in prepared units"
        );
        assert_eq!(g.car.mesh, m, "the model kept whole for its animations");
    }

    #[test]
    fn a_gun_without_a_recognised_rig_is_not_holdable() {
        let mut m = Mesh::default();
        part(&mut m, 0, &[300], 0.0);
        part(&mut m, 0, &[120], -20.0);
        let g = HeldGun::new(
            "Lever action",
            car(m.clone()),
            "sha256:bb",
            &Hands::default(),
        );
        assert!(!g.holdable());
        assert_eq!(g.grip, None);
        assert_eq!(g.model.faces.len(), m.faces.len(), "drawn whole");
        // With an override naming its arm piece, it is.
        let h = Hands::parse(b"model sha256:bb\ndrop piece 1\n");
        let g = HeldGun::new("Lever action", car(m), "sha256:bb", &h);
        assert!(g.holdable());
        assert_eq!(g.model.faces.len(), 298);
        assert!(summary(&[Some(g), None]).contains("Lever action (parts left out"));
    }
}
