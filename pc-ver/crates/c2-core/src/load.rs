//! Loading a hunt's data through a [`Vfs`] (a cooked pack on the 3DS).
//! Paths are as the original opens them, relative to the game folder:
//! `HUNTDAT/_RES.TXT`, `HUNTDAT/<character>.CAR`, `HUNTDAT/WEAPONS/<weapon>.CAR`.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec, vec::Vec};

use c2_formats::car::{self, Car};
use c2_formats::res_txt::{ResData, ResTxt};
use c2_formats::{map, rsc, FormatError};
use c2_vfs::{Vfs, VfsError};

use crate::area::{Area, TimeOfDay};
use crate::dino::{DinoInfo, Species};
use crate::game::{WeaponAnims, WeaponInfo};
use crate::model::car_anim_duration_ms;

#[derive(Debug)]
pub enum LoadError {
    Vfs(VfsError),
    Format { path: String, source: FormatError },
}

// By hand rather than `thiserror`, so no_std targets need no extra crate.
impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadError::Vfs(e) => e.fmt(f),
            LoadError::Format { path, source } => write!(f, "{path}: {source}"),
        }
    }
}

impl core::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            LoadError::Vfs(e) => e.source(),
            LoadError::Format { source, .. } => Some(source),
        }
    }
}

impl From<VfsError> for LoadError {
    fn from(e: VfsError) -> Self {
        LoadError::Vfs(e)
    }
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

/// The predation settings (`_PREDATION.TXT`, a setting not in the
/// original) resolved for `species`, whose menu prices must be set
/// (`game::price_species`). `None` when the file is missing or changes
/// nothing. Survival and the trophy room leave it off.
pub fn predation(
    vfs: &Vfs,
    species: &[Species],
) -> Option<alloc::boxed::Box<crate::dino::predation::run::Predators>> {
    crate::dino::predation::load(&vfs.read(c2_formats::predation::PATH).ok()?, species)
}

pub fn res(vfs: &Vfs) -> Result<ResData, LoadError> {
    parse(vfs, "HUNTDAT/_RES.TXT", |d| {
        Ok(ResTxt::parse(&String::from_utf8_lossy(d))?.to_data())
    })
}

/// A `.CAR`, or `None` (logged) if it is missing or broken.
fn optional_car(vfs: &Vfs, path: &str) -> Option<Car> {
    parse(vfs, path, car::parse)
        .inspect_err(|_e| {
            #[cfg(feature = "std")]
            tracing::warn!("{_e}");
        })
        .ok()
}

fn empty_car() -> Car {
    Car {
        name_raw: [0; 32],
        mesh: Default::default(),
        animations: vec![],
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    }
}

/// Every `_RES.TXT` character, with its `.CAR` where there is one (a species
/// without one has no animations and is never placed).
pub fn species(vfs: &Vfs, res: &ResData) -> (Vec<Species>, Vec<Option<Car>>) {
    species_with(vfs, res, &|_| true)
}

/// [`species`], reading the `.CAR` only for characters `keep` accepts (by
/// index); the others get no model, so they are never placed.
pub fn species_with(
    vfs: &Vfs,
    res: &ResData,
    keep: &dyn Fn(usize) -> bool,
) -> (Vec<Species>, Vec<Option<Car>>) {
    let mut species = Vec::new();
    let mut cars = Vec::new();
    for (i, def) in res.characters.iter().enumerate() {
        let info = DinoInfo::from_def(def);
        let car = keep(i)
            .then(|| optional_car(vfs, &format!("HUNTDAT/{}", info.file)))
            .flatten();
        species.push(Species::new(info, car.as_ref().unwrap_or(&empty_car())));
        cars.push(car);
    }
    (species, cars)
}

/// A `_RES.TXT` weapon: its info, animation lengths and sounds, and its
/// view model.
pub struct LoadedWeapon {
    pub info: WeaponInfo,
    pub anims: WeaponAnims,
    pub car: Option<Car>,
}

pub fn weapons(vfs: &Vfs, res: &ResData) -> Vec<LoadedWeapon> {
    weapons_with(vfs, res, &|_| true)
}

/// [`weapons`], reading the view model only for weapons `keep` accepts.
pub fn weapons_with(vfs: &Vfs, res: &ResData, keep: &dyn Fn(usize) -> bool) -> Vec<LoadedWeapon> {
    res.weapons
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let info = WeaponInfo::from_def(d);
            let car = keep(i)
                .then(|| optional_car(vfs, &format!("HUNTDAT/WEAPONS/{}", info.file)))
                .flatten();
            let anims = car
                .as_ref()
                .map(|c| WeaponAnims::from_car(&info, c))
                .unwrap_or_default();
            LoadedWeapon { info, anims, car }
        })
        .collect()
}

/// `HUNTDAT/AREAS/<name>.MAP` and `.RSC`.
pub fn area(vfs: &Vfs, name: &str, time: TimeOfDay) -> Result<Area, LoadError> {
    area_with(vfs, name, time, false)
}

/// [`area`]; `lean` keeps only `time`'s light map and drops `texture2`
/// (see [`map::parse_lean`]), for a renderer that never changes time of day.
pub fn area_with(vfs: &Vfs, name: &str, time: TimeOfDay, lean: bool) -> Result<Area, LoadError> {
    let light = lean.then_some(time as usize);
    let m = parse(vfs, &format!("HUNTDAT/AREAS/{name}.MAP"), |d| {
        map::parse_lean(d, light)
    })?;
    let r = parse(vfs, &format!("HUNTDAT/AREAS/{name}.RSC"), rsc::parse)?;
    let mut a = Area::new(m, r, time);
    // Snow of its own (the phone's Ice Age areas).
    if let Ok(d) = vfs.read(&format!("HUNTDAT/AREAS/{name}.SNOW")) {
        a.set_snow(&String::from_utf8_lossy(&d));
    }
    if name.to_ascii_lowercase().contains("trophy") {
        a.set_trophy_mode();
        // A room of its own layout (the phone game's).
        if let Ok(d) = vfs.read(&format!("HUNTDAT/AREAS/{name}.SPOT")) {
            a.set_trophy_spots(&String::from_utf8_lossy(&d));
        }
    }
    Ok(a)
}

/// The ship's landing animation length (`SHIP2A.CAR`, animation 0), ms.
pub fn ship_anim_ms(vfs: &Vfs) -> i32 {
    optional_car(vfs, "HUNTDAT/SHIP2A.CAR")
        .and_then(|c| c.animations.first().map(car_anim_duration_ms))
        .map_or(1, |ms| ms.max(1) as i32)
}
