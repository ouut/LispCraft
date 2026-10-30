//! # LispCraft 3D Client
//!
//! Client binary for LispCraft 3D, supporting both Native and WASM32 targets.
//! Dual-mode Macroquad Host driven 100% by Scheme rendering:
//! - Mode A (Online Mode): Connects to game server, syncs BundleScript and state, drains events
//! - Mode B (Offline Standalone Hot-Reload Mode): Loads directory and hot-reloads on .scm file change

use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;

use game_net::{ClientEnvelope, CodecError, DualCodec, ServerEnvelope};
use lc_client_script::ClientSchemeRuntime;
use lc_core::{ClientState, LispEvent, LispGameCommand, LispMotionInput};
use lc_render::{ClientPrediction, OrbitCamera, SnapshotInterpolationBuffer};
use macroquad::prelude::*;
use quad_net::web_socket::WebSocket;

/// Extension trait enabling `DualCodec::decode_server::<TSnapshot, TEvent>(data)` syntax.
pub trait DualCodecServerExt {
    fn decode_server<TSnapshot: serde::de::DeserializeOwned, TEvent: serde::de::DeserializeOwned>(
        data: &[u8],
    ) -> Result<ServerEnvelope<TSnapshot, TEvent>, CodecError>;
}

impl DualCodecServerExt for DualCodec {
    fn decode_server<TSnapshot: serde::de::DeserializeOwned, TEvent: serde::de::DeserializeOwned>(
        data: &[u8],
    ) -> Result<ServerEnvelope<TSnapshot, TEvent>, CodecError> {
        DualCodec::decode_adaptive::<ServerEnvelope<TSnapshot, TEvent>>(data).or_else(|_| {
            serde_json::from_slice::<ServerEnvelope<TSnapshot, TEvent>>(data)
                .map_err(CodecError::Json)
        })
    }
}

/// Production-ready cross-platform WebSocket wrapper.
pub struct GameSocket {
    ws: WebSocket,
}

impl GameSocket {
    /// Connects to the given WebSocket address, catching panics on connection failure.
    pub fn connect(addr: &str) -> Result<Self, String> {
        let addr_str = addr.to_string();
        let res = std::panic::catch_unwind(move || WebSocket::connect(addr_str));
        match res {
            Ok(Ok(ws)) => Ok(GameSocket { ws }),
            Ok(Err(e)) => Err(format!("{:?}", e)),
            Err(_) => Err(format!("Could not connect to WebSocket at {}", addr)),
        }
    }

    /// Sends raw byte payload, sending as text if valid UTF-8, binary otherwise.
    pub fn send(&mut self, data: &[u8]) {
        if let Ok(text) = std::str::from_utf8(data) {
            self.ws.send_text(text);
        } else {
            self.ws.send_bytes(data);
        }
    }

    /// Sends a text frame over the WebSocket.
    pub fn send_text(&mut self, text: &str) {
        self.ws.send_text(text);
    }

    /// Serializes and sends a client envelope over the WebSocket.
    pub fn send_envelope(&mut self, env: &ClientEnvelope<LispMotionInput, LispGameCommand>) {
        if let Ok(text) = serde_json::to_string(env) {
            self.ws.send_text(&text);
        }
    }

    /// Checks if the WebSocket is currently connected.
    pub fn is_connected(&self) -> bool {
        self.ws.connected()
    }

    /// Attempts to receive a pending frame from the socket.
    pub fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.ws.try_recv()
    }
}

pub fn safe_get_time() -> f64 {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(get_time));
    std::panic::set_hook(prev_hook);
    res.unwrap_or(0.0)
}

/// HUD log and notification entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub text: String,
    pub color: Color,
    pub timestamp: f64,
}

/// Appends a new log entry to the circular log buffer.
pub fn add_log(logs: &mut Vec<LogEntry>, text: String, color: Color) {
    logs.push(LogEntry {
        text,
        color,
        timestamp: safe_get_time(),
    });
    if logs.len() > 40 {
        logs.remove(0);
    }
}

/// Parameters for Mode A (Online Mode).
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineParams {
    pub server_addr: String,
    pub game_id: String,
    pub token: String,
}

/// Startup Mode for LispCraft Client Host.
#[derive(Debug, Clone, PartialEq)]
pub enum HostMode {
    Online(OnlineParams),
    OfflineHotReload { dir_path: PathBuf },
}

