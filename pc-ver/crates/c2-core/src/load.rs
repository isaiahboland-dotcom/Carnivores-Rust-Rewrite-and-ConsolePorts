//! Loading a hunt's data through a [`Vfs`] (a cooked pack on the 3DS).
//! Paths are as the original opens them, relative to the game folder:
//! `HUNTDAT/_RES.TXT`, `HUNTDAT/<character>.CAR`, `HUNTDAT/WEAPONS/<weapon>.CAR`.

use c2_formats::car::{self, Car};
use c2_formats::res_txt::{ResData, ResTxt};
use c2_formats::{map, rsc, FormatError};
use c2_vfs::{Vfs, VfsError};

use crate::area::{Area, TimeOfDay};
use crate::dino::{DinoInfo, Species};
use crate::game::WeaponInfo;
use crate::model::car_anim_duration_ms;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error("{path}: {source}")]
    Format {
        path: String,
        #[source]
        source: FormatError,
    },
}

fn parse<T>(
    vfs: &Vfs,
    path: &str,
    f: impl FnOnce(&[u8]) -> Result<T, FormatError>,
) -> Result<T, LoadError> {
    f(&vfs.read(path)?).map_err(|source| LoadError::Format {
        path: path.into(),
        source,
    })
}

pub fn res(vfs: &Vfs) -> Result<ResData, LoadError> {
    parse(vfs, "HUNTDAT/_RES.TXT", |d| {
        Ok(ResTxt::parse(&String::from_utf8_lossy(d))?.to_data())
    })
}

/// A `.CAR`, or `None` (logged) if it is missing or broken.
fn optional_car(vfs: &Vfs, path: &str) -> Option<Car> {
    match parse(vfs, path, car::parse) {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!("{e}");
            None
        }
    }
}

fn empty_car() -> Car {
    Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: vec![],
        sounds: vec![],
        anim_sounds: None,
    }
}

/// Every `_RES.TXT` character, with its `.CAR` where there is one (a species
/// without one has no animations and is never placed).
pub fn species(vfs: &Vfs, res: &ResData) -> (Vec<Species>, Vec<Option<Car>>) {
    let mut species = Vec::new();
    let mut cars = Vec::new();
    for def in &res.characters {
        let info = DinoInfo::from_def(def);
        let car = optional_car(vfs, &format!("HUNTDAT/{}", info.file));
        species.push(Species::new(info, car.as_ref().unwrap_or(&empty_car())));
        cars.push(car);
    }
    (species, cars)
}

/// A `_RES.TXT` weapon: its info, `[draw, fire, reload, hide]` animation
/// lengths and its view model.
pub struct LoadedWeapon {
    pub info: WeaponInfo,
    pub anims: [i32; 4],
    pub car: Option<Car>,
}

pub fn weapons(vfs: &Vfs, res: &ResData) -> Vec<LoadedWeapon> {
    res.weapons
        .iter()
        .map(|d| {
            let info = WeaponInfo::from_def(d);
            let car = optional_car(vfs, &format!("HUNTDAT/WEAPONS/{}", info.file));
            let mut anims = [0; 4];
            if let Some(c) = &car {
                for (slot, t) in anims.iter_mut().enumerate() {
                    *t = c
                        .animations
                        .get(slot)
                        .map_or(0, |a| car_anim_duration_ms(a) as i32);
                }
            }
            LoadedWeapon { info, anims, car }
        })
        .collect()
}

/// `HUNTDAT/AREAS/<name>.MAP` and `.RSC`.
pub fn area(vfs: &Vfs, name: &str, time: TimeOfDay) -> Result<Area, LoadError> {
    let m = parse(vfs, &format!("HUNTDAT/AREAS/{name}.MAP"), map::parse)?;
    let r = parse(vfs, &format!("HUNTDAT/AREAS/{name}.RSC"), rsc::parse)?;
    let mut a = Area::new(m, r, time);
    if name.to_ascii_lowercase().contains("trophy") {
        a.set_trophy_mode();
    }
    Ok(a)
}

/// The ship's landing animation length (`SHIP2A.CAR`, animation 0), ms.
pub fn ship_anim_ms(vfs: &Vfs) -> i32 {
    optional_car(vfs, "HUNTDAT/SHIP2A.CAR")
        .and_then(|c| c.animations.first().map(car_anim_duration_ms))
        .map_or(1, |ms| ms.max(1) as i32)
}
