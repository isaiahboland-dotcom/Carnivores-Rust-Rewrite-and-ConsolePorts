//! c2rs for the Nintendo 3DS: opens a pack made by `c2-cook` from
//! `sdmc:/3ds/c2rs/`, then menu -> hunt -> menu.
//!
//! Hunting: the world on the top screen, weapons | map | calls on the
//! bottom. Menu: the picks on the top screen, touch steps on the bottom.

#![feature(alloc_error_hook)]

mod draw2d;
mod gpu;
mod hunt;
mod menu;
mod rules;
mod save;
mod settings;
mod shot;
mod sound;
mod ui;
mod world;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use c2_core::game::{GameEvent, Outcome};
use c2_vfs::{PackBackend, Vfs};
use ctru::console::Console;
use ctru::prelude::*;
use glam::{Mat4, Vec3};

use crate::draw2d::{Canvas, WHITE};
use crate::gpu::Gpu;
use crate::hunt::{Game, PackInfo, Pad, Setup, PACK_VERSION};
use crate::menu::Menu;
use crate::shot::Script;
use crate::ui::{Tap, Ui};
use crate::world::{Poser, World};

/// The app's folder on the SD card: where it was started from
/// (`sdmc:/3ds/<name>/<name>.3dsx`), or for an installed `.cia` the name
/// the launcher put in it, so installs under different names keep their
/// packs, saves and settings apart; `sdmc:/3ds/c2rs` otherwise.
pub fn dir() -> &'static str {
    static D: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    D.get_or_init(|| {
        std::env::args()
            .next()
            .and_then(|a| {
                a.replace('\\', "/")
                    .rsplit_once('/')
                    .map(|(d, _)| d.to_string())
            })
            .filter(|d| d.len() > "sdmc:/3ds/".len() && d.starts_with("sdmc:/3ds/"))
            .or_else(cia_dir)
            .unwrap_or_else(|| "sdmc:/3ds/c2rs".into())
    })
}

/// How [`dir`] decided, for the "no pack" screen.
static DIR_NOTE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// An installed `.cia` has no path to go by: the launcher writes the
/// install name to `name.txt` in its RomFS (a `.3dsx` has none).
fn cia_dir() -> Option<String> {
    let dev = c"romfs";
    let rc = unsafe { ctru_sys::romfsMountSelf(dev.as_ptr()) };
    if rc < 0 {
        let _ = DIR_NOTE.set(format!("no RomFS (0x{rc:08X})"));
        return None;
    }
    let name = std::fs::read_to_string("romfs:/name.txt");
    let _ = unsafe { ctru_sys::romfsUnmount(dev.as_ptr()) };
    let name = match name {
        Ok(n) => n.trim().to_string(),
        Err(e) => {
            let _ = DIR_NOTE.set(format!("RomFS name.txt: {e}"));
            return None;
        }
    };
    let _ = DIR_NOTE.set(format!("RomFS name {name:?}"));
    let safe = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    (!name.is_empty() && name.chars().all(safe)).then(|| format!("sdmc:/3ds/{name}"))
}

fn ticks() -> u64 {
    unsafe { ctru_sys::svcGetSystemTick() }
}

/// ARM11 ticks per second (`SYSCLOCK_ARM11`).
const TICKS_PER_SEC: f64 = 268_111_856.0;

/// Milliseconds for `Hunt::prof`.
fn clock_ms() -> f64 {
    ticks() as f64 / TICKS_PER_SEC * 1000.0
}

/// The last frame's tick count and each part's mean / worst ms per tick
/// (`c2_core::prof::SECTIONS`, by initial).
fn tick_line(game: &hunt::Game) -> String {
    let p = &game.hunt.prof;
    let parts: Vec<String> = c2_core::prof::SECTIONS
        .iter()
        .zip(p.mean.iter().zip(&p.worst))
        .map(|(n, (m, w))| format!("{}{m:.1}/{w:.0}", &n[..1]))
        .collect();
    format!(
        "{}t {} fp {:x}/{:x}",
        game.ticks_last,
        parts.join(" "),
        FPSCR_BOOT.load(std::sync::atomic::Ordering::Relaxed) >> 22,
        fpscr() >> 22
    )
}

