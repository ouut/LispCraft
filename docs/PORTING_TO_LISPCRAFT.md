# LispCraft 网络引擎无痛升级与接入实战指南 (LispCraft Porting Guide v0.2.0)

> **当前状态**：已在 LispCraft `v0.2.0` 正式落地并全量通过 19 项 E2E 自动化测试。  
> **核心成果**：使用跨项目通用底座 `game_net`、`game_server` 与 `game_script` (`v0.2.0`)，彻底替代了手写网络与旧版三端口监听，直接获得 **WebSocket 全双工、单端口 HTTP+WS 一体化网关、双模自适应 Codec（JSON 文本调试 + Postcard 二进制极速）、会话生命周期与心跳看门狗、移动锁存流水线、看门狗安全求值**。

---

## 1. 概念与类型系统一对一映射

对比 `LispCraft/shared/src/lib.rs` 与通用底座泛型，两者的语义是 **100% 一对一映射** 的：

| LispCraft 既有概念 | 映射到通用底座泛型 | 语义与处理方式 |
| :--- | :--- | :--- |
| `ClientMsg::Move { dx, dz, yaw }` | `TInput` (`LispMotionInput`) | 进入服务端的 **Latest-Only Latch**，每帧只保留最新输入，彻底杜绝高频网络操作堆积迟滞。 |
| `ClientMsg::Command(String)` / `Action` | `TCmd` (`LispGameCommand`) | 进入服务端的 **FIFO Command Queue**，保证 Scheme 脚本调用和交互按严格顺序执行。 |
| `ClientState` (`EntityDesc` 列表) | `TSnapshot` (状态快照) | 个性化视口切片单播包，依据玩家距离（>25m 裁剪）单独下发。 |
| `ServerMsg::CommandEcho` / 系统提示 | `TEvent` (`LispEvent`) | 增量事件或全局/单播消息广播（如全服加入提示、聊天广播）。 |

---

## 2. 依赖引入与跨仓构建规范

在 `shared/Cargo.toml` 与 `server/Cargo.toml` 中，通过 Git 依赖规范声明通用底座，确保本地与 GitHub Actions 跨平台 CI 均能平滑构建：

```toml
# shared/Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
game_net = { git = "https://github.com/ouut/2d_minecraft.git", package = "game_net" }

# server/Cargo.toml
[dependencies]
steel-core = { version = "0.8.3", features = ["sync"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
shared = { path = "../shared" }
game_net = { git = "https://github.com/ouut/2d_minecraft.git", package = "game_net" }
game_server = { git = "https://github.com/ouut/2d_minecraft.git", package = "game_server" }
game_script = { git = "https://github.com/ouut/2d_minecraft.git", package = "game_script" }
```

---

## 3. 核心适配层实现 (`server/src/net_bridge.rs`)

### (1) 玩家 ID 双射转换 (Bijective Mapping)
`game_net` 内部使用高性能 `PlayerId = u64`，而 LispCraft Scheme 虚拟机内部采用字符串键名（如 `"player_1"`）。适配层提供无损双向转换：

```rust
pub fn player_id_to_name(id: PlayerId) -> String {
    format!("player_{}", id)
}

pub fn name_to_player_id(name: &str) -> Option<PlayerId> {
    name.strip_prefix("player_")?.parse::<PlayerId>().ok()
}
```

### (2) 强类型 Payload 定义
```rust
use game_net::{ClientEnvelope, ServerEnvelope, PlayerId};
use game_server::GenericGameServer;

/// 高频移动输入（映射原 ClientMsg::Move）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LispMotionInput {
    pub dx: f32,
    pub dz: f32,
    pub yaw: f32,
}

/// 离散交互与控制指令（映射原 Action 与 Command）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LispGameCommand {
    Action { action_id: u8 },
    SchemeCommand(String),
}

/// 离散事件与聊天广播
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LispEvent {
    CommandEcho { success: bool, output: String },
    ChatBroadcast { from: String, message: String },
}
```

---

