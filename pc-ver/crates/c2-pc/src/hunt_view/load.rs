//! Loading a hunt: the area, the game's data, weapons and HUD pictures.

use super::*;

/// The Modder's Edition hunt conditions for `setup` (area from the map
/// name, dawn/day/night, the menu slots picked).
fn mee_hunt(setup: &HuntSetup) -> c2_formats::mee::Hunt {
    let stem = setup
        .map
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    c2_formats::mee::Hunt {
        area: c2_formats::mee::area_of(&stem),
        time: match setup.time {
            TimeOfDay::Dusk => 0,
            TimeOfDay::Day => 1,
            TimeOfDay::Night => 2,
        },
        // Menu slot i is target bit 10 + i, so up to 22 slots (add-ons
        // past the engine's 10 get slots of their own).
        picked: setup.targets.down(10),
    }
}

fn menu_pic(hud: &mut Hud, gpu: &Gpu, huntdat: &Path, name: &str) -> Option<usize> {
    let p = find_file(&find_file(huntdat, "MENU")?, name)?;
    let (w, h, rgba) = load_tga(&p).ok()?;
    Some(hud.add_image(gpu, w, h, &rgba))
}

impl HuntView {
    /// A survival held at night (hardcore).
    pub(super) fn endless_night(setup: &HuntSetup) -> bool {
        setup.survival.is_some_and(|k| k.endless_night())
    }

    /// The dynamic day cycle this hunt plays with, if any: the setting's,
    /// from the hunt's time; hardcore survival's endless night whatever the
    /// setting says.
    pub(super) fn day_cycle(setup: &HuntSetup) -> Option<c2_core::day_cycle::DayCycle> {
        use c2_core::day_cycle::DayCycle;
        if setup.trophy() {
            None
        } else if Self::endless_night(setup) {
            Some(DayCycle::endless_night())
        } else {
            (setup.pc.dynamic_time != 0).then(|| DayCycle::new(setup.time))
        }
    }

