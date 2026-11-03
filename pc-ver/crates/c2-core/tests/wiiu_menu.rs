#![cfg(feature = "portable-load")]
//! The Wii U menu, drawn on the host. With C2_ORIGINAL_HUNTDAT set it reads
//! the original menu pictures and texts; with C2_MENU_SHOTS=DIR it also
//! writes every page as PNGs at the TV's and the GamePad's sizes, with
//! C2_MENU_FONT=FILE.ttf standing in for the console's system font.
extern crate alloc;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/hunt_setup.rs"]
mod hunt_setup;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/ui.rs"]
mod ui;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/menu.rs"]
mod menu;
#[allow(dead_code)]
// The Wii U sources keep their own workspace's style.
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/overlay.rs"]
mod overlay;
#[allow(dead_code)]
#[rustfmt::skip]
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
#[path = "../../../../wiiu-ver/perf-probe/src/install.rs"]
mod install;
mod wiiu_common;
use menu::{button::*, Action, Id, Menu, Page};

fn font() -> ui::Font {
    let bytes = std::env::var("C2_MENU_FONT")
        .ok()
        .and_then(|p| std::fs::read(p).ok());
    ui::Font::new(bytes.map(|b| &*Box::leak(b.into_boxed_slice())))
}

/// The menu with the original game (C2_ORIGINAL_HUNTDAT); `None` without.
fn menu_with_files() -> Option<Menu> {
    let (_, inst) = wiiu_common::original()?;
    let mut m = Menu::new(font());
    m.set_install(inst);
    Some(m)
}

/// Every animal of each cooked install (C2_WIIU_INSTALLS, folders separated
/// by `:`) has a menu picture: roaming add-ons have none of their own, so
/// theirs are drawn from their models.
#[test]
fn every_animal_has_a_picture() {
    let Ok(dirs) = std::env::var("C2_WIIU_INSTALLS") else {
        eprintln!("C2_WIIU_INSTALLS not set: skipped");
        return;
    };
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        let root = std::path::Path::new(dir);
        let mut vfs = c2_vfs::Vfs::new();
        vfs.push("install", Box::new(wiiu_common::Folder::new(root)));
        let info = install::Info::parse(
            &std::fs::read_to_string(root.join("INFO.TXT")).expect("INFO.TXT"),
        );
        let inst = install::Install::open(vfs, info).expect("rules");
        let names: Vec<String> = inst.animals.iter().map(|a| a.name.clone()).collect();
        let mut m = Menu::new(font());
        m.set_install(inst);
        for (i, name) in names.iter().enumerate() {
            assert!(
                m.animal_picture(i).is_some(),
                "{dir}: {name} has no picture"
            );
        }
        m.page = Page::Animals;
        shot(
            &m,
            &format!("animals-{}", root.file_name().unwrap().to_string_lossy()),
        );
    }
}

fn shot(m: &Menu, name: &str) {
    for (w, h, tag) in [(1920usize, 1080usize, "tv"), (854, 480, "gamepad")] {
        // The GamePad texture's rows are padded, as GX2 pads them.
        let pitch = w.next_multiple_of(32);
        let mut px = vec![0u8; pitch * h * 4];
        m.draw(&mut ui::Canvas::new(&mut px, pitch, w, h));
        let Ok(dir) = std::env::var("C2_MENU_SHOTS") else {
            continue;
        };
        let mut rows = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            rows.extend_from_slice(&px[y * pitch * 4..(y * pitch + w) * 4]);
        }
        let file =
            std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}-{tag}.png")))
                .unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.write_header().unwrap().write_image_data(&rows).unwrap();
    }
}