/// Parses a URL string or target specification into (server_addr, Option<game_id>, Option<token>).
///
/// Supports:
/// - `http://ip:port/:game_id/:token` or `http://ip:port/game_id/token`
/// - `ws://ip:port/ws` or `ws://ip:port/:game_id/:token`
/// - `127.0.0.1:8082` or `8082`
pub fn parse_url_string(
    input: &str,
    default_port: &str,
) -> (String, Option<String>, Option<String>) {
    let trimmed = input.trim();

    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ws://")
        || trimmed.starts_with("wss://")
    {
        let is_secure = trimmed.starts_with("https://") || trimmed.starts_with("wss://");
        let scheme = if is_secure { "wss" } else { "ws" };

        let rest = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .or_else(|| trimmed.strip_prefix("wss://"))
            .or_else(|| trimmed.strip_prefix("ws://"))
            .unwrap_or(trimmed);

        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        let host_port = if parts.is_empty() {
            format!("127.0.0.1:{}", default_port)
        } else {
            let hp = parts[0];
            if !hp.contains(':') {
                format!("{}:{}", hp, default_port)
            } else {
                hp.to_string()
            }
        };

        let segments: Vec<String> = parts
            .iter()
            .skip(1)
            .filter(|s| **s != "ws")
            .map(|s| s.trim_start_matches(':').to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let game_id = segments.first().cloned();
        let token = segments.get(1).cloned();

        let server_addr = format!("{}://{}/ws", scheme, host_port);
        (server_addr, game_id, token)
    } else if trimmed.chars().all(|c| c.is_ascii_digit()) {
        let server_addr = format!("ws://127.0.0.1:{}/ws", trimmed);
        (server_addr, None, None)
    } else if trimmed.contains(':') {
        let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
        let host_port = parts[0];
        let segments: Vec<String> = parts
            .iter()
            .skip(1)
            .filter(|s| **s != "ws")
            .map(|s| s.trim_start_matches(':').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let game_id = segments.first().cloned();
        let token = segments.get(1).cloned();
        let server_addr = format!("ws://{}/ws", host_port);
        (server_addr, game_id, token)
    } else {
        let server_addr = format!("ws://{}:{}/ws", trimmed, default_port);
        (server_addr, None, None)
    }
}

/// Resolves OnlineParams from CLI arguments and environment variables.
pub fn resolve_online_params(args: &[String], default_port: &str) -> OnlineParams {
    let mut server_addr: Option<String> = None;
    let mut game_id: Option<String> = None;
    let mut token: Option<String> = None;
    let mut target_port = default_port.to_string();
    let mut target_host = "127.0.0.1".to_string();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-p" || arg == "--port" || arg == "-port" || arg == "--ws-port" {
            if i + 1 < args.len() {
                target_port = args[i + 1].clone();
                i += 1;
            }
        } else if let Some(p) = arg
            .strip_prefix("--port=")
            .or_else(|| arg.strip_prefix("-p="))
            .or_else(|| arg.strip_prefix("-port="))
            .or_else(|| arg.strip_prefix("--ws-port="))
        {
            target_port = p.to_string();
        } else if arg == "--game-id" || arg == "--game" {
            if i + 1 < args.len() {
                game_id = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(g) = arg
            .strip_prefix("--game-id=")
            .or_else(|| arg.strip_prefix("--game="))
        {
            game_id = Some(g.to_string());
        } else if arg == "--token" {
            if i + 1 < args.len() {
                token = Some(args[i + 1].clone());
                i += 1;
            }
        } else if let Some(t) = arg.strip_prefix("--token=") {
            token = Some(t.to_string());
        } else if arg.starts_with("http://")
            || arg.starts_with("https://")
            || arg.starts_with("ws://")
            || arg.starts_with("wss://")
        {
            let (addr, gid, tok) = parse_url_string(arg, default_port);
            server_addr = Some(addr);
            if gid.is_some() {
                game_id = gid;
            }
            if tok.is_some() {
                token = tok;
            }
        } else if arg.chars().all(|c| c.is_ascii_digit()) {
            target_port = arg.clone();
        } else if arg.contains(':') {
            let (addr, gid, tok) = parse_url_string(arg, default_port);
            server_addr = Some(addr);
            if gid.is_some() {
                game_id = gid;
            }
            if tok.is_some() {
                token = tok;
            }
        } else if !arg.starts_with('-') {
            target_host = arg.clone();
        }
        i += 1;
    }

    let resolved_addr =
        server_addr.unwrap_or_else(|| format!("ws://{}:{}/ws", target_host, target_port));
    let resolved_game = game_id.unwrap_or_else(|| "space_arena".to_string());
    let resolved_token =
        token.unwrap_or_else(|| format!("guest_{}", (safe_get_time() * 1000.0) as u64));

    OnlineParams {
        server_addr: resolved_addr,
        game_id: resolved_game,
        token: resolved_token,
    }
}

/// Resolves host mode (Online vs Offline Hot-Reload) from arguments.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_host_mode(args: &[String], default_port: &str) -> HostMode {
    if let Some(first_arg) = args.get(1) {
        let p = Path::new(first_arg);
        if p.is_dir() {
            return HostMode::OfflineHotReload {
                dir_path: p.to_path_buf(),
            };
        }
    }
    HostMode::Online(resolve_online_params(args, default_port))
}

/// Resolves server WebSocket address based on CLI arguments and platform.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_server_address(args: &[String], default_port: &str) -> String {
    resolve_online_params(args, default_port).server_addr
}

#[cfg(target_arch = "wasm32")]
mod wasm_shims {
    extern "C" {
        pub fn lispcraft_get_url_pathname(ptr: *mut u8, max_len: usize) -> usize;
        pub fn lispcraft_get_random(ptr: *mut u8, len: usize);
    }

