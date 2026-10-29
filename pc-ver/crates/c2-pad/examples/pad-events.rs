//! Prints every gamepad event (raw codes and gilrs's names) for a while:
//! for finding how a pad reports its buttons and sticks.
//! `cargo run -p c2-pad --example pad-events [SECONDS] [--raw]`

use gilrs::{EventType, Gilrs};

fn main() {
    let secs: u64 = std::env::args()
        .skip(1)
        .find_map(|s| s.parse().ok())
        .unwrap_or(20);
    // As `c2_pad::Pads` sees them (with its extra mappings) unless `--raw`.
    let raw = std::env::args().any(|a| a == "--raw");
    let g = if raw {
        Gilrs::new()
    } else {
        gilrs::GilrsBuilder::new()
            .add_mappings(c2_pad::MAPPINGS)
            .build()
    };
    let Ok(mut g) = g else {
        eprintln!("no gamepad support here");
        return;
    };
    for (id, p) in g.gamepads() {
        println!(
            "pad {id}: {} (mapping: {:?}, uuid {:02x?})",
            p.name(),
            p.mapping_source(),
            p.uuid()
        );
    }
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        while let Some(ev) = g.next_event() {
            match ev.event {
                EventType::AxisChanged(a, v, code) if v.abs() > 0.5 => {
                    println!("axis {a:?} {v:+.2} (code {code:?})")
                }
                EventType::AxisChanged(..) => {}
                e => println!("{e:?}"),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
