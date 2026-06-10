use rocketsim::{
    ArenaState, BallState, BoostPadConfig, BoostPadState, CarBodyConfig, CarControls, CarInfo,
    CarState, DropshotInfo, GameMode, HeatseekerInfo, Mat3A, PhysState, Team, TileDamageState,
    TileStates, Vec3A, WheelPairConfig, consts,
};

use crate::{FromFlat, RlviserMessage, TICK_RATE, ToFlat, flat::rocketsim as fb};

impl ToFlat for RlviserMessage {
    type Flat = fb::Message;

    fn to_flat(&self) -> Self::Flat {
        match self {
            Self::Connection => fb::Message::Connection(Box::default()),
            Self::Quit => fb::Message::Quit(Box::default()),
            Self::Speed(speed) => fb::Message::Speed(Box::new(fb::Speed { speed: *speed })),
            Self::Paused(paused) => fb::Message::Paused(Box::new(fb::Paused { paused: *paused })),
            Self::GameState(game_state) => fb::Message::GameState(game_state.clone()),
        }
    }
}

impl FromFlat<fb::Message> for Option<RlviserMessage> {
    fn from_flat(message: fb::Message) -> Self {
        match message {
            fb::Message::Connection(_) => Some(RlviserMessage::Connection),
            fb::Message::Quit(_) => Some(RlviserMessage::Quit),
            fb::Message::Speed(speed) => Some(RlviserMessage::Speed(speed.speed)),
            fb::Message::Paused(paused) => Some(RlviserMessage::Paused(paused.paused)),
            fb::Message::GameState(game_state) => Some(RlviserMessage::GameState(game_state)),
            fb::Message::AddRender(_) | fb::Message::RemoveRender(_) => None,
        }
    }
}

impl ToFlat for ArenaState {
    type Flat = fb::GameState;

    fn to_flat(&self) -> Self::Flat {
        fb::GameState {
            tick_rate: TICK_RATE,
            tick_count: self.tick_count,
            game_mode: self.game_mode().to_flat(),
            cars: Some(self.cars.iter().map(ToFlat::to_flat).collect()),
            ball: self.ball.to_flat(),
            pads: Some(self.boost_pads.iter().map(ToFlat::to_flat).collect()),
            tiles: None,
        }
    }
}

impl ToFlat for (CarInfo, CarState) {
    type Flat = fb::CarInfo;

    fn to_flat(&self) -> Self::Flat {
        let (info, state) = self;
        fb::CarInfo {
            id: (info.idx + 1) as u64,
            team: info.team.to_flat(),
            state: Box::new(state.to_flat()),
            config: info.config.to_flat(),
        }
    }
}

impl ToFlat for CarState {
    type Flat = fb::CarState;

    fn to_flat(&self) -> Self::Flat {
        fb::CarState {
            physics: self.phys.to_flat(),
            is_on_ground: self.is_on_ground,
            wheels_with_contact: fb::WheelsWithContact {
                front_left: self.wheels_with_contact[0],
                front_right: self.wheels_with_contact[1],
                rear_left: self.wheels_with_contact[2],
                rear_right: self.wheels_with_contact[3],
            },
            has_jumped: self.has_jumped,
            has_double_jumped: self.has_double_jumped,
            has_flipped: self.has_flipped,
            flip_rel_torque: self.flip_rel_torque.to_flat(),
            jump_time: self.jump_time,
            flip_time: self.flip_time,
            is_flipping: self.is_flipping,
            is_jumping: self.is_jumping,
            air_time: self.air_time,
            air_time_since_jump: self.air_time_since_jump,
            boost: self.boost,
            time_since_boosted: self.time_since_boosted,
            is_boosting: self.is_boosting,
            boosting_time: self.boosting_time,
            is_supersonic: self.is_supersonic,
            supersonic_time: self.supersonic_time,
            handbrake_val: self.handbrake_val,
            is_auto_flipping: self.is_auto_flipping,
            auto_flip_timer: self.auto_flip_timer,
            auto_flip_torque_scale: self.auto_flip_torque_scale,
            world_contact_normal: self.world_contact_normal.map(|normal| normal.to_flat()),
            car_contact: None,
            is_demoed: self.is_demoed,
            demo_respawn_timer: self.demo_respawn_timer,
            ball_hit_info: None,
            last_controls: self.prev_controls.to_flat(),
        }
    }
}

