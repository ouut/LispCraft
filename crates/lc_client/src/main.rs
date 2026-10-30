//! # LispCraft 3D Client
//!
//! Client binary for LispCraft 3D, supporting both Native and WASM32 targets.
//! Powered by Macroquad, quad-net, and game_net dual-mode networking protocol.

use std::collections::HashSet;

use game_net::{ClientEnvelope, CodecError, DualCodec, ServerEnvelope};
use lc_core::{ClientState, LispEvent, LispGameCommand, LispMotionInput};
use lc_render::{
    ClientPrediction, OrbitCamera, RadarRenderer, SceneRenderer, SnapshotInterpolationBuffer,
};
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

fn safe_get_time() -> f64 {
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

/// Resolves server WebSocket address based on CLI arguments and platform.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_server_address(args: &[String], default_port: &str) -> String {
    let mut target_port = default_port.to_string();
    let mut target_host = "127.0.0.1".to_string();
    let mut explicit_addr: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-p" || arg == "--port" || arg == "-port" || arg == "--ws-port" {
            if i + 1 < args.len() {
                target_port = args[i + 1].clone();
                i += 1;
            }
        } else if let Some(p) = arg.strip_prefix("--port=") {
            target_port = p.to_string();
        } else if let Some(p) = arg.strip_prefix("-p=") {
            target_port = p.to_string();
        } else if arg.starts_with("ws://") || arg.starts_with("wss://") {
            explicit_addr = Some(arg.clone());
        } else if arg.chars().all(|c| c.is_ascii_digit()) {
            target_port = arg.clone();
        } else if arg.contains(':') {
            explicit_addr = Some(format!("ws://{}/ws", arg.trim_end_matches('/')));
        } else if !arg.starts_with('-') {
            target_host = arg.clone();
        }
        i += 1;
    }

    if let Some(explicit) = explicit_addr {
        if explicit.ends_with("/ws") {
            explicit
        } else {
            format!("{}/ws", explicit.trim_end_matches('/'))
        }
    } else {
        format!("ws://{}:{}/ws", target_host, target_port)
    }
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
                .unwrap_or_else(|_| "8087".to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            "8087".to_string()
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let server_addr = resolve_server_address(&args, &default_port);

    #[cfg(target_arch = "wasm32")]
    let server_addr = format!("ws://127.0.0.1:{}/ws", default_port);

    println!("[Client] Connecting to {}", server_addr);
    let mut socket = match GameSocket::connect(&server_addr) {
        Ok(mut s) => {
            let auth_env = ClientEnvelope::Auth {
                token: format!("token_{}", (get_time() * 1000.0) as u64),
                client_version: "0.2.0".to_string(),
            };
            s.send_envelope(&auth_env);
            Some(s)
        }
        Err(e) => {
            eprintln!("[Client] Connect error: {}", e);
            None
        }
    };

    // Instantiate and manage modular runtime components
    let mut camera = OrbitCamera::new();
    let scene_renderer = SceneRenderer::new();
    let radar_renderer = RadarRenderer::new();
    let mut interpolation_buffer = SnapshotInterpolationBuffer::new();
    let mut client_prediction = ClientPrediction::default();

    // Command mode state
    let mut command_mode = false;
    let mut command_input = String::new();
    let mut command_history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;

    // Logs & state tracking
    let mut message_logs: Vec<LogEntry> = Vec::new();
    let mut known_server_logs: HashSet<String> = HashSet::new();

    let mut last_reconnect_time = 0.0_f32;
    let mut last_ping_time = 0.0_f32;
    let mut ping_ms: Option<u64> = None;

    let mut player_id_str = "player_unknown".to_string();
    let mut hp: f32 = 100.0;
    let mut max_hp: f32 = 100.0;
    let mut score: i32 = 0;
    let mut cam_initialized = false;
    let mut last_mouse_pos = vec2(0.0, 0.0);

    add_log(
        &mut message_logs,
        "System: Initializing LispCraft 3D Engine...".to_string(),
        Color::new(0.0, 0.85, 1.0, 1.0),
    );

    loop {
        let dt = get_frame_time().min(0.1);
        let time = get_time() as f32;

        // Auto reconnect every 2 seconds if disconnected
        if socket.is_none() && time - last_reconnect_time > 2.0 {
            last_reconnect_time = time;
            if let Ok(mut s) = GameSocket::connect(&server_addr) {
                let auth_env = ClientEnvelope::Auth {
                    token: format!("token_{}", (get_time() * 1000.0) as u64),
                    client_version: "0.2.0".to_string(),
                };
                s.send_envelope(&auth_env);
                add_log(
                    &mut message_logs,
                    format!("[NET] Connected to Scheme Game Server ({})", server_addr),
                    Color::new(0.2, 0.95, 0.6, 1.0),
                );
                socket = Some(s);
            }
        }

        // Periodic ping heartbeat (every 3 seconds)
        if time - last_ping_time >= 3.0 {
            last_ping_time = time;
            if let Some(ref mut sock) = socket {
                let client_time_ms = (get_time() * 1000.0) as u64;
                let ping_env = ClientEnvelope::Ping { client_time_ms };
                sock.send_envelope(&ping_env);
            }
        }

        // (a) Poll incoming socket packets
        let mut conn_dropped = false;
        if let Some(ref mut sock) = socket {
            if !sock.is_connected() {
                conn_dropped = true;
            } else {
                while let Some(bytes) = sock.try_recv() {
                    if let Ok(envelope) = DualCodec::decode_server::<ClientState, LispEvent>(&bytes)
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
                                hp = state.hp;
                                max_hp = state.max_hp;
                                score = state.score;
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
                                client_prediction.reconcile(target);
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
                                LispEvent::CommandEcho { success, output } => {
                                    let col = if success {
                                        Color::new(0.25, 0.95, 0.5, 1.0)
                                    } else {
                                        Color::new(1.0, 0.35, 0.35, 1.0)
                                    };
                                    add_log(&mut message_logs, format!("[ECHO] {}", output), col);
                                }
                                LispEvent::ChatBroadcast { from, message } => {
                                    add_log(
                                        &mut message_logs,
                                        format!("[{}] {}", from, message),
                                        Color::new(0.3, 0.85, 1.0, 1.0),
                                    );
                                }
                                LispEvent::SystemNotice(notice) => {
                                    add_log(
                                        &mut message_logs,
                                        format!("[SYS] {}", notice),
                                        Color::new(1.0, 0.85, 0.2, 1.0),
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
                        hp = state.hp;
                        max_hp = state.max_hp;
                        score = state.score;
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
                        client_prediction.reconcile(target);
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
                }
            }
        }
        if conn_dropped {
            socket = None;
            add_log(
                &mut message_logs,
                "[NET] Disconnected from server. Reconnecting...".to_string(),
                Color::new(1.0, 0.4, 0.4, 1.0),
            );
        }

        // (b) Input handling
        if is_key_pressed(KeyCode::Enter) {
            if !command_mode {
                command_mode = true;
                command_input.clear();
                history_index = None;
            } else {
                let trimmed = command_input.trim().to_string();
                if trimmed.is_empty() {
                    command_mode = false;
                } else {
                    if command_history.last() != Some(&trimmed) {
                        command_history.push(trimmed.clone());
                    }
                    history_index = None;

                    if let Some(ref mut sock) = socket {
                        let cmd_env = ClientEnvelope::Command(LispGameCommand::SchemeCommand(
                            trimmed.clone(),
                        ));
                        sock.send_envelope(&cmd_env);
                    }

                    add_log(
                        &mut message_logs,
                        format!("[CMD] {}", trimmed),
                        Color::new(0.35, 0.85, 1.0, 1.0),
                    );

                    command_input.clear();
                    command_mode = false;
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
        } else {
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

            if move_input.length_squared() > 0.001 {
                let norm = move_input.normalize();
                let input = LispMotionInput {
                    dx: norm.x,
                    dz: norm.y,
                    yaw: camera.camera_yaw,
                };
                let speed = 8.0_f32;
                client_prediction.predict_move(&input, speed, dt);
                if let Some(ref mut sock) = socket {
                    sock.send_envelope(&ClientEnvelope::Input(input));
                }
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

        // (c) Update
        camera.update(dt, client_prediction.pos());

        // (d) Rendering (Dual-sided Decoupled)
        let entities = interpolation_buffer
            .sample(get_time() - 0.05)
            .unwrap_or_default();
        scene_renderer.draw_scene(&camera.to_camera3d(), &entities, client_prediction.pos());
        radar_renderer.draw_radar(
            vec2(90.0, screen_height() - 90.0),
            70.0,
            client_prediction.pos(),
            camera.camera_yaw,
            &entities,
        );

        // Draw 2D HUD (set_default_camera is active from draw_scene)
        set_default_camera();

        // Top-left dashboard
        let dash_x = 16.0;
        let dash_y = 16.0;
        let dash_w = 340.0;
        let dash_h = 138.0;

        draw_rectangle(
            dash_x,
            dash_y,
            dash_w,
            dash_h,
            Color::new(0.02, 0.05, 0.10, 0.85),
        );
        draw_rectangle_lines(
            dash_x,
            dash_y,
            dash_w,
            dash_h,
            1.5,
            Color::new(0.12, 0.45, 0.75, 0.8),
        );
        draw_line(
            dash_x,
            dash_y,
            dash_x + dash_w,
            dash_y,
            2.5,
            Color::new(0.0, 0.85, 1.0, 1.0),
        );
        draw_line(
            dash_x,
            dash_y + 12.0,
            dash_x,
            dash_y,
            2.5,
            Color::new(0.0, 0.85, 1.0, 1.0),
        );
        draw_line(
            dash_x + dash_w,
            dash_y + 12.0,
            dash_x + dash_w,
            dash_y,
            2.5,
            Color::new(0.0, 0.85, 1.0, 1.0),
        );

        draw_text(
            "◆ LISPCRAFT SYSTEM HUD",
            dash_x + 14.0,
            dash_y + 22.0,
            16.0,
            Color::new(0.0, 0.9, 1.0, 1.0),
        );

        // Connection status & Ping
        let is_conn = socket.as_ref().map(|s| s.is_connected()).unwrap_or(false);
        let (status_text, status_col) = if is_conn {
            if let Some(p) = ping_ms {
                (format!("ONLINE ({} ms)", p), Color::new(0.2, 0.95, 0.6, 1.0))
            } else {
                ("ONLINE".to_string(), Color::new(0.2, 0.95, 0.6, 1.0))
            }
        } else {
            ("CONNECTING...".to_string(), Color::new(1.0, 0.4, 0.3, 1.0))
        };
        draw_text(
            format!("PILOT: {} | {}", player_id_str, status_text),
            dash_x + 14.0,
            dash_y + 44.0,
            14.0,
            status_col,
        );

        // HP Bar
        draw_text(
            "HP",
            dash_x + 14.0,
            dash_y + 68.0,
            14.0,
            Color::new(0.7, 0.85, 1.0, 1.0),
        );
        let hp_bar_x = dash_x + 40.0;
        let hp_bar_y = dash_y + 56.0;
        let hp_bar_w = 200.0;
        let hp_bar_h = 14.0;
        draw_rectangle(
            hp_bar_x,
            hp_bar_y,
            hp_bar_w,
            hp_bar_h,
            Color::new(0.08, 0.10, 0.15, 0.9),
        );
        draw_rectangle_lines(
            hp_bar_x,
            hp_bar_y,
            hp_bar_w,
            hp_bar_h,
            1.0,
            Color::new(0.25, 0.5, 0.7, 0.8),
        );

        let hp_ratio = (hp / max_hp.max(1.0)).clamp(0.0, 1.0);
        let hp_col = if hp_ratio > 0.5 {
            Color::new(0.12, 0.92, 0.35, 1.0)
        } else if hp_ratio > 0.25 {
            Color::new(1.0, 0.8, 0.1, 1.0)
        } else {
            Color::new(1.0, 0.25, 0.2, 1.0)
        };
        if hp_ratio > 0.0 {
            draw_rectangle(
                hp_bar_x + 1.0,
                hp_bar_y + 1.0,
                (hp_bar_w - 2.0) * hp_ratio,
                hp_bar_h - 2.0,
                hp_col,
            );
        }
        draw_text(
            format!("{:.0}/{:.0}", hp, max_hp),
            hp_bar_x + hp_bar_w + 10.0,
            dash_y + 68.0,
            13.0,
            WHITE,
        );

        let cur_pos = client_prediction.pos();
        let yaw_deg = camera.camera_yaw.to_degrees().rem_euclid(360.0);
        let cardinal = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
            [((yaw_deg + 22.5) / 45.0) as usize % 8];
        draw_text(
            format!("POS: ({:.1}, {:.1}, {:.1})", cur_pos.x, cur_pos.y, cur_pos.z),
            dash_x + 14.0,
            dash_y + 92.0,
            14.0,
            Color::new(0.75, 0.85, 0.95, 1.0),
        );
        draw_text(
            format!("HDG: {:.0}° [{}] | SCORE: {}", yaw_deg, cardinal, score),
            dash_x + 14.0,
            dash_y + 112.0,
            14.0,
            Color::new(0.2, 0.85, 0.7, 1.0),
        );
        draw_text(
            "[WASD] Move | [Drag/Wheel] Camera | [Enter] Console",
            dash_x + 14.0,
            dash_y + 130.0,
            12.0,
            Color::new(0.45, 0.6, 0.75, 0.8),
        );

        // Message logs at top-left (below dashboard)
        let log_box_x = 16.0;
        let log_box_y = dash_y + dash_h + 8.0;
        let log_box_w = 340.0;
        let log_box_h = 100.0;

        draw_rectangle(
            log_box_x,
            log_box_y,
            log_box_w,
            log_box_h,
            Color::new(0.02, 0.04, 0.08, 0.80),
        );
        draw_rectangle_lines(
            log_box_x,
            log_box_y,
            log_box_w,
            log_box_h,
            1.0,
            Color::new(0.12, 0.35, 0.6, 0.7),
        );
        draw_text(
            "◆ COMMUNICATIONS & SYSTEM LOG",
            log_box_x + 12.0,
            log_box_y + 18.0,
            13.0,
            Color::new(0.0, 0.8, 1.0, 0.85),
        );

        let recent_logs: Vec<_> = message_logs.iter().rev().take(4).collect();
        for (i, entry) in recent_logs.iter().rev().enumerate() {
            let line_y = log_box_y + 38.0 + (i as f32) * 15.0;
            draw_text(&entry.text, log_box_x + 14.0, line_y, 13.0, entry.color);
        }

        // Command Prompt input box if in command_mode
        if command_mode {
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
            draw_line(
                bar_x,
                bar_y,
                bar_x + 24.0,
                bar_y,
                3.0,
                Color::new(0.2, 1.0, 0.8, 1.0),
            );

            let cursor = if ((get_time() * 3.0) as usize).is_multiple_of(2) {
                "_"
            } else {
                " "
            };
            draw_text(
                format!("COMMAND > {}{}", command_input, cursor),
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

            draw_text(
                "Quick commands: (heal) (tp x z) (spawn-cube x y z) (say msg)",
                bar_x + 4.0,
                bar_y - 6.0,
                12.0,
                Color::new(0.35, 0.7, 0.9, 0.75),
            );
        } else {
            draw_text(
                "Press [Enter] to open Scheme console",
                180.0,
                screen_height() - 14.0,
                13.0,
                Color::new(0.4, 0.6, 0.8, 0.7),
            );
        }

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
    fn test_event_decoding() {
        let echo_env: ServerEnvelope<ClientState, LispEvent> =
            ServerEnvelope::Event(LispEvent::CommandEcho {
                success: true,
                output: "42".to_string(),
            });
        let json_bytes = serde_json::to_vec(&echo_env).unwrap();
        let decoded =
            DualCodec::decode_server::<ClientState, LispEvent>(&json_bytes).expect("decode echo");
        match decoded {
            ServerEnvelope::Event(LispEvent::CommandEcho { success, output }) => {
                assert!(success);
                assert_eq!(output, "42");
            }
            _ => panic!("Expected CommandEcho"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_resolve_server_address() {
        let args = vec!["client".to_string(), "--port".to_string(), "9000".to_string()];
        assert_eq!(
            resolve_server_address(&args, "8087"),
            "ws://127.0.0.1:9000/ws"
        );

        let explicit = vec![
            "client".to_string(),
            "ws://192.168.1.100:8088/ws".to_string(),
        ];
        assert_eq!(
            resolve_server_address(&explicit, "8087"),
            "ws://192.168.1.100:8088/ws"
        );
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
