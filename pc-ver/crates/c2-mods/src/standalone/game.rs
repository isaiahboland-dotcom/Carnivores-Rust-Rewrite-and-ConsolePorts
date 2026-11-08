//! One standalone game added to the combined game ([`super::build`] calls
//! [`Combined::add`] for each, in order): its numbers moved after what is
//! there, its files placed, its `_RES.TXT` and menu entries appended.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use c2_formats::car::Car;
use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::{c_atoi, Block, Entry, ResTxt, Sub};
use c2_vfs::key;

use super::{
    area_number, blocks, fail, find_ci, game_hunter, prefix_files, read_res, renumber, section_mut,
    set, skip, table_subs, tie, ties, to_dialect, unquote, walk, weapon_file, weapon_files, Moves,
    Options, Result, Skip,
};
use crate::ModError;

/// One standalone game, read.
struct Game {
    slug: String,
    name: String,
    root: PathBuf,
    res: ResTxt,
    /// Its menu file, or its `_RES.TXT` when it has none.
    menu: ResTxt,
}

/// The combined game as it is built: the base, then each game after it.
pub(super) struct Combined<'a> {
    pub o: &'a Options,
    pub report: String,
    pub res: ResTxt,
    /// The base's `_RES.TXT` as read: donors for original-engine games.
    pub base_res: ResTxt,
    pub menu: ResTxt,
    /// Character names in use, lowercase.
    pub names: HashSet<String>,
    pub n_dinos: usize,
    pub n_weapons: usize,
    pub n_areas: usize,
    /// The hunter model all kills are merged into.
    pub hunter: Car,
    /// The games' files: where each goes, where it is.
    pub files: Vec<(String, PathBuf)>,
    /// Weapons so far, by lowercase name and model data (to share them).
    pub weapon_cars: Vec<(String, Vec<u8>)>,
    pub slugs: HashSet<String>,
}

/// How a game's numbers move to the combined game's.
struct Numbers {
    /// Areas and huntables before this game.
    a0: usize,
    d0: usize,
    /// Its kept areas (from 1) and huntable slots (from 0), in order.
    kept_areas: Vec<u32>,
    kept_dinos: Vec<u32>,
    /// Its weapons' combined slots (from 0); `None` when left out.
    weapon_slot: Vec<Option<usize>>,
}

impl Numbers {
    fn area(&self, n: u32) -> Option<u32> {
        self.kept_areas
            .iter()
            .position(|&k| k == n)
            .map(|i| self.a0 as u32 + i as u32 + 1)
    }

    fn slot(&self, n: u32) -> Option<u32> {
        self.kept_dinos
            .iter()
            .position(|&k| k == n)
            .map(|i| self.d0 as u32 + i as u32)
    }

    /// An `AREAn` file's number.
    fn file_area(&self, n: usize) -> Option<usize> {
        self.area(n as u32).map(|g| g as usize)
    }

    /// A `DINOn` / `CALLn` file's number (from 1, as slot + 1).
    fn file_slot(&self, n: usize) -> Option<usize> {
        u32::try_from(n)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| self.slot(n))
            .map(|g| g as usize + 1)
    }

    /// A `WEAPONn` file's number (from 1).
    fn weapon(&self, n: usize) -> Option<usize> {
        n.checked_sub(1)
            .and_then(|i| self.weapon_slot.get(i).copied().flatten())
            .map(|j| j + 1)
    }
}

/// What of a game is left out, in its own numbers.
struct Gone {
    areas: Vec<u32>,
    /// Huntable slots, from 0.
    slots: Vec<u32>,
    /// Huntable names, lowercase.
    names: HashSet<String>,
}

