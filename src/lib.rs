//! Stream a RocketSim arena to RLViser over UDP.
//!
//! This crate connects `rocketsim` simulations to
//! [RLViser](https://github.com/VirxEC/rlviser): attach an [`Rlviser`] to an
//! arena and every simulation snapshot is forwarded to the visualizer, which
//! in turn can pause the sim, change its speed, or push edited game state back.
//!
//! # Quick start
//!
//! ```no_run
//! use rlviser_rocketsim::ArenaRlviserExt;
//! use rocketsim::{Arena, ArenaConfig, GameMode, init_from_default};
//!
//! // Load collision meshes from `./collision_meshes/`.
//! init_from_default(true).unwrap();
//!
//! let mut arena = Arena::new_with_config(ArenaConfig::new(GameMode::Soccar));
//! // Opens a UDP socket and announces us to RLViser.
//! arena.set_rlviser_enabled(true).unwrap();
//!
//! for _ in 0..120 {
//!     // Adopt any pause/speed/state edits sent by RLViser.
//!     arena.handle_rlviser_messages().unwrap();
//!     if !arena.rlviser_paused() {
//!         arena.step_tick();
//!     }
//! }
//! ```
//!
//! # Protocol
//!
//! Messages are [`RlviserMessage`] values encoded with Planus (FlatBuffers).
//! Each UDP packet is an 8-byte big-endian payload length
//! ([`PACKET_SIZE_BYTES`]) followed by the payload. [`PacketCodec`] handles
//! both ends; the default ports are [`RLVISER_PORT`] (visualizer) and
//! [`ROCKETSIM_PORT`] (simulator).
//!
//! See `examples/watch.rs` for a complete runnable setup.
#[allow(dead_code, clippy::wrong_self_convention)]
mod flat {
    include!(concat!(env!("OUT_DIR"), "/flat.rs"));
}
mod flat_ext;

use std::{
    any::Any,
    io,
    net::{IpAddr, SocketAddr, UdpSocket},
    str::FromStr,
};

use rocketsim::{
    Arena, ArenaState, BallState, BoostPadState, CarState, TileDamageState, Vis, consts,
};

use crate::flat::rocketsim as fb;

/// Default UDP port RLViser listens on.
///
/// Pass a different port to [`Rlviser::with_ports`] to talk to a visualizer
/// bound elsewhere.
pub const RLVISER_PORT: u16 = 45243;
/// Default local UDP port the simulator binds.
///
/// [`Rlviser::new`] binds this port, so only one default visualizer fits per
/// machine; use [`Rlviser::with_ports`] with port `0` for an OS-assigned
/// ephemeral port instead.
pub const ROCKETSIM_PORT: u16 = 34254;
/// Length in bytes of the big-endian payload-size header prefixing every packet.
///
/// See [`PacketCodec::packet_len_from_header`].
pub const PACKET_SIZE_BYTES: usize = size_of::<u64>();
/// Simulation ticks per second (`120.0`), handy for fixed-step loop timing.
///
/// ```
/// use std::time::Duration;
///
/// use rlviser_rocketsim::TICK_RATE;
///
/// let tick = Duration::from_secs_f64(1.0 / f64::from(TICK_RATE));
/// assert_eq!(tick, Duration::from_secs_f64(1.0 / 120.0));
/// ```
pub const TICK_RATE: f32 = consts::TICK_RATE;

/// Convert a simulation value into its FlatBuffers (Planus) representation.
///
/// Implemented for the message type in this crate and for the `rocketsim`
/// state/config types. The associated `Flat` types are crate-private
/// generated structs, so callers use converted values opaquely — typically by
/// handing them to [`PacketCodec`] or a [`FromFlat`] round trip.
///
/// ```
/// use rlviser_rocketsim::ToFlat;
/// use rocketsim::Vec3A;
///
/// let pos = Vec3A::new(1.0, 2.0, 3.0);
/// let flat = pos.to_flat();
/// assert_eq!((flat.x, flat.y, flat.z), (1.0, 2.0, 3.0));
/// ```
pub trait ToFlat {
    /// The generated FlatBuffers counterpart of `Self`.
    type Flat;

