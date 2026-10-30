#![cfg(feature = "portable-hunt")]
//! The Wii U menu, drawn on the host. With C2_ORIGINAL_HUNTDAT set it reads
//! the original menu pictures and texts; with C2_MENU_SHOTS=DIR it also
//! writes every page as PNGs at the TV's and the GamePad's sizes, with
//! C2_MENU_FONT=FILE.ttf standing in for the console's system font.
extern crate alloc;
#[allow(dead_code)]
#[path = "../../../../wiiu-ver/perf-probe/src/hunt_setup.rs"]
mod hunt_setup;
#[allow(dead_code)]
#[path = "../../../../wiiu-ver/perf-probe/src/ui.rs"]
mod ui;
#[allow(dead_code)]
#[path = "../../../../wiiu-ver/perf-probe/src/menu.rs"]
mod menu;
use menu::{button::*, Action, Id, Menu, Page, FILES};

fn font() -> ui::Font {
    let bytes = std::env::var("C2_MENU_FONT").ok().and_then(|p| std::fs::read(p).ok());
    ui::Font::new(bytes.map(|b| &*Box::leak(b.into_boxed_slice())))
}

fn menu_with_files() -> Menu {
    let mut m = Menu::new(font());
    if let Ok(root) = std::env::var("C2_ORIGINAL_HUNTDAT") {
        let root = std::path::Path::new(&root).join("MENU");
        for (slot, name) in FILES.iter().enumerate() {
            let dir = if name.ends_with(".TGA") && *name != "MENUM.TGA" { "PICS" } else if name.ends_with(".TGA") { "" } else { "TXT" };
            let bytes = std::fs::read(root.join(dir).join(name)).unwrap();
            assert!(m.set_file(slot, &bytes), "{name}");
        }
    }
    m
}

fn shot(m: &Menu, name: &str) {
    for (w, h, tag) in [(1920usize, 1080usize, "tv"), (854, 480, "gamepad")] {
        // The GamePad texture's rows are padded, as GX2 pads them.
        let pitch = w.next_multiple_of(32);
        let mut px = vec![0u8; pitch * h * 4];
        m.draw(&mut ui::Canvas::new(&mut px, pitch, w, h));
        let Ok(dir) = std::env::var("C2_MENU_SHOTS") else { continue };
        let mut rows = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            rows.extend_from_slice(&px[y * pitch * 4..(y * pitch + w) * 4]);
        }
        let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}-{tag}.png"))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.write_header().unwrap().write_image_data(&rows).unwrap();
    }
}

#[test]
fn every_page_draws_and_the_setup_reaches_a_hunt() {
    let mut m = menu_with_files();
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
    m.input(DOWN);
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
}

#[test]
fn touches_pick_hexagons_and_settings_survive_a_save() {
    let mut m = Menu::new(ui::Font::Bitmap);
    m.input(A);
    let area = m.items().into_iter().find(|(id, _)| *id == Id::Area(4)).unwrap().1;
    // A corner of the hexagon's box is outside the hexagon.
    assert!(!m.touch(area.x + 2.0, area.y + 2.0) || m.choice.area != 4);
    let (cx, cy) = area.center();
    assert!(m.touch(cx, cy));
    assert_eq!(m.choice.area, 4);
    m.choice = hunt_setup::HuntChoice { time: 2, animals: 0x1ff, weapons: 0b110, camo: 1, run_pct: 70, units_us: 1, ambient: 0, ..m.choice };
    let text = m.settings_text();
    let mut n = Menu::new(ui::Font::Bitmap);
    n.load_settings(&text);
    assert_eq!(n.choice, m.choice);
    n.load_settings("run_pct=7\ntime=9\nanimals=4294967295\ncamo=5\njunk\nblood=x\n");
    assert_eq!((n.choice.run_pct, n.choice.time, n.choice.animals, n.choice.camo), (50, 2, 0x1ff, 1));
}

#[test]
fn descriptions_are_reflowed() {
    let s = menu::reflow("Weapon : Pistol\r\n\r\nThis weapon has fast \r\nshooting rate.\r\n\r\nName   : Rex\r\nPoints : 25\r\n Extremely dangerous! \r\n");
    assert_eq!(s, "Weapon : Pistol\n\nThis weapon has fast shooting rate.\n\nName   : Rex\nPoints : 25\nExtremely dangerous!");
}

#[test]
#[ignore]
fn draw_time() {
    let mut m = menu_with_files();
    m.input(A);
    m.input(R);
    let pitch = 1920;
    let mut px = vec![0u8; pitch * 1080 * 4];
    m.draw(&mut ui::Canvas::new(&mut px, pitch, 1920, 1080));
    let t = std::time::Instant::now();
    for _ in 0..20 {
        m.draw(&mut ui::Canvas::new(&mut px, pitch, 1920, 1080));
    }
    eprintln!("TV draw: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
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
            c.hex(ui::rect(40.0 + (i % 3) as f32 * 212.0, 98.0 + (i / 3) as f32 * 176.0, 196.0, 166.0), None, 5.0, [255; 4]);
        }
    }
    eprintln!("9 hex outlines: {:.2} ms", t.elapsed().as_secs_f64() * 50.0);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        for i in 0..30 {
            c.text(&font, 40.0, i as f32 * 22.0, 21.0, [255; 4], "This weapon has fast shooting rate, but");
        }
    }
    eprintln!("30 lines of text: {:.2} ms", t.elapsed().as_secs_f64() * 50.0);
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
    let mut m = menu_with_files();
    let sizes = [(1920usize, 1080usize), (854, 480)];
    let mut kept: Vec<Vec<u8>> = sizes.iter().map(|&(w, h)| vec![0u8; w.next_multiple_of(32) * h * 4]).collect();
    let steps = [A, RIGHT, DOWN, A, RIGHT, RIGHT, UP, LEFT, A, R, RIGHT, A, DOWN, DOWN, A, LEFT, R, DOWN, A, DOWN, RIGHT, A, DOWN, DOWN, A, UP, UP, LEFT, L, B, B, DOWN, A, LEFT, RIGHT, DOWN, A, B];
    for (step, &b) in [0].iter().chain(steps.iter()).enumerate() {
        if b != 0 {
            m.input(b);
        }
        for (k, &(w, h)) in sizes.iter().enumerate() {
            let pitch = w.next_multiple_of(32);
            m.draw(&mut ui::Canvas::new(&mut kept[k], pitch, w, h));
            let mut full = vec![0u8; pitch * h * 4];
            let mut fresh = menu_with_files();
            fresh.page = m.page;
            fresh.focus = m.focus;
            fresh.choice = m.choice;
            fresh.draw(&mut ui::Canvas::new(&mut full, pitch, w, h));
            let differ = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| {
                let i = (y * pitch + x) * 4;
                kept[k][i..i + 4] != full[i..i + 4]
            }).count();
            assert_eq!(differ, 0, "step {step} ({:?} focus {:?}) at {w}x{h}: {differ} pixels differ", m.page, m.focus);
        }
        m.drawn();
    }
}
