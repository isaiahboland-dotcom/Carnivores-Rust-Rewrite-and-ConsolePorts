//! Synthetic CPU pose comparison; no game files or GPU required.
//! Run: cargo run --release -p c2-core --example pose_bench
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use c2_core::dino::{pose, Character, PMORPH_TIME};
use c2_formats::{
    car::{Animation, Car},
    model::{Mesh, Vertex},
};

fn synthetic_car(vertices: usize) -> Car {
    let animation = |offset| Animation {
        name_raw: [0; 32],
        kps: 12,
        frames: 4,
        data: (0..vertices * 3 * 4)
            .map(|i| ((i * 17 + offset) % 4000) as i16 - 2000)
            .collect(),
    };
    Car {
        name_raw: [0; 32],
        mesh: Mesh {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    owner: 0,
                    hide: 0
                };
                vertices
            ],
            ..Default::default()
        },
        animations: vec![animation(0), animation(300)],
        sounds: vec![],
        anim_sounds: None,
        trailing: vec![],
    }
}

fn run(car: &Car, reuse: bool) -> Duration {
    let mut out = Vec::with_capacity(car.mesh.vertices.len());
    let mut c = Character {
        scale: 1.2,
        beta: 0.2,
        gamma: -0.1,
        bend: 0.3,
        prev_phase: 1,
        pp_morph_time: PMORPH_TIME / 2,
        ..Default::default()
    };
    let start = Instant::now();
    for i in 0..5000 {
        c.ftime = i % 300;
        c.prev_pf_time = (i + 73) % 300;
        if reuse {
            black_box(pose::pose_into(
                black_box(&c),
                black_box(car),
                &[],
                &mut out,
            ));
        } else {
            black_box(pose::pose(black_box(&c), black_box(car), &[]));
        }
    }
    start.elapsed()
}

fn main() {
    println!("Median of five runs, 5000 poses each (CPU only; not FPS):");
    for vertices in [64, 512, 2048] {
        let car = synthetic_car(vertices);
        // Warm both paths, then alternate order to reduce ordering bias.
        black_box(run(&car, false));
        black_box(run(&car, true));
        let mut allocating = Vec::new();
        let mut reused = Vec::new();
        for round in 0..5 {
            for reuse in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let elapsed = run(&car, reuse);
                if reuse {
                    reused.push(elapsed);
                } else {
                    allocating.push(elapsed);
                }
            }
        }
        allocating.sort_unstable();
        reused.sort_unstable();
        println!(
            "{vertices:4} vertices: allocating {:?}, reused {:?}",
            allocating[2], reused[2]
        );
    }
}
