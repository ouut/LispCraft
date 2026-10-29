//! Protocol conversion bridge between game_net/game_server Layer 0 generic envelopes
//! and LispCraft domain-specific structures.

pub use shared::{
    name_to_player_id, player_id_to_name, ClientEnvelope, ClientState, EntityDesc, LispEvent,
    LispGameCommand, LispMotionInput, PlayerId, ServerEnvelope,
};

/// Type alias for the authoritative LispCraft server handle
pub type LispCraftServerHandle =
    game_server::ServerHandle<LispMotionInput, LispGameCommand, ClientState, LispEvent>;

/// Type alias for the client envelope in LispCraft
pub type LispClientEnvelope = ClientEnvelope<LispMotionInput, LispGameCommand>;

/// Type alias for the server envelope in LispCraft
pub type LispServerEnvelope = ServerEnvelope<ClientState, LispEvent>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_id_bijective_mapping() {
        for id in [1, 2, 42, 100, 999999] {
            let name = player_id_to_name(id);
            assert_eq!(name, format!("player_{}", id));
            let parsed_id = name_to_player_id(&name);
            assert_eq!(parsed_id, Some(id));
        }
    }

    #[test]
    fn test_motion_input_mapping() {
        let input = LispMotionInput {
            dx: 1.0,
            dz: -0.5,
            yaw: std::f32::consts::PI,
        };
        let env: LispClientEnvelope = ClientEnvelope::Input(input);
        let serialized = serde_json::to_string(&env).unwrap();
        let deserialized: LispClientEnvelope = serde_json::from_str(&serialized).unwrap();
        assert_eq!(env, deserialized);
    }

    #[test]
    fn test_game_command_fifo_mapping() {
        let action = LispGameCommand::Action { action_id: 1 };
        let cmd = LispGameCommand::SchemeCommand("heal".into());

        let env_action: LispClientEnvelope = ClientEnvelope::Command(action);
        let env_cmd: LispClientEnvelope = ClientEnvelope::Command(cmd);

        let json_act = serde_json::to_string(&env_action).unwrap();
        let json_cmd = serde_json::to_string(&env_cmd).unwrap();

        let parsed_act: LispClientEnvelope = serde_json::from_str(&json_act).unwrap();
        let parsed_cmd: LispClientEnvelope = serde_json::from_str(&json_cmd).unwrap();

        assert_eq!(env_action, parsed_act);
        assert_eq!(env_cmd, parsed_cmd);
    }

    #[test]
    fn test_snapshot_and_event_mapping() {
        let state = ClientState {
            client_id: "player_1".into(),
            camera_target: [0.0, 1.0, 0.0],
            camera_yaw: 0.0,
            hp: 100.0,
            max_hp: 100.0,
            score: 0,
            entities: vec![],
            logs: vec!["Welcome".into()],
        };
        let env_snap: LispServerEnvelope = ServerEnvelope::Snapshot(state.clone());
        let json_snap = serde_json::to_string(&env_snap).unwrap();
        let parsed_snap: LispServerEnvelope = serde_json::from_str(&json_snap).unwrap();
        assert_eq!(env_snap, parsed_snap);

        let event = LispEvent::CommandEcho {
            success: true,
            output: "Teleported".into(),
        };
        let env_evt: LispServerEnvelope = ServerEnvelope::Event(event);
        let json_evt = serde_json::to_string(&env_evt).unwrap();
        let parsed_evt: LispServerEnvelope = serde_json::from_str(&json_evt).unwrap();
        assert_eq!(env_evt, parsed_evt);
    }
}