    pub fn load(gpu: &Gpu, mut setup: HuntSetup) -> Result<Self> {
        if let Some(kind) = setup.survival {
            survival_setup(&mut setup, kind)?;
        }
        let mut view = AreaView::load(gpu, &setup.map, setup.time)?;
        view.narrow_flashlight = setup.pc.narrow_flashlight != 0;
        view.fly = false;
        if let Some(c) = Self::day_cycle(&setup) {
            view.area.cycle = Some(c);
            view.night_vision = Some(false);
            view.flashlight = setup.flashlight && Self::endless_night(&setup);
        }
        let seed = setup.seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        });
        let huntdat = setup
            .map
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let (res, mee) = read_res(&huntdat, &setup)?;
        if setup.survival.is_some() {
            setup.weapons = 1u128 << shotgun(&res)?;
        }
        let (mut species, cars) = load_species(&huntdat, &res)?;
        if let Some(t) = &mee {
            c2_core::mee::tune(&mut species, t);
        }
        super::variants::load(&huntdat, &setup.map, &mut species)?;
        let menu = crate::menu::menu_data(&huntdat)?;
        c2_core::game::price_species(&mut species, &menu);
        if setup.pc.survival_budget == 0 {
            setup.pc.survival_budget = menu.survival_budget.min(i32::MAX as u32) as i32;
        }
        let mut hud = Hud::new(gpu);
        let (weapon_infos, weapon_anims, mut weapons) =
            load_weapons(gpu, view.globals(), &mut hud, &huntdat, &res.weapons);
        let kit = load_kit(gpu, &mut hud, &huntdat, &setup, res.weapons.len());
        weapons.hide_scoped = kit.is_some();
        let gauges = kit
            .is_none()
            .then(|| gauges::Gauges::load(gpu, view.globals(), &huntdat))
            .flatten();
        weapons.hide_binoculars = kit.as_ref().is_some_and(HuntKit::has_binocular_mask);
        let exit_img = menu_pic(&mut hud, gpu, &huntdat, "EXIT.TGA");
        let plaque_img = menu_pic(
            &mut hud,
            gpu,
            &huntdat,
            if setup.trophy() {
                "TROPHY.TGA"
            } else {
                "TROPHY_G.TGA"
            },
        );
        let trophy_exit_img = menu_pic(&mut hud, gpu, &huntdat, "TROPHY_E.TGA");
        let pause_img = menu_pic(&mut hud, gpu, &huntdat, "PAUSE.TGA");
        let (map_img, map_at) = map_picture(gpu, &mut hud, &huntdat, &view, kit.as_ref());
        let (call_imgs, call_pics) = call_pictures(gpu, &mut hud, &huntdat, &setup, kit.as_ref());
        let dinos = DinoAssets::new(gpu, view.globals(), cars, load_ship(&huntdat));
        let species_cars: Vec<Option<&Car>> = dinos
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        let sounds = SoundBank::load(
            &huntdat,
            &species_cars,
            &weapons.cars(),
            dinos.ship_car(),
            &view.area().rsc,
            setup.targets,
        );
        let mut hunt = Self::new_hunt(
            &view,
            species,
            &weapon_infos,
            &weapon_anims,
            seed,
            &setup,
            mee.as_ref(),
        );
        anyhow::ensure!(
            hunt.survival.as_ref().is_none_or(|s| s.opts.budget_ready()),
            "Survival needs a loaded animal that can attack and fits the configured starting budget"
        );
        hunt.ship_anim_ms = dinos.ship_anim_ms();
        if let Some(ms) = weapons.muzzle_ms() {
            hunt.weapons.muzzle_ms = ms;
        }
        fit_view(&mut view, &hunt, &setup);
        let prev_cam = hunt.camera().0;
        let effects = EffectsView::new(gpu, view.globals(), &huntdat);
        Ok(Self {
            view,
            hunt,
            dinos,
            weapons,
            weapon_infos,
            weapon_anims,
            hud,
            exit_img,
            map_img,
            map_at,
            map_heading: setup.pc.map_heading != 0,
            map_mobile: setup.pc.map_mobile != 0 || kit.is_some(),
            kit,
            gauges,
            gauges_hidden: false,
            map_cursor: None,
            screen: (1280, 720),
            map_viewport: map_viewport::MapViewport::default(),
            teleport_pick: None,
            call_imgs,
            call_pics,
            plaque_img,
            pause_img,
            trophy_exit_img,
            setup,
            prev_cam,
            acc_ms: 0.0,
            run: false,
            pad_crouch: false,
            touch: touch::Touch::default(),
            pad_last: false,
            pending: HuntInput::default(),
            events: Vec::new(),
            message: None,
            kill: None,
            ask: None,
            picker: None,
            picker_kind: Pick::Weapon,
            picker_held: false,
            held_pick: None,
            weapon_flash: 0.0,
            seed,
            done: false,
            audio: Audio::new(),
            effects,
            sounds,
            mee,
            ammo_low: (usize::MAX, 0.0),
        })
    }
}

/// A survival: Mount Ravan by day, every animal, no observer, double ammo
/// or night vision; hardcore at night with the flashlight in hand.
fn survival_setup(setup: &mut HuntSetup, kind: SurvivalKind) -> Result<()> {
    setup.map = setup
        .map
        .parent()
        .and_then(|p| find_file(p, "AREA5.MAP"))
        .ok_or_else(|| anyhow::anyhow!("Survival requires Mount Ravan (AREA5.MAP)"))?;
    // Hardcore: the night, the flashlight in hand.
    let night = kind.endless_night();
    setup.time = if night {
        TimeOfDay::Night
    } else {
        TimeOfDay::Day
    };
    setup.targets = c2_formats::bits::Bits::ALL;
    setup.observer = false;
    setup.double_ammo = false;
    setup.night_vision = false;
    setup.flashlight = night;
    Ok(())
}