/// The packs in [`dir`], or why the folder can't be read.
fn find_packs() -> Result<Vec<PathBuf>, std::io::Error> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("c2pk"))
        })
        .collect();
    v.sort();
    Ok(v)
}

/// With several packs (say `c2rs.c2pk` and `modded.c2pk`), asks which one
/// to play: D-Pad and A on the bottom screen's text console.
fn choose_pack(packs: &[PathBuf], apt: &Apt, hid: &mut Hid, gfx: &Gfx) -> Option<PathBuf> {
    if packs.len() < 2 {
        return packs.first().cloned();
    }
    let name = |p: &PathBuf| {
        p.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut at = 0usize;
    let mut drawn = None;
    while apt.main_loop() {
        if drawn != Some(at) {
            // Clear and home (the console understands ANSI codes).
            print!("\x1b[2J\x1b[1;1H");
            println!("c2rs for 3DS\n\nWhich game?\n");
            for (i, p) in packs.iter().enumerate() {
                let mark = if i == at { '>' } else { ' ' };
                println!(" {mark} {}", name(p));
            }
            println!("\nD-Pad to choose, A to play, START to quit.");
            drawn = Some(at);
        }
        hid.scan_input();
        let down = hid.keys_down();
        if down.contains(KeyPad::DPAD_UP) {
            at = (at + packs.len() - 1) % packs.len();
        } else if down.contains(KeyPad::DPAD_DOWN) {
            at = (at + 1) % packs.len();
        } else if down.contains(KeyPad::A) {
            return packs.get(at).cloned();
        } else if down.contains(KeyPad::START) {
            return None;
        }
        gfx.wait_for_vblank();
    }
    None
}

fn wait_for_start(apt: &Apt, hid: &mut Hid, gfx: &Gfx) {
    println!("\nPress START to quit.");
    while apt.main_loop() {
        hid.scan_input();
        if hid.keys_down().contains(KeyPad::START) {
            break;
        }
        gfx.wait_for_vblank();
    }
}

/// libctru: linear (GPU- and DSP-visible) memory to reserve at start; the
/// rest of the app's memory becomes the heap. libctru's default is 32 MiB,
/// which leaves an Old 3DS only ~30 MiB of heap; textures, vertex buffers
/// and sounds need under 16 (`c2-cook` checks each pack).
#[no_mangle]
static mut __ctru_linear_heap_size: u32 = 24 << 20;

/// Out of memory: say so and wait, instead of the default abort (which the
/// system shows as a crash and restarts the console).
fn out_of_memory(layout: std::alloc::Layout) {
    // Nothing here may allocate. The bottom screen may be the GPU's: put
    // the text console back on it.
    unsafe {
        ctru_sys::gfxSetDoubleBuffering(ctru_sys::GFX_BOTTOM, false);
        ctru_sys::consoleInit(ctru_sys::GFX_BOTTOM, std::ptr::null_mut());
    }
    println!(
        "\n\nOut of memory ({} bytes wanted).\nPick fewer animals or weapons,\n\
         or cook the pack with --target o3ds.\n\nPress START to quit.",
        layout.size()
    );
    unsafe {
        loop {
            ctru_sys::hidScanInput();
            if ctru_sys::hidKeysDown() & ctru_sys::KEY_START != 0 {
                break;
            }
            ctru_sys::gspWaitForEvent(ctru_sys::GSPGPU_EVENT_VBlank0, true);
        }
    }
    std::process::exit(1);
}

extern "C" {
    /// libctru: the app heap reserved at start (all the memory left after
    /// the linear heap).
    static __ctru_heap_size: u32;
    /// newlib's allocator statistics.
    fn mallinfo() -> MallInfo;
}

#[repr(C)]
struct MallInfo {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    /// Bytes in use.
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

/// Heap in use / reserved, and free linear (GPU-visible) memory.
/// FPSCR as the system started the app (shown on the pause screen).
static FPSCR_BOOT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The VFP status and control register.
fn fpscr() -> u32 {
    let v: u32;
    unsafe { std::arch::asm!("vmrs {}, fpscr", out(reg) v) };
    v
}

/// Flush-to-zero and default-NaN ("RunFast"): the ARM11's VFP otherwise
/// hands every operation on a denormal to slow support code, where a PC
/// pays almost nothing; a decaying value that reaches the denormal range
/// could make a tick many times slower on the 3DS only. Rounding is left
/// as it is. Per thread; the game runs on the main one.
fn run_fast() {
    let v = fpscr() | 1 << 24 | 1 << 25;
    unsafe { std::arch::asm!("vmsr fpscr, {}", in(reg) v) };
}

fn mem_line() -> String {
    unsafe {
        let used = mallinfo().uordblks as f32 / (1024.0 * 1024.0);
        let heap = __ctru_heap_size as f32 / (1024.0 * 1024.0);
        let lin = ctru_sys::linearSpaceFree() as f32 / (1024.0 * 1024.0);
        format!("heap {used:.1}/{heap:.1}M linear free {lin:.1}M")
    }
}

/// A touch in progress: where it started, where it was last frame, and
/// whether it has moved far enough to be a drag (look) rather than a tap.
#[derive(Default)]
struct Touch {
    start: Option<(f32, f32)>,
    last: (f32, f32),
    dragged: bool,
}

/// Keys to the hunt's controls, and a finished tap on the bottom screen.
fn read_pad(hid: &Hid, held: KeyPad, down: KeyPad, touch: &mut Touch) -> (Pad, Option<(f32, f32)>) {
    let (cx, cy) = hid.circlepad_position();
    let mut cs = ctru_sys::circlePosition { dx: 0, dy: 0 };
    unsafe { ctru_sys::irrstCstickRead(&mut cs) };
    let mut look = (f32::from(cs.dx) / 12.0, -f32::from(cs.dy) / 12.0);
    let mut tap = None;
    if held.contains(KeyPad::TOUCH) {
        let (x, y) = hid.touch_position();
        let p = (f32::from(x), f32::from(y));
        match touch.start {
            None => {
                *touch = Touch {
                    start: Some(p),
                    last: p,
                    dragged: false,
                }
            }
            Some(s) => {
                if !touch.dragged && (p.0 - s.0).hypot(p.1 - s.1) > 6.0 {
                    touch.dragged = true;
                }
                if touch.dragged {
                    look.0 += (p.0 - touch.last.0) * 2.0;
                    look.1 += (p.1 - touch.last.1) * 2.0;
                }
                touch.last = p;
            }
        }
    } else {
        if !touch.dragged {
            tap = touch.start;
        }
        *touch = Touch::default();
    }
    let stick = |v: i16| (f32::from(v) / 150.0).clamp(-1.0, 1.0);
    let pad = Pad {
        stick: (stick(cx), stick(cy)),
        look,
        // Face buttons look (a second stick for the Old 3DS); the D-Pad
        // does what they did. A / B still answer the leave prompt.
        turn_left: held.contains(KeyPad::Y),
        turn_right: held.contains(KeyPad::A),
        look_up: held.contains(KeyPad::X),
        look_down: held.contains(KeyPad::B),
        fire: held.contains(KeyPad::R),
        run: down.contains(KeyPad::DPAD_UP),
        jump: held.contains(KeyPad::L),
        crouch: held.contains(KeyPad::ZL),
        crouch_down: down.contains(KeyPad::ZL),
        toggle_weapon: down.contains(KeyPad::DPAD_RIGHT),
        next_weapon: down.contains(KeyPad::DPAD_DOWN),
        binoculars: down.contains(KeyPad::DPAD_LEFT),
        call: down.contains(KeyPad::ZR),
        pause: down.contains(KeyPad::SELECT) && !held.contains(KeyPad::L),
        escape: down.contains(KeyPad::START),
        confirm: down.contains(KeyPad::A),
        cancel: down.contains(KeyPad::B),
        restart: down.contains(KeyPad::Y) || down.contains(KeyPad::X),
        select_weapon: None,
        select_call: None,
        resupply: false,
    };
    (pad, tap)
}

/// The camera: the original's yaw/pitch, as the PC view does it.
fn camera(pos: Vec3, alpha: f32, beta: f32) -> (Mat4, Vec3) {
    let (yaw, pitch) = (-alpha, -beta);
    let fwd = Vec3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );
    (glam::camera::rh::view::look_to_mat4(pos, fwd, Vec3::Y), fwd)
}

/// Input and test-script state shared by the menu and the hunts.
struct Io<'a> {
    apt: &'a Apt,
    hid: &'a mut Hid,
    script: Option<Script>,
    /// Frames since start (the test script counts these).
    frame: u32,
    touch: Touch,
    shot_n: u32,
}

