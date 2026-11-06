//! The `c2-pc` binary; see [`c2_pc::cli`].

use anyhow::Result;

fn main() -> Result<()> {
    c2_pc::init_logging();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (arguments, root) = c2_pc::portable::arguments(arguments, &std::env::current_exe()?)?;
    if let Some(root) = root {
        std::env::set_current_dir(root)?;
    }
    c2_pc::cli::run(arguments)
}
