use serde::{Deserialize, Serialize};

/// 客户端发送给服务端的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// 移动输入向量 (dx: X轴位移, dz: Z轴位移, yaw: 视角偏航角)
    Move { dx: f32, dz: f32, yaw: f32 },
    /// 动作触发
    Action { action_id: u8 },
    /// 任意字符串命令 (控制台/聊天/REPL)
    Command(String),
}

/// 场景中的渲染图元描述 (服务端只发几何描述，客户端只管画)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDesc {
    pub id: String,
    pub shape: String,      // "cube" | "sphere"
    pub pos: [f32; 3],      // [x, y, z]
    pub size: [f32; 3],     // [w, h, d]
    pub color: [f32; 4],    // [r, g, b, a]
    pub label: String,      // 3D 浮动文字标签
}

/// 服务端下发给特定客户端的个性化状态包
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientState {
    pub client_id: String,
    pub camera_target: [f32; 3],   // 客户端摄像机跟随的目标坐标
    pub camera_yaw: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub score: i32,
    pub entities: Vec<EntityDesc>, // 该玩家视野内的实体
    pub logs: Vec<String>,         // 针对该玩家的命令响应/系统提示
}

/// 服务端推送给客户端的总包
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome { client_id: String },
    SyncState(ClientState),
    CommandEcho { success: bool, output: String },
}