#[test]
fn every_page_draws_and_the_setup_reaches_a_hunt() {
    let Some(mut m) = menu_with_files() else {
        return;
    };
    shot(&m, "1-title");
    m.input(A);
    assert_eq!(m.page, Page::Area);
    m.input(RIGHT);
    m.input(A);
    m.input(DOWN);
    m.input(DOWN);
    m.input(RIGHT);
    m.input(A);
    assert_eq!(m.choice.area, 1);
    assert!(matches!(m.focus, Id::Time(_)));
    shot(&m, "2-area");
    m.input(R);
    assert_eq!(m.page, Page::Animals);
    m.input(RIGHT);
    m.input(A);
    m.input(DOWN);
    m.input(A);
    // Right steps to the next (half a row lower) column; Down lands on the
    // hexagon directly below.
    assert_eq!(m.choice.animals, 0b100_0011);
    shot(&m, "3-animals");
    m.input(R);
    assert_eq!(m.page, Page::Gear);
    m.input(DOWN);
    m.input(DOWN);
    m.input(A);
    shot(&m, "4-gear");
    m.choice.observer = 1;
    m.focus = Id::Observer;
    shot(&m, "5-observer");
    m.choice.observer = 0;
    m.choice.animals = 0;
    m.input(PLUS);
    assert_eq!(m.action, Action::None);
    assert_eq!(m.page, Page::Animals);
    shot(&m, "6-warning");
    m.choice.animals = 1;
    m.input(PLUS);
    assert_eq!(m.action, Action::Hunt);
    assert!(m.loading);
    shot(&m, "7-loading");
    m.open_title();
    for _ in 0..4 {
        m.input(DOWN);
    }
    m.input(A);
    assert_eq!(m.page, Page::Settings);
    m.input(LEFT);
    m.input(LEFT);
    assert_eq!(m.choice.run_pct, 80);
    shot(&m, "8-settings");
    m.input(B);
    assert_eq!((m.page, m.focus), (Page::Title, Id::Settings));
    m.open_death(12, 3);
    shot(&m, "9-death");
    m.input(DOWN);
    m.input(A);
    assert_eq!(m.action, Action::MainMenu);

    // Survival: only the hunters can come in waves; no observer.
    m.open_title();
    m.input(DOWN);
    m.input(A);
    assert_eq!(
        (m.page, m.choice.mode),
        (Page::Area, hunt_setup::MODE_SURVIVAL)
    );
    m.input(R);
    assert_eq!(m.page, Page::Animals);
    m.choice.animals = 0b1;
    m.focus = Id::Animal(1);
    m.input(A);
    assert_eq!(
        m.choice.animals, 0b1,
        "Ankylosaurus is not a survival animal"
    );
    m.input(PLUS);
    assert_eq!(m.action, Action::None);
    m.focus = Id::Animal(3);
    m.input(A);
    m.focus = Id::SurvivalKind(1);
    m.input(A);
    assert_eq!(m.choice.survival, 1);
    shot(&m, "10-survival");
    m.input(R);
    m.focus = Id::Observer;
    m.input(A);
    assert_eq!(m.choice.observer, 0);
    m.input(PLUS);
    assert_eq!(m.action, Action::Hunt);

    // The trophy room starts at once; the record shows the save.
    m.open_title();
    m.focus = Id::Trophies;
    m.input(A);
    assert_eq!(
        (m.action, m.choice.mode),
        (Action::Hunt, hunt_setup::MODE_TROPHY)
    );
    m.open_title();
    m.room.score = 345;
    m.record.kills.push(c2_core::record::Kill {
        animal: "Allosaurus".into(),
        score: 120,
        range: 42.0,
        weapon: "Rifle".into(),
        ..Default::default()
    });
    m.focus = Id::Record;
    m.input(A);
    assert_eq!(m.page, Page::Record);
    shot(&m, "11-record");
    m.input(B);
    assert_eq!((m.page, m.focus), (Page::Title, Id::Record));
    let saved = m.profile_bytes(menu::PROFILE_SAVE);
    let record = m.profile_bytes(menu::PROFILE_RECORD);
    let mut other = Menu::new(font());
    other.load_profile(menu::PROFILE_SAVE, &saved);
    other.load_profile(menu::PROFILE_RECORD, &record);
    assert_eq!((other.room.score, other.record.kills.len()), (345, 1));
}