    /// Convert `self` into its FlatBuffers representation.
    fn to_flat(&self) -> Self::Flat;
}

/// Rebuild a simulation value from its FlatBuffers (Planus) representation.
///
/// This is the inverse of [`ToFlat`], lossy by design where the wire schema
/// stores less than the sim tracks: per-wheel contact travels as a `bool`
/// while `CarState` holds full raycast info, so contact is rebuilt with a
/// neutral placeholder.
///
/// ```
/// use rlviser_rocketsim::{FromFlat, ToFlat};
/// use rocketsim::Vec3A;
///
/// let pos = Vec3A::new(1.0, 2.0, 3.0);
/// assert_eq!(Vec3A::from_flat(pos.to_flat()), pos);
/// ```
pub trait FromFlat<T> {
    /// Rebuild `Self` from its FlatBuffers representation.
    fn from_flat(flat: T) -> Self;
}

/// A packet exchanged between the simulator and RLViser.
///
/// `Connection`/`Quit` bracket a session, `Speed`/`Paused` flow from the
/// visualizer to the sim, and `GameState` flows from the sim to the
/// visualizer (visualizer-side edits come back through
/// [`ArenaRlviserExt::handle_rlviser_messages`]).
///
/// Encode with [`PacketCodec`]:
///
/// ```
/// use rlviser_rocketsim::{PacketCodec, RlviserMessage};
///
/// let mut codec = PacketCodec::new();
/// let bytes = codec.encode(RlviserMessage::Paused(true)).to_vec();
/// assert!(!bytes.is_empty());
/// ```
#[derive(Clone, Debug)]
pub enum RlviserMessage {
    /// Announce a new simulator session (sent automatically on connect).
    Connection,
    /// Announce simulator shutdown (sent automatically on drop).
    Quit,
    /// Playback-speed multiplier requested by the visualizer.
    Speed(f32),
    /// Pause flag requested by the visualizer.
    Paused(bool),
    /// Full arena snapshot for the visualizer to render.
    ///
    /// Holds an already-converted snapshot; build one from arena state with
    /// [`ToFlat`].
    GameState(Box<fb::GameState>),
}

/// Reusable encoder/decoder for [`RlviserMessage`] UDP packets.
///
/// Keeps a scratch Planus builder and byte buffer so hot loops don't
/// reallocate. Cheap to create but not `Sync`; prefer one per thread (or per
/// [`Rlviser`], which owns one internally).
///
/// Wire format: a [`PACKET_SIZE_BYTES`]-byte big-endian payload length
/// followed by the Planus payload.
///
/// ```
/// use rlviser_rocketsim::{PacketCodec, RlviserMessage};
///
/// let mut codec = PacketCodec::new();
/// let bytes = codec.encode(RlviserMessage::Connection).to_vec();
/// assert!(!bytes.is_empty());
/// ```
pub struct PacketCodec {
    builder: planus::Builder,
    buffer: Vec<u8>,
}

