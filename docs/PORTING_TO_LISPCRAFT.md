# LispCraft 网络引擎无痛升级与接入指南 (Porting Guide to LispCraft)

> **目标读者**：负责 `/workspace/LispCraft` 的开发与重构 Agent。  
> **核心目标**：使用官方发布的通用底座 `game_net` 与 `game_server`，彻底替换原有的手写网络模块，直接获得 **WebSocket 全双工、Web 浏览器 WASM 直连（单端口 HTTP+WS）、双模自适应 Codec（JSON 文本调试 + Postcard 二进制极速）、心跳看门狗与高频移动锁存流水线**。

---

## 1. 为什么 LispCraft 可以零侵入平移？

对比 `LispCraft/shared/src/lib.rs` 与通用底座，两者的概念是 **100% 一对一映射** 的：

| LispCraft 既有类型 | 映射到通用底座泛型 | 语义与处理方式 |
| :--- | :--- | :--- |
| `ClientMsg::Move { dx, dz, yaw }` | `TInput` (高频输入) | 进入服务端的 **Latest-Only Latch**，每帧只保留最新输入，彻底杜绝操作堆积迟滞。 |
| `ClientMsg::Action` & `ClientMsg::Command(String)` | `TCmd` (离散指令) | 进入服务端的 **FIFO Command Queue**，保证 Scheme 脚本调用和交互严格按顺序执行。 |
| `ClientState` (`EntityDesc` 列表) | `TSnapshot` (状态快照) | 周期性全量/视口状态同步包，推送给指定客户端。 |
| `ServerMsg::CommandEcho` / 系统提示 | `TEvent` (离散事件) | 增量事件或全局/单播消息广播。 |

---

## 2. 改造步骤

### 第一步：在 `server/Cargo.toml` 中引入依赖

直接使用已发布在 crates.io 上的版本：

```toml
[dependencies]
game_net = "0.1.0"
game_server = "0.1.0"
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.38", features = ["full"] }
```

### 第二步：定义类型别名（适配层）

在 `server/src/net_bridge.rs`（或 `shared`）中声明具体的 Payload：

```rust
use serde::{Deserialize, Serialize};
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

// 复用已有的 ClientState 与 EntityDesc 作为快照
use crate::shared::{ClientState, ServerMsg};

/// 离散回执与事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LispEvent {
    CommandEcho { success: bool, output: String },
    BroadcastNotice(String),
}

/// LispCraft 专有服务端类型定义
pub type LispCraftServer = GenericGameServer<LispMotionInput, LispGameCommand, ClientState, LispEvent>;
```

### 第三步：启动一体化网关与主循环

将原先混杂了 TCP/WS 与 Scheme 调度的旧 server 替换为极简的流水线调用：

```rust
use std::path::PathBuf;
use game_server::{ServerConfig, start_server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置 HTTP 静态托管 (自动托管 web/ 目录，支持 WASM) 与 WebSocket 路由 (/ws)
    let config = ServerConfig {
        bind_addr: "0.0.0.0:8080".parse()?,
        heartbeat_timeout_secs: 10,
        grace_period_secs: 60, // 断线 60 秒内重连自动恢复实体
        static_dir: Some(PathBuf::from("web")),
    };

    let (server, _handle) = start_server::<LispMotionInput, LispGameCommand, ClientState, LispEvent>(config).await?;
    println!("LispCraft 一体化网关已启动：");
    println!("  - Web 浏览器直接访问: http://localhost:8080");
    println!("  - WebSocket 联机端点: ws://localhost:8080/ws");

    // 2. 游戏仿真主循环 (20 TPS)
    let mut tick_timer = tokio::time::interval(std::time::Duration::from_millis(50));
    loop {
        tick_timer.tick().await;

        // A. 高频移动拉取 (每位玩家仅返回最新一帧，锁存覆盖)
        let inputs = server.drain_inputs();
        for (player_id, motion) in inputs {
            // 在 Scheme 世界中驱动玩家位置更新: (move-player! player-id dx dz yaw)
        }

        // B. 离散指令拉取 (严格有序执行)
        let commands = server.drain_commands();
        for (player_id, cmds) in commands {
            for cmd in cmds {
                match cmd {
                    LispGameCommand::Action { action_id } => {
                        // (trigger-action! player-id action_id)
                    }
                    LispGameCommand::SchemeCommand(expr) => {
                        // 在 Scheme VM 中求值并返回 Echo:
                        // let result = scheme_vm.eval(&expr);
                        // server.send_to(player_id, ServerEnvelope::Event(LispEvent::CommandEcho { ... }));
                    }
                }
            }
        }

        // C. 周期性广播视口快照
        // server.broadcast(ServerEnvelope::Snapshot(current_world_state));
    }
}
```

---

## 3. Web 浏览器 (WASM) 客户端支持

得益于 `game_server` 的一体化能力：
1. 浏览器打开 `http://localhost:8080/` 时，`game_server` 会自动下发 `web/index.html` 与 WASM 二进制，并注入 `Cross-Origin-Opener-Policy` 和 `Cross-Origin-Embedder-Policy` 头，支持高精度计时与多线程。
2. 前端只需直接连接 `ws://localhost:8080/ws`：
   - **开发模式**：发送/接收 JSON 文本，在浏览器 Network > WS 面板中直接可视化调试。
   - **生产模式**：发送二进制包，服务端自动无缝切换为 Postcard 解码，包体压缩 80%+。
