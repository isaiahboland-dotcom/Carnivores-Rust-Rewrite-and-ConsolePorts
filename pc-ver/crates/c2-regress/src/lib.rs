//! Gameplay regression scenarios (docs/REGRESSION.md): scenario files, the
//! headless runner, recordings, comparison against baselines, and the HTML
//! and PDF reports. The scripted run itself is `c2_core::scenario`, shared
//! with the PC renderer (`c2-pc regress`).

pub mod behaviour_parity;
pub mod compare;
pub mod game;
pub mod headless;
pub mod html;
pub mod image_diff;
pub mod kill_parity;
pub mod metrics;
pub mod original;
pub mod output;
pub mod pdf;
pub mod pdf_report;
pub mod publish;
pub mod record;
pub mod render_state;
pub mod scenario_file;
pub mod texture_review;
