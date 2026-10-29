use c2_core::area::Area;
use c2_core::player::{Controls, Player, PlayerEvent, HEAD_CROUCH};

#[test]
fn scripted_flat_ground_walk_run_jump_and_crouch() {
    let area = Area::flat(10);
    let mut p = Player::new(&area, (500, 500));
    let start_z = p.z;
    let mut events = Vec::new();
    for _ in 0..200 {
        p.tick(
            &area,
            &Controls {
                forward: true,
                ..Default::default()
            },
            10,
            &mut events,
        );
    }
    assert!((p.vspeed - 0.3).abs() < 1e-6);
    assert!((400.0..600.0).contains(&(start_z - p.z)));
    for _ in 0..200 {
        p.tick(
            &area,
            &Controls {
                forward: true,
                run: true,
                ..Default::default()
            },
            10,
            &mut events,
        );
    }
    assert!((p.vspeed - 0.7).abs() < 1e-6);
    events.clear();
    for _ in 0..10 {
        p.tick(
            &area,
            &Controls {
                jump: true,
                ..Default::default()
            },
            10,
            &mut events,
        );
    }
    assert!(events.contains(&PlayerEvent::Jump));
    assert!(p.y > 680.0);
    for _ in 0..200 {
        p.tick(
            &area,
            &Controls {
                crouch: true,
                ..Default::default()
            },
            10,
            &mut events,
        );
    }
    assert!((p.head_y - HEAD_CROUCH).abs() < 1e-4);
    assert!((p.y - 640.0).abs() < 1e-4);
}
