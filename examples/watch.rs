use std::{io, time::Instant};

use rlviser_rocketsim::{ArenaRlviserExt, RLVISER_PORT, TICK_RATE};
use rocketsim::{
    Arena, ArenaConfig, BallState, CarBodyConfig, CarControls, GameMode, PhysState, Team, Vec3A,
    init_from_default,
};

fn main() -> io::Result<()> {
    init_from_default(true)?;

    let mut args = std::env::args();
    let _ = args.next();
    let arena_type = match args.next().as_deref() {
        Some("hoops") => GameMode::Hoops,
        Some("dropshot") => GameMode::Dropshot,
        _ => GameMode::Soccar,
    };

    let mut arena = setup_arena(arena_type);
    arena.set_rlviser_enabled(true)?;

    println!("Connected to RLViser on port {RLVISER_PORT}");
    println!("Usage: cargo run --example watch -- [soccar|hoops|dropshot]");

    let tick_interval = std::time::Duration::from_secs_f64(1.0 / f64::from(TICK_RATE));
    let mut next_tick = Instant::now();
    loop {
        if arena.is_ball_scored() {
            arena.reset_to_random_kickoff(None);
        }

        arena.step_tick();

        next_tick += tick_interval;
        let now = Instant::now();
        if next_tick > now {
            std::thread::sleep(next_tick - now);
        } else {
            // If a tick takes too long, resync instead of permanently running behind.
            next_tick = now;
        }
    }
}

fn setup_arena(arena_type: GameMode) -> Arena {
    let mut arena = Arena::new_with_config(ArenaConfig {
        rng_seed: Some(0),
        ..ArenaConfig::new(arena_type)
    });

    let car_idxs = [
        arena.add_car(Team::Blue, CarBodyConfig::OCTANE),
        arena.add_car(Team::Blue, CarBodyConfig::DOMINUS),
        arena.add_car(Team::Blue, CarBodyConfig::MERC),
        arena.add_car(Team::Orange, CarBodyConfig::BREAKOUT),
        arena.add_car(Team::Orange, CarBodyConfig::HYBRID),
        arena.add_car(Team::Orange, CarBodyConfig::PLANK),
    ];

    let mut ball_state = BallState::default();
    ball_state.phys = PhysState {
        pos: Vec3A::new(3236.619, 4695.641, 789.734),
        rot_mat: ball_state.phys.rot_mat,
        vel: Vec3A::new(742.26917, 1717.2388, -1419.7668),
        ang_vel: Vec3A::new(-0.2784555, 2.6806574, 0.9157419),
    };
    arena.set_ball_state(ball_state);

    for car_idx in car_idxs {
        arena.set_car_controls(
            car_idx,
            CarControls {
                steer: 0.2,
                throttle: 1.0,
                pitch: -0.1,
                boost: true,
                ..Default::default()
            },
        );
    }

    arena
}