/// The game's `_RES.TXT`. Modder's Edition data is resolved for this hunt
/// first (per-area models, spawn tables); original data is read as it is.
fn read_res(
    huntdat: &Path,
    setup: &HuntSetup,
) -> Result<(
    c2_formats::res_txt::ResData,
    Option<c2_formats::mee::Tables>,
)> {
    let res_txt = find_file(huntdat, "_RES.TXT")
        .and_then(|p| std::fs::read(p).ok())
        .map(|d| c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(&d)))
        .transpose()?
        .filter(c2_formats::mee::is_dialect);
    Ok(match res_txt {
        Some(r) => {
            let h = mee_hunt(setup);
            // Picked roaming add-ons hunt by their menu slots.
            let roamers = crate::menu::roamer_slots(huntdat, &r);
            let chars = c2_core::mee::picked_roamers(&h, &roamers);
            let mut m = c2_core::mee::read_picked_anywhere(&r, &h, &chars);
            c2_core::mee::pick_roamers(&h, &mut m.tables, &roamers);
            (m.res, Some(m.tables))
        }
        None => (load_res(huntdat)?, None),
    })
}

/// The shotgun's weapon slot (survival hunts with it alone): by its model,
/// else by its name.
fn shotgun(res: &c2_formats::res_txt::ResData) -> Result<usize> {
    res.weapons
        .iter()
        .position(|w| {
            w.file
                .as_deref()
                .is_some_and(|f| f.eq_ignore_ascii_case("shotgun.car"))
        })
        .or_else(|| {
            res.weapons.iter().position(|w| {
                w.name
                    .as_deref()
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case("shotgun"))
            })
        })
        .ok_or_else(|| anyhow::anyhow!("Survival requires the shotgun"))
}

/// A game built from an APK: the hunt screen as the phone game's, with
/// its painted map of the area at the hunt's time of day.
fn load_kit(
    gpu: &Gpu,
    hud: &mut Hud,
    huntdat: &Path,
    setup: &HuntSetup,
    weapons: usize,
) -> Option<HuntKit> {
    let kit_map = setup
        .map
        .file_stem()
        .and_then(|s| c2_formats::mee::area_of(&s.to_string_lossy()))
        .map(|n| {
            let t = match setup.time {
                TimeOfDay::Dusk => "DAWN",
                TimeOfDay::Day => "DAY",
                TimeOfDay::Night => "NIGHT",
            };
            format!("AREA{n}_{t}")
        });
    // Slot numbers can belong to addons after Pick parts. Only use painted
    // APK artwork when its source map still matches the selected terrain.
    let kit_map = kit_map.filter(|name| {
        let area = name.split('_').next().unwrap_or(name);
        let original = huntdat
            .join("MOBILE")
            .join("MAPS")
            .join(format!("{area}.MAP"));
        same_map(&original, &setup.map)
    });
    HuntKit::load(huntdat, hud, gpu, weapons, kit_map.as_deref())
}

// Missing provenance in older exports deliberately uses the terrain overview.
fn same_map(original: &Path, selected: &Path) -> bool {
    match (std::fs::read(original), std::fs::read(selected)) {
        (Ok(original), Ok(selected)) => original == selected,
        _ => false,
    }
}

/// The map: the frame with the area overview painted in at (11, 23), and
/// where the area sits in it. No frame (an APK-built game): the area alone,
/// the kit's frame drawn around it.
fn map_picture(
    gpu: &Gpu,
    hud: &mut Hud,
    huntdat: &Path,
    view: &AreaView,
    kit: Option<&HuntKit>,
) -> (Option<usize>, (f32, f32)) {
    let map_img = find_file(huntdat, "MENU")
        .and_then(|m| find_file(&m, "MAPFRAME.TGA"))
        .and_then(|p| c2_formats::tga::parse(&std::fs::read(p).ok()?).ok())
        .map(|mut frame| {
            let px = Hunt::map_image(view.area());
            for y in 0..256 {
                for x in 0..256 {
                    let (fx, fy) = (x + 11, y + 23);
                    if fx < frame.width && fy < frame.height {
                        frame.pixels[fy * frame.width + fx] = px[y * 256 + x];
                    }
                }
            }
            let rgba = c2_core::color::image_to_rgba8(&frame.pixels, false);
            hud.add_image(gpu, frame.width as u32, frame.height as u32, &rgba)
        });
    match map_img {
        Some(i) => (Some(i), (11.0, 23.0)),
        None if kit.is_some_and(|k| k.map.is_some()) => (kit.and_then(|k| k.map), (0.0, 0.0)),
        None if kit.is_some() => {
            let rgba = c2_core::color::image_to_rgba8(&Hunt::map_image(view.area()), false);
            (Some(hud.add_image(gpu, 256, 256, &rgba)), (0.0, 0.0))
        }
        None => (None, (11.0, 23.0)),
    }
}

