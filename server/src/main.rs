pub mod net_bridge;
pub mod scheme_game;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use game_net::ServerEnvelope;
use game_server::{
    start_server, ServerConfig, ServerHandle, SessionEvent, StaticAssetSource,
};
use shared::ClientState;

use net_bridge::{
    player_id_to_name, LispEvent, LispGameCommand, LispMotionInput,
};
use scheme_game::SchemeGame;

pub fn find_web_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("web"),
        PathBuf::from("../web"),
        PathBuf::from("/workspace/LispCraft/web"),
    ];
    for c in &candidates {
        if c.exists() && c.is_dir() {
            return c.clone();
        }
    }
    PathBuf::from("web")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "games/space_arena.scm".to_string());

    let port: u16 = std::env::var("PORT")
        .or_else(|_| std::env::var("HTTP_PORT"))
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
    let web_dir = find_web_dir();

    println!("====================================================");
    println!("  LispCraft 3D Industrial Gateway Server (v0.2.0)");
    println!("====================================================");
    println!(" Script:             {}", script_path);
    println!(" Web Assets:         {}", web_dir.display());
    println!(" Single-Port Gateway: http://127.0.0.1:{}/", port);
    println!(" WebSocket Endpoint:  ws://127.0.0.1:{}/ws", port);

    // 1. 初始化 Scheme 游戏世界
    let game = Arc::new(SchemeGame::new(&script_path)?);

    // 2. 配置单端口网关
    let config = ServerConfig::new(bind_addr)
        .with_static_assets(StaticAssetSource::FileSystem(web_dir))
        .with_heartbeat_timeout_secs(10)
        .with_grace_period_secs(60);

    // 3. 启动通用游戏服务端
    let server: ServerHandle<LispMotionInput, LispGameCommand, ClientState, LispEvent> =
        start_server(config).await?;

    println!(" Server bound to: {}", server.local_addr());
    println!(" Ready for connections! Press Ctrl+C to exit.");

    // 4. 20 TPS (50ms) 物理仿真与空间裁剪单播下发循环
    let tick_duration = Duration::from_millis(50);
    let mut last_tick_time = Instant::now();

    loop {
        let tick_start = Instant::now();
        let dt = tick_start.duration_since(last_tick_time).as_secs_f32().min(0.1);
        last_tick_time = tick_start;

        // (a) 拉取会话生命周期事件 (玩家加入、重连、断开、注销)
        for event in server.drain_session_events() {
            match event {
                SessionEvent::PlayerJoined { player_id, token } => {
                    let player_name = player_id_to_name(player_id);
                    println!(
                        "[Session] PlayerJoined: {} (pid={}, token={})",
                        player_name, player_id, token
                    );
                    game.on_join(&player_name);

                    server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                        from: "System".to_string(),
                        message: format!("{} joined the galaxy!", player_name),
                    }));
                }
                SessionEvent::PlayerReconnected { player_id } => {
                    let player_name = player_id_to_name(player_id);
                    println!("[Session] PlayerReconnected: {} (pid={})", player_name, player_id);
                }
                SessionEvent::PlayerDisconnected { player_id, grace_period_secs } => {
                    let player_name = player_id_to_name(player_id);
                    println!(
                        "[Session] PlayerDisconnected: {} (grace period {}s)",
                        player_name, grace_period_secs
                    );
                }
                SessionEvent::PlayerTerminated { player_id } => {
                    let player_name = player_id_to_name(player_id);
                    println!("[Session] PlayerTerminated: {}", player_name);
                    game.on_leave(&player_name);

                    server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                        from: "System".to_string(),
                        message: format!("{} departed from the world.", player_name),
                    }));
                }
            }
        }

        // (b) 拉取高频输入 (Latest-Only Latch)
        for (player_id, input) in server.drain_inputs() {
            let player_name = player_id_to_name(player_id);
            if input.dx != 0.0 || input.dz != 0.0 {
                game.on_move(&player_name, input.dx, input.dz, input.yaw);
            }
        }

        // (c) 拉取离散命令队列 (FIFO Queue)
        for (player_id, commands) in server.drain_commands() {
            let player_name = player_id_to_name(player_id);
            for cmd in commands {
                match cmd {
                    LispGameCommand::Action { action_id } => {
                        let echo = game.on_command(&player_name, &format!("action {}", action_id));
                        server.send_to(
                            player_id,
                            ServerEnvelope::Event(LispEvent::CommandEcho {
                                success: true,
                                output: echo,
                            }),
                        );
                    }
                    LispGameCommand::SchemeCommand(cmd_str) => {
                        let echo = game.on_command(&player_name, &cmd_str);

                        // 广播指令：以 "say " 或 "all " 开头
                        if let Some(content) = cmd_str.strip_prefix("say ").or_else(|| cmd_str.strip_prefix("all ")) {
                            let broadcast_text = format!("{}: {}", player_name, content.trim());
                            server.broadcast(ServerEnvelope::Event(LispEvent::CommandEcho {
                                success: true,
                                output: broadcast_text,
                            }));
                        } else {
                            server.send_to(
                                player_id,
                                ServerEnvelope::Event(LispEvent::CommandEcho {
                                    success: true,
                                    output: echo,
                                }),
                            );
                        }

                        if cmd_str.starts_with("spawn ") {
                            server.broadcast(ServerEnvelope::Event(LispEvent::CommandEcho {
                                success: true,
                                output: format!("[World] {} spawned an entity!", player_name),
                            }));
                        }
                    }
                }
            }
        }

        // (d) 驱动 Scheme 物理仿真
        game.tick(dt);

        // (e) 针对每个玩家的 25m 裁剪视口，通过 server.send_to 单播下发 Snapshot
        let active_pids = server.session_manager().all_active_player_ids();
        for pid in active_pids {
            let player_name = player_id_to_name(pid);
            if let Some(state) = game.get_client_state(&player_name) {
                server.send_to(pid, ServerEnvelope::Snapshot(state));
            }
        }

        // (f) 保持 20 TPS 周期
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_dir_finder() {
        let dir = find_web_dir();
        assert!(dir.exists(), "Web dir should exist");
    }

    #[test]
    fn test_scheme_game_lifecycle() {
        let game = SchemeGame::new("games/space_arena.scm").expect("Failed to initialize game");
        game.on_join("tester_1");
        game.on_move("tester_1", 1.0, 0.0, 0.0);
        let echo = game.on_command("tester_1", "heal");
        assert!(echo.contains("Health restored") || echo.contains("100%"));
        let state = game.get_client_state_json("tester_1");
        assert!(state.contains("\"client_id\":\"tester_1\""));
        game.on_leave("tester_1");

        // 测试地牢模式脚本
        let dungeon = SchemeGame::new("games/dungeon.scm").expect("Failed to initialize dungeon");
        dungeon.on_join("tester_2");
        let echo2 = dungeon.on_command("tester_2", "torch");
        assert!(echo2.contains("torch") || echo2.contains("Placed"));
        let state2 = dungeon.get_client_state_json("tester_2");
        assert!(state2.contains("\"client_id\":\"tester_2\""));
        dungeon.on_leave("tester_2");
    }

    #[test]
    fn test_scheme_game_eval_and_precompiled_tick() {
        let game = SchemeGame::new("games/space_arena.scm").expect("Failed to initialize game");
        // Test high-frequency tick directly executing safe_call_function
        game.tick(0.05);

        // Test safe_eval for Scheme expressions
        let eval_res = game.safe_eval("(+ 10 20)", 100).expect("Eval failed");
        assert!(eval_res.contains("30"));
    }
}