impl FromFlat<&fb::CarState> for CarState {
    fn from_flat(state: &fb::CarState) -> Self {
        Self {
            phys: PhysState::from_flat(state.physics),
            controls: CarControls::from_flat(state.last_controls),
            prev_controls: CarControls::from_flat(state.last_controls),
            is_on_ground: state.is_on_ground,
            wheels_with_contact: [
                state.wheels_with_contact.front_left,
                state.wheels_with_contact.front_right,
                state.wheels_with_contact.rear_left,
                state.wheels_with_contact.rear_right,
            ],
            has_jumped: state.has_jumped,
            has_double_jumped: state.has_double_jumped,
            has_flipped: state.has_flipped,
            flip_rel_torque: Vec3A::from_flat(state.flip_rel_torque),
            jump_time: state.jump_time,
            flip_time: state.flip_time,
            is_flipping: state.is_flipping,
            is_jumping: state.is_jumping,
            air_time: state.air_time,
            air_time_since_jump: state.air_time_since_jump,
            boost: state.boost,
            time_since_boosted: state.time_since_boosted,
            is_boosting: state.is_boosting,
            boosting_time: state.boosting_time,
            is_supersonic: state.is_supersonic,
            supersonic_time: state.supersonic_time,
            handbrake_val: state.handbrake_val,
            is_auto_flipping: state.is_auto_flipping,
            auto_flip_timer: state.auto_flip_timer,
            auto_flip_torque_scale: state.auto_flip_torque_scale,
            bump_cooldown_timer: state.car_contact.as_ref().map_or(0.0, |c| c.cooldown_timer),
            world_contact_normal: state.world_contact_normal.map(Vec3A::from_flat),
            is_demoed: state.is_demoed,
            demo_respawn_timer: state.demo_respawn_timer,
        }
    }
}

impl ToFlat for BallState {
    type Flat = fb::BallState;

    fn to_flat(&self) -> Self::Flat {
        fb::BallState {
            physics: self.phys.to_flat(),
            hs_info: fb::HeatseekerInfo {
                y_target_dir: f32::from(self.hs_info.y_target_dir),
                cur_target_speed: self.hs_info.cur_target_speed,
                time_since_hit: self.hs_info.time_since_hit,
            },
            ds_info: fb::DropshotInfo {
                charge_level: i32::from(self.ds_info.charge_level),
                accumulated_hit_force: self.ds_info.accumulated_hit_force,
                y_target_dir: f32::from(self.ds_info.y_target_dir),
                has_damaged: self.ds_info.last_damage_tick.is_some(),
                last_damage_tick: self.ds_info.last_damage_tick.unwrap_or_default(),
            },
        }
    }
}

impl FromFlat<fb::BallState> for BallState {
    fn from_flat(ball: fb::BallState) -> Self {
        Self {
            phys: PhysState::from_flat(ball.physics),
            hs_info: HeatseekerInfo {
                y_target_dir: ball.hs_info.y_target_dir as i8,
                cur_target_speed: ball.hs_info.cur_target_speed,
                time_since_hit: ball.hs_info.time_since_hit,
            },
            ds_info: DropshotInfo {
                charge_level: ball.ds_info.charge_level as u8,
                accumulated_hit_force: ball.ds_info.accumulated_hit_force,
                y_target_dir: ball.ds_info.y_target_dir as i8,
                last_damage_tick: ball
                    .ds_info
                    .has_damaged
                    .then_some(ball.ds_info.last_damage_tick),
            },
            last_extra_hit_tick: None,
            tick_count_since_kickoff: 0,
        }
    }
}

impl ToFlat for (BoostPadConfig, BoostPadState) {
    type Flat = fb::BoostPadInfo;

    fn to_flat(&self) -> Self::Flat {
        let (config, state) = self;
        fb::BoostPadInfo {
            config: config.to_flat(),
            state: fb::BoostPadState {
                is_active: state.is_active(),
                cooldown: state.cooldown,
                cur_locked_car: 0,
                prev_locked_car_id: 0,
            },
        }
    }
}

impl FromFlat<&fb::BoostPadInfo> for (BoostPadConfig, BoostPadState) {
    fn from_flat(pad: &fb::BoostPadInfo) -> Self {
        (
            BoostPadConfig::from_flat(pad.config),
            BoostPadState {
                cooldown: pad.state.cooldown,
            },
        )
    }
}

