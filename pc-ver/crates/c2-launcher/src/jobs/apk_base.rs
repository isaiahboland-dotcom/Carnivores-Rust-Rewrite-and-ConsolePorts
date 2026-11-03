//! Rebuild an APK base before publishing its destination to the UI.
use super::{Completion, Job};
use std::path::PathBuf;
pub(crate) fn build(
    job: &Job,
    apk: PathBuf,
    to: PathBuf,
    for_recipe: bool,
) -> Result<Completion, String> {
    job.log(format!("Reading {}", apk.display()));
    let stage = crate::publish::Staged::new(&to)?;
    let b = match c2_mods::mobile_base::build(&apk, &stage.path) {
        Ok(b) => b,
        Err(e) => return Err(stage.abort(e.to_string())),
    };
    if !stage.path.join("HUNTDAT/_RES.TXT").is_file() {
        return Err(stage.abort("Build did not produce HUNTDAT/_RES.TXT".into()));
    }
    if let Some(warning) = stage.publish()? {
        job.log(warning);
    }
    job.log(format!(
        "{} areas, {} animals, {} weapons",
        b.areas.len(),
        b.animals.len(),
        b.weapons.len()
    ));
    for n in b.notes {
        job.log(format!("  {n}"));
    }
    Ok(Completion::ApkBase {
        out: to,
        for_recipe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_apk_does_not_publish_or_remove_the_previous_game() {
        let temp = crate::publish::tests::Temp::new();
        let out = temp.0.join("game");
        std::fs::create_dir(&out).unwrap();
        std::fs::write(out.join("previous"), b"keep").unwrap();
        let job = Job::start("test", vec![]);
        for for_recipe in [false, true] {
            assert!(build(&job, temp.0.join("missing.apk"), out.clone(), for_recipe).is_err());
            assert_eq!(std::fs::read(out.join("previous")).unwrap(), b"keep");
            assert!(crate::publish::pending(&temp.0).is_empty());
        }
    }
}
