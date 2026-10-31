//! `c2-wiiu-builder`: the launcher's Wii U side only.

fn main() -> eframe::Result {
    // `--install NAME ... -- COOK_ARGS`: one install without the window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--install") {
        match c2_launcher::wiiu_cli(&args) {
            Ok(p) => println!("{}", p.display()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    c2_launcher::run(c2_launcher::Mode::WiiU)
}
