//! Prints what `c2_pad::Pads` reads (sticks, actions pressed) for a while.
//! `cargo run -p c2-pad --example pad-state [SECONDS]`

use c2_pad::{Action, Pads};

fn main() {
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let mut pads = Pads::new();
    println!("pads: {:?}", pads.names());
    let all = [
        Action::Draw,
        Action::Fire,
        Action::Binoculars,
        Action::Map,
        Action::Jump,
        Action::Run,
        Action::Call,
        Action::NextWeapon,
        Action::Crouch,
        Action::NextCall,
        Action::Pause,
        Action::Exit,
        Action::Recenter,
    ];
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut last = String::new();
    while std::time::Instant::now() < end {
        let f = pads.poll();
        for a in all {
            if f.pressed(a) {
                println!("pressed {a:?}");
            }
        }
        let now = format!(
            "walk {:+.1} {:+.1}  look {:+.1} {:+.1}",
            f.walk[0], f.walk[1], f.look[0], f.look[1]
        );
        if now != last {
            println!("{now}");
            last = now;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