#[test]
fn touches_pick_hexagons_and_settings_survive_a_save() {
    let Some(mut m) = menu_with_files() else {
        return;
    };
    m.input(A);
    let area = m
        .items()
        .into_iter()
        .find(|(id, _)| *id == Id::Area(4))
        .unwrap()
        .1;
    // A corner of the hexagon's box is outside the hexagon.
    assert!(!m.touch(area.x + 2.0, area.y + 2.0) || m.choice.area != 4);
    let (cx, cy) = area.center();
    assert!(m.touch(cx, cy));
    assert_eq!(m.choice.area, 4);
    m.choice = hunt_setup::HuntChoice {
        time: 2,
        animals: 0x1ff,
        weapons: 0b110,
        camo: 1,
        run_pct: 70,
        units_us: 1,
        ambient: 0,
        map_mobile: 0,
        teleport: 1,
        hints: 1,
        ..m.choice
    };
    let text = m.settings_text();
    let mut n = menu_with_files().unwrap();
    n.load_settings(&text);
    assert_eq!(n.choice, m.choice);
    n.load_settings("run_pct=7\ntime=9\nanimals=4294967295\ncamo=5\njunk\nblood=x\n");
    assert_eq!(
        (
            n.choice.run_pct,
            n.choice.time,
            n.choice.animals,
            n.choice.camo
        ),
        (50, 2, 0x1ff, 1)
    );
}

#[test]
fn descriptions_are_reflowed() {
    let s = menu::reflow("Weapon : Pistol\r\n\r\nThis weapon has fast \r\nshooting rate.\r\n\r\nName   : Rex\r\nPoints : 25\r\n Extremely dangerous! \r\n");
    assert_eq!(s, "Weapon : Pistol\n\nThis weapon has fast shooting rate.\n\nName   : Rex\nPoints : 25\nExtremely dangerous!");
}

#[test]
#[ignore]
fn draw_time() {
    let Some(mut m) = menu_with_files() else {
        return;
    };
    m.input(A);
    m.input(R);
    let pitch = 1920;
    let mut px = vec![0u8; pitch * 1080 * 4];
    m.draw(&mut ui::Canvas::new(&mut px, pitch, 1920, 1080));
    let t = std::time::Instant::now();
    for _ in 0..20 {
        m.draw(&mut ui::Canvas::new(&mut px, pitch, 1920, 1080));
    }
    eprintln!(
        "TV draw: {:.1} ms",
        t.elapsed().as_secs_f64() * 1000.0 / 20.0
    );
}

#[test]
#[ignore]
fn part_times() {
    let font = font();
    let pitch = 1920;
    let mut px = vec![0u8; pitch * 1080 * 4];
    let mut c = ui::Canvas::new(&mut px, pitch, 1920, 1080);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        for i in 0..9 {
            c.hex(
                ui::rect(
                    40.0 + (i % 3) as f32 * 212.0,
                    98.0 + (i / 3) as f32 * 176.0,
                    196.0,
                    166.0,
                ),
                None,
                5.0,
                [255; 4],
            );
        }
    }
    eprintln!("9 hex outlines: {:.2} ms", t.elapsed().as_secs_f64() * 50.0);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        for i in 0..30 {
            c.text(
                &font,
                40.0,
                i as f32 * 22.0,
                21.0,
                [255; 4],
                "This weapon has fast shooting rate, but",
            );
        }
    }
    eprintln!(
        "30 lines of text: {:.2} ms",
        t.elapsed().as_secs_f64() * 50.0
    );
    let t = std::time::Instant::now();
    for _ in 0..20 {
        c.fill(ui::rect(0.0, 0.0, 1280.0, 720.0), [16, 22, 16, 255]);
    }
    eprintln!("full fill: {:.2} ms", t.elapsed().as_secs_f64() * 50.0);
}