impl ToFlat for BoostPadConfig {
    type Flat = fb::BoostPadConfig;

    fn to_flat(&self) -> Self::Flat {
        fb::BoostPadConfig {
            pos: self.pos.to_flat(),
            is_big: self.is_big,
        }
    }
}

impl FromFlat<fb::BoostPadConfig> for BoostPadConfig {
    fn from_flat(config: fb::BoostPadConfig) -> Self {
        Self {
            pos: Vec3A::from_flat(config.pos),
            is_big: config.is_big,
        }
    }
}

impl ToFlat for CarBodyConfig {
    type Flat = fb::CarConfig;

    fn to_flat(&self) -> Self::Flat {
        fb::CarConfig {
            hitbox_size: self.hitbox_size.to_flat(),
            hitbox_pos_offset: self.hitbox_pos_offset.to_flat(),
            front_wheels: self.front_wheels.to_flat(),
            back_wheels: self.back_wheels.to_flat(),
            three_wheels: self.three_wheels,
            dodge_deadzone: self.dodge_deadzone,
        }
    }
}

impl FromFlat<fb::CarConfig> for CarBodyConfig {
    fn from_flat(config: fb::CarConfig) -> Self {
        Self {
            hitbox_size: Vec3A::from_flat(config.hitbox_size),
            hitbox_pos_offset: Vec3A::from_flat(config.hitbox_pos_offset),
            front_wheels: WheelPairConfig::from_flat(config.front_wheels),
            back_wheels: WheelPairConfig::from_flat(config.back_wheels),
            three_wheels: config.three_wheels,
            dodge_deadzone: config.dodge_deadzone,
        }
    }
}

impl ToFlat for WheelPairConfig {
    type Flat = fb::WheelPairConfig;

    fn to_flat(&self) -> Self::Flat {
        fb::WheelPairConfig {
            wheel_radius: self.wheel_radius,
            suspension_rest_length: self.suspension_rest_length,
            connection_point_offset: self.connection_point_offset.to_flat(),
        }
    }
}

impl FromFlat<fb::WheelPairConfig> for WheelPairConfig {
    fn from_flat(config: fb::WheelPairConfig) -> Self {
        Self {
            wheel_radius: config.wheel_radius,
            suspension_rest_length: config.suspension_rest_length,
            connection_point_offset: Vec3A::from_flat(config.connection_point_offset),
        }
    }
}

impl ToFlat for CarControls {
    type Flat = fb::CarControls;

    fn to_flat(&self) -> Self::Flat {
        fb::CarControls {
            throttle: self.throttle,
            steer: self.steer,
            pitch: self.pitch,
            yaw: self.yaw,
            roll: self.roll,
            jump: self.jump,
            boost: self.boost,
            handbrake: self.handbrake,
        }
    }
}

impl FromFlat<fb::CarControls> for CarControls {
    fn from_flat(controls: fb::CarControls) -> Self {
        Self {
            throttle: controls.throttle,
            steer: controls.steer,
            pitch: controls.pitch,
            yaw: controls.yaw,
            roll: controls.roll,
            jump: controls.jump,
            boost: controls.boost,
            handbrake: controls.handbrake,
        }
    }
}

impl ToFlat for PhysState {
    type Flat = fb::PhysState;

    fn to_flat(&self) -> Self::Flat {
        fb::PhysState {
            pos: self.pos.to_flat(),
            rot_mat: self.rot_mat.to_flat(),
            vel: self.vel.to_flat(),
            ang_vel: self.ang_vel.to_flat(),
        }
    }
}

impl FromFlat<fb::PhysState> for PhysState {
    fn from_flat(phys: fb::PhysState) -> Self {
        Self {
            pos: Vec3A::from_flat(phys.pos),
            rot_mat: Mat3A::from_flat(phys.rot_mat),
            vel: Vec3A::from_flat(phys.vel),
            ang_vel: Vec3A::from_flat(phys.ang_vel),
        }
    }
}

impl ToFlat for GameMode {
    type Flat = fb::GameMode;

    fn to_flat(&self) -> Self::Flat {
        match self {
            Self::Soccar => fb::GameMode::Soccar,
            Self::Hoops => fb::GameMode::Hoops,
            Self::Heatseeker => fb::GameMode::Heatseeker,
            Self::Snowday => fb::GameMode::Snowday,
            Self::Dropshot => fb::GameMode::Dropshot,
            Self::TheVoid => fb::GameMode::TheVoid,
        }
    }
}

