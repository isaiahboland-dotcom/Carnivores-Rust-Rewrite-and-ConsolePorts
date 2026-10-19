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
mod save;
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

pub const DIR: &str = "sdmc:/3ds/c2rs";

fn ticks() -> u64 {
    unsafe { ctru_sys::svcGetSystemTick() }
}

/// ARM11 ticks per second (`SYSCLOCK_ARM11`).
const TICKS_PER_SEC: f64 = 268_111_856.0;

fn find_packs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(DIR)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("c2pk"))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
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
        toggle_weapon: down.contains(KeyPad::DPAD_RIGHT),
        next_weapon: down.contains(KeyPad::DPAD_DOWN),
        binoculars: down.contains(KeyPad::DPAD_LEFT),
        call: down.contains(KeyPad::ZR),
        pause: down.contains(KeyPad::SELECT) && !held.contains(KeyPad::L),
        escape: down.contains(KeyPad::START),
        confirm: down.contains(KeyPad::A),
        cancel: down.contains(KeyPad::B),
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
        let p = format!("{DIR}/shots/{name}.tga");
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
fn hunt(vfs: &Vfs, info: &PackInfo, setup: Setup, gpu: &mut Gpu, io: &mut Io) -> Result<String> {
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
    let mut world = World::new(
        vfs,
        &game.area,
        game.draw.clone(),
        &game.cars,
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
        for e in game.events.drain(..) {
            if let GameEvent::Message(m) = e {
                message = Some((m, frame + 180));
            }
        }
        match game.hunt.outcome() {
            Some(Outcome::Exited) => {
                return Ok(match save::store(&game.hunt.room) {
                    Ok(()) => format!("Saved. Score {}", game.hunt.room.score),
                    Err(e) => format!("save failed: {e}"),
                });
            }
            // Dying discards this hunt's trophies (the save isn't written).
            Some(Outcome::Died) => return Ok("You died. Nothing was saved.".into()),
            None => {}
        }

        // Interpolated camera.
        let (pos, alpha, beta) = game.hunt.camera();
        let t = game.alpha().clamp(0.0, 1.0);
        let pos = Vec3::from(game.prev_cam).lerp(Vec3::from(pos), t);
        let (view, fwd) = camera(pos, alpha, beta);
        let fovy = 2.0 * (0.6 / game.hunt.zoom().max(0.1)).atan();
        let t_wait = ticks();
        gpu.begin(world.sky);
        let m_wait = ms_of(t_wait);
        let t_draw = ticks();
        let poser = Poser {
            cars: &game.cars,
            weapon_cars: &game.weapon_cars,
        };
        world.draw(gpu, &game.area, &game.hunt, &poser, &view, pos, fwd, fovy);
        let m_draw = ms_of(t_draw);
        let msg = message
            .as_ref()
            .filter(|m| m.1 > frame)
            .map(|m| m.0.clone());
        let debug = format!(
            "{} {fps:.0}fps cmd {:.0}% {}\n{}\nms: sim {:.1} world {:.1} 2d {:.1} wait {:.1}",
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
        );
        let t_ui = ticks();
        ui.new_frame();
        ui.draw_top(gpu, &game, &game.hunt, &debug);
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
            return Ok(String::new());
        }
        io.frame += 1;
    }
    Ok(String::new())
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
        script: Script::load(&format!("{DIR}/autotest.txt")),
        frame: 0,
        touch: Touch::default(),
        shot_n: 0,
    };
    let mut note = None;
    loop {
        let Some(setup) = menu(&vfs, &info, &mut gpu, &mut io, note.take())? else {
            return Ok(());
        };
        note = Some(match hunt(&vfs, &info, setup, &mut gpu, &mut io) {
            Ok(n) => n,
            Err(e) => format!("{e:#}"),
        });
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
    println!("c2rs for 3DS (test build)\n");
    let packs = find_packs();
    let Some(pack) = packs.first() else {
        println!("No pack found in {DIR}/.");
        println!("\nOn your PC, from 3ds-ver/:");
        println!("  cargo run --release -p c2-cook --");
        println!("    --game <your Carnivores 2>");
        println!("    --out c2rs.c2pk");
        println!("then copy c2rs.c2pk to {DIR}/");
        wait_for_start(&apt, &mut hid, &gfx);
        return;
    };
    println!("pack: {}", pack.display());
    if let Err(e) = run(pack, &apt, &mut hid) {
        console.select();
        println!("\nerror: {e:#}");
        wait_for_start(&apt, &mut hid, &gfx);
    }
    unsafe { ctru_sys::irrstExit() };
}
