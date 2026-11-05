//! The recipe page's checks, on synthetic games and recipes.
use super::*;

fn settle(p: &mut Page) {
    let mut handle = crate::jobs::Handle::default();
    for _ in 0..500 {
        p.dispatch(&mut handle);
        if let Some(result) = handle.poll() {
            match result {
                Ok(crate::jobs::Completion::Recipe(messages)) => p.apply(messages),
                _ => panic!("unexpected recipe worker result"),
            }
        }
        if !p.working() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("checks never finished");
}

#[test]
fn apk_recipe_finds_its_own_base_and_rejects_a_wrong_selected_game() {
    let dir = std::env::temp_dir().join(format!("c2rs-apk-recipe-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("bases")).unwrap_or_else(|e| panic!("{e}"));
    let correct = dir.join("converted");
    std::fs::create_dir_all(&correct).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        correct.join(c2_mods::mobile_base::SOURCE_APK),
        "sha256:synthetic",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        dir.join("bases/phone.toml"),
        "[base]\nid = 'phone'\napk = 'sha256:synthetic'\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let path = dir.join("phone.toml");
    std::fs::write(&path, "[recipe]\nid = 'phone'\nbase = 'phone'\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let wrong = dir.join("wrong");
    let mut p =
        Page::open(&path, None, vec![wrong.clone()], vec![]).unwrap_or_else(|e| panic!("{e}"));
    settle(&mut p);
    assert!(!p.ready());
    p.candidates.push(correct.clone());
    p.check_base(vec![wrong.clone()]);
    settle(&mut p);
    assert_eq!(p.game, Some(correct));
    assert!(p.ready());
    p.game = Some(wrong.clone());
    p.check_base(vec![wrong.clone()]);
    settle(&mut p);
    assert_eq!(p.game, Some(wrong));
    assert!(
        !p.ready(),
        "an explicit wrong selection must not silently switch"
    );
    std::fs::remove_dir_all(dir).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn standalone_base_found_past_the_game_tabs() {
    let dir = std::env::temp_dir().join(format!("c2rs-recipe-base-{}", std::process::id()));
    let game = |name: &str, data: &[u8]| {
        let h = dir.join(name).join("HUNTDAT");
        std::fs::create_dir_all(&h).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(h.join("X.CAR"), data).unwrap_or_else(|e| panic!("{e}"));
        dir.join(name)
    };
    let (vanilla, other, standalone) = (
        game("vanilla", b"c2"),
        game("other", b"zz"),
        game("standalone", b"sa"),
    );
    std::fs::create_dir_all(dir.join("recipes/bases")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        dir.join("recipes/bases/sa.toml"),
        format!(
            "[base]\nid = \"sa\"\n\n[files]\n\"HUNTDAT/X.CAR\" = \"{}\"\n",
            recipe::sha256(b"sa")
        ),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let path = dir.join("recipes/sa.toml");
    std::fs::write(
        &path,
        "[recipe]\nid = \"sa\"\nname = \"SA\"\nbase = \"sa\"\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut p = Page::open(&path, None, vec![vanilla], vec![other, standalone.clone()])
        .unwrap_or_else(|e| panic!("{e}"));
    settle(&mut p);
    let (got, status) = (p.game.clone(), p.base_status.clone());
    std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got, Some(standalone));
    assert!(matches!(status, Status::Ok(_)), "{status:?}");
}

/// Owner's report 2026-11-05: an instance made for one game played
/// another (the launcher's game, switched since). The game it is
/// checked against is kept as its own, and a game that isn't its base
/// stops it until the player says to use it anyway.
#[test]
fn a_wrong_base_is_not_played_silently_and_the_game_is_kept() {
    let dir = std::env::temp_dir().join(format!("c2rs-recipe-guard-{}", std::process::id()));
    let h = dir.join("other/HUNTDAT");
    std::fs::create_dir_all(&h).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(h.join("X.CAR"), b"zz").unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir_all(dir.join("bases")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        dir.join("bases/sa.toml"),
        format!(
            "[base]\nid = \"sa\"\n\n[files]\n\"HUNTDAT/X.CAR\" = \"{}\"\n",
            recipe::sha256(b"sa")
        ),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let path = dir.join("sa.toml");
    std::fs::write(&path, "[recipe]\nid = \"sa\"\nbase = \"sa\"\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut p = Page::open(&path, None, vec![dir.join("other")], Vec::new())
        .unwrap_or_else(|e| panic!("{e}"));
    settle(&mut p);
    let differs = p.base_differs();
    let blocked = !p.ready();
    p.accept_base = true;
    let accepted = p.ready();
    let kept = p.to_use().game;
    std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(differs && blocked, "a different game must not be ready");
    assert!(accepted, "the player can use another version on purpose");
    assert_eq!(kept, Some(dir.join("other")));
}

#[test]
fn entries_found_in_a_downloads_folder() {
    let dir = std::env::temp_dir().join(format!("c2rs-recipe-page-{}", std::process::id()));
    let huntdat = dir.join("downloads").join("Some Mod").join("HUNTDAT");
    std::fs::create_dir_all(&huntdat).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(huntdat.join("X.CAR"), b"abc").unwrap_or_else(|e| panic!("{e}"));
    let r = format!(
        "[recipe]\nid = \"t\"\nname = \"T\"\n\n[[mods]]\nid = \"some-mod\"\n\n\
             [mods.files]\n\"HUNTDAT/X.CAR\" = \"{}\"\n",
        recipe::sha256(b"abc")
    );
    let path = dir.join("t.toml");
    std::fs::write(&path, r).unwrap_or_else(|e| panic!("{e}"));
    let mut p = Page::open(&path, None, vec![dir.join("game")], Vec::new())
        .unwrap_or_else(|e| panic!("{e}"));
    settle(&mut p);
    assert!(!p.ready());
    p.scan(vec![dir.join("downloads")]);
    settle(&mut p);
    let used = p.to_use();
    std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(p.ready(), "{:?}", p.rows[0].status);
    assert_eq!(
        used.folders.get("some-mod").and_then(|f| f.file_name()),
        Some(std::ffi::OsStr::new("Some Mod"))
    );
}

mod job_tests {
    use super::*;

    #[test]
    fn recipe_checks_share_the_handle_and_failure_can_be_retried() {
        let temp = crate::publish::tests::Temp::new();
        let path = temp.0.join("recipe.toml");
        std::fs::write(&path, "[recipe]\nid = 'test'\n").unwrap();
        let mut p = Page::open(&path, None, vec![], vec![]).unwrap();
        p.scan(vec![temp.0.clone()]);
        // Use a real row check so an otherwise empty recipe has worker work.
        p.rows.push(Row {
            kind: "mod",
            entry: RecipeMod {
                id: "missing".into(),
                ..Default::default()
            },
            game: false,
            folder: None,
            status: Status::Unset,
        });
        p.check_row(0, temp.0.join("missing"));
        let mut handle = crate::jobs::Handle::default();
        p.dispatch(&mut handle);
        assert!(handle.start("replace page", vec![]).is_err());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while handle.result().is_none() {
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        assert!(handle.busy(), "undelivered recipe results protect the page");
        match handle.poll() {
            Some(Ok(crate::jobs::Completion::Recipe(messages))) => p.apply(messages),
            _ => panic!("expected recipe checks"),
        }
        assert!(!p.working());
        assert!(matches!(p.rows[0].status, Status::Bad(_)));
        p.check_row(0, temp.0.join("missing"));
        p.failed("worker stopped");
        assert!(!p.working());
        assert!(p.requests.is_empty());
        p.check_row(0, temp.0.join("retry"));
        assert!(p.working());
    }
}
