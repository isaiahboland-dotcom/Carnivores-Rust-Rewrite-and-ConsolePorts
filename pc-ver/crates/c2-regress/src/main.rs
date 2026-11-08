//! `c2-regress`: run a scenario, compare two runs, build reports.
//! See docs/REGRESSION.md.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use c2_regress::compare::{compare, Thresholds};
use c2_regress::game::Game;
use c2_regress::record::{self, RunContext};
use c2_regress::scenario_file::{GameStatus, ScenarioFile};
use c2_regress::{headless, html, output, pdf_report, publish};

const USAGE: &str = "\
usage:
  c2-regress list    --scenario FILE
  c2-regress species --game DIR --area AREA
                     the game's characters as the runner loads them for AREA:
                     index (for index:N), AI number, model, name
  c2-regress run     --scenario FILE --game-id ID --game DIR [--out ROOT] [--repeat N]
                     [--label TEXT] [--no-render] [--size WxH] [--c2-pc PATH]
                        [--baseline RUN_DIR] [--thresholds FILE] [--inject FAULT]
                     [--engine rust|original] [--hook-dir DIR]
     --engine original: the game's own hunt executable under Wine, driven by
     the hook (pc-ver/crates/c2-regress/original/build.sh)
  c2-regress compare BASELINE_DIR CANDIDATE_DIR [--thresholds FILE] [--fail-on-warn]
  c2-regress report  RUN_DIR
  c2-regress pdf     --out FILE RUN_DIR [RUN_DIR...]
  c2-regress publish RUN_DIR --hash-list recipes/bases/BASE.toml [--to DIR]
  c2-regress original-profile OUT.SAV [--name NAME] [--score N] [--res 0-7]
                     a hunter in the original's save format (trophy00.sav), with
                     the original's default options, for original-engine runs

--baseline published: the reference baseline published in this checkout
(pc-ver/crates/c2-regress/baselines/<scenario>/<game>/).

Exit codes: 0 passed, 1 failed (functional or visual), 2 usage or set-up
error, 3 the game is unsupported or planned for this scenario, 4 performance
warnings with --fail-on-warn.";

struct Args {
    words: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(raw: &[String]) -> Self {
        let switches = ["--no-render", "--fail-on-warn"];
        let (mut words, mut flags) = (Vec::new(), Vec::new());
        let mut it = raw.iter();
        while let Some(a) = it.next() {
            if switches.contains(&a.as_str()) {
                flags.push((a.clone(), None));
            } else if a.starts_with("--") {
                flags.push((a.clone(), it.next().cloned()));
            } else {
                words.push(a.clone());
            }
        }
        Self { words, flags }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_deref())
    }

    fn need(&self, name: &str) -> Result<&str> {
        self.get(name)
            .with_context(|| format!("{name} is required\n\n{USAGE}"))
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }
}

/// A `Write` into a shared buffer, for capturing the engine's log.
#[derive(Clone)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut v) = self.0.lock() {
            v.extend_from_slice(b);
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn thresholds(a: &Args) -> Result<Thresholds> {
    a.get("--thresholds").map_or_else(
        || Ok(Thresholds::default()),
        |p| Thresholds::load(Path::new(p)),
    )
}

fn c2_pc_path(a: &Args) -> Option<PathBuf> {
    if let Some(p) = a.get("--c2-pc") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    let p = exe.with_file_name(format!("c2-pc{}", std::env::consts::EXE_SUFFIX));
    p.exists().then_some(p)
}