impl Io<'_> {
    /// Next frame's keys (held, pressed), with the test script's.
    fn keys(&mut self) -> (KeyPad, KeyPad) {
        self.hid.scan_input();
        let (mut held, mut down) = (self.hid.keys_held(), self.hid.keys_down());
        if let Some(s) = self.script.as_ref() {
            let (h, d) = s.keys(self.frame);
            held |= h;
            down |= d;
        }
        (held, down)
    }

    fn script_tap(&self) -> Option<(f32, f32)> {
        self.script.as_ref().and_then(|s| s.tap(self.frame))
    }

    fn quit(&self) -> bool {
        self.script.as_ref().is_some_and(|s| s.done(self.frame))
    }

    /// L+SELECT, or the script: save the top screen.
    fn screenshot(&mut self, held: KeyPad, down: KeyPad) -> Option<String> {
        let name = (held.contains(KeyPad::L) && down.contains(KeyPad::SELECT))
            .then(|| {
                self.shot_n += 1;
                format!("shot{:03}", self.shot_n)
            })
            .or_else(|| self.script.as_ref().and_then(|s| s.shot(self.frame)))?;
        let p = format!("{}/shots/{name}.tga", dir());
        Some(match shot::save_top(&p) {
            Ok(()) => format!("saved {p}"),
            Err(e) => format!("screenshot failed: {e}"),
        })
    }
}

