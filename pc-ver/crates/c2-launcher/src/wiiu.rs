//! Wii U installs (`wiiu-ver/`). Each game is its own install: a folder
//! `wiiu/apps/<name>/` on the SD card holding the merged game as the app
//! reads it (`c2-cook --target wiiu`: the original formats, upper-case
//! names, `INFO.TXT`, `FILES.TXT`), its saves and logs, and `<name>.wuhb`,
//! a home-menu entry of its own. The `.wuhb` bundles `install.txt` (the
//! folder's name, which the app reads from its `/vol/content`) and art cut
//! from the game's own menu picture (`cia::wiiu_art`), so several games sit
//! side by side on the Wii U Menu (say Triassic, Mandibles, and Carnivores
//! 2 with the Modder's Edition and add-ons). An `.rpx` (Homebrew Launcher)
//! has no bundle: it can only be the default install, `carnivores-probe`.

use std::path::{Path, PathBuf};

use crate::jobs::Step;

/// The install an `.rpx`, or a `.wuhb` without `install.txt`, reads.
pub const SD_FOLDER: &str = "carnivores-probe";

/// The Wii U app as the Wii U build makes it (`wiiu-ver/perf-probe`).
pub const APP: &str = "c2-wiiu-perf-probe.rpx";
pub const WUHB_TOOL: &str = "wuhbtool";

/// One game for the Wii U.
pub struct Install<'a> {
    /// Its folder under `wiiu/apps/` and the `.wuhb`'s file name.
    pub name: &'a str,
    /// Its name on the Wii U Menu and title screen.
    pub title: &'a str,
    /// `c2-cook`'s arguments for the game (`--game`, `--overlay`, `--mod`,
    /// `--recipe` ...), without `--target` and `--out`.
    pub cook_args: Vec<String>,
    /// The game folder whose menu picture the home-menu art is cut from.
    pub art_from: Option<&'a Path>,
    /// The user's own icon, TV and GamePad start screens (PNGs), each used
    /// instead of the cut one.
    pub own_art: [Option<&'a Path>; 3],
}

/// The tools an install is built with.
pub struct Tools<'a> {
    pub cook: &'a Path,
    pub app: &'a Path,
    /// `None`: an `.rpx` for the Homebrew Launcher.
    pub wuhbtool: Option<&'a Path>,
}

/// The game folder `c2-cook` is given (`--game`), or the last overlay (a
/// Modder's Edition release has its own menu picture).
pub fn art_source(cook_args: &[String]) -> Option<PathBuf> {
    let last = |key: &str| {
        cook_args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == key)
            .filter_map(|(i, _)| cook_args.get(i + 1))
            .next_back()
            .map(PathBuf::from)
    };
    last("--overlay").or_else(|| last("--game"))
}