fn list(a: &Args) -> Result<ExitCode> {
    let f = ScenarioFile::load(Path::new(a.need("--scenario")?))?;
    println!("{} v{}", f.id(), f.version());
    for g in f.games() {
        println!(
            "  {:<14} {:<12} {}",
            g.id,
            format!("{:?}", g.status),
            g.name
        );
        if !g.reason.is_empty() {
            println!("  {:<14} {}", "", g.reason);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn species(a: &Args) -> Result<ExitCode> {
    let area = a.need("--area")?;
    let game = Game::load(Path::new(a.need("--game")?), area)?;
    let dialect = if game.dialect {
        ", Modder's Edition"
    } else {
        ""
    };
    println!("{area}{dialect}: {} characters", game.species.len());
    println!("  {:>5} {:>4} {:<24} name", "index", "ai", "model");
    for (i, s) in game.species.iter().enumerate() {
        let model = if s.ani_time.is_empty() {
            "(none)"
        } else {
            s.info.file.as_str()
        };
        println!("  {i:>5} {:>4} {model:<24} {}", s.info.ai, s.info.name);
    }
    Ok(ExitCode::SUCCESS)
}

fn run(a: &Args) -> Result<ExitCode> {
    let path = PathBuf::from(a.need("--scenario")?);
    let file = ScenarioFile::load(&path)?;
    let def = file.game(a.need("--game-id")?)?;
    if def.status != GameStatus::Supported {
        println!(
            "{}: {:?} for {}: {}",
            def.name,
            def.status,
            file.id(),
            def.reason
        );
        return Ok(ExitCode::from(3));
    }
    let scenario = file.for_game(def)?;
    let area = def
        .area
        .as_deref()
        .context("the game section names no area")?;
    let repeat: usize = a.get("--repeat").map_or(Ok(1), str::parse)?;
    let original = match a.get("--engine") {
        None | Some("rust") => false,
        Some("original") => true,
        Some(e) => bail!("--engine rust|original, not {e:?}"),
    };
    let platform = format!("pc-{}", std::env::consts::OS);
    // A deliberate fault, to check that the checks catch it.
    let inject = a
        .get("--inject")
        .map(c2_core::scenario::Inject::parse)
        .transpose()
        .map_err(anyhow::Error::msg)?;

    let buf = Buf(Arc::new(Mutex::new(Vec::new())));
    let writer = buf.clone();
    let sub = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let (game, runs) = tracing::subscriber::with_default(sub, || -> Result<_> {
        let game = Game::load(Path::new(a.need("--game")?), area)?;
        // The original engine runs its own repeats (below).
        let ours = if original { 0 } else { repeat.max(1) };
        let runs: Vec<_> = (0..ours)
            .map(|i| {
                tracing::info!("headless run {} of {}", i + 1, repeat.max(1));
                headless::run(&game, &scenario, inject)
            })
            .collect();
        Ok((game, runs))
    })?;

    let root = a
        .get("--out")
        .map_or_else(output::default_root, PathBuf::from);
    if original && scenario.script != c2_core::scenario::Script::Tour {
        bail!("--engine original runs the tour script only (its memory is not read, so a kill cannot be checked)");
    }
    if original {
        return run_original(a, &file, def, &scenario, &game, &root, repeat);
    }
    let dir = output::run_dir(
        &root,
        file.id(),
        &def.id,
        &platform,
        a.get("--label").unwrap_or(""),
    );
    std::fs::create_dir_all(&dir)?;
    let (mut render, mut render_missing, mut render_failed) = (None, None, false);
    if a.has("--no-render") {
        render_missing = Some("--no-render given".to_string());
    } else if let Some(exe) = c2_pc_path(a) {
        let status = Command::new(&exe)
            .args(["regress", "--scenario"])
            .arg(&path)
            .args(["--game-id", &def.id, "--game", a.need("--game")?, "--out"])
            .arg(&dir)
            .args(["--size", a.get("--size").unwrap_or("960x540")])
            .args(a.get("--inject").map(|i| ["--inject", i]).iter().flatten())
            .status()
            .with_context(|| format!("starting {}", exe.display()))?;
        match output::read_json(&dir.join("render.json")) {
            Ok(r) if status.success() => render = Some(r),
            _ => {
                render_failed = true;
                render_missing = Some(format!("the render pass failed: {status}"));
            }
        }
    } else {
        render_missing = Some(
            "c2-pc not found beside c2-regress (cargo build --release -p c2-pc), or pass --c2-pc"
                .to_string(),
        );
    }
    let mut log =
        String::from_utf8_lossy(&buf.0.lock().map(|b| b.clone()).unwrap_or_default()).into_owned();
    for (i, r) in runs.iter().enumerate() {
        for (what, v) in [("set-up error", &r.setup_error), ("panic", &r.panic)] {
            if let Some(v) = v {
                log.push_str(&format!("ERROR run {}: {what}: {v}\n", i + 1));
            }
        }
    }
    if let Ok(r) = std::fs::read_to_string(dir.join("render.log")) {
        log.push_str("--- render pass ---\n");
        log.push_str(&r);
    }
    let ctx = RunContext {
        file: &file,
        def,
        game: &game,
        scenario: &scenario,
        platform: &platform,
        runs: &runs,
        render,
        render_missing,
        inject: a.get("--inject").map(str::to_string),
        log,
    };
    let mut passed = record::write(&dir, &ctx)? && !render_failed;
    if let Some(base) = a.get("--baseline") {
        let base = if base == "published" {
            publish::published_dir(file.id(), &def.id)
        } else {
            PathBuf::from(base)
        };
        let v = compare(&base, &dir, &thresholds(a)?)?;
        passed &= v.functional_ok && v.visual_ok;
    }
    html::write(&dir)?;
    println!(
        "{}: {}",
        if passed { "PASSED" } else { "FAILED" },
        dir.display()
    );
    Ok(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[allow(clippy::too_many_arguments)]
fn run_original(
    a: &Args,
    file: &ScenarioFile,
    def: &c2_regress::scenario_file::GameDef,
    scenario: &c2_core::scenario::Scenario,
    game: &Game,
    root: &Path,
    repeat: usize,
) -> Result<ExitCode> {
    use c2_regress::original::{default_hook_dir, run, Original};
    let hook_dir = a
        .get("--hook-dir")
        .map(PathBuf::from)
        .or_else(default_hook_dir)
        .context("no --hook-dir")?;
    let label = a.get("--label").unwrap_or("");
    let dir = output::run_dir(root, file.id(), &def.id, "original-wine", label);
    std::fs::create_dir_all(&dir)?;
    let work = root.join("_original").join(&def.id);
    let o = Original {
        file,
        def,
        scenario,
        game,
        game_dir: Path::new(a.need("--game")?),
        work: &work,
        hook_dir: &hook_dir,
        repeat,
    };
    let mut passed = run(&o, &dir)?;
    if let Some(base) = a.get("--baseline") {
        let base = if base == "published" {
            publish::published_dir(file.id(), &def.id)
        } else {
            PathBuf::from(base)
        };
        let v = compare(&base, &dir, &thresholds(a)?)?;
        passed &= v.functional_ok && v.visual_ok;
    }
    html::write(&dir)?;
    println!(
        "{}: {}",
        if passed { "PASSED" } else { "FAILED" },
        dir.display()
    );
    Ok(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn compare_cmd(a: &Args) -> Result<ExitCode> {
    let [base, cand] = a.words.as_slice() else {
        bail!("compare takes BASELINE_DIR CANDIDATE_DIR\n\n{USAGE}");
    };
    let v = compare(Path::new(base), Path::new(cand), &thresholds(a)?)?;
    html::write(Path::new(cand))?;
    println!("{}", serde_json::to_string_pretty(&v.report)?);
    let _ = std::io::stdout().flush();
    Ok(if !(v.functional_ok && v.visual_ok) {
        ExitCode::FAILURE
    } else if v.perf_warnings > 0 && a.has("--fail-on-warn") {
        ExitCode::from(4)
    } else {
        ExitCode::SUCCESS
    })
}

fn original_profile(a: &Args) -> Result<ExitCode> {
    let [out] = a.words.as_slice() else {
        bail!("original-profile takes OUT.SAV\n\n{USAGE}");
    };
    let score: i32 = a.get("--score").map_or(Ok(5000), str::parse)?;
    let mut save = c2_core::profile::new_player(a.get("--name").unwrap_or("regress"), 0, score);
    // Game.cpp: OptRes 0..7 = 320x240, 400x300, 512x384, 640x480, 800x600, ...
    if let Some(r) = a.get("--res") {
        save.options.res = r.parse()?;
    }
    std::fs::write(out, c2_formats::sav::write(&save))?;
    println!("{out}");
    Ok(ExitCode::SUCCESS)
}

fn publish_cmd(a: &Args) -> Result<ExitCode> {
    let [run] = a.words.as_slice() else {
        bail!("publish takes RUN_DIR\n\n{USAGE}");
    };
    let run = Path::new(run);
    let list = a.need("--hash-list")?;
    let to = match a.get("--to") {
        Some(d) => PathBuf::from(d),
        None => {
            let m = output::read_json(&run.join("manifest.json"))?;
            let id = |p: &str| m.pointer(p).and_then(|v| v.as_str()).map(str::to_string);
            publish::published_dir(
                &id("/scenario/id").context("manifest without scenario id")?,
                &id("/game/id").context("manifest without game id")?,
            )
        }
    };
    // The list as the repository names it (no local path in the dataset);
    // the published-baselines test finds it from there.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (repo, full) = (
        repo.canonicalize()?,
        Path::new(list)
            .canonicalize()
            .with_context(|| format!("reading {list}"))?,
    );
    let label = full
        .strip_prefix(&repo)
        .context("the hash list must be a file of this repository")?
        .to_path_buf();
    publish::publish(run, &full, &label.to_string_lossy(), &to)?;
    println!("published {} -> {}", run.display(), to.display());
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = raw.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let a = Args::parse(rest);
    let r = match cmd.as_str() {
        "list" => list(&a),
        "species" => species(&a),
        "run" => run(&a),
        "compare" => compare_cmd(&a),
        "report" => match a.words.first() {
            Some(d) => html::write(Path::new(d)).map(|()| ExitCode::SUCCESS),
            None => Err(anyhow::anyhow!("report takes RUN_DIR\n\n{USAGE}")),
        },
        "pdf" => a.need("--out").and_then(|out| {
            let dirs: Vec<PathBuf> = a.words.iter().map(PathBuf::from).collect();
            pdf_report::write(&dirs, Path::new(out))?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }),
        "publish" => publish_cmd(&a),
        "original-profile" => original_profile(&a),
        _ => Err(anyhow::anyhow!("unknown command {cmd:?}\n\n{USAGE}")),
    };
    r.unwrap_or_else(|e| {
        eprintln!("error: {e:#}");
        ExitCode::from(2)
    })
}
