//! Building the menus: boards, lists, texts and the saved hunters.

use super::*;

impl Menu {
    /// `game_dir`: the install (or its `HUNTDAT`). `player`: sign straight in
    /// as this hunter (back from a hunt).
    pub fn new(gpu: &Gpu, game_dir: &Path, player: Option<i32>) -> Result<Self> {
        let (game_dir, huntdat) = match find_file(game_dir, "HUNTDAT") {
            Some(h) => (game_dir.to_path_buf(), h),
            None => (
                game_dir.parent().unwrap_or(Path::new(".")).to_path_buf(),
                game_dir.to_path_buf(),
            ),
        };
        let data = load_res(&huntdat).context("reading the game data (is this a game folder?)")?;
        // Before the settings load: the instance may fix some.
        crate::instance_rules::load(&huntdat);
        let classic_survival = data
            .characters
            .iter()
            .filter(|d| {
                c2_core::game::classic_identity(
                    d.name.as_deref().unwrap_or(""),
                    d.file.as_deref().unwrap_or(""),
                )
            })
            .count()
            >= 6;
        let dir = find_file(&huntdat, "MENU").unwrap_or_else(|| huntdat.join("MENU"));
        let mut hud = Hud::new(gpu);
        // The style's frames where it has them (else the game's).
        let art = MENU_ART
            .get()
            .and_then(|d| find_file(d, "HUNTDAT").or_else(|| Some(d.clone())))
            .and_then(|h| find_file(&h, "MENU"))
            .filter(|m| find_file(m, "MENUM.TGA").is_some())
            .unwrap_or_else(|| dir.clone());
        let mut board =
            |base: &str, on: &str, map: &str| Board::load(&mut hud, gpu, &art, base, on, map);
        let main = board("MENUM.TGA", "MENUM_ON.TGA", "MAIN_MAP.RAW");
        let stats_board = board("MENUS.TGA", "MENUM_ON.TGA", "MAIN_MAP.RAW");
        let quit_board = board("MENUQ.TGA", "MENUQ_ON.TGA", "MQ_MAP.RAW");
        let reg = board("MENUR.TGA", "MENUR_ON.TGA", "MR_MAP.RAW");
        let waiver = board("MENUL.TGA", "MENUL_ON.TGA", "ML_MAP.RAW");
        let delete = board("MENUD.TGA", "MENUD_ON.TGA", "MD_MAP.RAW");
        let setup = board("MENU2.TGA", "MENU2_ON.TGA", "M2_MAP.RAW");
        let options = board("OPT_OFF.TGA", "OPT_ON.TGA", "OPT_MAP.RAW");
        let credits = board("CREDITS.TGA", "", "");
        // A game built from the player's APK keeps the phone's interface
        // art and has no PC frames: its menus are drawn as the phone's,
        // also when `--menu-art` names a folder without frames (the APK's
        // own, picked as "Original": the PC boards would be black).
        let kit = (art == dir && find_file(&dir, "MENUM.TGA").is_none())
            .then(|| kit::Kit::load(&huntdat, &mut hud, gpu))
            .flatten();
        let mut kit_loading = None;
        let mut more_board = Board::default();
        let survive = true;
        let (main, stats_board, quit_board, reg, waiver, delete, options, credits) = match &kit {
            Some(k) => {
                let b = k.boards(&mut hud, gpu, &game_title(&huntdat), survive);
                kit_loading = Some(b.loading);
                more_board = b.more;
                (
                    b.main, b.stats, b.quit, b.reg, b.waiver, b.delete, b.options, b.credits,
                )
            }
            None => (
                main,
                stats_board,
                quit_board,
                reg,
                waiver,
                delete,
                options,
                credits,
            ),
        };
        let mut pic = |name: &str, linear: bool| {
            read_tga(&art, name)
                .or_else(|| read_tga(&dir, name))
                .map(|t| {
                    let rgba = c2_core::color::image_to_rgba8(&t.pixels, false);
                    hud.add_image_filtered(gpu, t.width as u32, t.height as u32, &rgba, linear)
                })
        };
        let slider = pic("SL_BAR.TGA", true);
        let knob = pic("SL_BUT.TGA", true);
        let loading = pic("LOADING.TGA", true);
        // The phone's slider where the game has no PC one.
        let (slider, knob) = match &kit {
            Some(k) if slider.is_none() => (
                k.image_sized("slider_body", 128, 14, &mut hud, gpu),
                k.image_sized("slider_controller", 8, 14, &mut hud, gpu),
            ),
            _ => (slider, knob),
        };

        let pics_dir = find_file(&dir, "PICS");
        let txt = find_file(&dir, "TXT");
        // The hunt lists: `_MENU.TXT` if the game has one (Modder's Edition
        // Engine), else `_RES.TXT`; see `c2_formats::menu_txt`.
        let mut menu = menu_data(&huntdat)?;
        let menu_animals = menu.huntables.len();
        // The phone's hunt setup cuts the pictures of items it has no art
        // for (add-ons') to its cells: kept until that art is made.
        hud.keep_pixels = kit.is_some();
        // The roaming add-ons' pictures (by menu slot): drawn from their
        // models, as they have none.
        let mut drawn: Vec<Option<usize>> = vec![None; menu_animals];
        // A dialect game's roaming add-ons (no menu slot of their own) are
        // picked as its animals are, in the slots after them; priced as the
        // menu animal with their AI (listed with "All dinos huntable" on).
        let dialect = dialect_res(&huntdat);
        if let Some(res) = &dialect {
            let first = menu.huntables.len();
            let roamers = c2_core::mee::roamers(res, first);
            let h = c2_formats::mee::Hunt {
                area: None,
                time: 1,
                picked: Bits::ALL,
            };
            let extras: Vec<_> = roamers.iter().map(|&(i, _)| i).collect();
            let resolved = c2_core::mee::read_picked_anywhere(res, &h, &extras);
            for (i, _) in roamers {
                let b = resolved.tables.characters.get(i).map(|c| &c.block);
                let get = |k: &str| b.and_then(|b| b.get(&[k]));
                let ai = get("ai").map_or(0, c2_formats::res_txt::c_atoi);
                let price = menu.huntables[..first]
                    .iter()
                    .find(|h| h.ai == ai)
                    .map_or(0, |h| h.price);
                drawn.push(
                    resolved
                        .res
                        .characters
                        .get(i)
                        .and_then(|c| c.file.as_deref())
                        .and_then(|f| model_picture(&mut hud, gpu, &huntdat, f)),
                );
                menu.huntables.push(c2_formats::menu_txt::MenuItem {
                    name: get("name")
                        .map_or_else(String::new, |n| n.trim().trim_matches('\'').to_string()),
                    price,
                    ai,
                    pic: None,
                    mass: get("mass").map_or(0.0, c2_formats::res_txt::c_atof),
                    length: get("length").map_or(0.0, c2_formats::res_txt::c_atof),
                });
            }
        }

        // A picture by name in `MENU/PICS`, or by a path from the game
        // folder (`huntdat\\menu\\pics\\x.tga`, as data may give).
        let mut view = |name: String| {
            let t = if name.contains(['/', '\\']) {
                c2_formats::tga::parse(&std::fs::read(find_file(&game_dir, &name)?).ok()?).ok()?
            } else {
                read_tga(pics_dir.as_deref()?, &name)?
            };
            let rgba = c2_core::color::image_to_rgba8(&t.pixels, false);
            Some(hud.add_image_filtered(gpu, t.width as u32, t.height as u32, &rgba, true))
        };
        // Locations: the `prices` areas whose .MAP exists (slot 6 may be
        // `external.map`), then `.c2map` descriptors; without any `prices`
        // areas, AREA1.. as found.
        let areas_dir = find_file(&huntdat, "AREAS");
        let map_of = |stem: &str| find_file(areas_dir.as_deref()?, &format!("{stem}.MAP"));
        if menu.areas.is_empty() {
            for n in 1..=9 {
                if map_of(&format!("area{n}")).is_none() {
                    break;
                }
                menu.areas.push(MenuArea {
                    name: None,
                    project: format!("area{n}"),
                    price: 0,
                    number: Some(n),
                    pic: None,
                    text: None,
                    snow: false,
                });
            }
        }
        if let Some(dir) = areas_dir.as_deref() {
            let mut c2maps: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("c2map"))
                })
                .collect();
            c2maps.sort();
            for p in c2maps {
                if let Some(a) = std::fs::read(&p)
                    .ok()
                    .and_then(|d| parse_c2map(&String::from_utf8_lossy(&d)))
                {
                    menu.add_c2map(a);
                }
            }
        }
        let mut area_list: Vec<(MenuArea, PathBuf)> = Vec::new();
        for a in &menu.areas {
            let map = map_of(&a.project)
                .or_else(|| (a.number == Some(6)).then(|| map_of("external")).flatten());
            if let Some(m) = map {
                area_list.push((a.clone(), m));
            }
        }
        let areas: Vec<PathBuf> = area_list.iter().map(|(_, m)| m.clone()).collect();
        // A path from the data (`huntdat\\menu\\pics\\x.tga`), from the game folder.
        let game_path = |p: &str| find_file(&game_dir, p);
        let mut cols = Vec::new();
        let area_texts: Vec<Vec<String>> = area_list
            .iter()
            .map(|(a, _)| match (&a.text, a.number) {
                (Some(t), _) => game_path(t)
                    .and_then(|p| std::fs::read(p).ok())
                    .map(|d| {
                        String::from_utf8_lossy(&d)
                            .lines()
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                (None, Some(n)) => read_text(txt.as_deref(), &format!("AREA{n}.TXT")),
                (None, None) => Vec::new(),
            })
            .collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (12.0, 376.0, 182.0, 170.0),
            names: area_list
                .iter()
                .zip(&area_texts)
                .enumerate()
                .map(|(i, ((a, _), t))| {
                    a.name
                        .clone()
                        .or_else(|| t.first().filter(|l| !l.is_empty()).cloned())
                        .unwrap_or_else(|| format!("Area {}", i + 1))
                })
                .collect(),
            prices: area_list.iter().map(|(a, _)| a.price).collect(),
            pics: area_list
                .iter()
                .map(|(a, _)| match (&a.pic, a.number) {
                    (Some(p), _) => view(p.clone()),
                    (None, Some(n)) => view(format!("AREA{n}.TGA")),
                    (None, None) => None,
                })
                .collect(),
            texts: area_texts.clone(),
            texts_us: area_texts,
        });
        // Huntables by menu slot (slot n = `DINOn+1.TGA` unless `pic` says).
        let dinos: Vec<String> = menu.huntables.iter().map(|d| d.name.clone()).collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (210.0, 376.0, 184.0, 170.0),
            prices: menu.huntables.iter().map(|d| d.price).collect(),
            pics: menu
                .huntables
                .iter()
                .enumerate()
                .map(|(k, d)| match &d.pic {
                    Some(p) => view(p.clone()),
                    // A roaming add-on has no picture: one drawn from its model.
                    None if k >= menu_animals => drawn.get(k).copied().flatten(),
                    None => view(format!("DINO{}.TGA", k + 1)),
                })
                .collect(),
            texts: dino_texts(&menu.huntables, txt.as_deref(), false),
            texts_us: dino_texts(&menu.huntables, txt.as_deref(), true),
            names: dinos,
        });
        let weapons: Vec<String> = menu.weapons.iter().map(|w| w.name.clone()).collect();
        let wtexts: Vec<Vec<String>> = (1..=weapons.len())
            .map(|n| read_text(txt.as_deref(), &format!("WEAPON{n}.TXT")))
            .collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (410.0, 376.0, 184.0, 170.0),
            prices: menu.weapons.iter().map(|w| w.price).collect(),
            pics: (1..=weapons.len())
                .map(|n| view(format!("WEAPON{n}.TGA")))
                .collect(),
            texts: wtexts.clone(),
            texts_us: wtexts,
            names: weapons,
        });
        let etexts: Vec<Vec<String>> = EQUIPMENT
            .iter()
            .map(|(_, f)| read_text(txt.as_deref(), f))
            .collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (610.0, 376.0, 184.0, 170.0),
            names: EQUIPMENT.iter().map(|(n, _)| n.to_string()).collect(),
            prices: (0..4)
                .map(|i| menu.accessory_prices.get(i).copied().unwrap_or(0))
                .collect(),
            pics: (1..=4).map(|n| view(format!("EQUIP{n}.TGA"))).collect(),
            texts: etexts.clone(),
            texts_us: etexts,
        });
        for (name, price, help) in [
            (
                "Night vision",
                150,
                "Costs 150 points. N toggles night vision. Using it cancels the 20% night score bonus.",
            ),
            (
                "Flashlight",
                15,
                "Costs 15 points. L toggles the flashlight. Night kills keep the 20% bonus. B switches between the wide beam and a narrow one that reaches farther.",
            ),
        ] {
            let gear = &mut cols[3];
            gear.names.push(name.into());
            gear.prices.push(price);
            gear.pics.push(None);
            gear.texts.push(vec![name.into(), help.into()]);
            gear.texts_us.push(vec![name.into(), help.into()]);
        }
        // Slotted add-ons can omit their menu art too. Render a portrait
        // from the same resolved model the hunt loads in that case.
        if let Some(res) = &dialect {
            let h = c2_formats::mee::Hunt {
                area: None,
                time: 1,
                picked: Bits::ALL,
            };
            let resolved = c2_core::mee::read_picked_anywhere(res, &h, &[]);
            if let Some(c) = cols.get_mut(1) {
                for (i, pic) in c.pics.iter_mut().enumerate().filter(|(_, p)| p.is_none()) {
                    let def = resolved.res.characters.iter().find(|d| {
                        d.name
                            .as_deref()
                            .is_some_and(|n| n.eq_ignore_ascii_case(&c.names[i]))
                    });
                    *pic = def
                        .and_then(|d| d.file.as_deref())
                        .and_then(|f| model_picture(&mut hud, gpu, &huntdat, f));
                }
            }
        }
        // Each huntable's points, from its `_RES.TXT` entry as when picked
        // (descriptions differ between games; this is what the hunt scores).
        let scores = hunt_scores(&huntdat, &menu);
        if let Some(c) = cols.get_mut(1) {
            for (k, sc) in scores.iter().enumerate() {
                let Some(sc) = sc else { continue };
                // Whole points, as the hunt adds them.
                let line = format!(
                    "Score when bagged: {} (tranquilized: {}, no trophy)",
                    *sc as i32,
                    (sc * 1.25) as i32
                );
                for t in [c.texts.get_mut(k), c.texts_us.get_mut(k)]
                    .into_iter()
                    .flatten()
                {
                    t.push(String::new());
                    t.push(line.clone());
                }
            }
        }
        // Where each comes from (combined games: an animal's own game's
        // maps). Picked, it comes on any map (`spawn_picked_anywhere`).
        let homes = hunt_homes(&huntdat, &menu, &areas);
        if let (Some(h), Some(c)) = (&homes, cols.get_mut(1)) {
            let area_names: Vec<String> = area_list
                .iter()
                .map(|(a, _)| a.name.clone().unwrap_or_else(|| a.project.clone()))
                .collect();
            for (k, list) in h.iter().enumerate() {
                if list.len() == areas.len() {
                    continue;
                }
                let names: Vec<&str> = list
                    .iter()
                    .filter_map(|&a| area_names.get(a).map(String::as_str))
                    .collect();
                let line = if names.is_empty() {
                    "Spawns on no map by itself.".to_string()
                } else {
                    format!("Native to: {}", names.join(", "))
                };
                for t in [c.texts.get_mut(k), c.texts_us.get_mut(k)]
                    .into_iter()
                    .flatten()
                {
                    t.push(String::new());
                    t.push(line.clone());
                }
            }
        }
        // Rows by price (stable, so equal prices keep their order); the
        // equipment keeps its fixed order.
        for (ci, c) in cols.iter_mut().enumerate() {
            c.order = (0..c.names.len()).collect();
            if ci < 3 {
                c.order
                    .sort_by_key(|&i| c.prices.get(i).copied().unwrap_or(0));
            }
        }
        // Wrap descriptions to the info panel (the phone's pages keep the
        // files' own lines, as the phone does).
        let info_w = 340.0;
        for c in cols.iter_mut().filter(|_| kit.is_none()) {
            for t in c.texts.iter_mut().chain(c.texts_us.iter_mut()) {
                *t = wrap(&hud, 12.0, info_w, t);
            }
        }
        let day_texts = (1..=3)
            .map(|n| {
                wrap(
                    &hud,
                    12.0,
                    info_w,
                    &read_text(txt.as_deref(), &format!("DAY{n}.NFO")),
                )
            })
            .collect();
        let tranq_text = wrap(&hud, 12.0, info_w, &read_text(txt.as_deref(), "TRANQ.NFO"));
        let fog_text = wrap(
            &hud,
            12.0,
            info_w,
            &[
                "Fog".to_string(),
                String::new(),
                "Regardless of render distance, this option locks the view distance to the original game's range, \
                 at any time of day."
                    .to_string(),
            ],
        );
        let observe_text = wrap(
            &hud,
            12.0,
            info_w,
            &read_text(txt.as_deref(), "OBSERVE.NFO"),
        );

        let mut players = save::players(&game_dir);
        // A hunter with fewer points than a new one starts with is lifted to
        // them: points are needed, not spent, and kills only add, so it is a
        // save made under a lower start (or one that could never hunt; under
        // the cheapest hunt alone missed games where some picks cost 0).
        let start = menu.start_points();
        for p in &mut players {
            p.score = p.score.max(start);
        }
        let signed_in = player.and_then(|r| players.iter().position(|p| p.reg_number == r));
        let mobile = (MOBILE_MENU.get().copied().unwrap_or(false) || kit.is_some()).then(|| {
            let mut art = mobile::Art::new(&mut hud, gpu);
            art.kit = kit.as_ref().map(|k| {
                let pics = |i: usize| cols.get(i).map_or(&[][..], |c: &Column| &c.pics[..]);
                k.art(&mut hud, gpu, [pics(0), pics(1), pics(2)])
            });
            art
        });
        hud.drop_pixels();
        let mut m = Self {
            mobile,
            mobile_page: 0,
            comb_page: 0,
            kit,
            record: Default::default(),
            clear_armed: false,
            tranq_ask: false,
            hud,
            game_dir,
            huntdat,
            start_score: menu.start_points(),
            main,
            stats_board,
            quit_board,
            reg,
            waiver,
            delete,
            setup,
            options,
            credits,
            slider,
            knob,
            loading,
            kit_loading,
            columns: cols,
            day_texts,
            tranq_text,
            fog_text,
            pc: PcSettings::load(),
            menu_animals,
            observe_text,
            areas,
            players,
            player: signed_in,
            screen: if signed_in.is_some() {
                Screen::Main
            } else {
                Screen::Register
            },
            stats: false,
            more: false,
            more_board,
            classic_survival,
            survival_pick: false,
            name: String::new(),
            picked: None,
            choice: Choice {
                time: 1,
                ..Default::default()
            },
            binding: None,
            opt_tab: 0,
            game_scroll: option_scroll::Scroll::default(),
            drag: None,
            launch: None,
            shown_loading: false,
            quit: false,
            screen_size: (800, 600),
            cursor: None,
        };
        m.restore_choice();
        m.offer_animals();
        Ok(m)
    }

    /// The animal list by "All dinos huntable": off, the roaming add-ons
    /// aren't listed (or picked); they roam their own maps only.
    pub(super) fn offer_animals(&mut self) {
        // The times of day the instance offers.
        self.choice.time = crate::instance_rules::get().allowed_time(self.choice.time);
        self.columns[3].order = (0..if self.pc.dynamic_time != 0 { 6 } else { 4 }).collect();
        if self.pc.dynamic_time == 0 {
            self.choice.equip &= 15;
        }
        let all = self.pc.all_huntable != 0;
        let first = self.menu_animals;
        if let Some(c) = self.columns.get_mut(1) {
            c.order = animal_order(&c.names, first, all);
            // Animals the instance keeps off the menu (`hide`).
            let rules = crate::instance_rules::get();
            c.order.retain(|&i| !rules.hides(&c.names, i));
            c.order
                .sort_by_key(|&i| c.prices.get(i).copied().unwrap_or(0));
            c.scroll = c.scroll.min(c.order.len().saturating_sub(c.visible()));
            let picked = self.choice.dinos;
            for i in picked.iter() {
                if !c.order.contains(&i) {
                    self.choice.dinos.clear(i);
                }
            }
        }
        if self.choice.focus.is_some_and(|(col, i)| {
            col == 1 && self.columns.get(1).is_none_or(|c| !c.order.contains(&i))
        }) {
            self.choice.focus = None;
        }
        if !all {
            for i in first..Bits::LEN {
                self.choice.dinos.clear(i);
            }
        }
    }
}
