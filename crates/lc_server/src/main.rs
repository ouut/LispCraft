//! # lc_server: Industrial Axum HTTP and WebSocket gateway server for LispCraft 3D
//!
//! Layer 2 server executable integrating `lc_core`, `lc_game`, `game_net`, and `game_server`.
//! Hosts the single-port HTTP (WASM/HTML) and WebSocket gateway, runs authoritative
//! Scheme simulation loop at 20 TPS across multiple dynamically mounted games,
//! and handles per-client distance-culled snapshots and Roblox-style RemoteEvents.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

use game_net::{PlayerId, ServerEnvelope};
use game_server::{
    start_server, ServerConfig, ServerHandle, SessionEvent, StaticAssetSource,
};
use lc_core::{
    name_to_player_id, player_id_to_name, ClientState, LispEvent, LispGameCommand,
    LispMotionInput,
};
use lc_game::GameManager;

/// Command-line arguments configuration.
#[derive(Debug, PartialEq, Clone)]
pub struct ServerArgs {
    pub games_dir: String,
    pub script_path: String,
    pub default_game: Option<String>,
    pub port: u16,
    pub show_help: bool,
}

/// Parses CLI arguments and environment variables with full fallback support.
pub fn parse_args(args: &[String]) -> Result<ServerArgs, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(ServerArgs {
            games_dir: "games".to_string(),
            script_path: "games/space_arena.scm".to_string(),
            default_game: None,
            port: 8087,
            show_help: true,
        });
    }

    let mut games_dir = "games".to_string();
    let mut script_path = "games/space_arena.scm".to_string();
    let mut default_game = None;
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
            let path = Path::new(first);
            if first.ends_with(".scm") {
                script_path = first.clone();
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    default_game = Some(stem.to_string());
                }
                if let Some(parent) = path.parent().and_then(|p| p.to_str()) {
                    if !parent.is_empty() {
                        games_dir = parent.to_string();
                    }
                }
            } else {
                games_dir = first.clone();
                script_path = format!("{}/space_arena.scm", first.trim_end_matches('/'));
            }

            if let Some(second) = positional.get(1) {
                if let Ok(p) = second.parse::<u16>() {
                    port = p;
                }
            }
        }
    }

    Ok(ServerArgs {
        games_dir,
        script_path,
        default_game,
        port,
        show_help: false,
    })
}

/// Extracts `game_id` and `clean_token` from client Auth packet token.
///
/// If token contains a slash or colon (e.g. `"space_arena/guest_1"` or `"dungeon:player_2"`),
/// extracts `game_id` and clean `token`. Otherwise returns `(default_game, token)`.
pub fn extract_game_id_and_token(token: &str, default_game: &str) -> (String, String) {
    if let Some((game, clean)) = token.split_once('/') {
        let clean_game = game.trim();
        let clean_token = clean.trim();
        if !clean_game.is_empty() {
            return (clean_game.to_string(), clean_token.to_string());
        }
    } else if let Some((game, clean)) = token.split_once(':') {
        let clean_game = game.trim();
        let clean_token = clean.trim();
        if !clean_game.is_empty() {
            return (clean_game.to_string(), clean_token.to_string());
        }
    }
    (default_game.to_string(), token.to_string())
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

/// Shared state for Axum gateway routes.
#[derive(Clone)]
pub struct AppState {
    pub web_dir: PathBuf,
    pub internal_ws_url: String,
}

/// Builds the Axum gateway router with static assets, dynamic game routing, and security headers.
pub fn build_router(app_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(root_index_handler))
        .route("/ws", get(ws_proxy_handler))
        .route("/:game_id/:token", get(game_token_handler))
        .route("/:file", get(static_file_handler))
        .layer(middleware::from_fn(coop_coep_middleware))
        .with_state(app_state)
}

/// Serves a static file from disk with correct MIME content-type.
async fn serve_file(path: &Path) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mime = match path.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("wasm") => "application/wasm",
                Some("js") | Some("mjs") => "application/javascript",
                Some("css") => "text/css",
                Some("json") => "application/json",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime)],
                bytes,
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "404 Not Found",
        )
            .into_response(),
    }
}

