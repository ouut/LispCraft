//! # lc_server: Industrial Axum HTTP and WebSocket gateway server for LispCraft 3D
//!
//! Layer 2 server executable integrating `lc_core`, `lc_game`, `game_net`, and `game_server`.
//! Hosts the single-port HTTP (WASM/HTML) and WebSocket gateway, runs authoritative
//! Scheme simulation loop at 20 TPS, and handles per-client distance-culled snapshots.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use game_net::ServerEnvelope;
use game_server::{
    start_server, ServerConfig, ServerHandle, SessionEvent, StaticAssetSource,
};
use lc_core::{
    player_id_to_name, ClientState, LispEvent, LispGameCommand, LispMotionInput,
};
use lc_game::SchemeGame;

/// Command-line arguments configuration.
#[derive(Debug, PartialEq, Clone)]
pub struct ServerArgs {
    pub script_path: String,
    pub port: u16,
    pub show_help: bool,
}

/// Parses CLI arguments and environment variables with full fallback support.
pub fn parse_args(args: &[String]) -> Result<ServerArgs, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(ServerArgs {
            script_path: "games/space_arena.scm".to_string(),
            port: 8087,
            show_help: true,
        });
    }

    let mut script_path = "games/space_arena.scm".to_string();
    let mut port: u16 = std::env::var("PORT")
        .or_else(|_| std::env::var("HTTP_PORT"))
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8087);

    let mut positional = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-p"
            || arg == "--port"
            || arg == "-port"
            || arg == "--ws-port"
            || arg == "--http-port"
        {
            if i + 1 < args.len() {
                match args[i + 1].parse::<u16>() {
                    Ok(p) => {
                        port = p;
                        i += 1;
                    }
                    Err(_) => {
                        return Err(format!("Invalid port value: '{}'", args[i + 1]));
                    }
                }
            } else {
                return Err(format!("Missing port value after '{}'", arg));
            }
        } else if let Some(p_str) = arg
            .strip_prefix("--port=")
            .or_else(|| arg.strip_prefix("-port="))
            .or_else(|| arg.strip_prefix("--ws-port="))
            .or_else(|| arg.strip_prefix("-p="))
        {
            match p_str.parse::<u16>() {
                Ok(p) => port = p,
                Err(_) => return Err(format!("Invalid port value: '{}'", p_str)),
            }
        } else if !arg.starts_with('-') {
            positional.push(arg.clone());
        }
        i += 1;
    }

    if let Some(first) = positional.first() {
        if let Ok(p) = first.parse::<u16>() {
            port = p;
        } else {
            script_path = first.clone();
            if let Some(second) = positional.get(1) {
                if let Ok(p) = second.parse::<u16>() {
                    port = p;
                }
            }
        }
    }

    Ok(ServerArgs {
        script_path,
        port,
        show_help: false,
    })
}