impl Default for PacketCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketCodec {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            builder: planus::Builder::with_capacity(capacity),
            buffer: Vec::with_capacity(capacity + PACKET_SIZE_BYTES),
        }
    }

    /// Encode `message` into a complete packet (header + payload).
    ///
    /// The returned slice borrows the codec's internal buffer and is reused
    /// by the next call — copy it out if you need to keep it.
    ///
    /// ```
    /// use rlviser_rocketsim::{PACKET_SIZE_BYTES, PacketCodec, RlviserMessage};
    ///
    /// let mut codec = PacketCodec::new();
    /// let packet = codec.encode(RlviserMessage::Quit).to_vec();
    /// assert!(packet.len() > PACKET_SIZE_BYTES);
    /// ```
    pub fn encode(&mut self, message: RlviserMessage) -> &[u8] {
        self.builder.clear();

        let packet = fb::Packet {
            message: message.to_flat(),
        };
        let payload = self.builder.finish(packet, None);
        let data_len_bin = u64::try_from(payload.len()).unwrap().to_be_bytes();

        self.buffer.clear();
        self.buffer.extend_from_slice(&data_len_bin);
        self.buffer.extend_from_slice(payload);

        &self.buffer
    }

    /// Decode one packet payload (the bytes *after* the length header).
    ///
    /// Returns `Ok(None)` for packets carrying no simulator-facing message
    /// (e.g. render add/remove packets) and `Err` for malformed input.
    ///
    /// ```
    /// use rlviser_rocketsim::{PACKET_SIZE_BYTES, PacketCodec, RlviserMessage};
    ///
    /// let mut codec = PacketCodec::new();
    /// let packet = codec.encode(RlviserMessage::Speed(0.5)).to_vec();
    ///
    /// let message = PacketCodec::decode_payload(&packet[PACKET_SIZE_BYTES..]).unwrap();
    /// assert!(matches!(message, Some(RlviserMessage::Speed(speed)) if speed == 0.5));
    /// ```
    pub fn decode_payload(payload: &[u8]) -> planus::Result<Option<RlviserMessage>> {
        let packet: fb::Packet =
            <fb::PacketRef<'_> as planus::ReadAsRoot>::read_as_root(payload)?.try_into()?;
        Ok(Option::<RlviserMessage>::from_flat(packet.message))
    }

    /// Read a packet's total length (header + payload) from its 8-byte header.
    ///
    /// Peek the header off the socket, then `recv` exactly this many bytes:
    ///
    /// ```
    /// use rlviser_rocketsim::{PACKET_SIZE_BYTES, PacketCodec, RlviserMessage};
    ///
    /// let mut codec = PacketCodec::new();
    /// let packet = codec.encode(RlviserMessage::Connection).to_vec();
    ///
    /// let mut header = [0u8; PACKET_SIZE_BYTES];
    /// header.copy_from_slice(&packet[..PACKET_SIZE_BYTES]);
    /// assert_eq!(PacketCodec::packet_len_from_header(header), packet.len());
    /// ```
    #[must_use]
    pub fn packet_len_from_header(header: [u8; PACKET_SIZE_BYTES]) -> usize {
        PACKET_SIZE_BYTES + u64::from_be_bytes(header) as usize
    }
}

/// UDP link streaming arena snapshots to a running RLViser instance.
///
/// Implements the simulator's `Vis` trait (attach it via [`ArenaRlviserExt`]):
/// every update sends the current [`RlviserMessage::GameState`]. It also
/// listens for control messages back — read them with [`paused`](Self::paused)
/// and [`speed`](Self::speed), or let
/// [`ArenaRlviserExt::handle_rlviser_messages`] apply them for you. Dropping
/// the visualizer sends [`RlviserMessage::Quit`].
///
/// ```no_run
/// use rlviser_rocketsim::{RLVISER_PORT, Rlviser};
///
/// // Ephemeral local port; announce to RLViser on its default port.
/// let mut viser = Rlviser::with_ports(0, RLVISER_PORT).unwrap();
/// assert!(!viser.paused());
/// assert_eq!(viser.speed(), 1.0);
/// viser.send_quit().unwrap();
/// ```
pub struct Rlviser {
    socket: UdpSocket,
    rlviser_addr: SocketAddr,
    packet_size_buffer: [u8; PACKET_SIZE_BYTES],
    packet_buffer: Vec<u8>,
    codec: PacketCodec,
    paused: bool,
    speed: f32,
}

impl Rlviser {
    /// Connect to RLViser on its default port from [`ROCKETSIM_PORT`].
    ///
    /// Sends [`RlviserMessage::Connection`] immediately. Only one default
    /// visualizer fits per machine (the local port would already be bound) —
    /// use [`with_ports`](Self::with_ports) for more.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the socket can't be bound.
    pub fn new() -> io::Result<Self> {
        Self::with_ports(ROCKETSIM_PORT, RLVISER_PORT)
    }

    /// Connect to `rlviser_port` from local `rocketsim_port`.
    ///
    /// Pass `0` as `rocketsim_port` for an OS-assigned ephemeral port.
    /// Sends [`RlviserMessage::Connection`] immediately.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the socket can't be bound.
    pub fn with_ports(rocketsim_port: u16, rlviser_port: u16) -> io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", rocketsim_port))?;
        let rlviser_addr = SocketAddr::new(
            IpAddr::from_str("127.0.0.1").expect("valid localhost address"),
            rlviser_port,
        );

        socket.set_nonblocking(true)?;