/// The hunted animals' call icons (`MENU/PICS/CALLn.TGA`), and the phone
/// kit's (its animals' pictures, `DINOn.TGA`), by call id 10.. (menu slot
/// + 10).
fn call_pictures(
    gpu: &Gpu,
    hud: &mut Hud,
    huntdat: &Path,
    setup: &HuntSetup,
    kit: Option<&HuntKit>,
) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let pics = find_file(huntdat, "MENU").and_then(|m| find_file(&m, "PICS"));
    let call_imgs: Vec<Option<usize>> = (1..=c2_core::game::MAX_CALL - 10)
        .map(|n| {
            if !setup.targets.has(n as usize + 9) {
                return None;
            }
            let p = find_file(pics.as_deref()?, &format!("CALL{n}.TGA"))?;
            let (w, h, rgba) = load_tga(&p).ok()?;
            Some(hud.add_image(gpu, w, h, &rgba))
        })
        .collect();
    let call_pics: Vec<Option<usize>> = match kit {
        Some(k) => (1..=c2_core::game::MAX_CALL - 10)
            .map(|n| {
                if !setup.targets.has(n as usize + 9) {
                    return None;
                }
                k.image(&format!("button_dino_active_{n}"), hud, gpu)
                    .or_else(|| {
                        let pics = pics.as_deref()?;
                        [format!("DINO{n}.TGA"), format!("CALL{n}.TGA")]
                            .iter()
                            .find_map(|name| {
                                let p = find_file(pics, name)?;
                                let (w, h, rgba) = load_tga(&p).ok()?;
                                Some(hud.add_image(gpu, w, h, &rgba))
                            })
                    })
            })
            .collect(),
        None => Vec::new(),
    };
    (call_imgs, call_pics)
}

/// How far the view draws (the hunt's range, or the PC's farther one) and
/// its fog, brightness and model range.
fn fit_view(view: &mut AreaView, hunt: &Hunt, setup: &HuntSetup) {
    let drawn = if hunt.opts.render_r < crate::settings::MAX_RENDER_CELLS {
        hunt.opts.render_r
    } else {
        setup.pc.render_cells(hunt.opts.view_r)
    };
    view.radius = drawn.max(hunt.opts.view_r) as u32;
    view.high_end = setup.pc.high_end_view != 0;
    // Past the original range, fog only softens the far edge.
    if hunt.opts.render_r > hunt.opts.view_r {
        view.fog_start = 0.85;
    }
    view.brightness = (hunt.room.options.brightness + 128) as f32 / 256.0;
    view.sprite_cells = setup.pc.model_cells(view.radius as i32);
}

#[cfg(test)]
mod map_art_tests {
    use super::same_map;

    #[test]
    fn painted_art_requires_matching_terrain_even_when_slots_match() {
        let root = std::env::temp_dir().join(format!("c2-map-art-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original.MAP");
        let selected = root.join("AREA1.MAP");
        std::fs::write(&original, b"APK terrain").unwrap();
        std::fs::write(&selected, b"addon terrain").unwrap();
        assert!(!same_map(&original, &selected));
        std::fs::write(&selected, b"APK terrain").unwrap();
        assert!(same_map(&original, &selected));
        assert!(!same_map(&root.join("missing.MAP"), &selected));
        std::fs::remove_dir_all(root).unwrap();
    }
}