/// Resolves static web assets directory across common relative and absolute paths.
pub fn find_web_dir() -> PathBuf {
    let mut candidates = vec![
        PathBuf::from("web"),
        PathBuf::from("../web"),
        PathBuf::from("../../web"),
        PathBuf::from("/workspace/LispCraft/web"),
    ];

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(manifest_dir);
        candidates.push(manifest_path.join("../../web"));
        candidates.push(manifest_path.join("../web"));
        candidates.push(manifest_path.join("web"));
    }

    for c in &candidates {
        if c.exists() && c.is_dir() {
            return c.clone();
        }
    }
    PathBuf::from("web")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    let parsed_args = match parse_args(&args) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };

    if parsed_args.show_help {
        println!("====================================================");
        println!("  LispCraft 3D Industrial Gateway Server (v0.2.0)");
        println!("====================================================");
        println!(
            "Usage: {} [GAME_SCRIPT] [PORT] [OPTIONS]",
            args.first().map(|s| s.as_str()).unwrap_or("server")
        );
        println!();
        println!("Arguments:");
        println!("  [GAME_SCRIPT]                  Scheme script file (default: games/space_arena.scm)");
        println!("  [PORT]                         Server port number");
        println!();
        println!("Options:");
        println!("  -p, --port, --ws-port <PORT>   Server port (default: 8087, or PORT env)");
        println!("  --port=<PORT>                  Server port assignment syntax");
        println!("  -h, --help                     Print help information");
        return Ok(());
    }

    let script_path = parsed_args.script_path;
    let port = parsed_args.port;
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
    let web_dir = find_web_dir();

    println!("====================================================");
    println!("  LispCraft 3D Industrial Gateway Server (v0.2.0)");
    println!("====================================================");
    println!(" Script:              {}", script_path);
    println!(" Web Assets:          {}", web_dir.display());
    println!(" Single-Port Gateway:  http://127.0.0.1:{}/", port);
    println!(" WebSocket Endpoint:   ws://127.0.0.1:{}/ws", port);

    // 1. 初始化 Scheme 游戏世界
    let game = Arc::new(
        SchemeGame::new(&script_path)
            .map_err(|e| format!("Scheme initialization error: {}", e))?,
    );

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

    // 优雅停机信号监听 (Ctrl+C)
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            println!("\n[Server] Shutdown signal (Ctrl+C) received.");
            let _ = shutdown_tx.send(true);
        }
    });

    while !*shutdown_rx.borrow() {
        let tick_start = Instant::now();
        let dt = tick_start
            .duration_since(last_tick_time)
            .as_secs_f32()
            .min(0.1);
        last_tick_time = tick_start;

        server.increment_server_tick();

        // (a) 会话生命周期事件处理
        for event in server.drain_session_events() {
            match event {
                SessionEvent::PlayerJoined { player_id, token } => {
                    let name = player_id_to_name(player_id);
                    println!(
                        "[Session] PlayerJoined: {} (pid={}, token={})",
                        name, player_id, token
                    );
                    game.on_join(&name);

                    server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                        from: "System".to_string(),
                        message: format!("{} joined the galaxy!", name),
                    }));
                }
                SessionEvent::PlayerReconnected { player_id } => {
                    let name = player_id_to_name(player_id);
                    println!("[Session] PlayerReconnected: {} (pid={})", name, player_id);
                }
                SessionEvent::PlayerDisconnected {
                    player_id,
                    grace_period_secs,
                } => {
                    let name = player_id_to_name(player_id);
                    println!(
                        "[Session] PlayerDisconnected: {} (grace period {}s)",
                        name, grace_period_secs
                    );
                }
                SessionEvent::PlayerTerminated { player_id } => {
                    let name = player_id_to_name(player_id);
                    println!("[Session] PlayerTerminated: {} (pid={})", name, player_id);
                    game.on_leave(&name);

                    server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                        from: "System".to_string(),
                        message: format!("{} departed from the world.", name),
                    }));
                }
            }
        }

        // (b) 拉取高频移动输入 (Latest-Only Latch)
        for (pid, motion) in server.drain_inputs() {
            let name = player_id_to_name(pid);
            game.on_move(&name, motion.dx, motion.dz, motion.yaw);
        }

        // (c) 拉取离散命令队列 (FIFO Queue)
        for (pid, cmds) in server.drain_commands() {
            let name = player_id_to_name(pid);
            for cmd in cmds {
                match cmd {
                    LispGameCommand::Action { action_id } => {
                        let expr = format!("action {}", action_id);
                        let output = game.on_command(&name, &expr);
                        server.send_to(
                            pid,
                            ServerEnvelope::Event(LispEvent::CommandEcho {
                                success: true,
                                output,
                            }),
                        );
                    }
                    LispGameCommand::SchemeCommand(expr) => {
                        let output = game.on_command(&name, &expr);

                        // 广播指令：以 "say " 或 "all " 开头
                        if let Some(content) =
                            expr.strip_prefix("say ").or_else(|| expr.strip_prefix("all "))
                        {
                            let broadcast_text = format!("{}: {}", name, content.trim());
                            server.broadcast(ServerEnvelope::Event(LispEvent::CommandEcho {
                                success: true,
                                output: broadcast_text,
                            }));
                            server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                                from: name.clone(),
                                message: content.trim().to_string(),
                            }));
                        } else {
                            server.send_to(
                                pid,
                                ServerEnvelope::Event(LispEvent::CommandEcho {
                                    success: true,
                                    output,
                                }),
                            );
                        }

                        if expr.starts_with("spawn ") {
                            server.broadcast(ServerEnvelope::Event(LispEvent::CommandEcho {
                                success: true,
                                output: format!("[World] {} spawned an entity!", name),
                            }));
                        }
                    }
                }
            }
        }

        // (d) 物理与 Scheme 仿真步进
        game.tick(dt);

        // (e) 针对每个玩家的 25m 裁剪视口，通过 server.send_to 单播下发 Snapshot
        let active_pids = server.session_manager().all_active_player_ids();
        for pid in active_pids {
            let name = player_id_to_name(pid);
            if let Some(state) = game.get_client_state(&name) {
                server.send_to(pid, ServerEnvelope::Snapshot(state));
            }
        }

        // (f) 保持 20 TPS 周期 (50ms interval)
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            let sleep_dur = tick_duration - elapsed;
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    break;
                }
                _ = tokio::time::sleep(sleep_dur) => {}
            }
        }
    }

    println!("[Server] Main simulation loop stopped. Shutting down gateway...");
    server.shutdown();
    println!("[Server] Gateway shutdown complete.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_dir_finder() {
        let dir = find_web_dir();
        assert!(dir.exists(), "Web directory should exist at {:?}", dir);
    }

    #[test]
    fn test_parse_args_defaults() {
        let args = vec!["server".to_string()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.script_path, "games/space_arena.scm");
        assert_eq!(parsed.port, 8087);
        assert!(!parsed.show_help);
    }

    #[test]
    fn test_parse_args_help() {
        let args = vec!["server".to_string(), "-h".to_string()];
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.show_help);

        let args2 = vec!["server".to_string(), "--help".to_string()];
        let parsed2 = parse_args(&args2).unwrap();
        assert!(parsed2.show_help);
    }

    #[test]
    fn test_parse_args_positional() {
        let args = vec!["server".to_string(), "games/dungeon.scm".to_string()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.script_path, "games/dungeon.scm");
        assert_eq!(parsed.port, 8087);

        let args2 = vec![
            "server".to_string(),
            "games/dungeon.scm".to_string(),
            "9000".to_string(),
        ];
        let parsed2 = parse_args(&args2).unwrap();
        assert_eq!(parsed2.script_path, "games/dungeon.scm");
        assert_eq!(parsed2.port, 9000);

        let args3 = vec!["server".to_string(), "9000".to_string()];
        let parsed3 = parse_args(&args3).unwrap();
        assert_eq!(parsed3.script_path, "games/space_arena.scm");
        assert_eq!(parsed3.port, 9000);
    }

    #[test]
    fn test_parse_args_flags() {
        let args = vec![
            "server".to_string(),
            "-p".to_string(),
            "8090".to_string(),
            "games/dungeon.scm".to_string(),
        ];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.script_path, "games/dungeon.scm");
        assert_eq!(parsed.port, 8090);

        let args2 = vec![
            "server".to_string(),
            "--port".to_string(),
            "8091".to_string(),
        ];
        let parsed2 = parse_args(&args2).unwrap();
        assert_eq!(parsed2.port, 8091);

        let args3 = vec![
            "server".to_string(),
            "--port=8087".to_string(),
            "games/space_arena.scm".to_string(),
        ];
        let parsed3 = parse_args(&args3).unwrap();
        assert_eq!(parsed3.script_path, "games/space_arena.scm");
        assert_eq!(parsed3.port, 8087);

        let args4 = vec![
            "server".to_string(),
            "-port".to_string(),
            "8092".to_string(),
        ];
        let parsed4 = parse_args(&args4).unwrap();
        assert_eq!(parsed4.port, 8092);

        let args5 = vec![
            "server".to_string(),
            "--ws-port".to_string(),
            "8093".to_string(),
        ];
        let parsed5 = parse_args(&args5).unwrap();
        assert_eq!(parsed5.port, 8093);
    }

    #[test]
    fn test_scheme_game_integration() {
        let game = SchemeGame::new("games/space_arena.scm").expect("Failed to initialize game");
        game.on_join("player_1");
        game.on_move("player_1", 1.0, 0.0, 0.0);
        let echo = game.on_command("player_1", "heal");
        assert!(echo.contains("Health") || echo.contains("100%"));

        let state = game.get_client_state("player_1").expect("State should exist");
        assert_eq!(state.client_id, "player_1");
        assert_eq!(state.hp, 100.0);
        game.on_leave("player_1");
    }
}
