//! Variant members from a standalone game combined beside the base: owner
//! and non-owner members, a member left out of the combine, and maps named
//! by the standalone's own area stems. Built as `c2-pc recipe build` does.
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use c2_formats::menu_txt::MenuData;
use c2_formats::res_txt::ResTxt;
use c2_mods::recipe::Recipe;
use c2_mods::standalone::{add_on_sources, build, Ambients, Options, Skip};
use c2_mods::variants::{Packed, PATH};

/// A one-vertex model with one animation (the hunter is parsed).
fn model() -> Vec<u8> {
    use c2_formats::car::{self, Animation, Car};
    use c2_formats::model::{Mesh, Vertex};
    let mut name_raw = [0u8; 32];
    name_raw[..5].copy_from_slice(b"stand");
    car::write(&Car {
        name_raw: [0; 32],
        mesh: Mesh {
            vertices: vec![Vertex {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                owner: 0,
                hide: 0,
            }],
            faces: vec![],
            texture: vec![],
        },
        animations: vec![Animation {
            name_raw,
            kps: 2,
            frames: 1,
            data: vec![0; 3],
        }],
        sounds: vec![],
        anim_sounds: None,
        trailing: Vec::new(),
    })
}

fn write(root: &Path, files: &[(&str, &[u8])]) {
    for (p, d) in files {
        let p = root.join(p);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(p, d).expect("write");
    }
}

const BASE_RES: &str = "characters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n\
{\n name = 'Raptor'\n file = 'velo.car'\n ai = 15\n callNo = 0\n spawninfo {\n  spawngroup = 0\n }\n}\n}\n\
spawntable {\n spawngroup {\n  spawnmax = 4\n }\n}\n\
prices {\n start = 100\n area = 10\n dino = 20\n}\n.\n";

const BASE_MENU: &str =
    "characters {\n{\n name = 'Raptor'\n ai = 15\n}\n}\nprices {\n start = 100\n area = 10\n dino = 20\n}\n.\n";

/// Two maps; a Raptor named like the base's (renamed when combined) and a
/// Spotted raptor whose fear call names the Raptor's slot.
const GAME_RES: &str = "characters {\n{\n name = 'Hunter'\n file = 'hunter.car'\n ai = 0\n}\n\
{\n name = 'Raptor'\n file = 'raptor.car'\n ai = 15\n callNo = 0\n\
 overwrite area1 {\n  file = 'raptor_snow.car'\n }\n spawninfo {\n  spawngroup = 0\n }\n}\n\
{\n name = 'Spotted'\n file = 'spotted.car'\n ai = 15\n callNo = 1\n spawninfo {\n  spawngroup = 0\n }\n}\n}\n\
spawntable {\n spawngroup {\n  spawnmax = 2\n }\n}\n\
prices {\n start = 100\n area = 30\n area = 35\n dino = 40\n dino = 45\n}\n.\n";

const VARIANTS: &str = r#"
[recipe]
id = "standalone-variants"

[[standalones]]
id = "game-a"

[[variants]]
id = "raptor"
menu_owner = "classic"

[[variants.members]]
id = "classic"
source = "base"
character = "Raptor"
weight = 50
map_weights = [{ map = { source = "game-a", area = "area2" }, weight = 0 }]

[[variants.members]]
id = "spotted"
source = "game-a"
character = "Spotted"
weight = 50
map_weights = [{ map = { source = "game-a", area = "area2" }, weight = 100 }]

[[variants]]
id = "game-raptor"
menu_owner = "own"

[[variants.members]]
id = "own"
source = "game-a"
character = "Raptor"
weight = 100
"#;