/// The steps for install `i` on the SD card `sd` (`work`: a folder for the
/// bundle's content and art).
pub fn steps(i: &Install, t: &Tools, sd: &Path, work: &Path) -> Vec<Step> {
    // An .rpx can't say which folder it is: the default one.
    let name = if t.wuhbtool.is_some() {
        i.name
    } else {
        SD_FOLDER
    };
    let dir = sd.join("wiiu").join("apps").join(name);
    let mut cook = i.cook_args.clone();
    cook.extend([
        "--target".into(),
        "wiiu".into(),
        "--title".into(),
        i.title.to_string(),
        "--out".into(),
        dir.display().to_string(),
    ]);
    let mut steps = vec![
        Step::Dir(dir.clone()),
        Step::Run {
            program: t.cook.to_path_buf(),
            args: cook,
        },
    ];
    let Some(tool) = t.wuhbtool else {
        steps.push(Step::Copy {
            from: t.app.to_path_buf(),
            to: dir.join(format!("{name}.rpx")),
        });
        return steps;
    };
    let content = work.join("content");
    steps.push(Step::Write {
        path: content.join("install.txt"),
        bytes: format!("{name}\n").into_bytes(),
    });
    let mut args = vec![
        t.app.display().to_string(),
        dir.join(format!("{name}.wuhb")).display().to_string(),
        format!("--name={}", i.title),
        format!("--short-name={}", i.title),
        "--author=Carnivores Rust Rewrite".into(),
        format!("--content={}", content.display()),
    ];
    let cut = i.art_from.and_then(crate::cia::wiiu_art);
    let cut: [Option<Vec<u8>>; 3] = match cut {
        Some([a, b, c]) => [Some(a), Some(b), Some(c)],
        None => [None, None, None],
    };
    for ((made, own), (file, flag)) in cut.into_iter().zip(i.own_art).zip([
        ("icon.png", "--icon"),
        ("tv.png", "--tv-image"),
        ("drc.png", "--drc-image"),
    ]) {
        if let Some(p) = own {
            args.push(format!("{flag}={}", p.display()));
        } else if let Some(bytes) = made {
            let path = work.join(file);
            args.push(format!("{flag}={}", path.display()));
            steps.push(Step::Write { path, bytes });
        }
    }
    steps.push(Step::Run {
        program: tool.to_path_buf(),
        args,
    });
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools<'a>(wuhb: Option<&'a Path>) -> Tools<'a> {
        Tools {
            cook: Path::new("c2-cook"),
            app: Path::new("app.rpx"),
            wuhbtool: wuhb,
        }
    }

    #[test]
    fn a_wuhb_names_its_folder_and_cooks_into_it() {
        let i = Install {
            name: "triassic",
            title: "Carnivores Triassic",
            cook_args: vec!["--game".into(), "T".into()],
            art_from: None,
            own_art: [None, None, None],
        };
        let steps = steps(
            &i,
            &tools(Some(Path::new("wuhbtool"))),
            Path::new("sd"),
            Path::new("w"),
        );
        let dir = Path::new("sd/wiiu/apps/triassic");
        let Step::Run { args, .. } = &steps[1] else {
            panic!("cook first")
        };
        assert_eq!(args[..2], ["--game".to_string(), "T".into()]);
        assert!(args
            .windows(2)
            .any(|w| w == ["--out", &dir.display().to_string()]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--title", "Carnivores Triassic"]));
        let Step::Write { path, bytes } = &steps[2] else {
            panic!("install.txt")
        };
        assert_eq!(
            (path.as_path(), bytes.as_slice()),
            (Path::new("w/content/install.txt"), &b"triassic\n"[..])
        );
        let Step::Run { args, .. } = steps.last().unwrap() else {
            panic!("wuhbtool last")
        };
        assert_eq!(args[1], dir.join("triassic.wuhb").display().to_string());
        assert!(args.contains(&"--name=Carnivores Triassic".to_string()));
    }

    #[test]
    fn an_rpx_is_the_default_install() {
        let i = Install {
            name: "triassic",
            title: "T",
            cook_args: Vec::new(),
            art_from: None,
            own_art: [None, None, None],
        };
        let steps = steps(&i, &tools(None), Path::new("sd"), Path::new("w"));
        let Step::Copy { to, .. } = steps.last().unwrap() else {
            panic!("copy")
        };
        assert_eq!(
            to,
            &Path::new("sd/wiiu/apps")
                .join(SD_FOLDER)
                .join(format!("{SD_FOLDER}.rpx"))
        );
    }

    #[test]
    fn the_users_own_art_is_used_as_picked() {
        let i = Install {
            name: "triassic",
            title: "T",
            cook_args: Vec::new(),
            art_from: None,
            own_art: [
                Some(Path::new("my-icon.png")),
                None,
                Some(Path::new("pad.png")),
            ],
        };
        let steps = steps(
            &i,
            &tools(Some(Path::new("wuhbtool"))),
            Path::new("sd"),
            Path::new("w"),
        );
        let Step::Run { args, .. } = steps.last().unwrap() else {
            panic!("wuhbtool last")
        };
        assert!(args.contains(&"--icon=my-icon.png".to_string()));
        assert!(args.contains(&"--drc-image=pad.png".to_string()));
        // No game picture to cut the TV's from: none given.
        assert!(!args.iter().any(|a| a.starts_with("--tv-image")));
    }

    #[test]
    fn art_comes_from_the_last_overlay_else_the_game() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(art_source(&a(&["--game", "G"])), Some(PathBuf::from("G")));
        assert_eq!(
            art_source(&a(&["--game", "G", "--overlay", "A", "--overlay", "B"])),
            Some(PathBuf::from("B"))
        );
    }
}
