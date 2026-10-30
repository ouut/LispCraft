//! # lc_core: Core Domain Contracts for LispCraft 3D
//!
//! Layer 0 domain contract crate defining shared protocol envelopes,
//! motion inputs, game commands, entity descriptions, and view constants.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// Re-export Layer 0 protocol types from game_net
pub use game_net::{ClientEnvelope, DualCodec, PlayerId, ServerEnvelope};

/// Standard visibility culling distance in world units (meters).
pub const VIEW_DISTANCE: f32 = 25.0;

/// 客户端移动输入向量 (TInput: Latest-Only Latch)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LispMotionInput {
    pub dx: f32,
    pub dz: f32,
    pub yaw: f32,
}

impl Default for LispMotionInput {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dz: 0.0,
            yaw: 0.0,
        }
    }
}

/// 客户端游戏指令与 RemoteEvent (TCmd: FIFO Queue)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "LispGameCommandHelper", into = "LispGameCommandHelper")]
pub enum LispGameCommand {
    Action { action_id: u8 },
    SchemeCommand(String),
    RemoteEvent { event_name: String, payload: serde_json::Value },
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum LispGameCommandHelper {
    Named(NamedCommandHelper),
    Plain(String),
}

#[derive(Serialize, Deserialize)]
enum NamedCommandHelper {
    Action { action_id: u8 },
    SchemeCommand(String),
    Command(String),
    RemoteEvent { event_name: String, payload: serde_json::Value },
}

impl From<LispGameCommandHelper> for LispGameCommand {
    fn from(helper: LispGameCommandHelper) -> Self {
        match helper {
            LispGameCommandHelper::Named(NamedCommandHelper::Action { action_id }) => {
                LispGameCommand::Action { action_id }
            }
            LispGameCommandHelper::Named(NamedCommandHelper::SchemeCommand(cmd)) => {
                LispGameCommand::SchemeCommand(cmd)
            }
            LispGameCommandHelper::Named(NamedCommandHelper::Command(cmd)) => {
                LispGameCommand::SchemeCommand(cmd)
            }
            LispGameCommandHelper::Named(NamedCommandHelper::RemoteEvent { event_name, payload }) => {
                LispGameCommand::RemoteEvent { event_name, payload }
            }
            LispGameCommandHelper::Plain(cmd) => LispGameCommand::SchemeCommand(cmd),
        }
    }
}

impl From<LispGameCommand> for LispGameCommandHelper {
    fn from(cmd: LispGameCommand) -> Self {
        match cmd {
            LispGameCommand::Action { action_id } => {
                LispGameCommandHelper::Named(NamedCommandHelper::Action { action_id })
            }
            LispGameCommand::SchemeCommand(s) => {
                LispGameCommandHelper::Named(NamedCommandHelper::SchemeCommand(s))
            }
            LispGameCommand::RemoteEvent { event_name, payload } => {
                LispGameCommandHelper::Named(NamedCommandHelper::RemoteEvent { event_name, payload })
            }
        }
    }
}

/// 场景中的渲染图元描述 (服务端广播权威属性，客户端 Scheme 查询并自主绘制)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityDesc {
    pub id: String,
    pub shape: String,      // "cube" | "sphere" | "custom"
    pub pos: [f32; 3],      // [x, y, z]
    pub size: [f32; 3],     // [w, h, d]
    pub color: [f32; 4],    // [r, g, b, a]
    pub label: String,      // 3D 浮动文字标签
}

/// 服务端下发给特定客户端的个性化状态包 (TSnapshot)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub client_id: String,
    pub camera_target: [f32; 3],   // 客户端摄像机跟随的目标坐标
    pub camera_yaw: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub score: i32,
    pub entities: Vec<EntityDesc>, // 该玩家视野内的实体
    pub logs: Vec<String>,         // 针对该玩家的命令响应/系统提示
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>, // 动态属性同步
}

/// 仿真事件、离散通知与 Roblox 脚本分发 (TEvent)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LispEvent {
    CommandEcho { success: bool, output: String },
    ChatBroadcast { from: String, message: String },
    SystemNotice(String),
    /// Roblox 风格双向 RemoteEvent
    RemoteEvent { event_name: String, payload: serde_json::Value },
    /// Roblox 风格客户端 LocalScript 脚本包热下发
    BundleScript { game_id: String, vfs: HashMap<String, String> },
}

/// 将 PlayerId (u64) 转换为标准玩家字符串标识 ("player_x")
pub fn player_id_to_name(id: PlayerId) -> String {
    format!("player_{}", id)
}

/// 将玩家字符串标识 ("player_x") 无损反解为 PlayerId (u64)
pub fn name_to_player_id(name: &str) -> Option<PlayerId> {
    if let Some(rest) = name.strip_prefix("player_") {
        rest.parse::<PlayerId>().ok()
    } else {
        name.parse::<PlayerId>().ok()
    }
}

// 兼容旧版消息类型 (用于平滑迁移)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Move { dx: f32, dz: f32, yaw: f32 },
    Action { action_id: u8 },
    Command(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome { client_id: String },
    SyncState(ClientState),
    CommandEcho { success: bool, output: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_id_mapping() {
        assert_eq!(player_id_to_name(42), "player_42");
        assert_eq!(name_to_player_id("player_42"), Some(42));
        assert_eq!(name_to_player_id("42"), Some(42));
        assert_eq!(name_to_player_id("invalid"), None);
    }

    #[test]
    fn test_command_serialization() {
        let cmd = LispGameCommand::SchemeCommand("tp 0 0".into());
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: LispGameCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);

        // Test untagged / plain string deserialization
        let parsed_plain: LispGameCommand = serde_json::from_str(r#""heal""#).unwrap();
        assert_eq!(parsed_plain, LispGameCommand::SchemeCommand("heal".into()));

        // Test action deserialization
        let act = LispGameCommand::Action { action_id: 2 };
        let act_json = serde_json::to_string(&act).unwrap();
        let parsed_act: LispGameCommand = serde_json::from_str(&act_json).unwrap();
        assert_eq!(act, parsed_act);

        // Test remote event serialization
        let re = LispGameCommand::RemoteEvent {
            event_name: "test_event".into(),
            payload: serde_json::json!({"x": 1.0, "spell": "flash"}),
        };
        let re_json = serde_json::to_string(&re).unwrap();
        let parsed_re: LispGameCommand = serde_json::from_str(&re_json).unwrap();
        assert_eq!(re, parsed_re);
    }

    #[test]
    fn test_bundle_script_event() {
        let mut vfs = HashMap::new();
        vfs.insert("main.scm".into(), "(displayln 1)".into());
        let evt = LispEvent::BundleScript {
            game_id: "space_arena".into(),
            vfs,
        };
        let json = serde_json::to_string(&evt).unwrap();
        let parsed: LispEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(evt, parsed);
    }
}
