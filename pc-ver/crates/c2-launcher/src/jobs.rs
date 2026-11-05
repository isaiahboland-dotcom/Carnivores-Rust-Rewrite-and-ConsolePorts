//! Work done off the UI thread: running `c2-pc` / `c2-cook`, copying the
//! 3DS app, unpacking add-on archives. Each job is a list of steps run in
//! order; its output goes to a shared log the UI shows.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

pub(crate) mod addons;
pub(crate) mod apk_base;
pub(crate) mod balance;
pub(crate) mod imports;
pub(crate) mod recipe;
pub(crate) mod recipe_edit;
pub(crate) mod saving;

/// Successful work delivered exactly once to the UI.
pub(crate) enum Completion {
    Imported(PathBuf),
    /// The recipe editor's instance values (`balance::read_instance`).
    RecipeBalance(Box<c2_mods::balance::Balance>),
    Pinned(
        Box<(
            crate::recipe_edit::Kind,
            c2_mods::recipe::RecipeMod,
            PathBuf,
        )>,
    ),
    Recipe(Vec<crate::recipe_page::Msg>),
    /// The editor's game folder and the base profile it matches.
    BaseMatch(Box<(PathBuf, Option<String>)>),
    Addons(Vec<PathBuf>),
    /// The game closed: the instance played, seconds, and any error.
    Played(Box<(Option<PathBuf>, u64, Option<String>)>),
    Saved(Box<(c2_mods::recipe::Recipe, crate::config::RecipeUse)>),
    ApkBase {
        out: PathBuf,
        for_recipe: bool,
    },
}

/// The single owner of launcher work and its unconsumed completion.
#[derive(Default)]
pub(crate) struct Handle {
    job: Option<Job>,
    completion: Option<Receiver<Completion>>,
}

impl Handle {
    pub fn busy(&self) -> bool {
        self.completion.is_some() || self.result().is_none() && self.job.is_some()
    }
    pub fn current(&self) -> Option<&Job> {
        self.job.as_ref()
    }
    pub fn result(&self) -> Option<bool> {
        self.job.as_ref().and_then(Job::result)
    }
    pub fn start(&mut self, title: impl Into<String>, steps: Vec<Step>) -> Result<(), String> {
        if self.busy() {
            return Err("Wait for the current operation to finish.".into());
        }
        self.job = Some(Job::start(title, steps));
        Ok(())
    }
    pub fn start_task(
        &mut self,
        title: impl Into<String>,
        task: impl FnOnce(&Job) -> Result<Completion, String> + Send + 'static,
    ) -> Result<(), String> {
        if self.busy() {
            return Err("Wait for the current operation to finish.".into());
        }
        let (tx, rx) = channel();
        self.start(
            title,
            vec![Step::Task(Box::new(move |job| {
                let done = task(job)?;
                tx.send(done)
                    .map_err(|_| "The launcher closed before completion.".into())
            }))],
        )?;
        self.completion = Some(rx);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Result<Completion, String>> {
        let ok = self.result()?;
        let rx = self.completion.take()?;
        Some(if ok {
            rx.try_recv()
                .map_err(|_| "Worker stopped without a result.".into())
        } else {
            Err("Operation failed: see the job log.".into())
        })
    }
}

/// Work a [`Step::Task`] does.
pub type Task = Box<dyn FnOnce(&Job) -> Result<(), String> + Send>;

pub enum Step {
    /// A program with its arguments; fails the job if it does.
    Run {
        program: PathBuf,
        args: Vec<String>,
    },
    /// Start the game; keep its job active until exit to protect its inputs.
    Launch {
        program: PathBuf,
        args: Vec<String>,
    },
    Copy {
        from: PathBuf,
        to: PathBuf,
    },
    /// An archive into a folder.
    Unpack {
        archive: PathBuf,
        to: PathBuf,
    },
    /// A generated file (the `.cia` build's spec and placeholders).
    Write {
        path: PathBuf,
        bytes: Vec<u8>,
    },
    /// A folder for a program's output.
    Dir(PathBuf),
    /// Work done here (the mobile import), logging to the job.
    Task(Task),
}

#[derive(Default)]
struct State {
    lines: Vec<String>,
    /// `None` while running.
    ok: Option<bool>,
}

#[derive(Clone)]
pub struct Job {
    pub title: String,
    state: Arc<Mutex<State>>,
}

impl Job {
    pub fn start(title: impl Into<String>, steps: Vec<Step>) -> Self {
        let job = Job {
            title: title.into(),
            state: Arc::default(),
        };
        let j = job.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut ok = true;
                for step in steps {
                    if let Err(e) = j.run(step) {
                        j.log(format!("Failed: {e}"));
                        ok = false;
                        break;
                    }
                }
                ok
            }));
            let ok = result.unwrap_or_else(|_| {
                j.log("Failed: worker panicked.");
                false
            });
            if let Ok(mut s) = j.state.lock() {
                s.ok = Some(ok);
            }
        });
        job
    }

    pub fn log(&self, line: impl Into<String>) {
        if let Ok(mut s) = self.state.lock() {
            s.lines.push(line.into());
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|s| s.lines.clone())
            .unwrap_or_default()
    }

    /// `None` while running, then whether every step worked.
    pub fn result(&self) -> Option<bool> {
        self.state.lock().ok().and_then(|s| s.ok)
    }

    pub(crate) fn run(&self, step: Step) -> Result<(), String> {
        match step {
            Step::Run { program, args } => {
                self.log(format!("> {} {}", program.display(), args.join(" ")));
                let mut child = Command::new(&program)
                    .args(&args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("{}: {e}", program.display()))?;
                let readers: Vec<_> = [
                    child
                        .stdout
                        .take()
                        .map(|o| Box::new(o) as Box<dyn Read + Send>),
                    child
                        .stderr
                        .take()
                        .map(|e| Box::new(e) as Box<dyn Read + Send>),
                ]
                .into_iter()
                .flatten()
                .map(|r| {
                    let j = self.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(r).lines().map_while(Result::ok) {
                            j.log(line);
                        }
                    })
                })
                .collect();
                for r in readers {
                    let _ = r.join();
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("{} stopped ({status})", program.display()))
                }
            }
            Step::Launch { program, args } => {
                self.log(format!("> {} {}", program.display(), args.join(" ")));
                let mut child = Command::new(&program)
                    .args(&args)
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("{}: {e}", program.display()))?;
                self.log("Started.");
                // This worker waits; the UI stays responsive. Holding the job
                // prevents an import/rebuild from replacing data the game uses.
                if let Some(err) = child.stderr.take() {
                    for line in BufReader::new(err).lines().map_while(Result::ok) {
                        if line.contains("Error") || line.contains("ERROR") {
                            self.log(line);
                        }
                    }
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("The game stopped ({status}): see the log above."))
                }
            }

            Step::Copy { from, to } => {
                if let Some(d) = to.parent() {
                    std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                }
                std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
                self.log(format!("Copied {} -> {}", from.display(), to.display()));
                Ok(())
            }
            Step::Write { path, bytes } => {
                if let Some(d) = path.parent() {
                    std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                }
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Step::Task(f) => f(self),
            Step::Dir(d) => {
                std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))
            }
            Step::Unpack { archive, to } => {
                self.log(format!("Unpacking {}", archive.display()));
                unpack(&archive, &to, self)?;
                self.log(format!("Unpacked to {}", to.display()));
                Ok(())
            }
        }
    }
}

