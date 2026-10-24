//! `c2-3ds-builder`: the launcher's 3DS side only.

fn main() -> eframe::Result {
    // `--cia NAME ...`: build a `.cia` without the window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--cia") {
        match c2_launcher::cia_cli(&args) {
            Ok(p) => println!("{}", p.display()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    c2_launcher::run(c2_launcher::Mode::ThreeDs)
}