/// The setup menu until a hunt is picked (`None`: quit).
fn menu(
    vfs: &Vfs,
    info: &PackInfo,
    gpu: &mut Gpu,
    io: &mut Io,
    note: Option<String>,
) -> Result<Option<Setup>> {
    let mut m = Menu::new(vfs, info.clone())?;
    if let Some(n) = note {
        m.set_message(n);
    }
    while io.apt.main_loop() {
        let (held, down) = io.keys();
        let (_, mut tap) = read_pad(io.hid, held, down, &mut io.touch);
        if let Some(t) = io.script_tap() {
            tap = Some(t);
        }
        let mut pick = tap.and_then(|(x, y)| m.tap(x, y));
        if down.contains(KeyPad::A) {
            pick = pick.or_else(|| m.next());
        }
        if down.contains(KeyPad::B) {
            m.back();
        }
        if down.contains(KeyPad::START) {
            m.start_button();
        }
        if m.quit || io.quit() {
            return Ok(None);
        }
        gpu.begin([0, 0, 0]);
        m.draw(gpu);
        gpu.end();
        if let Some(msg) = io.screenshot(held, down) {
            m.set_message(msg);
        }
        io.frame += 1;
        if pick.is_some() {
            return Ok(pick);
        }
    }
    Ok(None)
}

