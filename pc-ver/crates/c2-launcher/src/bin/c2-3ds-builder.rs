//! `c2-3ds-builder`: the launcher's 3DS side only.

fn main() -> eframe::Result {
    c2_launcher::run(c2_launcher::Mode::ThreeDs)
}
