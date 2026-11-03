//! Import workers. Publication finishes before the UI receives a result.
use super::{Completion, Job};
use std::path::PathBuf;

pub(crate) fn mobile(
    job: &Job,
    apk: PathBuf,
    bases: Vec<PathBuf>,
    to: PathBuf,
) -> Result<Completion, String> {
    job.log(format!("Reading {}", apk.display()));
    let base = c2_mods::vanilla(&bases).map_err(|e| e.to_string())?;
    let stage = crate::publish::Staged::new(&to)?;
    let done = match c2_mods::mobile::import(&apk, &base, &stage.path) {
        Ok(done) => done,
        Err(e) => return Err(stage.abort(e.to_string())),
    };
    if done.added.is_empty() {
        return Err(stage.abort(
            "No compatible mobile add-ons were imported; previous install preserved.".into(),
        ));
    }
    if let Some(warning) = stage.publish()? {
        job.log(warning);
    }
    for s in done.skipped {
        job.log(format!("Not imported: {s}"));
    }
    for a in done.added {
        job.log(format!("{} (kind of the {})", a.name, a.donor));
        for n in a.notes {
            job.log(format!("  {n}"));
        }
    }
    Ok(Completion::Imported(to))
}

pub(crate) fn ice_age(
    job: &Job,
    ia: PathBuf,
    bases: Vec<PathBuf>,
    to: PathBuf,
) -> Result<Completion, String> {
    job.log(format!("Reading {}", ia.display()));
    let base = c2_mods::vanilla(&bases).map_err(|e| e.to_string())?;
    let stage = crate::publish::Staged::new(&to)?;
    let done = match c2_mods::ice_age::import(&ia, &base, &stage.path) {
        Ok(done) => done,
        Err(e) => return Err(stage.abort(e.to_string())),
    };
    if done.added.is_empty() {
        return Err(stage.abort(
            "No compatible Ice Age add-ons were imported; previous install preserved.".into(),
        ));
    }
    let added: Vec<_> = done
        .added
        .iter()
        .filter_map(|p| p.strip_prefix(&stage.path).ok().map(|p| to.join(p)))
        .collect();
    if let Some(warning) = stage.publish()? {
        job.log(warning);
    }
    for s in done.skipped {
        job.log(format!("Not imported: {s}"));
    }
    for d in added {
        job.log(d.display().to_string());
    }
    Ok(Completion::Imported(to))
}
