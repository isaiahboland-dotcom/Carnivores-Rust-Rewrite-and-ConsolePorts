//! A game folder (the one holding `HUNTDAT/`) loaded for a scenario, with
//! a hash of every file the run depends on. Nothing is copied or written.

use std::path::Path;

use anyhow::{Context, Result};
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::Species;
use c2_core::load::{self, LoadedWeapon};
use c2_formats::car::Car;
use c2_formats::res_txt::ResTxt;
use c2_vfs::{DirBackend, Vfs};
use sha2::{Digest, Sha256};

pub struct Game {
    pub area_name: String,
    pub area: Area,
    pub species: Vec<Species>,
    pub cars: Vec<Option<Car>>,
    pub weapons: Vec<LoadedWeapon>,
    /// `(path, sha256)` of each file the run reads, in a fixed order.
    pub files: Vec<(String, String)>,
    /// SHA-256 over `files`.
    pub content_hash: String,
    /// The game's `_RES.TXT` is in the Modder's Edition dialect.
    pub dialect: bool,
}

fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// The hunt's characters and weapons. A Modder's Edition `_RES.TXT` is
/// resolved for the area by day with no menu slots picked, as the PC hunt
/// does (`HuntView::load`), and its tables tune the species.
fn resolve(
    txt: &ResTxt,
    area_name: &str,
) -> (
    c2_formats::res_txt::ResData,
    Option<c2_formats::mee::Tables>,
) {
    if !c2_formats::mee::is_dialect(txt) {
        return (txt.to_data(), None);
    }
    let h = c2_formats::mee::Hunt {
        area: c2_formats::mee::area_of(area_name),
        time: 1,
        picked: c2_formats::bits::Bits::NONE,
    };
    let m = c2_core::mee::read_picked_anywhere(txt, &h, &[]);
    (m.res, Some(m.tables))
}

impl Game {
    pub fn load(dir: &Path, area_name: &str) -> Result<Self> {
        let mut vfs = Vfs::new();
        let backend =
            DirBackend::new(dir).with_context(|| format!("indexing {}", dir.display()))?;
        vfs.push("game", Box::new(backend));
        let text = vfs
            .read("HUNTDAT/_RES.TXT")
            .context("no readable HUNTDAT/_RES.TXT in the game folder")?;
        let txt = ResTxt::parse(&String::from_utf8_lossy(&text)).context("reading _RES.TXT")?;
        let area = load::area(&vfs, area_name, TimeOfDay::Day)
            .with_context(|| format!("loading area {area_name}"))?;
        let (res, tables) = resolve(&txt, area_name);
        let (mut species, cars) = load::species(&vfs, &res);
        if let Some(t) = &tables {
            c2_core::mee::tune(&mut species, t);
        }
        let weapons = load::weapons(&vfs, &res);

        let mut paths = vec![
            "HUNTDAT/_RES.TXT".to_string(),
            format!("HUNTDAT/AREAS/{area_name}.MAP"),
            format!("HUNTDAT/AREAS/{area_name}.RSC"),
        ];
        paths.extend(species.iter().map(|s| format!("HUNTDAT/{}", s.info.file)));
        paths.extend(
            weapons
                .iter()
                .map(|w| format!("HUNTDAT/WEAPONS/{}", w.info.file)),
        );
        // A base rebuilt from an APK (`c2-pc mobile-base`): the phone's HUD,
        // which the render pass draws. PC data has no such folder.
        paths.extend(
            vfs.files()
                .filter(|(k, _)| k.starts_with("huntdat/mobile/"))
                .map(|(_, r)| r.path.clone()),
        );
        let mut files = Vec::new();
        let mut all = Sha256::new();
        for p in paths {
            // Missing files are hashed as missing: the run reports them.
            let h = vfs.read(&p).map_or_else(|_| "missing".into(), |d| sha(&d));
            all.update(p.to_ascii_uppercase().as_bytes());
            all.update(h.as_bytes());
            files.push((p, h));
        }
        Ok(Self {
            area_name: area_name.into(),
            area,
            species,
            cars,
            weapons,
            files,
            content_hash: format!("{:x}", all.finalize()),
            dialect: tables.is_some(),
        })
    }

    pub fn weapon_names(&self) -> Vec<String> {
        self.weapons.iter().map(|w| w.info.name.clone()).collect()
    }
}