// Redrawing only what changed gives the same pixels as drawing everything,
// at both sizes, through a walk over every page.
#[test]
fn partial_redraws_match_full_draws() {
    let Some(mut m) = menu_with_files() else {
        return;
    };
    let sizes = [(1920usize, 1080usize), (854, 480)];
    let mut kept: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&(w, h)| vec![0u8; w.next_multiple_of(32) * h * 4])
        .collect();
    let steps = [
        A, RIGHT, DOWN, A, RIGHT, RIGHT, UP, LEFT, A, R, RIGHT, A, DOWN, DOWN, A, LEFT, R, DOWN, A,
        DOWN, RIGHT, A, DOWN, DOWN, A, UP, UP, LEFT, L, B, B, DOWN, A, LEFT, RIGHT, DOWN, A, B,
    ];
    for (step, &b) in [0].iter().chain(steps.iter()).enumerate() {
        if b != 0 {
            m.input(b);
        }
        for (k, &(w, h)) in sizes.iter().enumerate() {
            let pitch = w.next_multiple_of(32);
            m.draw(&mut ui::Canvas::new(&mut kept[k], pitch, w, h));
            let mut full = vec![0u8; pitch * h * 4];
            let mut fresh = menu_with_files().unwrap();
            fresh.page = m.page;
            fresh.focus = m.focus;
            fresh.choice = m.choice;
            fresh.draw(&mut ui::Canvas::new(&mut full, pitch, w, h));
            let differ = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let i = (y * pitch + x) * 4;
                    kept[k][i..i + 4] != full[i..i + 4]
                })
                .count();
            assert_eq!(
                differ, 0,
                "step {step} ({:?} focus {:?}) at {w}x{h}: {differ} pixels differ",
                m.page, m.focus
            );
        }
        m.drawn();
    }
}

// The hunt overlay after a kill: the plaque (a trophy) or the notice (a
// tranquilized animal), drawn into its texture and composited over grey as
// the GPU blends it; a second draw changes nothing.
#[test]
fn hunt_overlay_shows_the_kill() {
    let Some(m) = menu_with_files() else {
        return;
    };
    // (tranq, shot, R map, mobile look, hints, quads).
    let cases = [
        (0, "12-overlay-trophy", false, false, false, 3),
        (1, "13-overlay-notice", false, false, false, 3),
        (0, "14-overlay-map", true, false, true, 1),
        (0, "15-overlay-map-mobile", true, true, true, 1),
        (0, "16-overlay-hints", false, false, true, 1),
    ];
    for (tranq, name, map, map_mobile, hints, want) in cases {
        let kill = name.ends_with("trophy") || name.ends_with("notice");
        let inst = m.install.as_ref().unwrap();
        let choice = hunt_setup::HuntChoice {
            animals: wiiu_common::slots(inst, 0b1000),
            weapons: 0b10000,
            tranq,
            radar: 1,
            ..Default::default()
        };
        let l = hunt_setup::load(Some(inst), &choice, &hunt_setup::new_save(), 1).unwrap();
        let (area, mut hunt) = (l.area, l.hunt);
        let i = hunt.herd.chars.iter().position(|c| c.ai >= 10).unwrap();
        let mut events = Vec::new();
        if kill {
            hunt.debug_bag(&area, i, &mut events);
        }
        if let Some(slot) = hunt.trophy_body {
            hunt.room.bodies[slot].weapon = 4;
            hunt.room.bodies[slot].date = (2026 << 20) + (10 << 10) + 30;
        }
        let mut notice = overlay::Notice::default();
        if kill {
            notice.kill(
                hunt.herd.species[hunt.herd.chars[i].ctype]
                    .info
                    .name
                    .clone(),
                41,
                63.5,
                tranq != 0,
            );
            notice.message("Trophy!".into(), 2000);
            hunt.exit_prompt = true;
        }
        hunt.player.alpha = 0.7;
        let view = overlay::View {
            menu: &m,
            hunt: &hunt,
            area: &area,
            map,
            map_mobile,
            hints,
            notice: &notice,
            ask: None,
            units_us: false,
            night: false,
            cross: None,
            masked: false,
        };
        let (w, h) = (overlay::TEX_W, overlay::TEX_H);
        let mut tex = vec![0u8; w * h * 4];
        let mut o = overlay::Overlay::default();
        let n = overlay::MAX_QUADS * overlay::QUAD_FLOATS;
        let (mut pos, mut uv) = (vec![0f32; n], vec![0f32; n]);
        let (quads, changed) = o.draw(
            &m,
            &view,
            &mut overlay::canvas(&mut tex, w),
            &mut pos,
            &mut uv,
        );
        assert!(changed);
        assert_eq!(
            quads, want,
            "{name}: plaque or notice, message, prompt; or the map; or the hints"
        );
        let (again, changed) = o.draw(
            &m,
            &view,
            &mut overlay::canvas(&mut tex, w),
            &mut pos,
            &mut uv,
        );
        assert!(again == quads && !changed);
        // Composite the quads over grey at the TV's size.
        let (sw, sh) = (1920usize, 1080usize);
        let mut screen: Vec<u8> = [90u8, 110, 80, 255].repeat(sw * sh);
        for q in 0..quads {
            let (p, t) = (&pos[q * 24..], &uv[q * 24..]);
            let x0 = ((p[0] + 1.0) / 2.0 * sw as f32) as usize;
            let x1 = ((p[4] + 1.0) / 2.0 * sw as f32) as usize;
            let y0 = ((1.0 - p[1]) / 2.0 * sh as f32) as usize;
            let y1 = ((1.0 - p[9]) / 2.0 * sh as f32) as usize;
            for y in y0..y1.min(sh) {
                for x in x0..x1.min(sw) {
                    let u = t[0] + (t[4] - t[0]) * (x - x0) as f32 / (x1 - x0) as f32;
                    let v = t[1] + (t[9] - t[1]) * (y - y0) as f32 / (y1 - y0) as f32;
                    let (tx, ty) = (
                        ((u * w as f32) as usize).min(w - 1),
                        ((v * h as f32) as usize).min(h - 1),
                    );
                    let src = &tex[(ty * w + tx) * 4..(ty * w + tx) * 4 + 4];
                    let a = src[3] as u32;
                    for k in 0..3 {
                        let d = &mut screen[(y * sw + x) * 4 + k];
                        *d = ((src[k] as u32 * a + *d as u32 * (255 - a)) / 255) as u8;
                    }
                }
            }
        }
        let Ok(dir) = std::env::var("C2_MENU_SHOTS") else {
            continue;
        };
        let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}-tv.png")))
            .unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw as u32, sh as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.write_header()
            .unwrap()
            .write_image_data(&screen)
            .unwrap();
    }
}

