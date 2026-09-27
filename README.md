# rlviser-rocketsim

`rlviser-rocketsim` connects [`RocketSim`](https://github.com/ZealanL/RocketSim) simulations to [`RLViser`](https://github.com/VirxEC/rlviser), allowing a running Rust RocketSim arena to stream game state to the visualizer over UDP.

The crate provides:

- A RocketSim `Vis` implementation that sends arena state updates to RLViser.
- An `ArenaRlviserExt` helper trait for enabling/disabling visualization on an `Arena`.
- FlatBuffers/Planus message encoding and decoding for the RLViser protocol.
- A runnable example in `examples/watch.rs`.

## Requirements

- RocketSim collision meshes in `./collision_meshes/` when calling `init_from_default`.
- A running RLViser instance listening on the RLViser port.

## Ports

By default, this crate uses localhost UDP communication:

| Constant         |    Port | Purpose                            |
| ---------------- | ------: | ---------------------------------- |
| `RLVISER_PORT`   | `45243` | RLViser listener port              |
| `ROCKETSIM_PORT` | `34254` | Local RocketSim/client socket port |

You can use the defaults with `Rlviser::new()` or provide custom ports with `Rlviser::with_ports(rocketsim_port, rlviser_port)`. Through the arena helper, use `set_rlviser_enabled(true)` for the defaults or `set_rlviser_enabled_with_ports(true, rocketsim_port, rlviser_port)` for custom ports (pass `0` as the local port for an OS-assigned ephemeral port).

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
        // Adopt pause/speed/state edits sent back by RLViser.
        arena.handle_rlviser_messages()?;
        if !arena.rlviser_paused() {
            arena.step_tick();
        }
    }
}
```

When attached, the visualizer sends a connection message immediately and streams `GameState` packets from RocketSim to RLViser on every arena visualization update. When dropped, it sends a quit message.

## Running the example

Start RLViser first, then run the automated-arena example, optionally choosing
the game mode with `-g`:

```bash
cargo run --example watch -- [-g <soccar|hoops|dropshot|heatseeker|snowday|thevoid>]
```

The example spawns six cars with simple throttle/steer controls, steps the
arena at `TICK_RATE` (120 Hz), resets to kickoff after goals, and honors the
pause/speed controls sent by RLViser.

## Pause, speed, and state edits

RLViser can send control messages back to the simulation. `Rlviser` tracks the
latest values internally — read them directly or through the arena helper:

```rust
let paused = arena.rlviser_paused();
let speed = arena.rlviser_speed();
```

Neither pausing nor speed is applied for you: skip `step_tick()` while paused,
and scale your own loop timing by `speed` (see `examples/watch.rs` for both).
Ball, car, and boost-pad edits from RLViser are applied by
`handle_rlviser_messages`. Dropshot tile edits are currently decoded but not
applied.

## Protocol notes

Packets are encoded with Planus from the FlatBuffers schemas in `spec/`.

Each UDP packet is structured as:

1. An 8-byte big-endian unsigned payload length header (`PACKET_SIZE_BYTES`).
2. A Planus-encoded `Packet` payload.

`PacketCodec` exposes helpers for encoding and decoding messages if you need to integrate with the protocol directly:

```rust
use rlviser_rocketsim::{PacketCodec, RlviserMessage};

let mut codec = PacketCodec::new();
let bytes = codec.encode(RlviserMessage::Connection).to_vec();
```