/// Handler for `GET /` -> serves `web/index.html`.
async fn root_index_handler(State(state): State<Arc<AppState>>) -> Response {
    let index_file = state.web_dir.join("index.html");
    serve_file(&index_file).await
}

/// Handler for `GET /:game_id/:token`.
///
/// If `:token` resolves to an existing static asset in `web_dir` (e.g. `/space_arena/gl.js`),
/// serves that static asset file. Otherwise serves `web/index.html` for client-side routing.
async fn game_token_handler(
    AxumPath((_game_id, token)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let candidate = state.web_dir.join(&token);
    if candidate.is_file() {
        return serve_file(&candidate).await;
    }
    let index_file = state.web_dir.join("index.html");
    serve_file(&index_file).await
}

/// Handler for root-level static asset files (`GET /:file`).
async fn static_file_handler(
    AxumPath(file): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let candidate = state.web_dir.join(&file);
    if candidate.is_file() {
        serve_file(&candidate).await
    } else {
        (
            StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "404 Not Found",
        )
            .into_response()
    }
}

/// Security headers middleware: injects COOP, COEP, and CORS headers.
async fn coop_coep_middleware(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-embedder-policy"),
        HeaderValue::from_static("require-corp"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        HeaderValue::from_static("*"),
    );
    resp
}

/// WebSocket proxy handler forwarding public WebSocket clients to internal `game_server`.
async fn ws_proxy_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let internal_ws_url = state.internal_ws_url.clone();
    ws.on_upgrade(move |client_socket| handle_ws_proxy(client_socket, internal_ws_url))
}