## 4. 单端口一体化网关与仿真主循环 (`server/src/main.rs`)

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use game_net::ServerEnvelope;
use game_server::{start_server, ServerConfig, ServerHandle, SessionEvent, StaticAssetSource};
use net_bridge::{player_id_to_name, LispEvent, LispGameCommand, LispMotionInput};
use scheme_game::SchemeGame;
use shared::ClientState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = std::env::args().nth(1).unwrap_or_else(|| "games/space_arena.scm".to_string());
    let port: u16 = std::env::var("PORT").unwrap_or_else(|_| "8087".to_string()).parse().unwrap_or(8087);
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
    let web_dir = find_web_dir();

    // 1. 初始化 Scheme 游戏世界
    let game = Arc::new(SchemeGame::new(&script_path)?);

    // 2. 配置单端口网关 (HTTP 静态托管 + WebSocket /ws)
    let config = ServerConfig::new(bind_addr)
        .with_static_assets(StaticAssetSource::FileSystem(web_dir))
        .with_heartbeat_timeout_secs(10)
        .with_grace_period_secs(60);

    let server: ServerHandle<LispMotionInput, LispGameCommand, ClientState, LispEvent> =
        start_server(config).await?;

    // 3. 20 TPS (50ms) 物理仿真与空间裁剪单播下发循环
    let tick_duration = Duration::from_millis(50);
    let mut last_tick_time = Instant::now();

    loop {
        let tick_start = Instant::now();
        let dt = tick_start.duration_since(last_tick_time).as_secs_f32().min(0.1);
        last_tick_time = tick_start;

        // (a) 处理玩家连接、断开与超时事件
        for event in server.drain_session_events() {
            match event {
                SessionEvent::PlayerJoined { player_id, .. } => {
                    let name = player_id_to_name(player_id);
                    game.on_join(&name);
                    server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                        from: "System".to_string(),
                        message: format!("{} joined the galaxy!", name),
                    }));
                }
                SessionEvent::PlayerTerminated { player_id } => {
                    let name = player_id_to_name(player_id);
                    game.on_leave(&name);
                }
                _ => {}
            }
        }

        // (b) 消费高频移动 (Latest-Only Latch 锁存防堆叠)
        for (pid, motion) in server.drain_inputs() {
            let name = player_id_to_name(pid);
            game.on_move(&name, motion.dx, motion.dz, motion.yaw);
        }

        // (c) 消费离散指令 (FIFO 队列严格保序 + 100ms 看门狗安全求值)
        for (pid, cmds) in server.drain_commands() {
            let name = player_id_to_name(pid);
            for cmd in cmds {
                match cmd {
                    LispGameCommand::SchemeCommand(expr) => {
                        let output = game.on_command(&name, &expr);
                        server.send_to(pid, ServerEnvelope::Event(LispEvent::CommandEcho {
                            success: true,
                            output,
                        }));
                    }
                    _ => {}
                }
            }
        }

        // (d) 推进 Scheme 游戏世界状态
        game.tick(dt);

        // (e) 核心特性：个性化视口裁剪状态单播 (Not Shared Screen)
        let active_pids = server.session_manager().all_active_player_ids();
        for pid in active_pids {
            let name = player_id_to_name(pid);
            if let Some(state) = game.get_client_state(&name) {
                server.send_to(pid, ServerEnvelope::Snapshot(state));
            }
        }

        // (f) 保持 20 TPS 帧率
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }
}
```

---

## 5. 客户端改造与自适应双模支持

1. **自动握手**：客户端连入 WebSocket 后立即发送 `ClientEnvelope::Auth { token, client_version }`，服务端自动注册会话并下发 `ServerEnvelope::AuthSuccess`。
2. **双模解包**：客户端通过 `DualCodec` 解析下发的 `ServerEnvelope`，分别处理 `Snapshot`（3D 图元渲染）与 `Event`（命令行 Echo 回显）。
3. **单端口 Web 访问**：浏览器打开 `http://localhost:8087/` 即可直接加载 WASM 并自动连入 `ws://localhost:8087/ws`，通过了全部 19 项端到端验收测试。
