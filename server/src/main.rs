mod http_server;
mod hub;
mod scheme_game;

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http_server::{find_web_dir, start_http_server};
use hub::MessageHub;
use scheme_game::SchemeGame;

use quad_net::quad_socket::server::{listen, Settings, SocketHandle};
use shared::{ClientMsg, ServerMsg};

#[derive(Default)]
struct ClientSession {
    id: Mutex<Option<String>>,
}

impl ClientSession {
    fn get_or_assign_id(
        &self,
        next_id: &AtomicUsize,
        hub: &MessageHub,
        game: &SchemeGame,
        handle: &mut SocketHandle,
    ) -> String {
        let mut id_lock = self.id.lock().unwrap();
        if let Some(ref existing) = *id_lock {
            return existing.clone();
        }

        let new_id = format!("player_{}", next_id.fetch_add(1, Ordering::SeqCst));
        *id_lock = Some(new_id.clone());

        hub.register(&new_id);
        game.on_join(&new_id);

        let active_count = hub.active_count();
        println!(
            "[Session] Client connected: {} (Active: {})",
            new_id, active_count
        );

        // 下发欢迎包
        let welcome = ServerMsg::Welcome {
            client_id: new_id.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&welcome) {
            let _ = handle.send(&bytes);
        }

        // 全服通知有新玩家加入
        hub.broadcast_except(
            &new_id,
            ServerMsg::CommandEcho {
                success: true,
                output: format!("[Broadcast] {} joined the world!", new_id),
            },
        );

        new_id
    }
}

