//! Modder's Edition weapon keys and underwater tuning.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use super::weapon_anim;
use c2_formats::res_txt::WeaponDef;

/// A dialect weapon's extras (`Resources.cpp: ReadWeaponLine` in the
/// Modder's Edition): flying bullets, recoil, firing modes, trackers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeeWeapon {
    /// `land_veloc`: bullets fly `64 * veloc` units per 17 ms
    /// (`AddBullet`); 0 (not given) keeps the original's instant trace, as
    /// the engine's own bullets wouldn't move.
    pub veloc: f32,
    /// `land_fall`: what a bullet's fall speed grows by per 17 ms.
    pub fall: f32,
    /// `recoil`: the view kicks up by `recoil / 100` rad (`ProcessShoot`).
    pub recoil: f32,
    /// `shake`: the aim's sway starts at `shake * 4` when drawn.
    pub shake: Option<f32>,
    /// `canRun`: the hunter may run with it out.
    pub can_run: bool,
    /// `fullAuto`: fires while the trigger is held; else once per pull.
    pub full_auto: bool,
    /// `cannotMortal`: a vital hit only does its power.
    pub cannot_mortal: bool,
    /// `radar` (`radR`, `radG`, `radB`, `radTime`): an animal it hits shows
    /// on the map in this colour, for `radTime` ms (0: for good).
    pub radar: Option<([u8; 3], i32)>,
    /// `optic`: the scope's zoom (the original's is 3).
    pub zoom: f32,
    /// `cross`: cross-hairs on the model's last vertex, scoped or (without
    /// `optic`) always; not given: none.
    pub cross: bool,
    /// `unzoom`: no zoom or cross-hairs while drawing, hiding, reloading.
    pub unzoom: bool,
    /// Model animation per [`weapon_anim`] slot (`getAnim`, `shtAnim`,
    /// `putAnim`, `rldAnimFull`, `rldAnimPart`, `rckAnim`, `modAnim`,
    /// `emptyAnim`, `getEmpAnim`, `putEmpAnim`): draw, fire and hide in the
    /// original order when not given; the rest none (`-1`, or `<= 0` for
    /// those the dialect tests so).
    pub anims: [Option<usize>; weapon_anim::SLOTS],
    /// Underwater sounds per slot (`getAqSnd`, `shtAqSnd`, `putAqSnd`,
    /// `rldAqSndFull`, `rldAqSndPart`, `rckAqSnd`, `modAqSnd`): the model's
    /// sound numbers.
    pub aq_sounds: [Option<usize>; weapon_anim::SLOTS],
    /// `mustRack`: rack after every shot; `autoRack`: on its own;
    /// `autoReload`: reload on its own when empty.
    pub must_rack: bool,
    pub auto_rack: bool,
    pub auto_reload: bool,
    /// `semiAuto` (with `fullAuto` and a `modAnim`: switchable).
    pub semi_auto: bool,
    /// `harpoon`: fires under water too, with the `aqua_*` numbers.
    pub harpoon: bool,
    /// `aqua_power`, `aqua_veloc`, `aqua_prec`, `aqua_fall`: under water
    /// (the land numbers where not given; the dialect would read -1).
    pub aqua: Aqua,
    /// `retrieve`: a round that stops in the ground or an object stays
    /// there to be picked up again.
    pub retrieve: bool,
    /// `muzzflash`, `chamflash`: the muzzle flash; the HUD's round flash.
    pub muzz_flash: bool,
    pub cham_flash: bool,
    /// `bModel`: the round's model in flight (`WEAPONS/`).
    pub bullet_model: Option<String>,
    /// `picc`: the chamber picture beside the rounds (`WEAPONS/`).
    pub chamber_pic: Option<String>,
}

/// A dialect weapon's numbers under water (`PowerAq`, `VelocAq`, `PrecAq`,
/// `FallAq`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aqua {
    pub power: f32,
    pub veloc: f32,
    pub prec: f32,
    pub fall: f32,
}

