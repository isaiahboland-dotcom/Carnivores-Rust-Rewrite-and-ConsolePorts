//! c2rs for the Nintendo 3DS: loads a pack made by `c2-cook` from
//! `sdmc:/3ds/c2rs/` and starts a hunt.
//!
//! Top screen: the world. Bottom screen: status and controls (text for now).

#![feature(alloc_error_hook)]

mod gpu;
mod hunt;
mod shot;
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

/// libctru: linear (GPU-visible) memory to reserve at start; the rest of the
/// app's memory becomes the heap. libctru's default is 32 MiB, which leaves
/// an Old 3DS only ~30 MiB of heap; the GPU data needs well under 16
/// (`c2-cook` checks each pack).
#[no_mangle]
static mut __ctru_linear_heap_size: u32 = 16 << 20;

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
        format!("heap {used:.1}/{heap:.1} MiB, linear free {lin:.1}")
    }
}

/// Keys to the hunt's controls. `touch` is the previous touch point.
fn read_pad(hid: &Hid, held: KeyPad, down: KeyPad, touch: &mut Option<(u16, u16)>) -> Pad {
    let (cx, cy) = hid.circlepad_position();
    let mut cs = ctru_sys::circlePosition { dx: 0, dy: 0 };
    unsafe { ctru_sys::irrstCstickRead(&mut cs) };
    let mut look = (f32::from(cs.dx) / 12.0, -f32::from(cs.dy) / 12.0);
    if held.contains(KeyPad::TOUCH) {
        let p = hid.touch_position();
        if let Some(prev) = *touch {
            look.0 += (f32::from(p.0) - f32::from(prev.0)) * 2.0;
            look.1 += (f32::from(p.1) - f32::from(prev.1)) * 2.0;
        }
        *touch = Some(p);
    } else {
        *touch = None;
    }
    let stick = |v: i16| (f32::from(v) / 150.0).clamp(-1.0, 1.0);
    Pad {
        stick: (stick(cx), stick(cy)),
        look,
        turn_left: held.contains(KeyPad::DPAD_LEFT),
        turn_right: held.contains(KeyPad::DPAD_RIGHT),
        look_up: held.contains(KeyPad::DPAD_UP),
        look_down: held.contains(KeyPad::DPAD_DOWN),
        fire: held.contains(KeyPad::R),
        run: down.contains(KeyPad::L),
        jump: held.contains(KeyPad::B),
        crouch: held.contains(KeyPad::ZL),
        toggle_weapon: down.contains(KeyPad::A),
        next_weapon: down.contains(KeyPad::X),
        binoculars: down.contains(KeyPad::Y),
        call: down.contains(KeyPad::ZR),
        pause: down.contains(KeyPad::SELECT) && !held.contains(KeyPad::L),
        escape: down.contains(KeyPad::START),
        confirm: down.contains(KeyPad::A),
        cancel: down.contains(KeyPad::B),
    }
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
    game.trim();
    drop(vfs);
    println!(
        "  ready in {:.1} s ({} animals)",
        (ticks() - t0) as f64 / TICKS_PER_SEC,
        game.hunt.herd.chars.len()
    );
    println!("  {}", mem_line());

    let mut script = Script::load(&format!("{DIR}/autotest.txt"));
    let mut touch = None;
    let mut last = ticks();
    let mut fps = 0.0f32;
    let mut frame = 0u32;
    let mut message: Option<(String, u32)> = None;
    let mut shot_n = 0;
    console.clear();
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
        let pad = read_pad(hid, held, down, &mut touch);
        let now = ticks();
        let dt = ((now - last) as f64 / TICKS_PER_SEC) as f32;
        last = now;
        if dt > 0.0 {
            fps = fps * 0.9 + (1.0 / dt) * 0.1;
        }
        game.update(&pad, dt.min(0.1));
        for e in game.events.drain(..) {
            if let GameEvent::Message(m) = e {
                message = Some((m, frame + 180));
            }
        }
        match game.hunt.outcome() {
            Some(Outcome::Exited) => break,
            Some(Outcome::Died) => game.restart(),
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

        if frame % 15 == 0 {
            status(
                &game,
                fps,
                &message
                    .as_ref()
                    .filter(|m| m.1 > frame)
                    .map(|m| m.0.clone()),
                &gpu,
            );
        }
        frame += 1;
    }
    Ok(())
}

/// The bottom screen.
fn status(game: &Game, fps: f32, message: &Option<String>, gpu: &Gpu) {
    let h = &game.hunt;
    let w = &h.weapons;
    let alive = h
        .herd
        .chars
        .iter()
        .filter(|c| c.health > 0 && c.ai >= 10)
        .count();
    let weapon = match w.pose() {
        Some(_) => format!(
            "{} ({} left)",
            w.info.get(w.current).map_or("?", |i| i.name.as_str()),
            w.shots_left.get(w.current).copied().unwrap_or(0)
        ),
        None => "hidden (A to draw)".into(),
    };
    let state = if h.exit_prompt {
        "Leave the hunt? A = yes, B = no"
    } else if h.paused {
        "PAUSED (SELECT)"
    } else if h.death.is_some() {
        "You are dead..."
    } else if h.player.underwater {
        "underwater"
    } else if game.run {
        "running"
    } else {
        ""
    };
    print!("\x1b[1;1H");
    println!(
        "c2rs 3DS ({})  {} {:?}  {:4.1} fps\x1b[K",
        game.info.target, game.info.area, game.info.time, fps
    );
    println!(
        "cmdbuf {:3.0}%  {}\x1b[K",
        gpu.cmdbuf_usage() * 100.0,
        mem_line()
    );
    let (cam, a, b) = h.camera();
    println!(
        "cam {:.0},{:.0},{:.0} {:.2},{:.2}\x1b[K",
        cam[0], cam[1], cam[2], a, b
    );
    println!("Weapon:  {weapon}\x1b[K");
    println!("Animals: {alive} huntable, {} bagged\x1b[K", h.successes);
    println!("{state}\x1b[K");
    println!("{}\x1b[K", message.as_deref().unwrap_or(""));
    println!("\x1b[K");
    println!("Circle Pad move     D-Pad turn/look\x1b[K");
    println!("C-Stick / touch     look\x1b[K");
    println!("R fire   A show/hide weapon\x1b[K");
    println!("X next weapon   Y binoculars\x1b[K");
    println!("L run on/off   B jump/swim\x1b[K");
    println!("ZL crouch   ZR call\x1b[K");
    println!("SELECT pause   L+SELECT screenshot\x1b[K");
    println!("START leave\x1b[K");
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