    fn custom_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
        unsafe {
            lispcraft_get_random(buf.as_mut_ptr(), buf.len());
        }
        Ok(())
    }

    getrandom::register_custom_getrandom!(custom_getrandom);
}

/// Resolves WASM host configuration with dynamic URL pathname extraction.
#[cfg(target_arch = "wasm32")]
pub fn resolve_wasm_online_params(default_port: &str) -> OnlineParams {
    let mut game_id = "space_arena".to_string();
    let mut token = format!("guest_{}", (safe_get_time() * 1000.0) as u64);

    let mut buf = [0u8; 512];
    let len = unsafe { wasm_shims::lispcraft_get_url_pathname(buf.as_mut_ptr(), buf.len()) };
    if len > 0 {
        if let Ok(pathname) = std::str::from_utf8(&buf[..len]) {
            let parts: Vec<&str> = pathname.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(first) = parts.first() {
                if *first != "ws"
                    && !first.ends_with(".html")
                    && !first.ends_with(".wasm")
                    && !first.ends_with(".js")
                {
                    game_id = first.to_string();
                }
            }
            if let Some(second) = parts.get(1) {
                token = second.to_string();
            }
        }
    }

    OnlineParams {
        server_addr: format!("ws://127.0.0.1:{}/ws", default_port),
        game_id,
        token,
    }
}

/// Recursively scans a directory for the latest `.scm` file modification time.
#[cfg(not(target_arch = "wasm32"))]
pub fn get_latest_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let mut latest: Option<std::time::SystemTime> = None;
    fn visit(p: &Path, latest: &mut Option<std::time::SystemTime>) {
        if let Ok(entries) = std::fs::read_dir(p) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, latest);
                } else if path.extension().and_then(|s| s.to_str()) == Some("scm") {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            *latest = Some(latest.map_or(mtime, |l| l.max(mtime)));
                        }
                    }
                }
            }
        }
    }
    visit(dir, &mut latest);
    if let Some(parent) = dir.parent() {
        let shared = parent.join("shared");
        if shared.is_dir() && !dir.ends_with("shared") {
            visit(&shared, &mut latest);
        }
    }
    latest
}