/// "Loading..." on both screens while a hunt loads.
fn loading_screen(vfs: &Vfs, gpu: &mut Gpu, what: &str) {
    let mut c = Canvas::new(ui::load(vfs, "C2RS/FONT.TX"));
    // Two frames, so both framebuffers show it.
    for _ in 0..2 {
        c.new_frame();
        gpu.begin([0, 0, 0]);
        gpu.begin_2d(None);
        c.text_c(gpu, 200.0, 112.0, 2.0, WHITE, "Loading...");
        c.text_c(gpu, 200.0, 136.0, 1.0, WHITE, what);
        gpu.begin_2d(Some([0, 0, 0]));
        gpu.end();
    }
}

/// One hunt, until the hunter leaves (saved) or dies (not saved). Returns a
/// line for the menu.
/// One hunt; `None` when the player asked for a restart.
fn hunt(
    vfs: &Vfs,
    info: &PackInfo,
    setup: Setup,
    gpu: &mut Gpu,
    io: &mut Io,
) -> Result<Option<String>> {
    loading_screen(vfs, gpu, &setup.area);
    let room0 = save::load(0);
    let t0 = ticks();
    let mut game = Game::load(
        vfs,
        info.clone(),
        setup,
        room0.clone(),
        ticks() as u32,
        &mut |_| {},
    )?;
    game.hunt.prof.clock = Some(clock_ms);
    let mut world = World::new(
        vfs,
        &game.area,
        game.draw.clone(),
        &game.cars,
        &game.keys,
        &game.weapon_cars,
    );
    let mut ui = Ui::new(vfs, &game);
    let (mut sound, sound_err) = match sound::Sound::new(vfs, &game) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };
    game.trim();
    let ready = format!(
        "ready in {:.1} s, {} animals",
        (ticks() - t0) as f64 / TICKS_PER_SEC,
        game.hunt.herd.chars.len()
    );
    let mut message: Option<(String, u32)> = Some((sound_err.unwrap_or(ready), io.frame + 300));
    let mut last = ticks();
    let mut fps = 0.0f32;
    // The last kill: species, points, shown until this frame.
    let mut kill: Option<(String, i32, u32)> = None;
    // Smoothed ms spent simulating, drawing the world, drawing the screens'
    // 2D, and waiting for the GPU / vsync.
    let mut ms = [0.0f32; 4];
    let ms_of = |t0: u64| ((ticks() - t0) as f64 / TICKS_PER_SEC * 1000.0) as f32;
    while io.apt.main_loop() {
        let frame = io.frame;
        let (held, down) = io.keys();
        if let Some([x, z, a, b]) = io.script.as_ref().and_then(|s| s.cam(frame)) {
            let p = &mut game.hunt.player;
            p.x = x;
            p.z = z;
            p.y = game.area.land_height(x, z);
            p.alpha = a;
            p.beta = b;
            game.hunt.paused = true;
            game.prev_cam = game.hunt.camera().0;
        }
        let (mut pad, mut tap) = read_pad(io.hid, held, down, &mut io.touch);
        if let Some(t) = io.script_tap() {
            tap = Some(t);
        }
        match tap.and_then(|(x, y)| ui.tap(x, y)) {
            Some(Tap::Weapon(i)) => pad.select_weapon = Some(i),
            Some(Tap::Call(ai)) if ai == game.hunt.target_call => pad.call = true,
            Some(Tap::Call(ai)) => pad.select_call = Some(ai),
            Some(Tap::MapZoom) => ui.map_local = !ui.map_local,
            Some(Tap::Binoculars) => pad.binoculars = true,
            Some(Tap::Pause) => pad.pause = true,
            Some(Tap::Resupply) => pad.resupply = true,
            None => {}
        }
        let now = ticks();
        let dt = ((now - last) as f64 / TICKS_PER_SEC) as f32;
        last = now;
        if dt > 0.0 {
            fps = fps * 0.9 + (1.0 / dt) * 0.1;
        }
        let t_upd = ticks();
        game.update(&pad, dt.min(0.1));
        if let Some(s) = sound.as_mut() {
            s.update(&game.events, &game.hunt, dt.min(0.1));
        }
        let m_upd = ms_of(t_upd);
        if frame % 120 == 0 {
            game.hunt.prof.reset_worst();
        }
        for e in game.events.drain(..) {
            match e {
                GameEvent::Message(m) => message = Some((m, frame + 180)),
                GameEvent::Scored { ctype, score, .. } => {
                    let name = game
                        .hunt
                        .herd
                        .species
                        .get(ctype)
                        .map_or_else(String::new, |s| s.info.name.clone());
                    kill = Some((name, score, frame + 240));
                }
                _ => {}
            }
        }
        // A restart drops this hunt unsaved, like dying.
        if pad.restart && game.hunt.show_exit() && !game.hunt.opts.trophy {
            return Ok(None);
        }
        match game.hunt.outcome() {
            Some(Outcome::Exited) => {
                return Ok(Some(match save::store(&game.hunt.room) {
                    Ok(()) => format!("Saved. Score {}", game.hunt.room.score),
                    Err(e) => format!("save failed: {e}"),
                }));
            }
            // Dying discards this hunt's trophies (the save isn't written).
            Some(Outcome::Died) => return Ok(Some("You died. Nothing was saved.".into())),
            None => {}
        }

        // Interpolated camera.
        let (pos, alpha, beta) = game.hunt.camera();
        let t = game.alpha().clamp(0.0, 1.0);
        let pos = Vec3::from(game.prev_cam).lerp(Vec3::from(pos), t);
        let (view, fwd) = camera(pos, alpha, beta);
        let fovy = 2.0 * (0.6 / game.hunt.zoom().max(0.1)).atan();
        let t_wait = ticks();
        // Dead with the death camera off: black, with the prompt.
        let black = game.hunt.death.is_some() && !game.hunt.opts.death_anim;
        gpu.begin(if black { [0; 3] } else { world.sky });
        let m_wait = ms_of(t_wait);
        let t_draw = ticks();
        let poser = Poser {
            cars: &game.cars,
            weapon_cars: &game.weapon_cars,
        };
        if !black {
            world.draw(gpu, &game.area, &game.hunt, &poser, &view, pos, fwd, fovy);
        }
        let m_draw = ms_of(t_draw);
        let msg = message
            .as_ref()
            .filter(|m| m.1 > frame)
            .map(|m| m.0.clone());
        let debug = format!(
            "{} {fps:.0}fps cmd {:.0}% {}\n{}\nms: sim {:.1} world {:.1} 2d {:.1} wait {:.1}\n{}",
            game.info.target,
            gpu.cmdbuf_usage() * 100.0,
            mem_line(),
            sound
                .as_ref()
                .map_or("no sound".into(), sound::Sound::status),
            ms[0],
            ms[1],
            ms[2],
            ms[3],
            tick_line(&game),
        );
        let t_ui = ticks();
        ui.new_frame();
        let shown = kill
            .as_ref()
            .filter(|k| k.2 > frame)
            .map(|k| (k.0.as_str(), k.1));
        ui.draw_top(gpu, &game, &game.hunt, &debug, shown);
        ui.draw_bottom(gpu, &game, msg.as_deref());
        gpu.end();
        let m_ui = ms_of(t_ui);
        // Frozen while paused, so the pause screen shows them running.
        if !game.hunt.paused {
            for (a, b) in ms.iter_mut().zip([m_upd, m_draw, m_ui, m_wait]) {
                *a = *a * 0.95 + b * 0.05;
            }
        }

        if let Some(m) = io.screenshot(held, down) {
            message = Some((m, frame + 120));
        }
        if io.quit() {
            return Ok(Some(String::new()));
        }
        io.frame += 1;
    }
    Ok(Some(String::new()))
}