        let mut vis = Self {
            socket,
            rlviser_addr,
            packet_size_buffer: [0; PACKET_SIZE_BYTES],
            packet_buffer: Vec::with_capacity(1024),
            codec: PacketCodec::new(),
            paused: false,
            speed: 1.0,
        };
        vis.send_message(RlviserMessage::Connection)?;

        Ok(vis)
    }

    /// Latest pause flag received from RLViser (`false` until told otherwise).
    #[must_use]
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Latest speed multiplier received from RLViser (`1.0` by default).
    ///
    /// Advisory only — apply it to your own loop timing.
    #[must_use]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Announce shutdown to RLViser now (also sent automatically on drop).
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the packet can't be sent.
    pub fn send_quit(&mut self) -> io::Result<()> {
        self.send_message(RlviserMessage::Quit)
    }

    fn send_message(&mut self, message: RlviserMessage) -> io::Result<()> {
        self.socket
            .send_to(self.codec.encode(message), self.rlviser_addr)?;
        Ok(())
    }

    fn handle_return_messages(&mut self) -> io::Result<Option<fb::GameState>> {
        let mut last_game_state = None;
        while self.socket.peek_from(&mut self.packet_size_buffer).is_ok() {
            let packet_size = PacketCodec::packet_len_from_header(self.packet_size_buffer);
            self.packet_buffer.resize(packet_size, 0);
            self.socket.recv_from(&mut self.packet_buffer)?;

            let Ok(Some(message)) =
                PacketCodec::decode_payload(&self.packet_buffer[PACKET_SIZE_BYTES..])
            else {
                continue;
            };

            match message {
                RlviserMessage::Connection => {}
                RlviserMessage::Speed(speed) => {
                    self.speed = speed;
                }
                RlviserMessage::Paused(paused) => {
                    self.paused = paused;
                }
                RlviserMessage::GameState(game_state) => {
                    last_game_state = Some(*game_state);
                }
                RlviserMessage::Quit => {}
            }
        }

        Ok(last_game_state)
    }
}

impl Drop for Rlviser {
    fn drop(&mut self) {
        let _ = self.send_quit();
    }
}

impl Vis for Rlviser {
    fn update(&mut self, arena_state: &ArenaState, _dt: f32) {
        let game_state = arena_state.to_flat();
        if let Err(err) = self.send_message(RlviserMessage::GameState(Box::new(game_state))) {
            eprintln!("Error sending game state to RLViser: {err}");
        }
    }
}

/// Helper trait attaching an [`Rlviser`] visualizer to an arena.
///
/// ```no_run
/// use rlviser_rocketsim::ArenaRlviserExt;
/// use rocketsim::{Arena, ArenaConfig, GameMode, init_from_default};
///
/// init_from_default(true).unwrap();
/// let mut arena = Arena::new_with_config(ArenaConfig::new(GameMode::Soccar));
/// arena.set_rlviser_enabled(true).unwrap();
///
/// for _ in 0..120 {
///     arena.handle_rlviser_messages().unwrap();
///     if !arena.rlviser_paused() {
///         arena.step_tick();
///     }
/// }
/// ```
pub trait ArenaRlviserExt {
    /// Attach (`true`) or detach (`false`) the visualizer. Idempotent:
    /// enabling twice keeps the existing connection.
    ///
    /// Attaching binds a socket and sends [`RlviserMessage::Connection`];
    /// detaching sends [`RlviserMessage::Quit`].
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the socket can't be bound.
    fn set_rlviser_enabled(&mut self, enabled: bool) -> io::Result<()>;
    /// Attach (`true`) or detach (`false`) the visualizer using custom ports.
    ///
    /// Same as [`set_rlviser_enabled`](Self::set_rlviser_enabled) but connects
    /// to `rlviser_port` from local `rocketsim_port` instead of the defaults.
    /// Pass `0` as `rocketsim_port` for an OS-assigned ephemeral port. Ports
    /// are ignored when detaching.
    ///
    /// ```no_run
    /// use rlviser_rocketsim::{ArenaRlviserExt, RLVISER_PORT};
    /// use rocketsim::{Arena, ArenaConfig, GameMode, init_from_default};
    ///
    /// init_from_default(true).unwrap();
    /// let mut arena = Arena::new_with_config(ArenaConfig::new(GameMode::Soccar));
    /// arena.set_rlviser_enabled_with_ports(true, 0, RLVISER_PORT).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the socket can't be bound.
    fn set_rlviser_enabled_with_ports(
        &mut self,
        enabled: bool,
        rocketsim_port: u16,
        rlviser_port: u16,
    ) -> io::Result<()> {
        let _ = (rocketsim_port, rlviser_port);
        self.set_rlviser_enabled(enabled)
    }
    /// Drain the inbox: adopt pause/speed and any edited game state from RLViser.
    ///
    /// Call once per tick; a no-op when no visualizer is attached.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the socket misbehaves.
    fn handle_rlviser_messages(&mut self) -> io::Result<()>;
    /// Latest pause flag from RLViser, or `false` with no visualizer.
    fn rlviser_paused(&self) -> bool;
    /// Latest speed multiplier from RLViser, or `1.0` with no visualizer.
    fn rlviser_speed(&self) -> f32;
}