#[macroquad::main("LispCraft 3D")]
async fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let args: Vec<String> = std::env::args().collect();
    let default_port = {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::env::var("PORT")
                .or_else(|_| std::env::var("WS_PORT"))
                .or_else(|_| std::env::var("HTTP_PORT"))
                .unwrap_or_else(|_| "8082".to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            "8082".to_string()
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let host_mode = resolve_host_mode(&args, &default_port);

    #[cfg(target_arch = "wasm32")]
    let host_mode = HostMode::Online(resolve_wasm_online_params(&default_port));

    // Initialize Scheme runtime engine
    let mut runtime =
        ClientSchemeRuntime::new().expect("Failed to initialize ClientSchemeRuntime");

    // Camera and dead-reckoning state
    let mut camera = OrbitCamera::new();
    let mut interpolation_buffer = SnapshotInterpolationBuffer::new();
    let mut client_prediction = ClientPrediction::default();

    // Mode-specific state
    let mut socket: Option<GameSocket> = None;
    let mut online_config: Option<OnlineParams> = None;

    #[cfg(not(target_arch = "wasm32"))]
    let mut hot_reload_dir: Option<PathBuf> = None;
    #[cfg(not(target_arch = "wasm32"))]
    let mut last_mtime: Option<std::time::SystemTime> = None;
    #[cfg(not(target_arch = "wasm32"))]
    let mut last_mtime_check = 0.0_f32;

    let display_target: String;

    match host_mode {
        HostMode::Online(params) => {
            println!(
                "[Client Host] Mode A (Online): target='{}', game='{}', token='{}'",
                params.server_addr, params.game_id, params.token
            );
            display_target = params.server_addr.clone();

            match GameSocket::connect(&params.server_addr) {
                Ok(mut s) => {
                    let auth_env = ClientEnvelope::Auth {
                        token: format!("{}/{}", params.game_id, params.token),
                        client_version: "0.2.0".to_string(),
                    };
                    s.send_envelope(&auth_env);
                    socket = Some(s);
                }
                Err(e) => {
                    eprintln!("[Client] Initial connect error: {}", e);
                }
            }
            online_config = Some(params);
        }
        HostMode::OfflineHotReload { dir_path } => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                println!(
                    "[Client Host] Mode B (Offline Standalone Hot-Reload): dir='{}'",
                    dir_path.display()
                );
                display_target = dir_path.to_string_lossy().to_string();

                if let Err(e) = runtime.load_directory(&dir_path) {
                    eprintln!("[HotReload] Error initial loading directory: {}", e);
                }
                if let Err(e) = runtime.init() {
                    eprintln!("[HotReload] Error executing client-init: {}", e);
                }

                last_mtime = get_latest_mtime(&dir_path);
                hot_reload_dir = Some(dir_path);
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = dir_path;
                display_target = "WASM Offline".to_string();
            }
        }
    }

    // Command console mode state
    let mut command_mode = false;
    let mut command_input = String::new();
    let mut command_history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;

    // Logs and timings
    let mut message_logs: Vec<LogEntry> = Vec::new();
    let mut known_server_logs: HashSet<String> = HashSet::new();

    let mut last_reconnect_time = 0.0_f32;
    let mut last_ping_time = 0.0_f32;
    let mut ping_ms: Option<u64> = None;
    let mut was_connected = false;

    let mut player_id_str = "player_local".to_string();
    let mut cam_initialized = false;
    let mut last_mouse_pos = vec2(0.0, 0.0);
    let mut was_moving = false;

    add_log(
        &mut message_logs,
        "System: Initializing LispCraft 3D Host...".to_string(),
        Color::new(0.0, 0.85, 1.0, 1.0),
    );

    loop {
        let dt = get_frame_time().min(0.1);
        let time = get_time() as f32;

        // --- Mode B: Periodic File mtime check (every 100ms) ---
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(ref dir) = hot_reload_dir {
            if time - last_mtime_check >= 0.1 {
                last_mtime_check = time;
                if let Some(mtime) = get_latest_mtime(dir) {
                    let is_newer = match last_mtime {
                        Some(prev) => mtime > prev,
                        None => true,
                    };
                    if is_newer {
                        last_mtime = Some(mtime);
                        match runtime.load_directory(dir) {
                            Ok(()) => {
                                if let Err(e) = runtime.init() {
                                    eprintln!("[HotReload] client-init error: {}", e);
                                }
                                println!("[HotReload] Reloaded client scripts in {:?}", mtime);
                            }
                            Err(e) => {
                                eprintln!("[HotReload] Failed to reload scripts: {}", e);
                            }
                        }
                    }
                }
            }
        }

        // --- Mode A: Online Network Processing ---
        if let Some(ref online) = online_config {
            // Auto reconnect every 2 seconds if disconnected
            if socket.is_none() && time - last_reconnect_time > 2.0 {
                last_reconnect_time = time;
                if let Ok(mut s) = GameSocket::connect(&online.server_addr) {
                    let auth_env = ClientEnvelope::Auth {
                        token: format!("{}/{}", online.game_id, online.token),
                        client_version: "0.2.0".to_string(),
                    };
                    s.send_envelope(&auth_env);
                    socket = Some(s);
                }
            }

            // Periodic ping heartbeat (every 3 seconds when connected)
            if was_connected && time - last_ping_time >= 3.0 {
                last_ping_time = time;
                if let Some(ref mut sock) = socket {
                    let client_time_ms = (get_time() * 1000.0) as u64;
                    let ping_env = ClientEnvelope::Ping { client_time_ms };
                    sock.send_envelope(&ping_env);
                }
            }

            // Poll incoming socket packets
            let mut conn_dropped = false;
            if let Some(ref mut sock) = socket {
                if sock.is_connected() {
                    if !was_connected {
                        was_connected = true;
                        add_log(
                            &mut message_logs,
                            format!("[NET] Connected to Scheme Game Server ({})", online.server_addr),
                            Color::new(0.2, 0.95, 0.6, 1.0),
                        );
                    }
                    while let Some(bytes) = sock.try_recv() {
                        if let Ok(envelope) =
                            DualCodec::decode_server::<ClientState, LispEvent>(&bytes)
                        {
                            match envelope {
                                ServerEnvelope::AuthSuccess {
                                    player_id,
                                    server_tick,
                                } => {
                                    player_id_str = format!("player_{}", player_id);
                                    println!(
                                        "[Client] AuthSuccess: joined world as {} (tick {})",
                                        player_id_str, server_tick
                                    );
                                    add_log(
                                        &mut message_logs,
                                        format!(
                                            "[SYS] Welcome Commander {}! (Server Tick {})",
                                            player_id_str, server_tick
                                        ),
                                        Color::new(0.2, 0.95, 0.6, 1.0),
                                    );
                                }
                                ServerEnvelope::Snapshot(state) => {
                                    if !state.client_id.is_empty() {
                                        player_id_str = state.client_id.clone();
                                    }
                                    let target = vec3(
                                        state.camera_target[0],
                                        state.camera_target[1],
                                        state.camera_target[2],
                                    );
                                    if !cam_initialized {
                                        client_prediction.reset(target);
                                        camera.target_pos = target;
                                        cam_initialized = true;
                                    }
                                    interpolation_buffer.push(get_time(), state.clone());
                                    client_prediction.reconcile_moving(target, was_moving);
                                    let mut synced_state = state.clone();
                                    let cur_pos = client_prediction.pos();
                                    synced_state.camera_target = [cur_pos.x, cur_pos.y, cur_pos.z];
                                    runtime.set_replicated_state(synced_state);

                                    for log in &state.logs {
                                        if !known_server_logs.contains(log) {
                                            known_server_logs.insert(log.clone());
                                            add_log(
                                                &mut message_logs,
                                                format!("[NET] {}", log),
                                                Color::new(0.9, 0.8, 0.3, 1.0),
                                            );
                                        }
                                    }
                                }
                                ServerEnvelope::Event(evt) => match evt {
                                    LispEvent::BundleScript { game_id, vfs } => {
                                        println!(
                                            "[Client] Received BundleScript for game '{}' ({} files)",
                                            game_id,
                                            vfs.len()
                                        );
                                        if let Err(e) = runtime.load_vfs(&vfs) {
                                            eprintln!("[Client] Failed to load BundleScript: {}", e);
                                        }
                                        if let Err(e) = runtime.init() {
                                            eprintln!("[Client] client-init error: {}", e);
                                        }
                                    }
                                    LispEvent::RemoteEvent { event_name, payload } => {
                                        runtime.handle_server_event(&event_name, &payload);
                                    }
                                    LispEvent::CommandEcho { success, output } => {
                                        let col = if success {
                                            Color::new(0.25, 0.95, 0.5, 1.0)
                                        } else {
                                            Color::new(1.0, 0.35, 0.35, 1.0)
                                        };
                                        add_log(&mut message_logs, format!("[ECHO] {}", output), col);
                                        runtime.handle_server_event(
                                            "command_echo",
                                            &serde_json::json!({
                                                "success": success,
                                                "output": output,
                                            }),
                                        );
                                    }
                                    LispEvent::ChatBroadcast { from, message } => {
                                        add_log(
                                            &mut message_logs,
                                            format!("[{}] {}", from, message),
                                            Color::new(0.3, 0.85, 1.0, 1.0),
                                        );
                                        runtime.handle_server_event(
                                            "chat",
                                            &serde_json::json!({
                                                "from": from,
                                                "message": message,
                                            }),
                                        );
                                    }
                                    LispEvent::SystemNotice(notice) => {
                                        add_log(
                                            &mut message_logs,
                                            format!("[SYS] {}", notice),
                                            Color::new(1.0, 0.85, 0.2, 1.0),
                                        );
                                        runtime.handle_server_event(
                                            "system_notice",
                                            &serde_json::json!({ "notice": notice }),
                                        );
                                    }
                                },
                                ServerEnvelope::Pong { client_time_ms, .. } => {
                                    let now_ms = (get_time() * 1000.0) as u64;
                                    ping_ms = Some(now_ms.saturating_sub(client_time_ms));
                                }
                                ServerEnvelope::AuthFailed { reason } => {
                                    add_log(
                                        &mut message_logs,
                                        format!("[AUTH FAILED] {}", reason),
                                        Color::new(1.0, 0.3, 0.3, 1.0),
                                    );
                                }
                                ServerEnvelope::Kick { reason } => {
                                    add_log(
                                        &mut message_logs,
                                        format!("[KICK] Disconnected by server: {}", reason),
                                        Color::new(1.0, 0.2, 0.2, 1.0),
                                    );
                                }
                            }
                        } else if let Ok(state) = serde_json::from_slice::<ClientState>(&bytes) {
                            if !state.client_id.is_empty() {
                                player_id_str = state.client_id.clone();
                            }
                            let target = vec3(
                                state.camera_target[0],
                                state.camera_target[1],
                                state.camera_target[2],
                            );
                            if !cam_initialized {
                                client_prediction.reset(target);
                                camera.target_pos = target;
                                cam_initialized = true;
                            }
                            interpolation_buffer.push(get_time(), state.clone());
                            client_prediction.reconcile_moving(target, was_moving);
                            let mut synced_state = state.clone();
                            let cur_pos = client_prediction.pos();
                            synced_state.camera_target = [cur_pos.x, cur_pos.y, cur_pos.z];
                            runtime.set_replicated_state(synced_state);
                        }
                    }
                } else if was_connected {
                    // Lost active connection
                    conn_dropped = true;
                } else if time - last_reconnect_time > 10.0 {
                    // Connection handshake timeout
                    conn_dropped = true;
                }
            }

            if conn_dropped {
                socket = None;
                was_connected = false;
                add_log(
                    &mut message_logs,
                    "[NET] Disconnected from server. Reconnecting...".to_string(),
                    Color::new(1.0, 0.4, 0.4, 1.0),
                );
            }

            // Drain outgoing RemoteEvents emitted from Scheme and forward to server
            for (event_name, payload) in runtime.drain_outgoing_events() {
                let cmd_env = ClientEnvelope::Command(LispGameCommand::RemoteEvent {
                    event_name,
                    payload,
                });
                if let Some(ref mut sock) = socket {
                    if was_connected {
                        sock.send_envelope(&cmd_env);
                    }
                }
            }
        }

        // --- Input Handling ---
        let mut just_exited_command_mode = false;
        if is_key_pressed(KeyCode::Enter) {
            if !command_mode {
                command_mode = true;
                command_input.clear();
                history_index = None;
            } else {
                let trimmed = command_input.trim().to_string();
                if trimmed.is_empty() {
                    command_mode = false;
                    just_exited_command_mode = true;
                } else {
                    if command_history.last() != Some(&trimmed) {
                        command_history.push(trimmed.clone());
                    }
                    history_index = None;

                    if was_connected && socket.is_some() {
                        let sock = socket.as_mut().unwrap();
                        let cmd_env = ClientEnvelope::Command(LispGameCommand::SchemeCommand(
                            trimmed.clone(),
                        ));
                        sock.send_envelope(&cmd_env);
                    } else {
                        // Offline local eval
                        match runtime.eval_repl(&trimmed) {
                            Ok(out) => {
                                add_log(
                                    &mut message_logs,
                                    format!("=> {}", out),
                                    Color::new(0.25, 0.95, 0.5, 1.0),
                                );
                            }
                            Err(e) => {
                                add_log(
                                    &mut message_logs,
                                    format!("ERR: {}", e),
                                    Color::new(1.0, 0.35, 0.35, 1.0),
                                );
                            }
                        }
                    }

                    add_log(
                        &mut message_logs,
                        format!("[CMD] {}", trimmed),
                        Color::new(0.35, 0.85, 1.0, 1.0),
                    );

                    command_input.clear();
                    command_mode = false;
                    just_exited_command_mode = true;
                }
            }
        }

        if command_mode {
            if is_key_pressed(KeyCode::Escape) {
                command_mode = false;
                command_input.clear();
                history_index = None;
            }

            if is_key_pressed(KeyCode::Up) {
                if !command_history.is_empty() {
                    let new_idx = match history_index {
                        None => command_history.len().saturating_sub(1),
                        Some(idx) => idx.saturating_sub(1),
                    };
                    history_index = Some(new_idx);
                    command_input = command_history[new_idx].clone();
                }
            } else if is_key_pressed(KeyCode::Down) {
                if let Some(idx) = history_index {
                    if idx + 1 < command_history.len() {
                        let new_idx = idx + 1;
                        history_index = Some(new_idx);
                        command_input = command_history[new_idx].clone();
                    } else {
                        history_index = None;
                        command_input.clear();
                    }
                }
            }

            while let Some(c) = get_char_pressed() {
                if (' '..='~').contains(&c) {
                    command_input.push(c);
                }
            }

            if is_key_pressed(KeyCode::Backspace) {
                command_input.pop();
            }
        } else if !just_exited_command_mode {
            // Camera orbit controls with arrow keys
            if is_key_down(KeyCode::Left) {
                camera.target_camera_yaw -= 2.2 * dt;
            }
            if is_key_down(KeyCode::Right) {
                camera.target_camera_yaw += 2.2 * dt;
            }
            if is_key_down(KeyCode::Up) {
                camera.target_camera_pitch =
                    (camera.target_camera_pitch - 1.2 * dt).clamp(0.12, 1.35);
            }
            if is_key_down(KeyCode::Down) {
                camera.target_camera_pitch =
                    (camera.target_camera_pitch + 1.2 * dt).clamp(0.12, 1.35);
            }

            // WASD directional movement relative to camera forward/right
            let fwd = camera.forward();
            let right = camera.right();

            let mut move_input = Vec2::ZERO;
            if is_key_down(KeyCode::W) {
                move_input += fwd;
            }
            if is_key_down(KeyCode::S) {
                move_input -= fwd;
            }
            if is_key_down(KeyCode::A) {
                move_input -= right;
            }
            if is_key_down(KeyCode::D) {
                move_input += right;
            }

            let is_moving = move_input.length_squared() > 0.001;
            if is_moving {
                let norm = move_input.normalize();
                let input = LispMotionInput {
                    dx: norm.x,
                    dz: norm.y,
                    yaw: camera.camera_yaw,
                };
                // Authoritative server simulation rate: 0.25 units/tick * 20 TPS = 5.0 m/s
                let speed = 5.0_f32;
                client_prediction.predict_move(&input, speed, dt);
                if let Some(ref mut sock) = socket {
                    if was_connected {
                        sock.send_envelope(&ClientEnvelope::Input(input));
                    }
                }
                was_moving = true;
            } else if was_moving {
                // Send explicit zero-velocity stop packet once to immediately clear server-side motion latch
                let stop_input = LispMotionInput {
                    dx: 0.0,
                    dz: 0.0,
                    yaw: camera.camera_yaw,
                };
                if let Some(ref mut sock) = socket {
                    if was_connected {
                        sock.send_envelope(&ClientEnvelope::Input(stop_input));
                    }
                }
                was_moving = false;
            }

            // Mouse orbit and zoom
            let mouse_pos = vec2(mouse_position().0, mouse_position().1);
            if is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right)
            {
                last_mouse_pos = mouse_pos;
            }
            if is_mouse_button_down(MouseButton::Left)
                || is_mouse_button_down(MouseButton::Right)
            {
                let delta_x = mouse_pos.x - last_mouse_pos.x;
                let delta_y = mouse_pos.y - last_mouse_pos.y;
                camera.handle_mouse_orbit(delta_x, delta_y);
            }
            let wheel = mouse_wheel().1;
            if wheel.abs() > 0.01 {
                camera.handle_zoom(wheel);
            }
            last_mouse_pos = mouse_pos;
        }

        // Camera and prediction synchronization
        camera.update(dt, client_prediction.pos());
        runtime.set_camera(camera.clone());

        // Update replicated state position with local prediction
        let mut st = runtime.replicated_state();
        let cur_pos = client_prediction.pos();
        st.camera_target = [cur_pos.x, cur_pos.y, cur_pos.z];
        st.camera_yaw = camera.camera_yaw;

        // Smooth snapshot interpolation for replicated entities
        if let Some(interpolated_entities) = interpolation_buffer.sample(get_time() - 0.05) {
            st.entities = interpolated_entities;
        }
        runtime.set_replicated_state(st);

        // --- Rendering Loop: 100% driven by Scheme ---
        if runtime.vfs().is_empty() {
            // Optional fallback: Loading spinner while connecting / awaiting BundleScript
            clear_background(Color::new(0.04, 0.06, 0.10, 1.0));
            set_default_camera();

            let sw = screen_width();
            let sh = screen_height();
            let center_x = sw * 0.5;
            let center_y = sh * 0.5;

            let time_val = get_time() as f32;
            let spinner_radius = 28.0;
            let angle = time_val * 4.0;
            let spinner_x = center_x + angle.cos() * spinner_radius;
            let spinner_y = center_y - 20.0 + angle.sin() * spinner_radius;

            draw_circle_lines(
                center_x,
                center_y - 20.0,
                spinner_radius,
                2.0,
                Color::new(0.12, 0.35, 0.6, 0.6),
            );
            draw_circle(spinner_x, spinner_y, 5.0, Color::new(0.0, 0.85, 1.0, 1.0));

            let msg = if socket.is_some() {
                "Connecting to Scheme game server..."
            } else {
                "Awaiting connection or local script..."
            };
            let font_size = 20.0;
            let text_w = measure_text(msg, None, font_size as u16, 1.0).width;
            draw_text(
                msg,
                center_x - text_w * 0.5,
                center_y + 40.0,
                font_size,
                Color::new(0.8, 0.9, 1.0, 0.9),
            );

            let sub_msg = format!("Target: {}", display_target);
            let sub_font = 13.0;
            let sub_w = measure_text(&sub_msg, None, sub_font as u16, 1.0).width;
            draw_text(
                &sub_msg,
                center_x - sub_w * 0.5,
                center_y + 65.0,
                sub_font,
                Color::new(0.4, 0.6, 0.75, 0.7),
            );
        } else {
            // Scheme script active: set 3D camera and invoke Scheme render hook
            clear_background(Color::new(0.02, 0.03, 0.06, 1.0));

            let cam3d = runtime.camera().to_camera3d();
            set_camera(&cam3d);

            // Scheme executes (client-render dt) and directly calls (draw-*!)
            if let Err(e) = runtime.render(dt) {
                set_default_camera();
                draw_text(
                    format!("[Scheme Render Error] {}", e).as_str(),
                    20.0,
                    30.0,
                    16.0,
                    Color::new(1.0, 0.3, 0.3, 1.0),
                );
            }

            set_default_camera();
        }

        // Draw Command Console Prompt if active
        if command_mode {
            set_default_camera();
            let bar_h = 44.0;
            let bar_y = screen_height() - bar_h - 12.0;
            let bar_w = screen_width() - 24.0;
            let bar_x = 12.0;

            draw_rectangle(
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                Color::new(0.01, 0.04, 0.08, 0.94),
            );
            draw_rectangle_lines(
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                2.0,
                Color::new(0.0, 0.95, 0.6, 1.0),
            );

            let cursor = if ((get_time() * 3.0) as usize).is_multiple_of(2) {
                "_"
            } else {
                " "
            };
            draw_text(
                format!("COMMAND > {}{}", command_input, cursor).as_str(),
                bar_x + 14.0,
                bar_y + 28.0,
                20.0,
                Color::new(0.2, 1.0, 0.65, 1.0),
            );

            draw_text(
                "[Enter] Execute | [Esc] Exit | [↑/↓] History",
                (bar_x + bar_w - 320.0).max(bar_x + 280.0),
                bar_y + 27.0,
                13.0,
                Color::new(0.5, 0.7, 0.85, 0.8),
            );
        }

        // Suppress unused variable warnings in release builds
        let _ = ping_ms;
        let _ = &player_id_str;

        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dual_codec_server_decoding() {
        let auth_env: ServerEnvelope<ClientState, LispEvent> = ServerEnvelope::AuthSuccess {
            player_id: 101,
            server_tick: 42,
        };
        let json_bytes = serde_json::to_vec(&auth_env).unwrap();

        let decoded =
            DualCodec::decode_server::<ClientState, LispEvent>(&json_bytes).expect("decode JSON");
        match decoded {
            ServerEnvelope::AuthSuccess {
                player_id,
                server_tick,
            } => {
                assert_eq!(player_id, 101);
                assert_eq!(server_tick, 42);
            }
            _ => panic!("Expected AuthSuccess"),
        }

        // Test binary format as well
        let bin_bytes = DualCodec::encode_binary(&auth_env).unwrap();
        let decoded_bin =
            DualCodec::decode_server::<ClientState, LispEvent>(&bin_bytes).expect("decode binary");
        assert_eq!(decoded_bin, auth_env);
    }

    #[test]
    fn test_bundle_script_decoding() {
        let mut vfs = std::collections::HashMap::new();
        vfs.insert("main.scm".to_string(), "(displayln 1)".to_string());
        let bundle_env: ServerEnvelope<ClientState, LispEvent> =
            ServerEnvelope::Event(LispEvent::BundleScript {
                game_id: "space_arena".to_string(),
                vfs,
            });

        let json_bytes = serde_json::to_vec(&bundle_env).unwrap();
        let decoded = DualCodec::decode_server::<ClientState, LispEvent>(&json_bytes)
            .expect("decode BundleScript");
        match decoded {
            ServerEnvelope::Event(LispEvent::BundleScript { game_id, vfs }) => {
                assert_eq!(game_id, "space_arena");
                assert_eq!(vfs.get("main.scm").unwrap(), "(displayln 1)");
            }
            _ => panic!("Expected BundleScript event"),
        }
    }

    #[test]
    fn test_remote_event_decoding() {
        let remote_env: ServerEnvelope<ClientState, LispEvent> =
            ServerEnvelope::Event(LispEvent::RemoteEvent {
                event_name: "bullet_hit".to_string(),
                payload: serde_json::json!({"damage": 25}),
            });
        let json_bytes = serde_json::to_vec(&remote_env).unwrap();
        let decoded = DualCodec::decode_server::<ClientState, LispEvent>(&json_bytes)
            .expect("decode RemoteEvent");
        match decoded {
            ServerEnvelope::Event(LispEvent::RemoteEvent {
                event_name,
                payload,
            }) => {
                assert_eq!(event_name, "bullet_hit");
                assert_eq!(payload["damage"], 25);
            }
            _ => panic!("Expected RemoteEvent"),
        }
    }

    #[test]
    fn test_parse_url_string() {
        // http url with :game_id/:token
        let (addr1, g1, t1) =
            parse_url_string("http://192.168.1.100:8082/:space_arena/:token_xyz", "8082");
        assert_eq!(addr1, "ws://192.168.1.100:8082/ws");
        assert_eq!(g1.as_deref(), Some("space_arena"));
        assert_eq!(t1.as_deref(), Some("token_xyz"));

        // http url without colons in path
        let (addr2, g2, t2) =
            parse_url_string("http://localhost:8082/dungeon/hero_42", "8082");
        assert_eq!(addr2, "ws://localhost:8082/ws");
        assert_eq!(g2.as_deref(), Some("dungeon"));
        assert_eq!(t2.as_deref(), Some("hero_42"));

        // ws url with /ws
        let (addr3, g3, t3) = parse_url_string("ws://127.0.0.1:8082/ws", "8082");
        assert_eq!(addr3, "ws://127.0.0.1:8082/ws");
        assert_eq!(g3, None);
        assert_eq!(t3, None);

        // port only
        let (addr4, g4, t4) = parse_url_string("9000", "8082");
        assert_eq!(addr4, "ws://127.0.0.1:9000/ws");
        assert_eq!(g4, None);
        assert_eq!(t4, None);

        // host:port
        let (addr5, g5, t5) = parse_url_string("10.0.0.1:8090", "8082");
        assert_eq!(addr5, "ws://10.0.0.1:8090/ws");
        assert_eq!(g5, None);
        assert_eq!(t5, None);
    }

    #[test]
    fn test_resolve_online_params() {
        let args = vec![
            "client".to_string(),
            "http://127.0.0.1:8082/:space_arena/:commander_1".to_string(),
        ];
        let params = resolve_online_params(&args, "8082");
        assert_eq!(params.server_addr, "ws://127.0.0.1:8082/ws");
        assert_eq!(params.game_id, "space_arena");
        assert_eq!(params.token, "commander_1");

        let default_args = vec!["client".to_string()];
        let default_params = resolve_online_params(&default_args, "8082");
        assert_eq!(default_params.server_addr, "ws://127.0.0.1:8082/ws");
        assert_eq!(default_params.game_id, "space_arena");
        assert!(default_params.token.starts_with("guest_"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_resolve_host_mode() {
        // Mode B: Directory argument
        let temp_dir = std::env::temp_dir().join("lc_host_test_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let dir_str = temp_dir.to_str().unwrap().to_string();
        let args_dir = vec!["client".to_string(), dir_str.clone()];
        let mode = resolve_host_mode(&args_dir, "8082");
        match mode {
            HostMode::OfflineHotReload { dir_path } => {
                assert_eq!(dir_path, PathBuf::from(dir_str));
            }
            _ => panic!("Expected HostMode::OfflineHotReload"),
        }

        // Mode A: Normal arguments
        let args_online = vec!["client".to_string(), "9000".to_string()];
        let mode_online = resolve_host_mode(&args_online, "8082");
        match mode_online {
            HostMode::Online(params) => {
                assert_eq!(params.server_addr, "ws://127.0.0.1:9000/ws");
            }
            _ => panic!("Expected HostMode::Online"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_log_queue_capacity() {
        let mut logs = Vec::new();
        for i in 0..50 {
            add_log(
                &mut logs,
                format!("Message {}", i),
                Color::new(1.0, 1.0, 1.0, 1.0),
            );
        }
        assert!(logs.len() <= 40);
        assert_eq!(logs.last().unwrap().text, "Message 49");
    }
}
