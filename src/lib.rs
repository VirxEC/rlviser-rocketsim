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

pub const RLVISER_PORT: u16 = 45243;
pub const ROCKETSIM_PORT: u16 = 34254;
pub const PACKET_SIZE_BYTES: usize = size_of::<u64>();
pub const TICK_RATE: f32 = consts::TICK_RATE;

pub trait ToFlat {
    type Flat;

    fn to_flat(&self) -> Self::Flat;
}

pub trait FromFlat<T> {
    fn from_flat(flat: T) -> Self;
}

#[derive(Clone, Debug)]
pub enum RlviserMessage {
    Connection,
    Quit,
    Speed(f32),
    Paused(bool),
    GameState(Box<fb::GameState>),
}

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

    pub fn decode_payload(payload: &[u8]) -> planus::Result<Option<RlviserMessage>> {
        let packet: fb::Packet =
            <fb::PacketRef<'_> as planus::ReadAsRoot>::read_as_root(payload)?.try_into()?;
        Ok(Option::<RlviserMessage>::from_flat(packet.message))
    }

    #[must_use]
    pub fn packet_len_from_header(header: [u8; PACKET_SIZE_BYTES]) -> usize {
        PACKET_SIZE_BYTES + u64::from_be_bytes(header) as usize
    }
}

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
    pub fn new() -> io::Result<Self> {
        Self::with_ports(ROCKETSIM_PORT, RLVISER_PORT)
    }

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

    #[must_use]
    pub fn paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub fn speed(&self) -> f32 {
        self.speed
    }

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

pub trait ArenaRlviserExt {
    fn set_rlviser_enabled(&mut self, enabled: bool) -> io::Result<()>;
    fn handle_rlviser_messages(&mut self) -> io::Result<()>;
    fn rlviser_paused(&self) -> bool;
    fn rlviser_speed(&self) -> f32;
}

impl ArenaRlviserExt for Arena {
    fn set_rlviser_enabled(&mut self, enabled: bool) -> io::Result<()> {
        match (enabled, self.is_vis_enabled()) {
            (true, false) => self.vis = Some(Box::new(Rlviser::new()?)),
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
