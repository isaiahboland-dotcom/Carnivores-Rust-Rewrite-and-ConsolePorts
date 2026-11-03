//! Extract all requested archives before registering their add-ons.
use super::{Completion, Job, Step};
use std::path::PathBuf;

pub(crate) fn unpack(job: &Job, archives: Vec<(PathBuf, PathBuf)>) -> Result<Completion, String> {
    let mut paths = Vec::new();
    for (archive, to) in archives {
        job.run(Step::Unpack {
            archive,
            to: to.clone(),
        })?;
        paths.push(to);
    }
    Ok(Completion::Addons(paths))
}