fn print_usage(program: &str) {
    println!("====================================================");
    println!("      LispCraft Game Server (Rust + Steel Scheme)   ");
    println!("====================================================");
    println!("Usage: {} [GAME_SCRIPT] [HTTP_PORT] [OPTIONS]", program);
    println!();
    println!("Positional Arguments:");
    println!("  GAME_SCRIPT                 Path to Scheme game script (default: games/space_arena.scm)");
    println!("  HTTP_PORT                   Optional HTTP port (default: 8087)");
    println!();
    println!("Options:");
    println!("  -p, --http-port <PORT>      HTTP Web server port (default: 8087, or env HTTP_PORT)");
    println!("  --tcp-port <PORT>           TCP native port (default: 8088, or env TCP_PORT)");
    println!("  --ws-port <PORT>            WebSocket port (default: 8089, or env WS_PORT)");
    println!("  -h, --help                  Print help information");
    println!();
    println!("Examples:");
    println!("  {} games/space_arena.scm", program);
    println!("  {} games/space_arena.scm 8087", program);
    println!("  {} --http-port 8087 --tcp-port 8088 --ws-port 8089", program);
    println!("  HTTP_PORT=8087 {} games/dungeon.scm", program);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage(&args[0]);
        return;
    }

    let mut script_path = "games/space_arena.scm".to_string();
    let mut http_port = std::env::var("HTTP_PORT").unwrap_or_else(|_| "8087".to_string());
    let mut tcp_port = std::env::var("TCP_PORT").unwrap_or_else(|_| "8088".to_string());
    let mut ws_port = std::env::var("WS_PORT").unwrap_or_else(|_| "8089".to_string());

    let mut non_flag_args: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-p" | "--http-port" => {
                if i + 1 < args.len() {
                    http_port = args[i + 1].clone();
                    i += 1;
                }
            }
            "--tcp-port" => {
                if i + 1 < args.len() {
                    tcp_port = args[i + 1].clone();
                    i += 1;
                }
            }
            "--ws-port" => {
                if i + 1 < args.len() {
                    ws_port = args[i + 1].clone();
                    i += 1;
                }
            }
            other if !other.starts_with('-') => {
                non_flag_args.push(other.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(s) = non_flag_args.get(0) {
        script_path = s.clone();
    }
    if let Some(p) = non_flag_args.get(1) {
        if p.chars().all(|c| c.is_ascii_digit()) {
            http_port = p.clone();
        }
    }

    println!("====================================================");
    println!("      LispCraft Game Server (Rust + Steel Scheme)   ");
    println!("====================================================");
    println!("Loading Scheme game definition from: {}", script_path);

    let game = Arc::new(SchemeGame::new(&script_path).expect("Failed to initialize Scheme game"));
    let hub = Arc::new(MessageHub::new());
    let next_id = Arc::new(AtomicUsize::new(1));

    // 1. 启动内置 HTTP 静态 Web 服务器 (默认 8087，支持参数及 HTTP_PORT 环境变量)
    let web_root = find_web_dir();
    let http_addr = format!("0.0.0.0:{}", http_port);
    if let Err(e) = start_http_server(&http_addr, web_root) {
        eprintln!("[HTTP Server Error] Failed to bind {}: {:?}", http_addr, e);
    }

    // 2. 启动物理与游戏逻辑 Tick 线程 (20 TPS, 50ms)
    let tick_game = game.clone();
    std::thread::Builder::new()
        .name("logic-tick".to_string())
        .spawn(move || loop {
            let start = Instant::now();
            tick_game.tick(0.05);
            let elapsed = start.elapsed();
            if elapsed > Duration::from_millis(5) {
                eprintln!("[Tick Warning] Logic tick took {:?}", elapsed);
            }
            if elapsed < Duration::from_millis(50) {
                std::thread::sleep(Duration::from_millis(50) - elapsed);
            }
        })
        .expect("Failed to spawn logic tick thread");

    // 3. 配置 quad-net 网络服务端 (TCP 8088 / WebSocket 8089)
    let game_for_msg = game.clone();
    let game_for_timer = game.clone();
    let game_for_disc = game.clone();

    let hub_for_msg = hub.clone();
    let hub_for_timer = hub.clone();
    let hub_for_disc = hub.clone();

    let next_id_for_msg = next_id.clone();
    let next_id_for_timer = next_id.clone();

    let settings = Settings {
        on_message: move |handle: &mut SocketHandle, session: &mut ClientSession, data: Vec<u8>| {
            let client_id = session.get_or_assign_id(
                &next_id_for_msg,
                &hub_for_msg,
                &game_for_msg,
                handle,
            );

            if let Ok(msg) = serde_json::from_slice::<ClientMsg>(&data) {
                match msg {
                    ClientMsg::Move { dx, dz, yaw } => {
                        game_for_msg.on_move(&client_id, dx, dz, yaw);
                    }
                    ClientMsg::Action { action_id: _ } => {}
                    ClientMsg::Command(cmd) => {
                        println!("[Server] Command from {}: '{}'", client_id, cmd);

                        // 广播指令：以 "say " 或 "all " 开头
                        if let Some(content) = cmd.strip_prefix("say ").or_else(|| cmd.strip_prefix("all ")) {
                            let broadcast_text = format!("{}: {}", client_id, content.trim());
                            hub_for_msg.broadcast(ServerMsg::CommandEcho {
                                success: true,
                                output: broadcast_text,
                            });
                        }
                        // 在线玩家查询：list 或 who
                        else if cmd == "list" || cmd == "who" {
                            let players = hub_for_msg.active_players().join(", ");
                            let echo = ServerMsg::CommandEcho {
                                success: true,
                                output: format!("Online players ({}): {}", hub_for_msg.active_count(), players),
                            };
                            if let Ok(bytes) = serde_json::to_vec(&echo) {
                                let _ = handle.send(&bytes);
                            }
                        }
                        // 私信指令：tell <target> <message>
                        else if let Some(rest) = cmd.strip_prefix("tell ") {
                            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                            if parts.len() == 2 {
                                let target = parts[0].trim();
                                let whisper = parts[1].trim();
                                hub_for_msg.send_to(
                                    target,
                                    ServerMsg::CommandEcho {
                                        success: true,
                                        output: format!("[Whisper from {}]: {}", client_id, whisper),
                                    },
                                );
                                let _ = handle.send(
                                    &serde_json::to_vec(&ServerMsg::CommandEcho {
                                        success: true,
                                        output: format!("[Whisper to {}]: {}", target, whisper),
                                    })
                                    .unwrap_or_default(),
                                );
                            } else {
                                let _ = handle.send(
                                    &serde_json::to_vec(&ServerMsg::CommandEcho {
                                        success: false,
                                        output: "Usage: tell <target_id> <message>".to_string(),
                                    })
                                    .unwrap_or_default(),
                                );
                            }
                        }
                        // 其他游戏逻辑/管理命令，交由 Scheme 执行
                        else {
                            let result = game_for_msg.on_command(&client_id, &cmd);
                            let echo = ServerMsg::CommandEcho {
                                success: true,
                                output: result,
                            };
                            if let Ok(bytes) = serde_json::to_vec(&echo) {
                                let _ = handle.send(&bytes);
                            }

                            // 如果是生成实体的命令，向其他玩家同步广播提示
                            if cmd.starts_with("spawn ") {
                                hub_for_msg.broadcast_except(
                                    &client_id,
                                    ServerMsg::CommandEcho {
                                        success: true,
                                        output: format!("[World] {} spawned an entity!", client_id),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        },
        on_timer: move |handle: &mut SocketHandle, session: &ClientSession| {
            // 获取或自动分配新连接客户端 ID
            let client_id = session.get_or_assign_id(
                &next_id_for_timer,
                &hub_for_timer,
                &game_for_timer,
                handle,
            );

            // 1. 投递待发送的私信与广播包
            for msg in hub_for_timer.drain(&client_id) {
                if let Ok(bytes) = serde_json::to_vec(&msg) {
                    let _ = handle.send(&bytes);
                }
            }

            // 2. 状态包定时同步 (~25 TPS)
            let json_str = game_for_timer.get_client_state_json(&client_id);
            if !json_str.is_empty() && json_str != "{}" {
                let _ = handle.send(json_str.as_bytes());
            }
        },
        on_disconnect: move |session: &ClientSession| {
            if let Some(client_id) = session.id.lock().unwrap().take() {
                hub_for_disc.unregister(&client_id);
                game_for_disc.on_leave(&client_id);

                let remaining = hub_for_disc.active_count();
                println!(
                    "[Session] Client disconnected: {} (Active: {})",
                    client_id, remaining
                );

                hub_for_disc.broadcast(ServerMsg::CommandEcho {
                    success: true,
                    output: format!("[Broadcast] {} left the world.", client_id),
                });
            }
        },
        timer: Some(Duration::from_millis(40)), // ~25 TPS 状态同步
        _marker: std::marker::PhantomData,
    };

    let tcp_addr = format!("0.0.0.0:{}", tcp_port);
    let ws_addr = format!("0.0.0.0:{}", ws_port);
    println!(
        "[Server] Listening on TCP: {} (Native) & WebSocket: {} (WASM)",
        tcp_addr, ws_addr
    );
    println!("====================================================");
    println!("Ready for connections! Press Ctrl+C to exit.");

    listen(tcp_addr, ws_addr, settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_hub() {
        let hub = MessageHub::new();
        hub.register("p1");
        hub.register("p2");
        assert_eq!(hub.active_count(), 2);

        // 私信测试
        hub.send_to(
            "p1",
            ServerMsg::CommandEcho {
                success: true,
                output: "hello p1".to_string(),
            },
        );
        let p1_msgs = hub.drain("p1");
        assert_eq!(p1_msgs.len(), 1);
        assert!(hub.drain("p1").is_empty());

        // 广播测试
        hub.broadcast(ServerMsg::CommandEcho {
            success: true,
            output: "all".to_string(),
        });
        assert_eq!(hub.drain("p1").len(), 1);
        assert_eq!(hub.drain("p2").len(), 1);

        hub.unregister("p1");
        assert_eq!(hub.active_count(), 1);
    }

    #[test]
    fn test_scheme_game() {
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
    fn test_mime_types() {
        use std::path::Path;
        assert_eq!(http_server::mime_type_for_path(Path::new("client.wasm")), "application/wasm");
        assert_eq!(http_server::mime_type_for_path(Path::new("gl.js")), "application/javascript");
        assert_eq!(http_server::mime_type_for_path(Path::new("index.html")), "text/html; charset=utf-8");
    }
}
