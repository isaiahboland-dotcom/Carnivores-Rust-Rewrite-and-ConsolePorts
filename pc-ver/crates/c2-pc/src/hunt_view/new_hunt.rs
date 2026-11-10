//! `ReInitGame`: a fresh `Hunt` (on load and on restart).

use super::*;

/// Snow for a map whose `.c2map` beside it says so (Ice Age's areas in
/// the combined game).
fn c2map_snow(map: &Path) -> Option<c2_formats::mee::Snow> {
    let stem = map.file_stem()?.to_string_lossy().to_ascii_lowercase();
    std::fs::read_dir(map.parent()?)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("c2map"))
        })
        .filter_map(|p| {
            c2_formats::menu_txt::parse_c2map(&String::from_utf8_lossy(&std::fs::read(p).ok()?))
        })
        .find(|a| a.project.eq_ignore_ascii_case(&stem))
        .filter(|a| a.snow)
        .map(|_| c2_formats::mee::Snow::ICE_AGE)
}

/// Local-ish date and time for trophy records (UTC; no timezone database).
fn local_clock() -> (i32, i32, i32, i32, i32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y as i32,
        m as i32,
        d as i32,
        (rem / 3600) as i32,
        (rem % 3600 / 60) as i32,
    )
}

impl HuntView {
    /// `ReInitGame`: the saved trophy room is (re)loaded, the hunter put at a
    /// random landing spot and the herd placed (or the trophies, in the room).
    pub(super) fn new_hunt(
        view: &AreaView,
        species: Vec<c2_core::dino::Species>,
        infos: &[WeaponInfo],
        anims: &[c2_core::game::WeaponAnims],
        seed: u32,
        setup: &HuntSetup,
        mee: Option<&c2_formats::mee::Tables>,
    ) -> Hunt {
        let area = view.area();
        let mut room = crate::save::load(&setup.game_dir, setup.reg);
        c2_core::dino::variants::remap_trophies(&species, &mut room);
        let trophy = setup.trophy();
        let mut rng = MsRand::new(seed);
        let player = if trophy {
            Player::new(
                area,
                mee.and_then(|t| t.trophy_start)
                    .or(area.trophy_start)
                    .unwrap_or((76, 70)),
            )
        } else {
            let cell = area.landings[seed as usize % area.landings.len().max(1)];
            Player::new(area, cell)
        };
        let herd = place_herd(area, species, &room, &player, &mut rng, setup, mee);
        if !trophy {
            log_targets(&herd, setup);
        }
        let carried = if trophy {
            c2_formats::bits::Bits::NONE
        } else {
            c2_formats::bits::Bits::from(setup.weapons)
        };
        let weapons = Weapons::new(infos.to_vec(), anims.to_vec(), carried, setup.double_ammo);
        let opts = hunt_options(&room.options, setup);
        let own_snow = area.snow;
        let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
        if !trophy && setup.survival.is_none() {
            apply_instance(&mut hunt, area, setup);
        }
        // The area's snow (Ice Age).
        hunt.effects.snow = mee
            .and_then(|t| t.snow)
            .or(own_snow)
            .or_else(|| c2map_snow(&setup.map))
            .filter(|_| !trophy)
            .map(|s| c2_core::effects::Snowfall::new(s, 2000));
        if let Some(s) = &hunt.effects.snow {
            tracing::info!("snow: {} flakes", s.kind.dens);
        }
        // Survival: the waves the menu prices buy, in Mount Ravan's ravine.
        if let Some(kind) = setup.survival {
            let mut opts = SurvivalOptions::pick(&hunt, kind, SURVIVAL_MAX);
            opts.starting_budget = setup.pc.survival_budget.max(0) as u32;
            hunt.start_survival(area, opts);
        }
        hunt.clock = local_clock();
        hunt.area_no = setup
            .map
            .file_stem()
            .and_then(|s| c2_formats::mee::area_of(&s.to_string_lossy()))
            .map_or(0, |n| n as i32);
        hunt
    }
}

/// The herd: the trophies in the room; else the dialect's spawn tables or
/// the original's placement around the hunter.
fn place_herd(
    area: &c2_core::area::Area,
    species: Vec<c2_core::dino::Species>,
    room: &c2_formats::sav::Save,
    player: &Player,
    rng: &mut MsRand,
    setup: &HuntSetup,
    mee: Option<&c2_formats::mee::Tables>,
) -> Herd {
    let o = &room.options;
    let mut herd = Herd {
        species,
        chars: Vec::new(),
        demo_index: None,
        ..Default::default()
    };
    // Dialect games: the trophy room's spots (where trophies stand, and
    // which a kill takes).
    if let Some(t) = mee {
        herd.trophy_spots = t.trophies.clone();
    }
    if setup.trophy() {
        herd.place_trophies(area, room, rng);
    } else if let Some(t) = mee {
        herd.place_mee(area, (player.x, player.z), rng, t, o.dens);
    } else {
        let opts = PlaceOptions {
            density: o.dens,
            opt_agres: o.agres,
            targets: setup.targets,
            max_ambient: None,
        };
        herd.place(area, (player.x, player.z), rng, opts);
    }
    tracing::info!("placed {} characters", herd.chars.len());
    herd
}

