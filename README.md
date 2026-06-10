# rlviser-rocketsim

`rlviser-rocketsim` connects [`RocketSim`](https://github.com/ZealanL/RocketSim) simulations to [`RLViser`](https://github.com/VirxEC/rlviser), allowing a running Rust RocketSim arena to stream game state to the visualizer over UDP.

The crate provides:

- A RocketSim `Vis` implementation that sends arena state updates to RLViser.
- An `ArenaRlviserExt` helper trait for enabling/disabling visualization on an `Arena`.
- FlatBuffers/Planus message encoding and decoding for the RLViser protocol.
- Runnable examples in `examples/watch.rs` and `examples/drive.rs`.

## Requirements

- Rust with Edition 2024 support.
- `rustfmt` available on `PATH`.
  - The build script generates Rust code from FlatBuffers schemas in `spec/` and formats it with `rustfmt`.
- A local RocketSim Rust crate at `../RocketSim/rocketsim`, as configured in `Cargo.toml`.
- RocketSim collision meshes in the runtime working directory when initializing RocketSim.
- A running RLViser instance listening on the default RLViser port.

## Ports

By default, this crate uses localhost UDP communication:

| Constant | Port | Purpose |
| --- | ---: | --- |
| `RLVISER_PORT` | `45243` | RLViser listener port |
| `ROCKETSIM_PORT` | `34254` | Local RocketSim/client socket port |

You can use the defaults with `Rlviser::new()` or provide custom ports with `Rlviser::with_ports(rocketsim_port, rlviser_port)`.

## Basic usage

Add the extension trait, initialize RocketSim, create an arena, then enable RLViser before stepping the simulation.

```rust
use rlviser_rocketsim::ArenaRlviserExt;
use rocketsim::{Arena, ArenaConfig, GameMode, init_from_default};

fn main() -> std::io::Result<()> {
    init_from_default(true)?;

    let mut arena = Arena::new_with_config(ArenaConfig::new(GameMode::Soccar));

    // Creates an Rlviser visualizer and attaches it to the arena.
    arena.set_rlviser_enabled(true)?;

    loop {
        arena.step_tick();
    }
}
```

When attached, the visualizer sends a connection message immediately and streams `GameState` packets from RocketSim to RLViser on every arena visualization update. When dropped, it sends a quit message.

## Running the examples

Start RLViser first, then run one of the examples.

To watch an automated arena, optionally choosing the game mode:

```bash
cargo run --example watch -- [soccar|hoops|dropshot]
```

To drive a single blue Breakout with keyboard/mouse controls:

```bash
cargo run --example drive
```

The `drive` example reads keyboard/mouse input for basic control:

| Input | Action |
| --- | --- |
| `W` / `S` | Throttle forward/backward |
| `A` / `D` | Steer / yaw |
| `Q` / `E` | Roll |
| `Left Shift` | Handbrake |
| Mouse buttons | Jump / boost |
| `Backspace` | Reset arena to kickoff |
| `2` | Move ball above the car for dribbling |
| `4` | Launch the ball upward |

## Pause and speed messages

RLViser can send control messages back to the simulation. `Rlviser` tracks these values internally:

```rust
let paused = rlviser.paused();
let speed = rlviser.speed();
```

The built-in `Vis` implementation skips sending game states while paused. The current speed value is stored but applying it to your simulation loop is up to your code.

> Note: full editor-originated `GameState` updates from RLViser are decoded but not applied by the `Vis` implementation because RocketSim's `Vis` trait only receives immutable arena state.

## Protocol notes

Packets are encoded with Planus from the FlatBuffers schemas in `spec/`.

Each UDP packet is structured as:

1. An 8-byte big-endian unsigned payload length header.
2. A Planus-encoded `Packet` payload.

`PacketCodec` exposes helpers for encoding and decoding messages if you need to integrate with the protocol directly:

```rust
use rlviser_rocketsim::{PacketCodec, RlviserMessage};

let mut codec = PacketCodec::new();
let bytes = codec.encode(RlviserMessage::Connection);
```