fn ext(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn is_archive(p: &Path) -> bool {
    p.is_file() && matches!(ext(p).as_str(), "zip" | "rar" | "7z")
}

// Desktop extraction policy. Actual output is counted independently of metadata.
const ENTRY_BYTES: u64 = 512 * 1024 * 1024;
const TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const ENTRY_COUNT: usize = 100_000;

fn copy_bounded(
    input: &mut impl Read,
    output: &mut impl std::io::Write,
    allowance: u64,
) -> std::io::Result<u64> {
    let mut copied = 0u64;
    let mut buf = [0u8; 8192];
    loop {
        let remaining = allowance - copied;
        let wanted = remaining.saturating_add(1).min(buf.len() as u64) as usize;
        let n = match input.read(&mut buf[..wanted]) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if n == 0 {
            return Ok(copied);
        }
        if n as u64 > remaining {
            return Err(std::io::Error::other("archive output budget exceeded"));
        }
        output.write_all(&buf[..n])?;
        copied += n as u64;
    }
}

fn unpack(archive: &Path, to: &Path, job: &Job) -> Result<(), String> {
    if ext(archive) != "zip" {
        return Err("Automatic RAR/7z extraction is disabled because external tools cannot enforce the launcher's output limits. Extract it separately and add the folder, or use ZIP.".into());
    }
    let stage = crate::publish::Staged::new(to)?;
    if let Err(error) = unpack_zip(archive, &stage.path, ENTRY_BYTES, TOTAL_BYTES, ENTRY_COUNT) {
        return Err(stage.abort(error));
    }
    if let Some(warning) = stage.publish()? {
        // A published install is usable; preserve the recovery path in the log.
        job.log(warning);
    }
    Ok(())
}

fn unpack_zip(
    archive: &Path,
    to: &Path,
    per_entry: u64,
    total: u64,
    count: usize,
) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    if z.len() > count {
        return Err("archive entry count budget exceeded".into());
    }
    let mut written = 0u64;
    for i in 0..z.len() {
        let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
        if entry.enclosed_name().is_none() {
            return Err("archive contains an unsafe path".into());
        }
        // Treat Windows separators as separators on every host. Reject drive
        // prefixes/ADS, parent traversal and Windows aliases before any writes.
        let name = entry.name().map_err(|e| e.to_string())?.replace('\\', "/");
        if !c2_vfs::is_safe_relative_path(&name) {
            return Err(format!("unsafe archive path: {name}"));
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 && kind != 0o040000 {
                return Err("archive links and special files are unsupported".into());
            }
        }
        let out = to.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        let allowance = per_entry.min(total - written);
        if entry.size() > allowance {
            return Err("archive output budget exceeded".into());
        }
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        // Fresh stage, regular files only, no overwrites of duplicate entries.
        let mut w = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)
            .map_err(|e| e.to_string())?;
        written += copy_bounded(&mut entry, &mut w, allowance).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::publish::tests::Temp;
    use std::io::Write;

    fn archive(root: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = root.join("test.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn extraction_enforces_entry_total_and_count_limits() {
        for (entry, total, count, ok) in [
            (4, 8, 2, true),
            (3, 8, 2, false),
            (4, 7, 2, false),
            (4, 8, 1, false),
        ] {
            let t = Temp::new();
            let input = archive(&t.0, &[("a", b"1234"), ("b", b"5678")]);
            let stage = crate::publish::Staged::new(&t.0.join("out")).unwrap();
            assert_eq!(
                unpack_zip(&input, &stage.path, entry, total, count).is_ok(),
                ok
            );
            stage.abort("done".into());
        }
    }

    #[test]
    fn rejects_unsafe_names_and_duplicate_output() {
        for names in [
            vec!["../outside"],
            vec!["..\\outside"],
            vec!["C:outside"],
            vec!["NUL.txt"],
            vec!["dir/a", "dir//a"],
        ] {
            let t = Temp::new();
            let entries: Vec<_> = names.iter().map(|s| (*s, &b"x"[..])).collect();
            let input = archive(&t.0, &entries);
            let out = t.0.join("out");
            let stage = crate::publish::Staged::new(&out).unwrap();
            assert!(unpack_zip(&input, &stage.path, 8, 8, 8).is_err());
            stage.abort("rejected".into());
            assert!(!out.exists());
            assert!(!t.0.join("outside").exists());
        }
    }

    #[test]
    fn bounded_copy_probes_overflow_and_retries_interrupted_reads() {
        struct Interrupted(bool);
        impl Read for Interrupted {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                (&b"12345"[..]).read(bytes)
            }
        }
        let mut out = Vec::new();
        assert!(copy_bounded(&mut Interrupted(false), &mut out, 4).is_err());
        assert!(out.is_empty());
        assert_eq!(copy_bounded(&mut &b"1234"[..], &mut out, 4).unwrap(), 4);
        assert_eq!(out, b"1234");
    }

    #[test]
    fn failed_zip_keeps_live_install_and_cleans_stage() {
        let t = Temp::new();
        let input = archive(&t.0, &[("safe", b"x"), ("../escape", b"bad")]);
        let out = t.0.join("out");
        std::fs::create_dir(&out).unwrap();
        std::fs::write(out.join("old"), b"old").unwrap();
        let job = Job {
            title: "test".into(),
            state: Arc::default(),
        };
        assert!(unpack(&input, &out, &job).is_err());
        assert_eq!(std::fs::read(out.join("old")).unwrap(), b"old");
        assert!(crate::publish::pending(&t.0).is_empty());
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;

    fn settle(handle: &Handle) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while handle.result().is_none() {
            assert!(std::time::Instant::now() < until, "worker did not finish");
            std::thread::yield_now();
        }
    }

    #[test]
    fn work_and_unconsumed_results_cannot_be_overwritten() {
        let mut handle = Handle::default();
        let (tx, rx) = channel();
        handle
            .start_task("first", move |_| {
                rx.recv().map_err(|e| e.to_string())?;
                Ok(Completion::Imported("first".into()))
            })
            .unwrap();
        assert!(handle.start("overlap", vec![]).is_err());
        tx.send(()).unwrap();
        settle(&handle);
        assert!(handle.busy());
        assert!(handle.start("lost result", vec![]).is_err());
        assert!(
            matches!(handle.poll(), Some(Ok(Completion::Imported(p))) if p == Path::new("first"))
        );
        assert!(handle.poll().is_none());
        assert!(!handle.busy());
        assert!(handle.start("next", vec![]).is_ok());
    }

    #[test]
    fn failed_or_panicking_workers_release_the_handle_once_polled() {
        for panic in [false, true] {
            let mut handle = Handle::default();
            handle
                .start_task("failure", move |_| {
                    if panic {
                        panic!("synthetic worker failure");
                    }
                    Err("synthetic error".into())
                })
                .unwrap();
            settle(&handle);
            assert_eq!(handle.result(), Some(false));
            assert!(handle.busy());
            assert!(handle.poll().unwrap().is_err());
            assert!(handle.poll().is_none());
            assert!(!handle.busy());
            assert!(handle.start("retry", vec![]).is_ok());
        }
    }
}