/// Logs how many of each selected target were placed (a warning for none).
fn log_targets(herd: &Herd, setup: &HuntSetup) {
    for (i, sp) in herd.species.iter().enumerate() {
        let slot = sp
            .tuning
            .menu_slot
            .or_else(|| {
                sp.mee
                    .as_ref()
                    .filter(|m| (10..30).contains(&m.ai))
                    .map(|m| m.call_no)
            })
            .unwrap_or(sp.info.ai - 10);
        if slot < 0 || !setup.targets.has(10 + slot as usize) {
            continue;
        }
        let count = herd.chars.iter().filter(|c| c.ctype == i).count();
        tracing::info!(species = %sp.info.name, model = %sp.info.file, slot, count, "hunt target placement");
        if count == 0 {
            tracing::warn!(species = %sp.info.name, "selected target has no placed animals on this map");
        }
    }
}

/// The hunt's options from the save's and the PC's settings.
fn hunt_options(o: &c2_formats::sav::Options, setup: &HuntSetup) -> HuntOptions {
    let trophy = setup.trophy();
    let half_tranq = o.tranq != 0
        && setup.pc.multi_ammo != 0
        && setup.pc.half_tranq != 0
        && setup.survival.is_none();
    let opts = HuntOptions {
        targets: setup.targets,
        // Hardcore survival hunts with the radar.
        radar: o.radar != 0 || HuntView::endless_night(setup),
        opt_agres: o.agres,
        opt_sens: o.sens,
        // ctViewR: 42 + OptViewR / 8 * 2 cells; 60 in the trophy room.
        view_r: if trophy { 60 } else { 42 + o.view_r / 8 * 2 },
        render_r: 0,
        camo: o.camo != 0,
        scent: o.scent != 0,
        // Multi ammo's Half tranq splits the rounds; Full tranq is the
        // original's tranquilizer.
        tranq: o.tranq != 0 && !half_tranq,
        half_tranq,
        free_observer: setup.observer && setup.pc.advanced_observer != 0,
        observer: setup.observer,
        trophy,
        score_factor: 1.0,
        dino_run_k: 1.0,
        blood: setup.pc.blood != 0,
        death_anim: setup.pc.death_anim != 0,
        afraid_every: 1,
        // The Wii U's staggered far AI is off on PC (every animal every tick).
        distant_ai: false,
        no_call_bonus: c2_formats::bits::Bits::NONE,
        notice_k: 1.0,
        stealth: false,
        teleport: setup.pc.teleport != 0,
        map_holsters: setup.pc.map_holsters != 0,
        auto_chamber: false,
        dynamic_night: None,
        night_vision_active: false,
    };
    let mut opts = opts;
    opts.render_r = if trophy {
        opts.view_r
    } else {
        // High-end view draws farther than this; the animals keep the
        // standard maximum (ReplaceCharacterForward places them past
        // it, which past the map's size would never succeed).
        setup
            .pc
            .render_cells(opts.view_r)
            .min(crate::settings::MAX_RENDER_CELLS)
    };
    opts
}

/// The instance's settings on a hunt: predation (`_PREDATION.TXT`, when the
/// recipe turns it on), the secret animals' cap, then its spawn rules.
fn apply_instance(hunt: &mut Hunt, area: &c2_core::area::Area, setup: &HuntSetup) {
    let huntdat = setup
        .map
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new("."));
    hunt.herd.predation = find_file(
        huntdat,
        c2_formats::predation::PATH.trim_start_matches("HUNTDAT/"),
    )
    .and_then(|p| std::fs::read(p).ok())
    .and_then(|b| c2_core::dino::predation::load(&b, &hunt.herd.species));
    if hunt.herd.predation.is_some() {
        tracing::info!("predation on");
    }
    let secret: Vec<bool> = (0..hunt.herd.species.len())
        .map(|ctype| {
            let c = c2_core::dino::Character {
                ctype,
                ai: hunt.herd.species[ctype].runs_as(),
                ..Default::default()
            };
            (setup.pc.secret_dinos == 0 || hunt.is_huntable(&c)) && !hunt.is_target(&c)
        })
        .collect();
    hunt.herd
        .cap_population(setup.pc.secret_dinos.max(0) as usize, |ct| secret[ct]);
    // The instance's spawn rules (`_RULES.TXT`), after the cap: its
    // extra animals always come.
    let rules = crate::instance_rules::read(huntdat);
    if !rules.is_empty() {
        let stem = setup.map.file_stem().unwrap_or_default().to_string_lossy();
        let extra = hunt.apply_spawn_rules(area, &rules, &stem);
        tracing::info!(area = %stem, extra, "instance spawn rules");
    }
}
