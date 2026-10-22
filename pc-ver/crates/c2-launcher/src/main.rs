//! `c2-launcher`: play on PC (with add-ons), and build 3DS installs if the
//! 3DS section is turned on.

fn main() -> eframe::Result {
    c2_launcher::run(c2_launcher::Mode::Full)
}