fn run(pack: &Path, apt: &Apt, hid: &mut Hid) -> Result<()> {
    let mut vfs = Vfs::new();
    vfs.push(
        "pack",
        Box::new(PackBackend::open(pack).with_context(|| format!("opening {}", pack.display()))?),
    );
    let info = PackInfo::parse(&String::from_utf8_lossy(
        &vfs.read("C2RS/INFO.TXT")
            .context("not a c2rs pack (no C2RS/INFO.TXT)")?,
    ));
    if info.pack != PACK_VERSION {
        anyhow::bail!(
            "this pack is version {}, this app reads version {PACK_VERSION}; cook it again",
            info.pack
        );
    }
    let mut gpu = Gpu::new().context("citro3d init failed")?;
    let mut io = Io {
        apt,
        hid,
        script: Script::load(&format!("{}/autotest.txt", dir())),
        frame: 0,
        touch: Touch::default(),
        shot_n: 0,
    };
    let mut note = None;
    loop {
        let Some(setup) = menu(&vfs, &info, &mut gpu, &mut io, note.take())? else {
            return Ok(());
        };
        loop {
            match hunt(&vfs, &info, setup.clone(), &mut gpu, &mut io) {
                Ok(Some(n)) => note = Some(n),
                Ok(None) if !io.quit() && io.apt.main_loop() => continue,
                Ok(None) => {}
                Err(e) => note = Some(format!("{e:#}")),
            }
            break;
        }
        if io.quit() || !io.apt.main_loop() {
            return Ok(());
        }
    }
}

