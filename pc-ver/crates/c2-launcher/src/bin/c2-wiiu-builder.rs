//! `c2-wiiu-builder`: the launcher's Wii U side only.

fn main() -> eframe::Result {
    c2_launcher::run(c2_launcher::Mode::WiiU)
}
