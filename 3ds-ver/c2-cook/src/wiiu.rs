//! `--target wiiu`: the merged game as the Wii U app reads it. The Wii U
//! takes the original formats as they are, so nothing is converted: every
//! `HUNTDAT` file of the merged game (base, overlays, add-ons, recipe,
//! balance) is written under the output folder with an upper-case name
//! (one spelling for the SD card's file system and the app's lookups),
//! with `INFO.TXT` (what the 3DS pack's says: areas, snow, roaming add-ons)
//! and `FILES.TXT` (every file, one a line: the app's index).
//! The folder is the install's `wiiu/apps/<name>/` on the SD card.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use c2_vfs::Vfs;

/// What the app's menus need to know without resolving the rules for
/// every area (`c2-3ds`'s `PackInfo`).
pub struct Info<'a> {
    /// The game's name, for the app's title screen.
    pub name: &'a str,
    /// `AREA1`.. as present, then `TROPHY`.
    pub areas: &'a [String],
    pub snow: Vec<&'a str>,
    /// Roaming add-ons' areas, by character.
    pub roams: BTreeMap<usize, Vec<&'a str>>,
}

/// `INFO.TXT`'s text.
fn info_text(i: &Info) -> String {
    let mut s = String::from("wiiu = 1\n");
    s += &format!("name = {}\n", i.name);
    s += &format!("areas = {}\n", i.areas.join(","));
    if !i.snow.is_empty() {
        s += &format!("snow = {}\n", i.snow.join(","));
    }
    for (c, list) in &i.roams {
        s += &format!("roam.{c} = {}\n", list.join(","));
    }
    s
}

/// Writes the install into `out` (created if needed; files already there
/// are replaced, others left alone).
pub fn install(vfs: &Vfs, info: &Info, out: &Path) -> Result<()> {
    let mut names: Vec<String> = vfs
        .files()
        .map(|(p, _)| p.replace('\\', "/"))
        .filter(|p| {
            p.split('/')
                .next()
                .is_some_and(|d| d.eq_ignore_ascii_case("HUNTDAT"))
        })
        .collect();
    names.sort_unstable_by_key(|p| p.to_ascii_uppercase());
    names.dedup_by_key(|p| p.to_ascii_uppercase());
    let mut index = String::new();
    let mut bytes = 0u64;
    for p in &names {
        let upper = p.to_ascii_uppercase();
        let data = vfs.read(p).with_context(|| format!("reading {p}"))?;
        let to = out.join(&upper);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
        }
        std::fs::write(&to, &data).with_context(|| format!("writing {}", to.display()))?;
        bytes += data.len() as u64;
        index += &upper;
        index.push('\n');
    }
    std::fs::write(out.join("FILES.TXT"), index)?;
    std::fs::write(out.join("INFO.TXT"), info_text(info))?;
    println!(
        "Wii U install: {} files, {:.1} MiB, in {}",
        names.len(),
        bytes as f64 / (1024.0 * 1024.0),
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_lists_areas_snow_and_roamers() {
        let areas = vec!["AREA1".to_string(), "AREA2".into(), "TROPHY".into()];
        let mut roams = BTreeMap::new();
        roams.insert(31, vec!["AREA2"]);
        let i = Info {
            name: "Carnivores 2",
            areas: &areas,
            snow: vec!["AREA2"],
            roams,
        };
        assert_eq!(
            info_text(&i),
            "wiiu = 1\nname = Carnivores 2\nareas = AREA1,AREA2,TROPHY\nsnow = AREA2\nroam.31 = AREA2\n"
        );
    }
}