impl MeeWeapon {
    /// The keys that make a weapon a dialect one.
    const KEYS: &[&str] = &[
        "land_veloc",
        "land_fall",
        "recoil",
        "shake",
        "canrun",
        "fullauto",
        "cannotmortal",
        "radar",
        "getanim",
        "shtanim",
        "putanim",
        "rldanimfull",
        "cross",
        "unzoom",
        "rldanimpart",
        "rckanim",
        "modanim",
        "emptyanim",
        "harpoon",
        "retrieve",
        "muzzflash",
        "chamflash",
        "bmodel",
        "picc",
        "mustrack",
        "autorack",
        "autoreload",
        "semiauto",
    ];

    pub(super) fn from_def(d: &WeaponDef) -> Option<Self> {
        let get = |k: &str| {
            d.extra
                .iter()
                .find(|e| e.key.eq_ignore_ascii_case(k))
                .map(|e| e.value.trim().to_string())
        };
        if !Self::KEYS.iter().any(|k| get(k).is_some()) {
            return None;
        }
        let num = |k: &str| get(k).map_or(0.0, |v| c2_formats::res_txt::c_atof(&v));
        let flag = |k: &str| get(k).is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");
        let int = |k: &str| get(k).map(|v| c2_formats::res_txt::c_atoi(&v));
        // `>= 0` counts for these (`rldAnimPart >= 0`), `> 0` for those
        // the dialect tests `<= 0` (`pmpAnim`, `modAnim`, the empties).
        let anim = |k: &str| int(k).and_then(|n| usize::try_from(n).ok());
        let anim_pos = |k: &str| int(k).filter(|&n| n > 0).map(|n| n as usize);
        let file = |k: &str| get(k).map(|v| v.trim_matches('\'').to_string());
        let land = (
            num("land_power"),
            num("land_veloc").max(0.0),
            num("land_prec"),
            num("land_fall"),
        );
        let or_land = |k: &str, l: f32| get(k).map_or(l, |v| c2_formats::res_txt::c_atof(&v));
        let colour = |k: &str| num(k).clamp(0.0, 255.0) as u8;
        Some(Self {
            veloc: num("land_veloc").max(0.0),
            fall: num("land_fall"),
            recoil: num("recoil"),
            shake: get("shake").map(|v| c2_formats::res_txt::c_atof(&v)),
            can_run: flag("canRun"),
            zoom: d.optic.unwrap_or(0.0),
            cross: flag("cross"),
            unzoom: flag("unzoom"),
            full_auto: flag("fullAuto"),
            cannot_mortal: flag("cannotMortal"),
            radar: flag("radar").then(|| {
                (
                    [colour("radR"), colour("radG"), colour("radB")],
                    num("radTime") as i32,
                )
            }),
            anims: [
                Some(anim("getAnim").unwrap_or(0)),
                Some(anim("shtAnim").unwrap_or(1)),
                Some(anim("putAnim").unwrap_or(2)),
                anim("rldAnimFull"),
                anim("rldAnimPart"),
                anim_pos("rckAnim"),
                anim_pos("modAnim"),
                anim_pos("emptyAnim"),
                anim_pos("getEmpAnim"),
                anim_pos("putEmpAnim"),
            ],
            aq_sounds: [
                anim("getAqSnd"),
                anim("shtAqSnd"),
                anim("putAqSnd"),
                anim("rldAqSndFull"),
                anim("rldAqSndPart"),
                anim("rckAqSnd"),
                anim("modAqSnd"),
                None,
                None,
                None,
            ],
            must_rack: flag("mustRack"),
            auto_rack: flag("autoRack"),
            auto_reload: flag("autoReload"),
            semi_auto: flag("semiAuto"),
            harpoon: flag("harpoon"),
            aqua: Aqua {
                power: or_land("aqua_power", land.0),
                veloc: or_land("aqua_veloc", land.1),
                prec: or_land("aqua_prec", land.2),
                fall: or_land("aqua_fall", land.3),
            },
            retrieve: flag("retrieve"),
            muzz_flash: flag("muzzflash"),
            cham_flash: flag("chamflash"),
            bullet_model: file("bModel"),
            chamber_pic: file("picc"),
        })
    }
}
