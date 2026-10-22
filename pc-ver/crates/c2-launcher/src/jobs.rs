//! Work done off the UI thread: running `c2-pc` / `c2-cook`, copying the
//! 3DS app, unpacking add-on archives. Each job is a list of steps run in
//! order; its output goes to a shared log the UI shows.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub enum Step {
    /// A program with its arguments; fails the job if it does.
    Run {
        program: PathBuf,
        args: Vec<String>,
    },
    /// Start a program and don't wait (the game).
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
            let mut ok = true;
            for step in steps {
                if let Err(e) = j.run(step) {
                    j.log(format!("Failed: {e}"));
                    ok = false;
                    break;
                }
            }
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

    fn run(&self, step: Step) -> Result<(), String> {
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
                Command::new(&program)
                    .args(&args)
                    .spawn()
                    .map_err(|e| format!("{}: {e}", program.display()))?;
                self.log("Started.");
                Ok(())
            }
            Step::Copy { from, to } => {
                if let Some(d) = to.parent() {
                    std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                }
                std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
                self.log(format!("Copied {} -> {}", from.display(), to.display()));
                Ok(())
            }
            Step::Unpack { archive, to } => {
                self.log(format!("Unpacking {}", archive.display()));
                unpack(&archive, &to)?;
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

/// `.zip` here; `.rar` / `.7z` with `7z`, `unrar` or `bsdtar` if installed.
fn unpack(archive: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    if ext(archive) == "zip" {
        let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        for i in 0..z.len() {
            let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
            // Names that would leave the folder are skipped.
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let out = to.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(d) = out.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let out = to.display().to_string();
    let a = archive.display().to_string();
    let tries: [(&str, Vec<String>); 3] = [
        (
            "7z",
            vec!["x".into(), "-y".into(), format!("-o{out}"), a.clone()],
        ),
        (
            "unrar",
            vec!["x".into(), "-o+".into(), a.clone(), format!("{out}/")],
        ),
        (
            "bsdtar",
            vec!["-xf".into(), a.clone(), "-C".into(), out.clone()],
        ),
    ];
    for (tool, args) in tries {
        if let Ok(s) = Command::new(tool).args(&args).output() {
            if s.status.success() {
                return Ok(());
            }
        }
    }
    Err(format!(
        "can't unpack {}: install 7-Zip (7z), unrar or bsdtar, or unpack it yourself and add the folder",
        archive.display()
    ))
}
