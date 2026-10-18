//! PC frontend. Viewer arrives in M2, the game in M3.

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    tracing::info!("c2rs PC frontend: nothing to run yet (see README roadmap)");
    Ok(())
}