/// Transparent bidirectional proxy between client WebSocket and internal `game_server`.
async fn handle_ws_proxy(client_socket: WebSocket, internal_url: String) {
    let Ok((server_stream, _)) = tokio_tungstenite::connect_async(&internal_url).await else {
        return;
    };
    let (mut client_write, mut client_read) = client_socket.split();
    let (mut server_write, mut server_read) = server_stream.split();

    let client_to_server = async move {
        while let Some(msg_res) = client_read.next().await {
            let Ok(msg) = msg_res else {
                break;
            };
            match msg {
                AxumMessage::Text(t) => {
                    if server_write.send(TungsteniteMessage::Text(t)).await.is_err() {
                        break;
                    }
                }
                AxumMessage::Binary(b) => {
                    if server_write.send(TungsteniteMessage::Binary(b)).await.is_err() {
                        break;
                    }
                }
                AxumMessage::Ping(p) => {
                    if server_write.send(TungsteniteMessage::Ping(p)).await.is_err() {
                        break;
                    }
                }
                AxumMessage::Pong(p) => {
                    if server_write.send(TungsteniteMessage::Pong(p)).await.is_err() {
                        break;
                    }
                }
                AxumMessage::Close(c) => {
                    let close_frame = c.map(|f| tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason,
                    });
                    let _ = server_write.send(TungsteniteMessage::Close(close_frame)).await;
                    break;
                }
            }
        }
    };

    let server_to_client = async move {
        while let Some(msg_res) = server_read.next().await {
            let Ok(msg) = msg_res else {
                break;
            };
            match msg {
                TungsteniteMessage::Text(t) => {
                    if client_write.send(AxumMessage::Text(t)).await.is_err() {
                        break;
                    }
                }
                TungsteniteMessage::Binary(b) => {
                    if client_write.send(AxumMessage::Binary(b)).await.is_err() {
                        break;
                    }
                }
                TungsteniteMessage::Ping(p) => {
                    if client_write.send(AxumMessage::Ping(p)).await.is_err() {
                        break;
                    }
                }
                TungsteniteMessage::Pong(p) => {
                    if client_write.send(AxumMessage::Pong(p)).await.is_err() {
                        break;
                    }
                }
                TungsteniteMessage::Close(c) => {
                    let close_frame = c.map(|f| axum::extract::ws::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason,
                    });
                    let _ = client_write.send(AxumMessage::Close(close_frame)).await;
                    break;
                }
                TungsteniteMessage::Frame(_) => {}
            }
        }
    };

    tokio::select! {
        _ = client_to_server => {}
        _ = server_to_client => {}
    }
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
            "Usage: {} [games_dir] [--port 8087] [OPTIONS]",
            args.first().map(|s| s.as_str()).unwrap_or("server")
        );
        println!();
        println!("Arguments:");
        println!("  [games_dir]                    Games directory or script (default: games)");
        println!();
        println!("Options:");
        println!("  -p, --port, --ws-port <PORT>   Server port (default: 8087, or PORT env)");
        println!("  --port=<PORT>                  Server port assignment syntax");
        println!("  -h, --help                     Print help information");
        return Ok(());
    }

    let port = parsed_args.port;
    let web_dir = find_web_dir();

    // 1. 初始化多游戏动态挂载管理器 (GameManager)
    let games_dir = parsed_args.games_dir;
    let manager = match GameManager::new(&games_dir) {
        Ok(m) => m,
        Err(_) => GameManager::new("games")
            .map_err(|e| format!("GameManager initialization error: {}", e))?,
    };
    let manager = Arc::new(manager);

    let default_game_id = parsed_args
        .default_game
        .filter(|g| manager.has_game(g))
        .unwrap_or_else(|| manager.default_game_id());

    println!("====================================================");
    println!("  LispCraft 3D Industrial Gateway Server (v0.2.0)");
    println!("====================================================");
    println!(" Games Root:          {}", games_dir);
    println!(" Mounted Games:       {:?}", manager.all_game_ids());
    println!(" Default Game:        {}", default_game_id);
    println!(" Web Assets:          {}", web_dir.display());
    println!(" Single-Port Gateway: http://127.0.0.1:{}/", port);
    println!(" WebSocket Endpoint:  ws://127.0.0.1:{}/ws", port);

    // 2. 启动内部工业级游戏服务端 (game_server on loopback ephemeral port)
    let internal_bind = SocketAddr::from(([127, 0, 0, 1], 0));
    let config = ServerConfig::new(internal_bind)
        .with_static_assets(StaticAssetSource::FileSystem(web_dir.clone()))
        .with_heartbeat_timeout_secs(10)
        .with_grace_period_secs(60);

    let server: ServerHandle<LispMotionInput, LispGameCommand, ClientState, LispEvent> =
        start_server(config).await?;
    let internal_addr = server.local_addr();
    let internal_ws_url = format!("ws://127.0.0.1:{}/ws", internal_addr.port());

    // 3. 构建并启动外部单端口 Axum 动态路由网关
    let app_state = Arc::new(AppState {
        web_dir: web_dir.clone(),
        internal_ws_url,
    });

    let app = build_router(app_state);

    let public_bind = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(public_bind).await?;
    println!(" Public gateway bound to: {}", listener.local_addr()?);
    println!(" Ready for connections! Press Ctrl+C to exit.");

    // 优雅停机信号监听 (Ctrl+C)
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            println!("\n[Server] Shutdown signal (Ctrl+C) received.");
            let _ = shutdown_tx.send(true);
        }
    });

    let mut gateway_shutdown_rx = shutdown_rx.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = gateway_shutdown_rx.changed().await;
            })
            .await;
    });

    // 4. 20 TPS (50ms) 物理仿真、多游戏调度与空间裁剪单播下发循环
    let tick_duration = Duration::from_millis(50);
    let mut last_tick_time = Instant::now();

    // 追踪每个玩家当前所在的 game_id: player_id -> game_id
    let mut player_games: HashMap<PlayerId, String> = HashMap::new();

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
                    let (target_game, clean_token) =
                        extract_game_id_and_token(&token, &default_game_id);
                    let game_id = if manager.has_game(&target_game) {
                        target_game
                    } else {
                        default_game_id.clone()
                    };

                    player_games.insert(player_id, game_id.clone());
                    let name = player_id_to_name(player_id);

                    println!(
                        "[Session] PlayerJoined: {} (pid={}, game={}, token={})",
                        name, player_id, game_id, clean_token
                    );

                    // 下发该游戏的 client VFS 脚本包 (ServerEnvelope::BundleScript)
                    if let Some(bundle_event) = manager.get_bundle_event(&game_id) {
                        server.send_to(player_id, ServerEnvelope::Event(bundle_event));
                    } else {
                        server.send_to(
                            player_id,
                            ServerEnvelope::Event(LispEvent::BundleScript {
                                game_id: game_id.clone(),
                                vfs: HashMap::new(),
                            }),
                        );
                    }

                    // 调用该游戏的 on_join 生命周期
                    if let Some(game) = manager.get_game(&game_id) {
                        game.on_join(&name);
                    }

                    // 广播加入通知
                    server.broadcast(ServerEnvelope::Event(LispEvent::ChatBroadcast {
                        from: "System".to_string(),
                        message: format!("{} joined the galaxy!", name),
                    }));
                }
                SessionEvent::PlayerReconnected { player_id } => {
                    let name = player_id_to_name(player_id);
                    let game_id = player_games
                        .get(&player_id)
                        .cloned()
                        .unwrap_or_else(|| default_game_id.clone());
                    println!(
                        "[Session] PlayerReconnected: {} (pid={}, game={})",
                        name, player_id, game_id
                    );

                    if let Some(bundle_event) = manager.get_bundle_event(&game_id) {
                        server.send_to(player_id, ServerEnvelope::Event(bundle_event));
                    }
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

                    if let Some(game_id) = player_games.remove(&player_id) {
                        if let Some(game) = manager.get_game(&game_id) {
                            game.on_leave(&name);
                        }
                    }

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
            if let Some(game_id) = player_games.get(&pid) {
                if let Some(game) = manager.get_game(game_id) {
                    game.on_move(&name, motion.dx, motion.dz, motion.yaw);
                }
            }
        }

        // (c) 拉取离散命令队列 (FIFO Queue) 与 RemoteEvent
        for (pid, cmds) in server.drain_commands() {
            let name = player_id_to_name(pid);
            let game_id = player_games
                .get(&pid)
                .cloned()
                .unwrap_or_else(|| default_game_id.clone());
            let maybe_game = manager.get_game(&game_id);

            for cmd in cmds {
                match cmd {
                    LispGameCommand::Action { action_id } => {
                        if let Some(ref game) = maybe_game {
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
                    }
                    LispGameCommand::SchemeCommand(expr) => {
                        if let Some(ref game) = maybe_game {
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
                    LispGameCommand::RemoteEvent { event_name, payload } => {
                        if let Some(ref game) = maybe_game {
                            game.on_remote_event(&name, &event_name, &payload);
                        }
                    }
                }
            }
        }

        // (d) 仿真步进：驱动所有已挂载游戏 (20 TPS)
        manager.tick_all(dt);

        // (e) 处理各游戏向客户端派发的 Outgoing RemoteEvents
        for game_id in manager.all_game_ids() {
            if let Some(game) = manager.get_game(&game_id) {
                for (target_player, name, payload) in game.drain_outgoing_events() {
                    let envelope = ServerEnvelope::Event(LispEvent::RemoteEvent {
                        event_name: name,
                        payload,
                    });
                    if let Some(target) = target_player {
                        if let Some(target_pid) = name_to_player_id(&target) {
                            server.send_to(target_pid, envelope);
                        }
                    } else {
                        // 广播给该游戏内的所有活跃玩家
                        let game_pids: Vec<PlayerId> = player_games
                            .iter()
                            .filter_map(|(&p, gid)| if gid == &game_id { Some(p) } else { None })
                            .collect();
                        server.multicast(&game_pids, envelope);
                    }
                }
            }
        }

        // (f) 针对各游戏活跃玩家的 25m 裁剪视口，通过 server.send_to 单播下发 Snapshot
        let active_pids = server.session_manager().all_active_player_ids();
        for pid in active_pids {
            let name = player_id_to_name(pid);
            if let Some(game_id) = player_games.get(&pid) {
                if let Some(game) = manager.get_game(game_id) {
                    if let Some(state) = game.get_client_state(&name) {
                        server.send_to(pid, ServerEnvelope::Snapshot(state));
                    }
                }
            }
        }

        // (g) 保持 20 TPS 周期 (50ms interval)
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
    use lc_game::{resolve_games_dir, SchemeGame};

    #[test]
    fn test_web_dir_finder() {
        let dir = find_web_dir();
        assert!(dir.exists(), "Web directory should exist at {:?}", dir);
    }

    #[test]
    fn test_parse_args_defaults() {
        let args = vec!["server".to_string()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.games_dir, "games");
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
        assert_eq!(parsed.default_game, Some("dungeon".to_string()));
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

        let args4 = vec!["server".to_string(), "games".to_string()];
        let parsed4 = parse_args(&args4).unwrap();
        assert_eq!(parsed4.games_dir, "games");
        assert_eq!(parsed4.port, 8087);
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
    fn test_extract_game_id_and_token() {
        assert_eq!(
            extract_game_id_and_token("space_arena/guest_1", "space_arena"),
            ("space_arena".to_string(), "guest_1".to_string())
        );
        assert_eq!(
            extract_game_id_and_token("dungeon:player_2", "space_arena"),
            ("dungeon".to_string(), "player_2".to_string())
        );
        assert_eq!(
            extract_game_id_and_token("my_token_123", "space_arena"),
            ("space_arena".to_string(), "my_token_123".to_string())
        );
        assert_eq!(
            extract_game_id_and_token("", "space_arena"),
            ("space_arena".to_string(), "".to_string())
        );
    }

    #[tokio::test]
    async fn test_axum_http_routing() {
        use axum::body::Body;
        use tower::ServiceExt;

        let web_dir = find_web_dir();
        let app_state = Arc::new(AppState {
            web_dir,
            internal_ws_url: "ws://127.0.0.1:8087/ws".to_string(),
        });
        let app = build_router(app_state);

        // 1. GET / -> serves index.html
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));
        assert_eq!(
            resp.headers()
                .get("cross-origin-opener-policy")
                .unwrap()
                .to_str()
                .unwrap(),
            "same-origin"
        );
        assert_eq!(
            resp.headers()
                .get("cross-origin-embedder-policy")
                .unwrap()
                .to_str()
                .unwrap(),
            "require-corp"
        );

        // 2. GET /:game_id/:token -> serves index.html
        let req = Request::builder()
            .uri("/space_arena/guest_1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));

        let req = Request::builder()
            .uri("/dungeon/player_42")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));

        // 3. GET /gl.js -> serves static file
        let req = Request::builder()
            .uri("/gl.js")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("javascript"));

        // 4. GET /space_arena/gl.js -> scoped static file fallback
        let req = Request::builder()
            .uri("/space_arena/gl.js")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("javascript"));

        // 5. GET /not_exist.wasm -> 404
        let req = Request::builder()
            .uri("/not_exist.wasm")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_game_manager_multi_game() {
        let games_dir = resolve_games_dir("games");
        let manager = GameManager::new(&games_dir.to_string_lossy()).expect("Failed to init GameManager");
        assert!(manager.has_game("space_arena"));
        assert!(manager.has_game("dungeon"));
        assert_eq!(manager.default_game_id(), "space_arena");

        let game = manager.get_game("space_arena").expect("space_arena should be mounted");
        game.on_join("tester_1");
        game.on_move("tester_1", 1.0, 0.0, 0.0);
        let echo = game.on_command("tester_1", "heal");
        assert!(echo.contains("Health") || echo.contains("100%"));
        let state = game.get_client_state("tester_1").expect("State should exist");
        assert_eq!(state.client_id, "tester_1");
        game.on_leave("tester_1");
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