#[test]
fn experimental_clock_is_saved_and_fits_settings_page() {
    let mut m = Menu::new(font());
    assert_eq!(m.choice.dynamic_time, 0);
    m.page = Page::Settings;
    let rect = m
        .items()
        .into_iter()
        .find(|(id, _)| *id == Id::DynamicTime)
        .unwrap()
        .1;
    assert!(rect.y + rect.h <= 586.0, "must not overlap help or Back");
    let (x, y) = rect.center();
    m.touch(x, y);
    assert_eq!(m.choice.dynamic_time, 1);
    let mut restored = Menu::new(font());
    restored.load_settings(&m.settings_text());
    assert_eq!(restored.choice.dynamic_time, 1);
}

#[test]
fn night_equipment_is_only_offered_for_dynamic_time() {
    let mut m = Menu::new(font());
    m.page = Page::Gear;
    assert!(!m
        .items()
        .iter()
        .any(|(id, _)| matches!(id, Id::Equip(4 | 5))));
    m.choice.dynamic_time = 1;
    for id in [Id::Equip(4), Id::Equip(5)] {
        let r = m.items().into_iter().find(|(i, _)| *i == id).unwrap().1;
        assert!(r.y + r.h < 632.0);
        let (x, y) = r.center();
        m.touch(x, y);
    }
    assert_eq!((m.choice.night_vision, m.choice.flashlight), (1, 1));
    let mut restored = Menu::new(font());
    restored.load_settings(&m.settings_text());
    assert_eq!(
        (restored.choice.night_vision, restored.choice.flashlight),
        (1, 1)
    );
    assert_eq!(std::mem::size_of::<hunt_setup::HuntChoice>(), 104);
    assert_eq!(
        std::mem::offset_of!(hunt_setup::HuntChoice, night_vision),
        92
    );
    assert_eq!(std::mem::offset_of!(hunt_setup::HuntChoice, flashlight), 96);
}