fn main() {
    let apt = Apt::new().ok();
    let hid = Hid::new().ok();
    let gfx = Gfx::new().ok();
    let (Some(apt), Some(mut hid), Some(gfx)) = (apt, hid, gfx) else {
        return;
    };
    let console = Console::new(gfx.bottom_screen.borrow_mut());
    std::alloc::set_alloc_error_hook(out_of_memory);
    unsafe {
        // 804 MHz and the extra cache on New 3DS (no effect on Old 3DS).
        ctru_sys::osSetSpeedupEnable(true);
        // C-Stick and ZL/ZR on New 3DS.
        let _ = ctru_sys::irrstInit();
    }
    FPSCR_BOOT.store(fpscr(), std::sync::atomic::Ordering::Relaxed);
    run_fast();
    println!("c2rs for 3DS (test build)\n");
    let found = find_packs();
    let packs = found.as_ref().cloned().unwrap_or_default();
    if packs.is_empty() {
        println!("No pack found in {}/.", dir());
        // Why, for bug reports: the folder's error, how it was chosen.
        if let Err(e) = &found {
            println!("({e})");
        }
        let start = std::env::args().next().unwrap_or_else(|| "none".into());
        let how = DIR_NOTE.get().map_or("", String::as_str);
        println!("(start path: {start}; {how})");
        println!("\nOn your PC, from 3ds-ver/:");
        println!("  cargo run --release -p c2-cook --");
        println!("    --game <your Carnivores 2>");
        println!("    --out c2rs.c2pk");
        println!("then copy c2rs.c2pk to {}/", dir());
        wait_for_start(&apt, &mut hid, &gfx);
        return;
    }
    let Some(pack) = choose_pack(&packs, &apt, &mut hid, &gfx) else {
        return;
    };
    save::use_pack(&pack);
    println!("pack: {}", pack.display());
    if let Err(e) = run(&pack, &apt, &mut hid) {
        console.select();
        println!("\nerror: {e:#}");
        wait_for_start(&apt, &mut hid, &gfx);
    }
    unsafe { ctru_sys::irrstExit() };
}
