use std::time::{Duration, Instant};

use device_query::{DeviceQuery, DeviceState, Keycode};
use rlviser_rocketsim::ArenaRlviserExt;
use rocketsim::{
    Arena, ArenaConfig, CarBodyConfig, CarControls, GameMode, Team, Vec3A, init_from_default,
};

fn determine_controls(device: &DeviceState) -> CarControls {
    let keys = device.get_keys();
    let mouse_state = device.get_mouse();
    let mut controls = CarControls::default();

    if keys.contains(&Keycode::A) {
        controls.steer -= 1.0;
    }
    if keys.contains(&Keycode::D) {
        controls.steer += 1.0;
    }
    if keys.contains(&Keycode::S) {
        controls.throttle -= 1.0;
    }
    if keys.contains(&Keycode::W) {
        controls.throttle += 1.0;
    }

    if keys.contains(&Keycode::Q) {
        controls.roll -= 1.0;
    }
    if keys.contains(&Keycode::E) {
        controls.roll += 1.0;
    }

    controls.handbrake = keys.contains(&Keycode::LShift);

    controls.jump = mouse_state.button_pressed[1];
    controls.boost = mouse_state.button_pressed[3];

    controls.yaw = controls.steer;
    controls.pitch = -controls.throttle;

    if controls.handbrake {
        controls.roll = controls.yaw;
        controls.yaw = 0.0;
    }

    controls
}

fn main() -> std::io::Result<()> {
    init_from_default(true)?;
    let mut arena = Arena::new_with_config(ArenaConfig {
        rng_seed: Some(0),
        ..ArenaConfig::new(GameMode::Soccar)
    });

    let car_idx = arena.add_car(Team::Blue, CarBodyConfig::BREAKOUT);

    arena.set_rlviser_enabled(true)?;

    println!(
        "Connected to RLViser on port {rlviser_port}",
        rlviser_port = rlviser_rocketsim::RLVISER_PORT
    );
    println!("Controls: WASD steer/throttle, QE roll, Shift handbrake, mouse buttons jump/boost");
    println!("Backspace resets, 2 teleports ball to dribble position, 4 launches ball");

    let device_state = DeviceState::new();
    let mut prev_keys = Vec::new();
    let tick_interval = Duration::from_secs_f64(1.0 / 120.0);
    let mut next_tick = Instant::now();
    loop {
        let held_keys = device_state.get_keys();

        let pressed_keys: Vec<Keycode> = held_keys
            .iter()
            .filter(|&key| !prev_keys.contains(key))
            .copied()
            .collect();

        let controls = determine_controls(&device_state);

        // Reset arena
        if pressed_keys.contains(&Keycode::Backspace) || arena.tick_count() == 0 {
            arena.reset_to_random_kickoff(None);
        }

        if pressed_keys.contains(&Keycode::Key2) {
            // Teleport ball to dribble position
            let car_state = arena.get_car_state(car_idx);
            let mut ball_state = *arena.get_ball_state();
            ball_state.phys.pos = car_state.phys.pos + Vec3A::new(0.0, 0.0, 150.0);
            ball_state.phys.vel = car_state.phys.vel;
            arena.set_ball_state(ball_state);
        } else if pressed_keys.contains(&Keycode::Key4) {
            // Launch ball
            let mut ball_state = *arena.get_ball_state();
            ball_state.phys.vel += Vec3A::new(0.0, 0.0, 1000.0);
            arena.set_ball_state(ball_state);
        }

        arena.set_car_controls(car_idx, controls);

        arena.step_tick();

        next_tick += tick_interval;
        let now = Instant::now();
        if next_tick > now {
            std::thread::sleep(next_tick - now);
        } else {
            // If a tick takes too long, resync instead of permanently running behind.
            next_tick = now;
        }

        prev_keys = held_keys;
    }
}
