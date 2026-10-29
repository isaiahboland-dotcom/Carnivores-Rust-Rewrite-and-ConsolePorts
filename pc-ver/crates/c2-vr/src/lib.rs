//! PC VR (README M17, `GAMEPLAN.md`). First draft: the launcher's "Play in
//! VR" runs `c2-vr play` with the same game, overlays and add-ons as
//! `c2-pc play`; it finds the active OpenXR runtime and checks the game,
//! then shows the main menu standing in the first area's camp (`game`,
//! `panel`), which leads to the area (`terrain`, with its objects and sky)
//! or the trophy room. The test scene (`test_scene`) checks a headset with
//! no game data. Animals, weapons and the hunt aren't in VR yet.

pub mod animals;
pub mod canvas;
pub mod game;
pub mod gpu;
pub mod hunt;
pub mod menus;
pub mod objects;
pub mod panel;
pub mod runtime;
pub mod scene;
pub mod sky;
pub mod terrain;
pub mod test_scene;
pub mod xr;

/// The game is played in the headset (not yet: `c2-vr play` shows the menu
/// and the areas, with no animals).
pub const PLAYABLE: bool = false;
