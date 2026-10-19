//! c2rs for the Nintendo 3DS: loads a pack made by `c2-cook` from
//! `sdmc:/3ds/c2rs/` and starts a hunt.
//!
//! Top screen: the world. Bottom screen: status and controls (text for now).

#![feature(alloc_error_hook)]

mod gpu;
mod hunt;
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

use crate::gpu::Gpu;
use crate::hunt::{Game, Pad};
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
static mut __ctru_linear_heap_size: u32 = 20 << 20;

/// Out of memory: say so and wait, instead of the default abort (which the
/// system shows as a crash and restarts the console).
fn out_of_memory(layout: std::alloc::Layout) {
    // Nothing here may allocate.
    println!(
        "\n\nOut of memory ({} bytes wanted).\nThis pack is too big for this console:\n\
         cook it with fewer animals or\n--target o3ds.\n\nPress START to quit.",
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
        run: down.contains(KeyPad::L),
        jump: held.contains(KeyPad::DPAD_UP),
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

fn run(pack: &Path, apt: &Apt, hid: &mut Hid, console: &Console) -> Result<()> {
    console.select();
    let mut vfs = Vfs::new();
    vfs.push(
        "pack",
        Box::new(PackBackend::open(pack).with_context(|| format!("opening {}", pack.display()))?),
    );
    let t0 = ticks();
    let mut game = Game::load(&vfs, ticks() as u32, &mut |what| {
        println!("  {}", mem_line());
        println!("  loading {what}...")
    })?;
    println!("  {}", mem_line());
    println!("  GPU data...");
    let mut gpu = Gpu::new().context("citro3d init failed")?;
    let mut world = World::new(
        &vfs,
        &game.area,
        game.info.draw.clone(),
        &game.cars,
        &game.weapon_cars,
    );
    let mut ui = Ui::new(&vfs, &game);
    let (mut sound, sound_err) = match sound::Sound::new(&vfs, game.info.sound_rate) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };
    game.trim();
    drop(vfs);
    println!(
        "  ready in {:.1} s ({} animals)",
        (ticks() - t0) as f64 / TICKS_PER_SEC,
        game.hunt.herd.chars.len()
    );
    println!("  {}", mem_line());

    let mut script = Script::load(&format!("{DIR}/autotest.txt"));
    let mut touch = Touch::default();
    let mut last = ticks();
    let mut fps = 0.0f32;
    let mut frame = 0u32;
    let mut message: Option<(String, u32)> = sound_err.map(|e| (e, 600));
    let mut shot_n = 0;
    while apt.main_loop() {
        hid.scan_input();
        let (mut held, mut down) = (hid.keys_held(), hid.keys_down());
        if let Some(s) = script.as_mut() {
            let (h, d) = s.keys(frame);
            held |= h;
            down |= d;
        }
        if let Some([x, z, a, b]) = script.as_ref().and_then(|s| s.cam(frame)) {
            let p = &mut game.hunt.player;
            p.x = x;
            p.z = z;
            p.y = game.area.land_height(x, z);
            p.alpha = a;
            p.beta = b;
            game.hunt.paused = true;
            game.prev_cam = game.hunt.camera().0;
        }
        let (mut pad, mut tap) = read_pad(hid, held, down, &mut touch);
        if let Some(t) = script.as_ref().and_then(|s| s.tap(frame)) {
            tap = Some(t);
        }
        match tap.and_then(|(x, y)| ui.tap(x, y)) {
            Some(Tap::Weapon(i)) => pad.select_weapon = Some(i),
            Some(Tap::Call(ai)) if ai == game.hunt.target_call => pad.call = true,
            Some(Tap::Call(ai)) => pad.select_call = Some(ai),
            Some(Tap::MapZoom) => ui.map_local = !ui.map_local,
            Some(Tap::Binoculars) => pad.binoculars = true,
            Some(Tap::Pause) => pad.pause = true,
            None => {}
        }
        let now = ticks();
        let dt = ((now - last) as f64 / TICKS_PER_SEC) as f32;
        last = now;
        if dt > 0.0 {
            fps = fps * 0.9 + (1.0 / dt) * 0.1;
        }
        game.update(&pad, dt.min(0.1));
        if let Some(s) = sound.as_mut() {
            s.update(&game.events, &game.hunt, dt.min(0.1));
        }
        for e in game.events.drain(..) {
            if let GameEvent::Message(m) = e {
                message = Some((m, frame + 180));
            }
        }
        match game.hunt.outcome() {
            Some(Outcome::Exited) => break,
            Some(Outcome::Died) => {
                if let Some(s) = sound.as_mut() {
                    s.stop_all();
                }
                game.restart();
            }
            None => {}
        }

        // Interpolated camera.
        let (pos, alpha, beta) = game.hunt.camera();
        let t = game.alpha().clamp(0.0, 1.0);
        let pos = Vec3::from(game.prev_cam).lerp(Vec3::from(pos), t);
        let (view, fwd) = camera(pos, alpha, beta);
        let fovy = 2.0 * (0.6 / game.hunt.zoom().max(0.1)).atan();
        gpu.begin(world.sky);
        let poser = Poser {
            cars: &game.cars,
            weapon_cars: &game.weapon_cars,
        };
        world.draw(
            &mut gpu, &game.area, &game.hunt, &poser, &view, pos, fwd, fovy,
        );
        let msg = message
            .as_ref()
            .filter(|m| m.1 > frame)
            .map(|m| m.0.clone());
        let debug = format!(
            "{} {fps:.0}fps cmd {:.0}% {}\n{}",
            game.info.target,
            gpu.cmdbuf_usage() * 100.0,
            mem_line(),
            sound
                .as_ref()
                .map_or("no sound".into(), sound::Sound::status)
        );
        ui.new_frame();
        ui.draw_top(&mut gpu, &game, &game.hunt, &debug);
        ui.draw_bottom(&mut gpu, &game, msg.as_deref());
        gpu.end();

        let shoot = (held.contains(KeyPad::L) && down.contains(KeyPad::SELECT))
            .then(|| {
                shot_n += 1;
                format!("shot{shot_n:03}")
            })
            .or_else(|| script.as_mut().and_then(|s| s.shot(frame)));
        if let Some(name) = shoot {
            let p = format!("{DIR}/shots/{name}.tga");
            match shot::save_top(&p) {
                Ok(()) => message = Some((format!("saved {p}"), frame + 120)),
                Err(e) => message = Some((format!("screenshot failed: {e}"), frame + 120)),
            }
        }
        if script.as_ref().is_some_and(|s| s.done(frame)) {
            break;
        }
        frame += 1;
    }
    Ok(())
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
    if let Err(e) = run(pack, &apt, &mut hid, &console) {
        println!("\nerror: {e:#}");
        wait_for_start(&apt, &mut hid, &gfx);
    }
    unsafe { ctru_sys::irrstExit() };
}