impl ArenaRlviserExt for Arena {
    fn set_rlviser_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.set_rlviser_enabled_with_ports(enabled, ROCKETSIM_PORT, RLVISER_PORT)
    }

    fn set_rlviser_enabled_with_ports(
        &mut self,
        enabled: bool,
        rocketsim_port: u16,
        rlviser_port: u16,
    ) -> io::Result<()> {
        match (enabled, self.is_vis_enabled()) {
            (true, false) => {
                self.vis = Some(Box::new(Rlviser::with_ports(rocketsim_port, rlviser_port)?));
            }
            (false, true) => self.vis = None,
            _ => {}
        }

        Ok(())
    }

    fn handle_rlviser_messages(&mut self) -> io::Result<()> {
        let Some(vis) = self.vis.as_deref_mut() else {
            return Ok(());
        };

        let vis: &mut dyn Any = vis;
        if let Some(rlviser) = vis.downcast_mut::<Rlviser>()
            && let Some(game_state) = rlviser.handle_return_messages()?
        {
            apply_game_state(self, game_state);
        }

        Ok(())
    }

    fn rlviser_paused(&self) -> bool {
        self.vis
            .as_deref()
            .and_then(|v| (v as &dyn Any).downcast_ref::<Rlviser>())
            .map(Rlviser::paused)
            .unwrap_or(false)
    }

    fn rlviser_speed(&self) -> f32 {
        self.vis
            .as_deref()
            .and_then(|v| (v as &dyn Any).downcast_ref::<Rlviser>())
            .map(Rlviser::speed)
            .unwrap_or(1.0)
    }
}

fn apply_game_state(arena: &mut Arena, game_state: fb::GameState) {
    arena.set_ball_state(BallState::from_flat(game_state.ball));

    if let Some(cars) = game_state.cars {
        for car_info in &cars {
            let car_idx = car_info.id as usize - 1;
            if car_idx < arena.num_cars() {
                arena.set_car_state(car_idx, CarState::from_flat(&car_info.state));
            }
        }
    }

    if let Some(pads) = &game_state.pads {
        for (i, pad_info) in pads.iter().enumerate() {
            if i < arena.num_boost_pads() {
                arena.set_boost_pad_state(
                    i,
                    BoostPadState {
                        cooldown: pad_info.state.cooldown,
                    },
                );
            }
        }
    }

    if let Some(tiles) = &game_state.tiles {
        let mut tile_states = rocketsim::TileStates::default();

        for (i, tile_info) in tiles.blue_tiles.iter().enumerate() {
            tile_states.states[0][i] = match tile_info.state {
                fb::TileState::Broken => TileDamageState::Broken,
                fb::TileState::Damaged => TileDamageState::Damaged,
                fb::TileState::Full => TileDamageState::Full,
            };
        }

        for (i, tile_info) in tiles.orange_tiles.iter().enumerate() {
            tile_states.states[1][i] = match tile_info.state {
                fb::TileState::Broken => TileDamageState::Broken,
                fb::TileState::Damaged => TileDamageState::Damaged,
                fb::TileState::Full => TileDamageState::Full,
            };
        }
    }
}