struct Fixture {
    root: PathBuf,
    base: PathBuf,
    game: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "c2-standalone-variants-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        write(
            &base,
            &[
                ("HUNTDAT/_RES.TXT", BASE_RES.as_bytes()),
                ("HUNTDAT/_MENU.TXT", BASE_MENU.as_bytes()),
                ("HUNTDAT/hunter.car", &model()),
                ("HUNTDAT/velo.car", b"base raptor"),
                ("HUNTDAT/AREAS/AREA1.MAP", b"base map"),
                ("HUNTDAT/MENU/DINO1.TGA", b"base raptor pic"),
            ],
        );
        let game = root.join("Game A");
        write(
            &game,
            &[
                ("HUNTDAT/_RES.TXT", GAME_RES.as_bytes()),
                ("HUNTDAT/raptor.car", b"game raptor"),
                ("HUNTDAT/raptor_snow.car", b"game raptor snow"),
                ("HUNTDAT/spotted.car", b"spotted"),
                ("HUNTDAT/AREAS/AREA1.MAP", b"a map 1"),
                ("HUNTDAT/AREAS/AREA2.MAP", b"a map 2"),
                ("HUNTDAT/MENU/DINO1.TGA", b"game raptor pic"),
                ("HUNTDAT/MENU/DINO2.TGA", b"spotted pic"),
            ],
        );
        Fixture { root, base, game }
    }

    /// The recipe's build with standalones (`mods_cli::build_recipe`).
    fn build(&self, r: &Recipe) -> Result<c2_mods::Pack, c2_mods::ModError> {
        let ids: Vec<String> = r.standalones.iter().map(|m| m.id.clone()).collect();
        let o = Options {
            ambients: Ambients::Tied,
            skip: r.standalones.iter().map(|m| m.skip.clone()).collect(),
            unlisted: c2_mods::variants::unlisted(&r.variants, &ids),
        };
        let games_dir = self.root.join("out.games");
        let built = build(
            std::slice::from_ref(&self.base),
            std::slice::from_ref(&self.game),
            &games_dir,
            &o,
        )?;
        let v = c2_mods::vanilla(std::slice::from_ref(&games_dir))?;
        let (sources, mut opts) = c2_mods::recipe::prepare(&self.root, r, &[], &v)?;
        opts.residents = built.residents(&ids);
        add_on_sources(&games_dir, sources, &built, &o, &opts)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn read_res(pack: &c2_mods::Pack, path: &str) -> ResTxt {
    ResTxt::parse(&String::from_utf8(pack.vfs.read(path).expect("read")).expect("utf8"))
        .expect("parse")
}

#[test]
fn standalone_members_share_menu_entries_and_their_maps_move() {
    let f = Fixture::new("ok");
    let r = Recipe::parse(VARIANTS).expect("recipe");
    let pack = f.build(&r).expect("build");
    let data: Packed =
        toml::from_str(&String::from_utf8(pack.vfs.read(PATH).expect("side file")).expect("utf8"))
            .expect("packed rules");

    // The Spotted raptor left the menu; the game's Raptor kept its entry
    // under its combined name, the base's Raptor its own.
    let menu = MenuData::from_res(&read_res(&pack, "HUNTDAT/_MENU.TXT"));
    let names: Vec<&str> = menu.huntables.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Raptor", "Raptor (Game A)"]);
    // Its picture went with it: the game's Raptor's is the second slot's.
    assert_eq!(
        pack.vfs.read("HUNTDAT/MENU/DINO2.TGA").expect("pic"),
        b"game raptor pic"
    );
    assert!(pack.vfs.resolve("HUNTDAT/MENU/DINO3.TGA").is_none());

    let res = &pack.merged.res;
    let chars = &res.section("characters").expect("characters").blocks;
    let raptor = &data.groups[0];
    assert_eq!(raptor.slot, 0);
    let models: Vec<&[u8]> = [b"base raptor".as_slice(), b"spotted".as_slice()].to_vec();
    for (m, model) in raptor.members.iter().zip(models) {
        assert_eq!(
            pack.vfs
                .read(&format!("HUNTDAT/{}", m.file))
                .expect("model"),
            model
        );
        assert!(m.file.starts_with("VARIANTS/raptor/"), "{}", m.file);
        assert_eq!(chars[m.index].get(&["menuSlot"]), Some("0"));
        assert_eq!(chars[m.index].get(&["callNo"]), Some("0"));
    }
    assert_eq!(raptor.members[1].name, "Spotted");
    // The game's area2 is the combined game's third map.
    let spotted = &raptor.rule.members[1];
    assert_eq!(spotted.map_weights[0].map.source, "base");
    assert_eq!(spotted.map_weights[0].map.area, "area3");

    let own = &data.groups[1];
    assert_eq!(own.slot, 1);
    let block = &chars[own.members[0].index];
    assert_eq!(own.members[0].name, "Raptor (Game A)");
    assert_eq!(block.get(&["menuSlot"]), Some("1"));
    // Its per-area model moved to the private namespace too.
    let snow = block
        .subs
        .iter()
        .find_map(|s| s.block.get(&["file"]))
        .expect("area model")
        .trim_matches('\'')
        .to_string();
    assert!(snow.starts_with("VARIANTS/game-raptor/own/"), "{snow}");
    assert_eq!(
        pack.vfs.read(&format!("HUNTDAT/{snow}")).expect("snow"),
        b"game raptor snow"
    );
}

#[test]
fn left_out_members_maps_and_base_non_owners_are_refused() {
    let f = Fixture::new("refused");
    let r = Recipe::parse(VARIANTS).expect("recipe");

    let mut left_out = r.clone();
    left_out.standalones[0].skip = Skip {
        huntables: vec![2],
        ..Default::default()
    };
    let e = f.build(&left_out).err().expect("left-out member");
    assert!(e.to_string().contains("left out"), "{e}");

    let mut no_map = r.clone();
    no_map.standalones[0].skip = Skip {
        areas: vec![2],
        ..Default::default()
    };
    let e = f.build(&no_map).err().expect("left-out map");
    assert!(e.to_string().contains("game-a:area2"), "{e}");

    let mut base_member = r.clone();
    base_member.variants[0].menu_owner = "spotted".into();
    let e = f.build(&base_member).err().expect("base non-owner");
    assert!(e.to_string().contains("menu animal"), "{e}");
}