impl FromFlat<fb::GameMode> for GameMode {
    fn from_flat(game_mode: fb::GameMode) -> Self {
        match game_mode {
            fb::GameMode::Soccar => Self::Soccar,
            fb::GameMode::Hoops => Self::Hoops,
            fb::GameMode::Heatseeker => Self::Heatseeker,
            fb::GameMode::Snowday => Self::Snowday,
            fb::GameMode::Dropshot => Self::Dropshot,
            fb::GameMode::TheVoid => Self::TheVoid,
        }
    }
}

impl ToFlat for Team {
    type Flat = fb::Team;

    fn to_flat(&self) -> Self::Flat {
        match self {
            Self::Blue => fb::Team::Blue,
            Self::Orange => fb::Team::Orange,
        }
    }
}

impl FromFlat<fb::Team> for Team {
    fn from_flat(team: fb::Team) -> Self {
        match team {
            fb::Team::Blue => Self::Blue,
            fb::Team::Orange => Self::Orange,
        }
    }
}

impl ToFlat for TileDamageState {
    type Flat = fb::TileState;

    fn to_flat(&self) -> Self::Flat {
        match self {
            Self::Full => fb::TileState::Full,
            Self::Damaged => fb::TileState::Damaged,
            Self::Broken => fb::TileState::Broken,
        }
    }
}

impl FromFlat<fb::TileState> for TileDamageState {
    fn from_flat(tile_state: fb::TileState) -> Self {
        match tile_state {
            fb::TileState::Full => Self::Full,
            fb::TileState::Damaged => Self::Damaged,
            fb::TileState::Broken => Self::Broken,
        }
    }
}

impl ToFlat for TileStates {
    type Flat = fb::DropshotTilesByTeam;

    fn to_flat(&self) -> Self::Flat {
        fn make_tile(idx: usize, state: TileDamageState) -> fb::DropshotTile {
            fb::DropshotTile {
                pos: tile_pos(idx),
                state: state.to_flat(),
            }
        }

        fb::DropshotTilesByTeam {
            blue_tiles: self.states[0]
                .iter()
                .copied()
                .enumerate()
                .map(|(idx, state)| make_tile(idx, state))
                .collect(),
            orange_tiles: self.states[1]
                .iter()
                .copied()
                .enumerate()
                .map(|(idx, state)| make_tile(idx, state))
                .collect(),
        }
    }
}

impl FromFlat<&fb::DropshotTilesByTeam> for TileStates {
    fn from_flat(tiles: &fb::DropshotTilesByTeam) -> Self {
        let mut tile_states = Self::DEFAULT;

        for (idx, tile) in tiles
            .blue_tiles
            .iter()
            .enumerate()
            .take(consts::dropshot::NUM_TILES_PER_TEAM)
        {
            tile_states.states[0][idx] = TileDamageState::from_flat(tile.state);
        }

        for (idx, tile) in tiles
            .orange_tiles
            .iter()
            .enumerate()
            .take(consts::dropshot::NUM_TILES_PER_TEAM)
        {
            tile_states.states[1][idx] = TileDamageState::from_flat(tile.state);
        }

        tile_states
    }
}

impl ToFlat for Mat3A {
    type Flat = fb::Mat3;

    fn to_flat(&self) -> Self::Flat {
        fb::Mat3 {
            forward: self.x_axis.to_flat(),
            right: self.y_axis.to_flat(),
            up: self.z_axis.to_flat(),
        }
    }
}

impl FromFlat<fb::Mat3> for Mat3A {
    fn from_flat(rot_mat: fb::Mat3) -> Self {
        Self::from_cols(
            Vec3A::from_flat(rot_mat.forward),
            Vec3A::from_flat(rot_mat.right),
            Vec3A::from_flat(rot_mat.up),
        )
    }
}

impl ToFlat for Vec3A {
    type Flat = fb::Vec3;

    fn to_flat(&self) -> Self::Flat {
        fb::Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

impl FromFlat<fb::Vec3> for Vec3A {
    fn from_flat(vec: fb::Vec3) -> Self {
        Self::new(vec.x, vec.y, vec.z)
    }
}

fn tile_pos(idx: usize) -> fb::Vec3 {
    let x = (idx % 10) as f32;
    let y = (idx / 10) as f32;

    fb::Vec3 { x, y, z: 0.0 }
}
