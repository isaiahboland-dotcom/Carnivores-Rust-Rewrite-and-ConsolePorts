//! Hunt options, input and call limits.

use crate::player::Controls;
use c2_formats::bits::Bits;

/// Hunt options (from the menu / save file).
#[derive(Debug, Clone, Copy)]
pub struct HuntOptions {
    /// Bit `ai` set for each species chosen as a target (`TargetDino`).
    pub targets: Bits,
    pub radar: bool,
    pub opt_agres: i32,
    pub opt_sens: i32,
    pub view_r: i32,
    /// Cells drawn (at least `view_r`; more with a longer PC render
    /// distance): how far shots carry and where animals out of sight are
    /// re-placed. Detection and aggression keep `view_r`.
    pub render_r: i32,
    pub camo: bool,
    pub scent: bool,
    pub tranq: bool,
    /// Observer mode (`-observ`): no weapons or calls, animals ignore you.
    pub observer: bool,
    /// The trophy room (`TrophyMode`): the area is `TROPHY.MAP` and the
    /// animals are the saved trophies, standing still.
    pub trophy: bool,
    /// Multiplies every kill's score (1 in the original; the 3DS's extra
    /// ammo gear lowers it).
    pub score_factor: f32,
    /// Multiplies the animals' running speed (1 in the original; a 3DS
    /// setting, 0.5..=1).
    pub dino_run_k: f32,
    /// Blood (setting; on in the original): off drops the spray of a hit
    /// and the wounded animals' trails.
    pub blood: bool,
    /// The death camera (setting; on in the original): off shows a black
    /// screen and the leave / restart prompt at once.
    pub death_anim: bool,
    /// `CheckAfraid` every this many ticks (1; the 3DS: 2, i.e. 25 times a
    /// second, as the original did once a frame at 25 fps). Its look
    /// traces are the costliest part of a tick on an Old 3DS.
    pub afraid_every: u32,
    /// Stagger calm stock targets beyond view range; platform opt-in.
    pub distant_ai: bool,
    /// Bit `10 + slot` per call slot whose kills score +10% (not in the
    /// original; the 3DS's roaming add-ons, hunted with no call).
    pub no_call_bonus: Bits,
    /// How far animals notice the hunter, by sight or smell, as a share of
    /// the original's distance (1; the 3DS's "3DS friendly" setting: less,
    /// to match its shorter view). Every animal, modded ones too.
    pub notice_k: f32,
    /// Stealth crouch (a 3DS setting): crouched, the hunter is half as easy
    /// to see and his shots carry half as far.
    pub stealth: bool,
    /// Teleporting by the map (the mobile game's, a setting).
    pub teleport: bool,
    /// The dialect's weapons reload and rack on their own (as with
    /// `autoReload` and `autoRack`): the consoles, which have no buttons
    /// left for them.
    pub auto_chamber: bool,
}

impl Default for HuntOptions {
    fn default() -> Self {
        Self {
            targets: Bits::NONE,
            radar: false,
            opt_agres: 128,
            opt_sens: 128,
            view_r: 74,
            render_r: 74,
            camo: false,
            scent: false,
            tranq: false,
            observer: false,
            trophy: false,
            score_factor: 1.0,
            dino_run_k: 1.0,
            blood: true,
            death_anim: true,
            afraid_every: 1,
            distant_ai: false,
            no_call_bonus: Bits::NONE,
            notice_k: 1.0,
            stealth: false,
            auto_chamber: false,
            teleport: false,
        }
    }
}

/// Call ids are `10 + menu slot` (the original's AI number); one past the
/// last the target set (`Bits`) can hold.
pub const MAX_CALL: i32 = Bits::LEN as i32;

/// One tick of player input.
#[derive(Debug, Clone, Copy, Default)]
pub struct HuntInput {
    pub controls: Controls,
    /// Fire held.
    pub fire: bool,
    /// Show/hide weapon pressed.
    pub toggle_weapon: bool,
    /// Weapon number key pressed (0-based).
    pub select_weapon: Option<usize>,
    /// Binoculars key pressed.
    pub toggle_binoculars: bool,
    /// Binocular zoom keys held.
    pub zoom_in: bool,
    pub zoom_out: bool,
    /// Call key pressed (`kfCall`).
    pub call: bool,
    /// Change-call key pressed.
    pub change_call: bool,
    /// Map key pressed.
    pub toggle_map: bool,
    /// The dialect's reload, rack and firing-mode keys, held
    /// (`ProcessReload`, `ProcessPump`, `ProcessFireMode`).
    pub reload: bool,
    pub rack: bool,
    pub switch_mode: bool,
}