impl Combined<'_> {
    /// Adds the standalone game in `dir`, without what `skip` leaves out.
    pub fn add(&mut self, dir: &Path, skip: &Skip) -> Result<()> {
        let g = self.read(dir)?;
        let gm = MenuData::from_res(&g.menu);
        let (d0, w0, a0) = (self.n_dinos, self.n_weapons, self.n_areas);
        // Areas its menu lists that it has maps for (a menu may list more).
        let has_map = |n: usize| {
            find_ci(&g.root, "AREAS")
                .is_some_and(|d| find_ci(&d, &format!("AREA{n}.MAP")).is_some())
        };
        let g_areas = (1..=gm.areas.len()).take_while(|&n| has_map(n)).count();
        if g_areas < gm.areas.len() {
            self.report.push_str(&format!(
                "- **{}**: its menu lists {} areas, it has maps for {}: those only\n",
                g.name,
                gm.areas.len(),
                g_areas
            ));
        }
        // What is kept (`Skip`) gets the next numbers, with no gaps. Slots
        // count from 0, the picker's numbers from 1.
        let mut num = Numbers {
            a0,
            d0,
            kept_areas: Skip::kept(&skip.areas, g_areas),
            kept_dinos: Skip::kept(&skip.huntables, gm.huntables.len())
                .into_iter()
                .map(|n| n - 1)
                .collect(),
            weapon_slot: Vec::new(),
        };
        let gone_slots: Vec<u32> = skip
            .huntables
            .iter()
            .filter_map(|n| n.checked_sub(1))
            .collect();
        let gone = Gone {
            areas: skip.areas.clone(),
            names: gm
                .huntables
                .iter()
                .enumerate()
                .filter(|(i, _)| gone_slots.contains(&(*i as u32)))
                .map(|(_, h)| h.name.to_ascii_lowercase())
                .collect(),
            slots: gone_slots,
        };

        let (new_weapons, shared) = self.weapons(&g, skip, &mut num);
        self.place_files(&g, &num, w0);
        let (added, renames) = self.add_res(&g, &num, &gone, &new_weapons);
        self.add_menu(&g, &gm, &num, &gone, &new_weapons, &renames, g_areas);
        self.n_dinos += num.kept_dinos.len();
        self.n_areas += num.kept_areas.len();
        if !skip.is_empty() {
            self.report.push_str(&format!(
                "- **{}**: left out (as picked): {}\n",
                g.name,
                skip.to_arg()
            ));
        }
        self.report.push_str(&format!(
            "- **{}**: areas {}–{}, huntables {}–{}, {} new weapons ({} shared), {} characters ({} renamed: same name as one already in), files under `MODS/{}/`\n",
            g.name,
            a0 + 1,
            self.n_areas,
            d0 + 1,
            self.n_dinos,
            new_weapons.len(),
            shared,
            added,
            renames.len(),
            g.slug
        ));
        if w0 + new_weapons.len() != self.n_weapons {
            return Err(fail("weapon numbering went wrong"));
        }
        Ok(())
    }

    /// Reads the game in `dir`, named and given a slug of its own; one made
    /// for the original engine is read as a dialect game.
    fn read(&mut self, dir: &Path) -> Result<Game> {
        let root = crate::pack::find_game_root(dir)
            .and_then(|r| find_ci(&r, "HUNTDAT"))
            .ok_or_else(|| ModError::NoHuntdat(dir.to_path_buf()))?;
        // Named after the first telling folder up its path (not `Game`).
        let name = root
            .ancestors()
            .skip(1)
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .find(|n| {
                let s = crate::manifest::slug(n);
                !matches!(s.as_str(), "game" | "huntdat" | "carnivores-2")
            })
            .unwrap_or_else(|| "mod".into());
        let mut slug = crate::manifest::slug(&name);
        while !self.slugs.insert(slug.clone()) {
            slug.push('2');
        }
        // Apply the same engine compatibility rules as when this game is
        // played alone (including original Triassic's Ice Age AI layouts).
        let game_vfs = crate::pack::vanilla(&[dir.to_path_buf()])?;
        let data = game_vfs.read("HUNTDAT/_RES.TXT")?;
        let mut g_res = ResTxt::parse(&String::from_utf8_lossy(&data))?;
        // Its menu as its own engine shows it. Converting moves the
        // original's AI 6 animal (the brachiosaur's slot) to the base's
        // brachiosaur AI, 30 or 34, which would count it as a huntable: one
        // slot more at the front, and every name off by one against its
        // picture (`DINOn.TGA`) and price.
        let own_menu = g_res.clone();
        // Made for the original engine (or another without spawn tables:
        // nothing of it would be placed).
        let spawns = g_res
            .sections
            .iter()
            .any(|s| s.name.to_ascii_lowercase().contains("spawntable"));
        if !c2_formats::mee::is_dialect(&g_res) || !spawns {
            let groot = root.clone();
            let model = move |f: &str| {
                crate::pack::find_path(&groot, f)
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|d| c2_formats::car::parse(&d).ok())
            };
            let (converted, notes) = to_dialect(&g_res, &self.base_res, &model);
            g_res = converted;
            self.report.push_str(&format!(
                "- **{name}**: made for the original engine; its animals take the base's same-AI animals' animations and spawns{}\n",
                if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) }
            ));
        }
        Ok(Game {
            slug,
            name,
            menu: match find_ci(&root, "_MENU.TXT") {
                Some(p) => read_res(&p)?,
                None => own_menu,
            },
            res: g_res,
            root,
        })
    }

    /// The game's hunter's kill animations merged into the shared model:
    /// the map of its animation numbers. A hunter with another body is kept
    /// as its own (returned), a second AI 0 character its animals name
    /// (`hunterFile`), its kills unmerged.
    fn merge_hunter(&mut self, g: &Game) -> (Vec<usize>, Option<Block>) {
        let g_hunter = game_hunter(&g.res)
            .and_then(|b| b.get(&["file"]))
            .map(unquote)
            .and_then(|f| crate::pack::find_path(&g.root, &f))
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|d| c2_formats::car::parse(&d).ok());
        let Some(h) = g_hunter else {
            return (Vec::new(), None);
        };
        match crate::hunter::merge(&mut self.hunter, std::slice::from_ref(&h)).remove(0) {
            crate::hunter::Outcome::Merged { map } => (map, None),
            crate::hunter::Outcome::Incompatible(why) => {
                self.report.push_str(&format!(
                    "- **{}**: its hunter has another body ({why}); kept as its own, for its animals' kills\n",
                    g.name
                ));
                let own = game_hunter(&g.res).cloned().map(|mut b| {
                    prefix_files(&mut b, &g.slug);
                    set(&mut b, "ai", "0".into());
                    set(&mut b, "name", format!("'Hunter ({})'", g.name));
                    b
                });
                (Vec::new(), own)
            }
        }
    }

    /// The game's weapons' slots (into `num`): shared when the game already
    /// has the same one. Returns the new ones (with their index in the
    /// game) and how many were shared.
    fn weapons(
        &mut self,
        g: &Game,
        skip: &Skip,
        num: &mut Numbers,
    ) -> (Vec<(usize, Block)>, usize) {
        let mut new_weapons = Vec::new();
        let mut shared = 0;
        for (i, b) in blocks(&g.res, "weapons").iter().enumerate() {
            if skip.weapons.contains(&(i as u32 + 1)) {
                num.weapon_slot.push(None);
                continue;
            }
            let wname = b.get(&["name"]).map(unquote).unwrap_or_default();
            let file = b.get(&["file"]).map(unquote).unwrap_or_default();
            let data = crate::pack::find_path(&g.root, &format!("WEAPONS/{file}"))
                .and_then(|p| std::fs::read(p).ok())
                .unwrap_or_default();
            if let Some(j) = self
                .weapon_cars
                .iter()
                .position(|(n, d)| *n == wname.to_ascii_lowercase() && *d == data)
            {
                num.weapon_slot.push(Some(j));
                shared += 1;
                continue;
            }
            self.weapon_cars.push((wname.to_ascii_lowercase(), data));
            self.n_weapons += 1;
            num.weapon_slot.push(Some(self.n_weapons - 1));
            new_weapons.push((i, b.clone()));
        }
        (new_weapons, shared)
    }

    /// Where each of the game's files goes in the combined game (`w0`: the
    /// weapons before it).
    fn place_files(&mut self, g: &Game, num: &Numbers, w0: usize) {
        let mut all = Vec::new();
        walk(&g.root, &mut all);
        for p in all {
            let Ok(rel) = p.strip_prefix(&g.root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let k = key(&format!("HUNTDAT/{rel}"));
            if !crate::detect::is_data(&k) || k.ends_with("/_res.txt") || k.ends_with("/_menu.txt")
            {
                continue;
            }
            let fname = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            let dest = if let Some(sub) = k.strip_prefix("huntdat/areas/") {
                // The engine reads `AREAS/` itself; a folder in it is the
                // maker's spare copies (another version of a map).
                let ext = fname.rsplit('.').next().unwrap_or("").to_ascii_uppercase();
                renumber(&fname, "area", &|n| num.file_area(n))
                    .filter(|_| (ext == "MAP" || ext == "RSC") && !sub.contains('/'))
                    .and_then(|_| {
                        let g = area_number(&fname).and_then(|n| num.area(n))?;
                        Some(format!("HUNTDAT/AREAS/AREA{g}.{ext}"))
                    })
            } else if k.starts_with("huntdat/menu/") {
                let dir = rel.rsplit_once('/').map_or("MENU", |(d, _)| d);
                let to = |n: usize| num.file_slot(n);
                renumber(&fname, "dino", &to)
                    .or_else(|| renumber(&fname, "call", &to))
                    .or_else(|| renumber(&fname, "weapon", &|n| num.weapon(n).filter(|&j| j > w0)))
                    .or_else(|| renumber(&fname, "area", &|n| num.file_area(n)))
                    .map(|f| format!("HUNTDAT/{dir}/{f}"))
            } else if k.starts_with("huntdat/soundfx/") {
                renumber(&fname, "call", &|n| num.file_slot(n))
                    .filter(|_| k.contains("/calls/"))
                    .map(|f| format!("HUNTDAT/SOUNDFX/CALLS/{f}"))
            } else if k.starts_with("huntdat/weapons/") {
                // Flattened (`MODELS/x.car` -> `<mod>_MODELS_x.car`), as the
                // entries are (`weapon_file`).
                let under = &rel[rel.find('/').map_or(0, |i| i + 1)..];
                Some(format!("HUNTDAT/WEAPONS/{}", weapon_file(&g.slug, under)))
            } else {
                Some(format!("HUNTDAT/MODS/{}/{rel}", g.slug))
            };
            if let Some(d) = dest {
                self.files.push((d, p.clone()));
            }
        }
    }

    /// The game's `_RES.TXT` appended: its spawn and pack tables, its
    /// characters (renumbered, tied to its maps), its new weapons. Returns
    /// how many characters were added and the renames (lowercase old name
    /// to new name).
    fn add_res(
        &mut self,
        g: &Game,
        num: &Numbers,
        gone: &Gone,
        new_weapons: &[(usize, Block)],
    ) -> (usize, HashMap<String, String>) {
        let (hunter_map, own_hunter) = self.merge_hunter(g);
        let hunter_file = own_hunter
            .as_ref()
            .and_then(|b| b.get(&["file"]).map(unquote));
        let my_areas: Vec<u32> = (1..=num.kept_areas.len() as u32)
            .map(|n| num.a0 as u32 + n)
            .collect();
        let moves = Moves {
            area: &|n| num.area(n),
            slot: &|n| num.slot(n),
            spawn_offset: table_subs(&self.res, "spawntable", "spawngroup").len(),
            pack_offset: table_subs(&self.res, "packtable", "packgroup").len(),
            hunter: &hunter_map,
        };
        for (table, sub) in [("spawntable", "spawngroup"), ("packtable", "packgroup")] {
            let mut add: Vec<Sub> = table_subs(&g.res, table, sub)
                .into_iter()
                .cloned()
                .collect();
            for s in &mut add {
                skip::drop_refs(&mut s.block, &gone.areas, &gone.slots);
                moves.block(&mut s.block);
            }
            let sec = section_mut(&mut self.res, table);
            let at = sec.entries.len();
            sec.subs.extend(add.into_iter().map(|mut s| {
                s.at = at;
                s
            }));
        }
        // Names already in the game get the game's name after them.
        let mut renames: HashMap<String, String> = HashMap::new();
        let mut added = 0;
        for b in blocks(&g.res, "characters")
            .iter()
            .filter(|b| b.get(&["ai"]).map(c_atoi).unwrap_or(0) != 0)
        {
            let mut b = b.clone();
            let left_out = b
                .get(&["name"])
                .is_some_and(|n| gone.names.contains(&unquote(n).to_ascii_lowercase()));
            if left_out {
                continue;
            }
            skip::drop_refs(&mut b, &gone.areas, &gone.slots);
            if let Some(n) = b.get(&["name"]).map(unquote) {
                let mut u = n.clone();
                if self.names.contains(&u.to_ascii_lowercase()) {
                    u = format!("{n} ({})", g.name);
                    renames.insert(n.to_ascii_lowercase(), u.clone());
                }
                self.names.insert(u.to_ascii_lowercase());
                set(&mut b, "name", format!("'{u}'"));
            }
            // Its models (also those its conditions switch to per area).
            prefix_files(&mut b, &g.slug);
            moves.block(&mut b);
            if ties(self.o, &b) {
                tie(&mut b, &my_areas);
            }
            if let Some(f) = &hunter_file {
                set(&mut b, "hunterFile", format!("'{f}'"));
            }
            section_mut(&mut self.res, "characters").blocks.push(b);
            added += 1;
        }
        if let Some(b) = own_hunter {
            section_mut(&mut self.res, "characters").blocks.push(b);
        }
        for (_, b) in new_weapons {
            let mut b = b.clone();
            weapon_files(&mut b, &g.slug);
            section_mut(&mut self.res, "weapons").blocks.push(b);
        }
        (added, renames)
    }

    /// The game's menu entries appended: huntables, new weapons, its kept
    /// areas' prices.
    #[allow(clippy::too_many_arguments)]
    fn add_menu(
        &mut self,
        g: &Game,
        gm: &MenuData,
        num: &Numbers,
        gone: &Gone,
        new_weapons: &[(usize, Block)],
        renames: &HashMap<String, String>,
        g_areas: usize,
    ) {
        let menu_dinos: Vec<&Block> = blocks(&g.menu, "characters")
            .iter()
            .filter(|b| b.get(&["ai"]).map(c_atoi).is_some_and(|a| a >= 10))
            .collect();
        let mut prices: Vec<Entry> = Vec::new();
        for (i, b) in menu_dinos.iter().enumerate() {
            if gone.slots.contains(&(i as u32)) {
                continue;
            }
            let mut b = (*b).clone();
            // The name its _RES entry got.
            if let Some(n) = b.get(&["name"]).map(unquote) {
                if let Some(u) = renames.get(&n.to_ascii_lowercase()) {
                    set(&mut b, "name", format!("'{u}'"));
                }
            }
            section_mut(&mut self.menu, "characters").blocks.push(b);
            prices.push(Entry {
                key: "dino".into(),
                value: gm.huntables.get(i).map_or(0, |h| h.price).to_string(),
            });
        }
        for (i, b) in new_weapons {
            let keep = ["name", "power", "prec", "loud", "rate"];
            let entry = Block {
                entries: b
                    .entries
                    .iter()
                    .filter(|e| keep.iter().any(|k| e.key.eq_ignore_ascii_case(k)))
                    .cloned()
                    .collect(),
                ..Default::default()
            };
            section_mut(&mut self.menu, "weapons").blocks.push(entry);
            prices.push(Entry {
                key: "weapon".into(),
                value: gm.weapons.get(*i).map_or(0, |w| w.price).to_string(),
            });
        }
        for (_, a) in gm
            .areas
            .iter()
            .take(g_areas)
            .enumerate()
            .filter(|(i, _)| num.kept_areas.contains(&(*i as u32 + 1)))
        {
            prices.push(Entry {
                key: "area".into(),
                value: a.price.to_string(),
            });
        }
        section_mut(&mut self.menu, "prices").entries.extend(prices);
    }
}
