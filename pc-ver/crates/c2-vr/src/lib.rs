//! PC VR (README M17, `GAMEPLAN.md`). First draft: the launcher's "Play in
//! VR" runs `c2-vr play` with the same game, overlays and add-ons as
//! `c2-pc play`; it finds the active OpenXR runtime and checks the game,
//! then shows the game's first area's terrain in the headset (`terrain`,
//! GAMEPLAN milestone 2). The test scene (`test_scene`) checks a headset
//! with no game data. Animals, objects, weapons and the hunt aren't in VR
//! yet.

pub mod gpu;
pub mod objects;
pub mod runtime;
pub mod scene;
pub mod sky;
pub mod terrain;
pub mod test_scene;
pub mod xr;

/// The game is played in the headset (not yet: `c2-vr play` shows an
/// area's terrain).
pub const PLAYABLE: bool = false;
