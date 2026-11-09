//! An area past the `.MAP`'s 254 kinds of object, loaded through its side
//! files (`AREAn.OBX`, `AREAn.RSX`; docs/EXPANSION.md).

use c2_core::area::{TimeOfDay, NO_OBJECT};
use c2_formats::map::{self, Map, CELLS, HALF_CELLS};
use c2_formats::obx::{self, Obx};
use c2_formats::rsc::{self, Object, ObjectInfo, Rsc, SPRITE_SIZE};
use c2_vfs::{MemBackend, Vfs};

fn object(radius: i32) -> Object {
    Object {
        info: ObjectInfo {
            radius,
            y_lo: 0,
            y_hi: 0,
            line_length: 0,
            line_intensity: 0,
            circle_radius: 0,
            circle_intensity: 0,
            flags: 0,
            grass_radius: 0,
            default_light: 0,
            last_ani_time: 0,
            bound_r: 0.0,
            reserved: [0; 16],
        },
        model: Default::default(),
        sprite: vec![0; SPRITE_SIZE * SPRITE_SIZE],
        animation: None,
    }
}

fn flat_map() -> Map {
    Map {
        height: vec![10; CELLS],
        texture1: vec![0; CELLS],
        texture2: vec![0; CELLS],
        objects: vec![NO_OBJECT; CELLS],
        flags: vec![0; CELLS],
        light: [vec![0; CELLS], vec![0; CELLS], vec![0; CELLS]],
        water: vec![0; CELLS],
        object_height: vec![0; CELLS],
        fogs: vec![0; HALF_CELLS],
        ambient: vec![0; HALF_CELLS],
    }
}

fn rsc_with(objects: Vec<Object>) -> Rsc {
    Rsc {
        fade_rgb: [[0; 3]; 3],
        trans_rgb: [[0; 3]; 3],
        textures: vec![vec![0; 128 * 128]],
        objects,
        skies: [vec![0; 256 * 256], vec![0; 256 * 256], vec![0; 256 * 256]],
        sky_map: vec![0; 128 * 128],
        fogs: vec![],
        random_sounds: vec![],
        ambients: vec![],
        waters: vec![],
    }
}

fn load(side_files: bool) -> c2_core::area::Area {
    let cell = |x: usize, y: usize| y * map::MAP_SIZE + x;
    let mut m = flat_map();
    m.objects[cell(100, 100)] = 0;
    let mut mem = MemBackend::new();
    mem.insert("HUNTDAT/AREAS/AREA1.MAP", map::write(&m));
    mem.insert(
        "HUNTDAT/AREAS/AREA1.RSC",
        rsc::write(&rsc_with(vec![object(1)])),
    );
    if side_files {
        let mut grid = m.objects.clone();
        grid[cell(200, 200)] = 300;
        grid[cell(201, 200)] = 299;
        let mut heights = m.object_height.clone();
        heights[cell(200, 200)] = 5;
        let extra: Vec<Object> = (2..=301).map(object).collect();
        mem.insert(
            "HUNTDAT/AREAS/AREA1.OBX",
            obx::write_obx(&Obx {
                objects: grid,
                object_height: heights,
            }),
        );
        mem.insert("HUNTDAT/AREAS/AREA1.RSX", obx::write_rsx(&extra));
    }
    let mut vfs = Vfs::new();
    vfs.push("game", Box::new(mem));
    c2_core::load::area(&vfs, "AREA1", TimeOfDay::Day).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn side_files_add_models_past_the_map_byte() {
    let a = load(true);
    assert_eq!(a.rsc.objects.len(), 301);
    let mut models: Vec<u16> = a.placements().map(|p| p.model).collect();
    models.sort_unstable();
    assert_eq!(models, vec![0, 299, 300]);
    let p = a.placements().find(|p| p.model == 300).unwrap();
    assert_eq!(p.cell, (200, 200));
    assert_eq!(a.rsc.objects[300].info.radius, 301);
}

#[test]
fn without_side_files_the_map_is_read_as_before() {
    let a = load(false);
    assert_eq!(a.rsc.objects.len(), 1);
    let models: Vec<u16> = a.placements().map(|p| p.model).collect();
    assert_eq!(models, vec![0]);
}
