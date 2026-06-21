use std::{
    io,
    time::{Duration, Instant},
};

use clap::Parser;
use rlviser_rocketsim::{ArenaRlviserExt, RLVISER_PORT, TICK_RATE};
use rocketsim::{
    Arena, ArenaConfig, CarBodyConfig, CarControls, GameMode, Team, init_from_default,
};
fn parse_gamemode(s: &str) -> Result<GameMode, String> {
    match s.to_lowercase().as_str() {
        "soccar" => Ok(GameMode::Soccar),
        "hoops" => Ok(GameMode::Hoops),
        "dropshot" => Ok(GameMode::Dropshot),
        "heatseeker" => Ok(GameMode::Heatseeker),
        "snowday" => Ok(GameMode::Snowday),
        "thevoid" | "the-void" | "void" => Ok(GameMode::TheVoid),
        _ => Err(format!(
            "unknown game mode '{s}' — expected soccar, hoops, dropshot, heatseeker, snowday, or thevoid"
        )),
    }
}

#[derive(Parser)]
struct Args {
    #[arg(short = 'g', default_value = "soccar", value_parser = parse_gamemode)]
    gamemode: GameMode,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let arena_type = args.gamemode;

    init_from_default(true)?;
    let mut arena = setup_arena(arena_type);
    arena.set_rlviser_enabled(true)?;

    println!("Connected to RLViser on port {RLVISER_PORT}");
    println!("Usage: cargo run --example watch -- [-g <soccar|hoops|dropshot>]");

    let tick_interval = Duration::from_secs_f64(1.0 / f64::from(TICK_RATE));
    let mut next_tick = Instant::now();
    loop {
        // Process incoming messages from RLViser (speed, paused, etc.)
        arena.handle_rlviser_messages()?;
        let speed = arena.rlviser_speed();
        let paused = arena.rlviser_paused();

        if arena.is_ball_scored() {
            arena.reset_to_random_kickoff(None);
        }

        if !paused {
            arena.step_tick();
        }

        if paused {
            // Don't advance next_tick when paused, so timing resumes cleanly on unpause
        } else {
            next_tick += Duration::from_secs_f64(tick_interval.as_secs_f64() / speed as f64);
        }

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
