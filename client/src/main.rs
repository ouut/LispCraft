use macroquad::prelude::*;
use quad_net::web_socket::WebSocket;
use shared::{
    ClientEnvelope, ClientState, LispEvent, LispGameCommand, LispMotionInput,
    ServerEnvelope, ServerMsg,
};
use std::collections::HashSet;

/// 生产级全平台通用 WebSocket 套接字封装 (消除 255 字节 TCP 限制并统一双端协议)
pub struct GameSocket {
    ws: WebSocket,
}

impl GameSocket {
    pub fn connect(addr: &str) -> Result<Self, String> {
        let addr_str = addr.to_string();
        let res = std::panic::catch_unwind(move || {
            WebSocket::connect(addr_str)
        });
        match res {
            Ok(Ok(ws)) => Ok(GameSocket { ws }),
            Ok(Err(e)) => Err(format!("{:?}", e)),
            Err(_) => Err(format!("Could not connect to WebSocket at {}", addr)),
        }
    }

    pub fn send(&mut self, data: &[u8]) {
        self.ws.send_bytes(data);
    }

    pub fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.ws.try_recv()
    }
}

/// 日志与通知条目
struct LogEntry {
    text: String,
    color: Color,
    #[allow(dead_code)]
    timestamp: f64,
}

/// 3D 浮动标牌投影数据
struct ProjectedLabel {
    pos: Vec2,
    text: String,
    color: Color,
}

/// 将 3D 世界坐标投射到 2D 屏幕坐标 (NDC 变换)
fn world_to_screen(camera: &Camera3D, point: Vec3) -> Option<Vec2> {
    let mat = camera.matrix();
    let p = mat * vec4(point.x, point.y, point.z, 1.0);
    if p.w <= 0.05 {
        return None;
    }
    let ndc_x = p.x / p.w;
    let ndc_y = p.y / p.w;
    let ndc_z = p.z / p.w;

    // 剔除近裁切面后方及远裁切面以外
    if ndc_z < -1.0 || ndc_z > 1.0 {
        return None;
    }

    let sw = screen_width();
    let sh = screen_height();
    let sx = (ndc_x + 1.0) * 0.5 * sw;
    let sy = (1.0 - ndc_y) * 0.5 * sh;

    // 屏幕边缘容差
    if sx < -100.0 || sx > sw + 100.0 || sy < -100.0 || sy > sh + 100.0 {
        return None;
    }

    Some(vec2(sx, sy))
}

fn add_log(logs: &mut Vec<LogEntry>, text: String, color: Color) {
    logs.push(LogEntry {
        text,
        color,
        timestamp: get_time(),
    });
    if logs.len() > 30 {
        logs.remove(0);
    }
}

#[macroquad::main("LispCraft 3D")]
async fn main() {
    let ws_port = std::env::var("PORT")
        .or_else(|_| std::env::var("WS_PORT"))
        .unwrap_or_else(|_| "8080".to_string());
    #[cfg(not(target_arch = "wasm32"))]
    let server_addr = {
        let mut addr = format!("ws://127.0.0.1:{}/ws", ws_port);
        if let Some(arg) = std::env::args().nth(1) {
            if arg.starts_with("ws://") || arg.starts_with("wss://") {
                addr = if arg.ends_with("/ws") {
                    arg
                } else {
                    format!("{}/ws", arg.trim_end_matches('/'))
                };
            } else if arg.contains(':') {
                addr = format!("ws://{}/ws", arg.trim_end_matches('/'));
            } else {
                addr = format!("ws://{}:{}/ws", arg, ws_port);
            }
        }
        addr
    };
    #[cfg(target_arch = "wasm32")]
    let server_addr = format!("ws://127.0.0.1:{}/ws", ws_port);

    println!("[Client] Connecting to {}", server_addr);
    let mut socket = match GameSocket::connect(&server_addr) {
        Ok(mut s) => {
            // 连接首包发送 ClientEnvelope::Auth
            let auth_env: ClientEnvelope<LispMotionInput, LispGameCommand> = ClientEnvelope::Auth {
                token: format!("token_{}", (get_time() * 1000.0) as u64),
                client_version: "0.2.0".to_string(),
            };
            if let Ok(bytes) = serde_json::to_vec(&auth_env) {
                s.send(&bytes);
            }
            Some(s)
        }
        Err(e) => {
            eprintln!("[Client] Connect error: {}", e);
            None
        }
    };
    let mut last_reconnect_time = 0.0_f32;
    let mut last_ping_time = 0.0_f32;

    let mut current_state: Option<ClientState> = None;
    let mut command_input = String::new();
    let mut command_mode = false;
    let mut command_history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;

    // 平滑摄像机状态
    let mut cam_target_pos = vec3(0.0, 0.0, 0.0);
    let mut cam_initialized = false;
    let mut camera_yaw: f32 = 0.0;
    let mut target_camera_yaw: f32 = 0.0;
    let mut camera_pitch: f32 = 0.45;
    let mut target_camera_pitch: f32 = 0.45;
    let mut camera_dist: f32 = 11.0;
    let mut target_camera_dist: f32 = 11.0;

    // 鼠标交互状态
    let mut last_mouse_pos = vec2(0.0, 0.0);

    // 日志系统
    let mut message_logs: Vec<LogEntry> = Vec::new();
    let mut known_server_logs: HashSet<String> = HashSet::new();

    add_log(
        &mut message_logs,
        "System: Initializing LispCraft 3D Engine...".to_string(),
        Color::new(0.0, 0.85, 1.0, 1.0),
    );

    loop {
        let dt = get_frame_time().min(0.1);
        let time = get_time() as f32;

        // 自动重连机制：若尚未连接服务端，每隔 2 秒尝试重新连接
        if socket.is_none() && time - last_reconnect_time > 2.0 {
            last_reconnect_time = time;
            if let Ok(mut s) = GameSocket::connect(&server_addr) {
                // 连接首包发送 ClientEnvelope::Auth
                let auth_env: ClientEnvelope<LispMotionInput, LispGameCommand> = ClientEnvelope::Auth {
                    token: format!("token_{}", (get_time() * 1000.0) as u64),
                    client_version: "0.2.0".to_string(),
                };
                if let Ok(bytes) = serde_json::to_vec(&auth_env) {
                    s.send(&bytes);
                }
                add_log(
                    &mut message_logs,
                    format!("[NET] Connected to Scheme Game Server ({})", server_addr),
                    Color::new(0.2, 0.95, 0.6, 1.0),
                );
                socket = Some(s);
            }
        }

        // 后台 3 秒 ClientEnvelope::Ping 心跳保活
        if time - last_ping_time >= 3.0 {
            last_ping_time = time;
            if let Some(ref mut sock) = socket {
                let client_time_ms = (get_time() * 1000.0) as u64;
                let ping_env: ClientEnvelope<LispMotionInput, LispGameCommand> = ClientEnvelope::Ping { client_time_ms };
                if let Ok(bytes) = serde_json::to_vec(&ping_env) {
                    sock.send(&bytes);
                }
            }
        }

        // --- 1. 网络收包与状态同步 ---
        if let Some(ref mut sock) = socket {
            while let Some(bytes) = sock.try_recv() {
                if let Ok(envelope) = serde_json::from_slice::<ServerEnvelope<ClientState, LispEvent>>(&bytes) {
                    match envelope {
                        ServerEnvelope::AuthSuccess { player_id, server_tick } => {
                            let client_id = format!("player_{}", player_id);
                            println!("[Client] AuthSuccess: joined world as {} (tick {})", client_id, server_tick);
                            add_log(
                                &mut message_logs,
                                format!("[SYS] Welcome Commander {}! (Server Tick {})", client_id, server_tick),
                                Color::new(0.2, 0.95, 0.6, 1.0),
                            );
                        }
                        ServerEnvelope::Snapshot(state) => {
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
                            current_state = Some(state);
                        }
                        ServerEnvelope::Event(evt) => {
                            match evt {
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
                            }
                        }
                        ServerEnvelope::Pong { client_time_ms, .. } => {
                            let now_ms = (get_time() * 1000.0) as u64;
                            let _rtt = now_ms.saturating_sub(client_time_ms);
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
                    // 兼容裸 ClientState 同步
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
                    current_state = Some(state);
                } else if let Ok(srv_msg) = serde_json::from_slice::<ServerMsg>(&bytes) {
                    // 兼容旧版 ServerMsg
                    match srv_msg {
                        ServerMsg::Welcome { client_id } => {
                            println!("[Client] Joined world as {}", client_id);
                            add_log(
                                &mut message_logs,
                                format!("[SYS] Welcome Commander {}!", client_id),
                                Color::new(0.2, 0.95, 0.6, 1.0),
                            );
                        }
                        ServerMsg::SyncState(st) => {
                            for log in &st.logs {
                                if !known_server_logs.contains(log) {
                                    known_server_logs.insert(log.clone());
                                    add_log(
                                        &mut message_logs,
                                        format!("[NET] {}", log),
                                        Color::new(0.9, 0.8, 0.3, 1.0),
                                    );
                                }
                            }
                            current_state = Some(st);
                        }
                        ServerMsg::CommandEcho { success, output } => {
                            let col = if success {
                                Color::new(0.25, 0.95, 0.5, 1.0)
                            } else {
                                Color::new(1.0, 0.35, 0.35, 1.0)
                            };
                            add_log(&mut message_logs, format!("[ECHO] {}", output), col);
                        }
                    }
                }
            }
        }

        // --- 2. 鼠标视角拖拽与滚轮缩放 ---
        let mouse_pos = vec2(mouse_position().0, mouse_position().1);
        if is_mouse_button_pressed(MouseButton::Left) || is_mouse_button_pressed(MouseButton::Right) {
            last_mouse_pos = mouse_pos;
        }

        if !command_mode {
            if is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right) {
                let delta_x = mouse_pos.x - last_mouse_pos.x;
                let delta_y = mouse_pos.y - last_mouse_pos.y;
                target_camera_yaw += delta_x * 0.006;
                target_camera_pitch = (target_camera_pitch + delta_y * 0.005).clamp(0.12, 1.35);
            }

            let wheel = mouse_wheel().1;
            if wheel.abs() > 0.01 {
                target_camera_dist = (target_camera_dist - wheel * 0.8).clamp(4.0, 26.0);
            }
        }
        last_mouse_pos = mouse_pos;

        // --- 3. 键盘与控制台交互 ---
        if is_key_pressed(KeyCode::Enter) {
            if !command_mode {
                // 唤起命令行输入模式
                command_mode = true;
                command_input.clear();
                history_index = None;
            } else {
                // 发送命令或关闭
                let trimmed = command_input.trim().to_string();
                if trimmed.is_empty() {
                    command_mode = false;
                } else {
                    if command_history.last() != Some(&trimmed) {
                        command_history.push(trimmed.clone());
                    }
                    history_index = None;

                    if let Some(ref mut sock) = socket {
                        let cmd_env: ClientEnvelope<LispMotionInput, LispGameCommand> =
                            ClientEnvelope::Command(LispGameCommand::SchemeCommand(trimmed.clone()));
                        if let Ok(bytes) = serde_json::to_vec(&cmd_env) {
                            sock.send(&bytes);
                        }
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
            // Esc 退出输入
            if is_key_pressed(KeyCode::Escape) {
                command_mode = false;
                command_input.clear();
                history_index = None;
            }

            // 历史命令回溯 (方向键上/下)
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

            // 字符键入
            while let Some(c) = get_char_pressed() {
                if c >= ' ' && c <= '~' {
                    command_input.push(c);
                }
            }

            // 退格删除
            if is_key_pressed(KeyCode::Backspace) {
                command_input.pop();
            }
        } else {
            // 自由移动与视角旋转控制
            if is_key_down(KeyCode::Left) {
                target_camera_yaw -= 2.2 * dt;
            }
            if is_key_down(KeyCode::Right) {
                target_camera_yaw += 2.2 * dt;
            }
            if is_key_down(KeyCode::Up) {
                target_camera_pitch = (target_camera_pitch - 1.2 * dt).clamp(0.12, 1.35);
            }
            if is_key_down(KeyCode::Down) {
                target_camera_pitch = (target_camera_pitch + 1.2 * dt).clamp(0.12, 1.35);
            }

            // 根据当前视角偏航角计算相对移动向量
            let fwd = vec2(-camera_yaw.sin(), -camera_yaw.cos());
            let right = vec2(camera_yaw.cos(), -camera_yaw.sin());

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
                if let Some(ref mut sock) = socket {
                    let input_env: ClientEnvelope<LispMotionInput, LispGameCommand> =
                        ClientEnvelope::Input(LispMotionInput {
                            dx: norm.x,
                            dz: norm.y,
                            yaw: camera_yaw,
                        });
                    if let Ok(bytes) = serde_json::to_vec(&input_env) {
                        sock.send(&bytes);
                    }
                }
            }
        }

        // --- 4. 摄像机平滑插值跟随 ---
        let smooth_yaw = 1.0 - (-16.0 * dt).exp();
        let smooth_pitch = 1.0 - (-16.0 * dt).exp();
        let smooth_dist = 1.0 - (-14.0 * dt).exp();
        let smooth_pos = 1.0 - (-12.0 * dt).exp();

        camera_yaw += (target_camera_yaw - camera_yaw) * smooth_yaw;
        camera_pitch += (target_camera_pitch - camera_pitch) * smooth_pitch;
        camera_dist += (target_camera_dist - camera_dist) * smooth_dist;

        // 收集需要渲染在 2D 上的实体名称标牌
        let mut projected_labels: Vec<ProjectedLabel> = Vec::new();

        // --- 5. 3D 渲染画面 ---
        clear_background(Color::new(0.05, 0.07, 0.11, 1.0));

        if let Some(ref state) = current_state {
            let target_point = vec3(
                state.camera_target[0],
                state.camera_target[1],
                state.camera_target[2],
            );

            if !cam_initialized {
                cam_target_pos = target_point;
                cam_initialized = true;
            } else {
                cam_target_pos = cam_target_pos.lerp(target_point, smooth_pos);
            }

            // 计算平滑第三人称相机位置
            let horiz_dist = camera_dist * camera_pitch.cos();
            let vert_dist = camera_dist * camera_pitch.sin();
            let cam_offset = vec3(
                camera_yaw.sin() * horiz_dist,
                vert_dist,
                camera_yaw.cos() * horiz_dist,
            );
            let cam_pos = cam_target_pos + cam_offset;

            let camera = Camera3D {
                position: cam_pos,
                target: cam_target_pos,
                up: vec3(0.0, 1.0, 0.0),
                fovy: 45.0_f32.to_radians(),
                aspect: None,
                projection: Projection::Perspective,
                render_target: None,
                viewport: None,
                z_near: 0.1,
                z_far: 1000.0,
            };
            set_camera(&camera);

            // 5.1 地面增强：未来网格底板
            draw_grid(
                60,
                1.0,
                Color::new(0.12, 0.22, 0.38, 0.6),
                Color::new(0.05, 0.08, 0.16, 0.35),
            );
            draw_grid(
                12,
                5.0,
                Color::new(0.0, 0.6, 0.9, 0.75),
                Color::new(0.08, 0.15, 0.28, 0.5),
            );

            // 5.2 地面增强：中心发光信标 (Central Beacon)
            // 基座科技平台
            draw_cube(
                vec3(0.0, -0.05, 0.0),
                vec3(8.0, 0.1, 8.0),
                None,
                Color::new(0.04, 0.07, 0.14, 0.95),
            );
            draw_cube_wires(
                vec3(0.0, -0.05, 0.0),
                vec3(8.0, 0.1, 8.0),
                Color::new(0.0, 0.6, 0.9, 0.8),
            );

            draw_cube(
                vec3(0.0, 0.08, 0.0),
                vec3(4.8, 0.16, 4.8),
                None,
                Color::new(0.07, 0.12, 0.22, 0.95),
            );
            draw_cube_wires(
                vec3(0.0, 0.08, 0.0),
                vec3(4.8, 0.16, 4.8),
                Color::new(0.15, 0.85, 1.0, 0.9),
            );

            // 4 根能量立柱
            for &(px, pz) in &[(-2.0, -2.0), (-2.0, 2.0), (2.0, -2.0), (2.0, 2.0)] {
                let pylon_pos = vec3(px, 0.75, pz);
                draw_cube(
                    pylon_pos,
                    vec3(0.4, 1.5, 0.4),
                    None,
                    Color::new(0.08, 0.14, 0.25, 1.0),
                );
                draw_cube_wires(
                    pylon_pos,
                    vec3(0.4, 1.5, 0.4),
                    Color::new(0.0, 0.8, 1.0, 0.75),
                );
                draw_sphere(
                    vec3(px, 1.55, pz),
                    0.18,
                    None,
                    Color::new(0.0, 1.0, 0.8, 0.95),
                );
            }

            // 悬浮旋转信标核心水晶
            let core_bob = (time * 2.2).sin() * 0.2;
            let core_pos = vec3(0.0, 1.8 + core_bob, 0.0);
            draw_cube(
                core_pos,
                vec3(0.85, 1.25, 0.85),
                None,
                Color::new(0.15, 0.85, 1.0, 0.85),
            );
            draw_cube_wires(
                core_pos,
                vec3(1.15, 1.55, 1.15),
                Color::new(0.8, 1.0, 1.0, 0.9),
            );
            draw_sphere(core_pos, 0.45, None, Color::new(0.8, 1.0, 1.0, 0.95));

            // 通天光柱 (Sky Beacon Beam)
            let beam_pulse = (time * 3.5).sin().abs();
            draw_cylinder(
                vec3(0.0, 22.0, 0.0),
                0.25,
                0.4,
                44.0,
                None,
                Color::new(0.0, 0.9, 1.0, 0.30 + 0.15 * beam_pulse),
            );
            draw_cylinder(
                vec3(0.0, 22.0, 0.0),
                0.75,
                1.1,
                44.0,
                None,
                Color::new(0.1, 0.4, 0.9, 0.12 + 0.05 * beam_pulse),
            );

            // 地面扩散冲击光环
            let r1 = (time * 3.5) % 14.0;
            let a1 = (1.0 - r1 / 14.0).max(0.0) * 0.55;
            draw_cylinder_wires(
                vec3(0.0, 0.04, 0.0),
                r1,
                r1,
                0.04,
                None,
                Color::new(0.0, 0.9, 1.0, a1),
            );

            let r2 = (time * 3.5 + 7.0) % 14.0;
            let a2 = (1.0 - r2 / 14.0).max(0.0) * 0.55;
            draw_cylinder_wires(
                vec3(0.0, 0.04, 0.0),
                r2,
                r2,
                0.04,
                None,
                Color::new(0.2, 0.6, 1.0, a2),
            );

            // 5.3 实体渲染：根据 shape ("cube", "sphere") 绘制几何体和边框
            for ent in &state.entities {
                let pos = vec3(ent.pos[0], ent.pos[1], ent.pos[2]);
                let size = vec3(ent.size[0], ent.size[1], ent.size[2]);
                let col = Color::new(ent.color[0], ent.color[1], ent.color[2], ent.color[3]);
                let wire_col = Color::new(
                    (ent.color[0] + 0.35).min(1.0),
                    (ent.color[1] + 0.35).min(1.0),
                    (ent.color[2] + 0.35).min(1.0),
                    0.85,
                );

                let top_y = if ent.shape == "sphere" {
                    let radius = size.x * 0.5;
                    draw_sphere(pos, radius, None, col);
                    draw_sphere_wires(pos, radius, None, wire_col);
                    pos.y + radius
                } else {
                    draw_cube(pos, size, None, col);
                    draw_cube_wires(pos, size, wire_col);
                    pos.y + size.y * 0.5
                };

                // 地面投影阴影圈
                draw_cylinder_wires(
                    vec3(pos.x, 0.02, pos.z),
                    size.x * 0.55,
                    size.z * 0.55,
                    0.02,
                    None,
                    Color::new(0.0, 0.0, 0.0, 0.45),
                );

                // 实体顶部 3D 浮动标牌位置投影
                if !ent.label.is_empty() {
                    let anchor_3d = vec3(pos.x, top_y + 0.55, pos.z);
                    if let Some(screen_p) = world_to_screen(&camera, anchor_3d) {
                        projected_labels.push(ProjectedLabel {
                            pos: screen_p,
                            text: ent.label.clone(),
                            color: col,
                        });
                    }
                }
            }

            // 5.4 玩家自身：独特的机体标志 (Avatar)
            let av_pos = cam_target_pos;

            // 机身主装甲躯干
            let torso_pos = av_pos + vec3(0.0, 0.85, 0.0);
            draw_cube(
                torso_pos,
                vec3(0.85, 0.95, 0.65),
                None,
                Color::new(0.18, 0.55, 0.92, 1.0),
            );
            draw_cube_wires(
                torso_pos,
                vec3(0.85, 0.95, 0.65),
                Color::new(0.5, 0.85, 1.0, 0.9),
            );

            // 胸口发光能量核心
            let core_pulse = (time * 6.0).sin() * 0.03;
            draw_sphere(
                torso_pos + vec3(0.0, 0.0, -0.34),
                0.16 + core_pulse,
                None,
                Color::new(0.0, 1.0, 0.85, 1.0),
            );

            // 驾驶舱头部与扫描面罩
            let head_pos = av_pos + vec3(0.0, 1.5, 0.0);
            draw_cube(
                head_pos,
                vec3(0.5, 0.38, 0.5),
                None,
                Color::new(0.12, 0.18, 0.28, 1.0),
            );
            draw_cube_wires(
                head_pos,
                vec3(0.5, 0.38, 0.5),
                Color::new(0.35, 0.65, 0.9, 0.85),
            );
            draw_cube(
                head_pos + vec3(0.0, 0.0, -0.26),
                vec3(0.35, 0.12, 0.04),
                None,
                Color::new(0.0, 1.0, 0.6, 1.0),
            );

            // 左右战术推进翼
            let wing_l = av_pos + vec3(-0.62, 0.95, 0.0);
            let wing_r = av_pos + vec3(0.62, 0.95, 0.0);
            draw_cube(
                wing_l,
                vec3(0.28, 0.55, 0.45),
                None,
                Color::new(0.14, 0.32, 0.58, 1.0),
            );
            draw_cube_wires(
                wing_l,
                vec3(0.28, 0.55, 0.45),
                Color::new(0.4, 0.7, 0.9, 0.8),
            );
            draw_cube(
                wing_r,
                vec3(0.28, 0.55, 0.45),
                None,
                Color::new(0.14, 0.32, 0.58, 1.0),
            );
            draw_cube_wires(
                wing_r,
                vec3(0.28, 0.55, 0.45),
                Color::new(0.4, 0.7, 0.9, 0.8),
            );

            // 尾部主喷射引擎及脉冲粒子尾焰
            let booster_pos = av_pos + vec3(0.0, 0.85, 0.35);
            draw_cylinder(
                booster_pos,
                0.16,
                0.20,
                0.30,
                None,
                Color::new(0.18, 0.24, 0.36, 1.0),
            );
            let flame_len = 0.25 + 0.12 * (time * 16.0).sin().abs();
            draw_cylinder(
                booster_pos + vec3(0.0, 0.0, 0.22),
                0.14,
                0.02,
                flame_len,
                None,
                Color::new(0.1, 0.75, 1.0, 0.85),
            );

            // 自身脚底光环底盘
            let shadow_r = 0.8 + 0.04 * (time * 4.0).sin();
            draw_cylinder_wires(
                av_pos + vec3(0.0, 0.03, 0.0),
                shadow_r,
                shadow_r,
                0.03,
                None,
                Color::new(0.0, 0.85, 1.0, 0.5),
            );

            // 投影玩家自身头顶位置
            let player_head_anchor = av_pos + vec3(0.0, 2.15, 0.0);
            let player_screen_p = world_to_screen(&camera, player_head_anchor);

            // --- 6. 2D 界面渲染 (切回正交 HUD) ---
            set_default_camera();

            // 6.1 实体顶部 3D 浮动名字标牌
            for label in &projected_labels {
                let text_dim = measure_text(&label.text, None, 14, 1.0);
                let tag_w = text_dim.width + 16.0;
                let tag_h = 20.0;
                let tag_x = label.pos.x - tag_w * 0.5;
                let tag_y = label.pos.y - tag_h;

                draw_rectangle(
                    tag_x,
                    tag_y,
                    tag_w,
                    tag_h,
                    Color::new(0.03, 0.06, 0.12, 0.82),
                );
                draw_rectangle_lines(
                    tag_x,
                    tag_y,
                    tag_w,
                    tag_h,
                    1.0,
                    Color::new(
                        (label.color.r * 0.8 + 0.2).min(1.0),
                        (label.color.g * 0.8 + 0.2).min(1.0),
                        (label.color.b * 0.8 + 0.2).min(1.0),
                        0.85,
                    ),
                );
                draw_text(&label.text, tag_x + 8.0, tag_y + 14.0, 14.0, WHITE);
            }

            // 6.2 玩家自身头顶绿色血条与代号
            if let Some(pp) = player_screen_p {
                let bar_w = 84.0;
                let bar_h = 8.0;
                let bar_x = pp.x - bar_w * 0.5;
                let bar_y = pp.y;

                let callsign = format!("[YOU] {}", state.client_id);
                let cdim = measure_text(&callsign, None, 13, 1.0);
                let cx = pp.x - cdim.width * 0.5;
                draw_rectangle(
                    cx - 4.0,
                    bar_y - 18.0,
                    cdim.width + 8.0,
                    15.0,
                    Color::new(0.02, 0.04, 0.08, 0.75),
                );
                draw_rectangle_lines(
                    cx - 4.0,
                    bar_y - 18.0,
                    cdim.width + 8.0,
                    15.0,
                    1.0,
                    Color::new(0.2, 0.75, 1.0, 0.8),
                );
                draw_text(
                    &callsign,
                    cx,
                    bar_y - 6.0,
                    13.0,
                    Color::new(0.3, 0.9, 1.0, 1.0),
                );

                draw_rectangle(
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_h,
                    Color::new(0.02, 0.05, 0.08, 0.85),
                );
                draw_rectangle_lines(
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_h,
                    1.2,
                    Color::new(0.2, 0.45, 0.65, 0.9),
                );

                let hp_ratio = (state.hp / state.max_hp.max(1.0)).clamp(0.0, 1.0);
                let hp_fill_w = (bar_w - 2.0) * hp_ratio;
                let hp_col = if hp_ratio > 0.5 {
                    Color::new(0.12, 0.92, 0.35, 1.0)
                } else if hp_ratio > 0.25 {
                    Color::new(1.0, 0.8, 0.1, 1.0)
                } else {
                    Color::new(1.0, 0.25, 0.2, 1.0)
                };
                if hp_fill_w > 0.0 {
                    draw_rectangle(bar_x + 1.0, bar_y + 1.0, hp_fill_w, bar_h - 2.0, hp_col);
                }
            }

            // 6.3 左上角专业级状态仪表盘
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
            draw_text(
                &format!("PILOT: {}", state.client_id),
                dash_x + 14.0,
                dash_y + 44.0,
                16.0,
                WHITE,
            );

            // 仪表盘血条
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

            let hp_ratio = (state.hp / state.max_hp.max(1.0)).clamp(0.0, 1.0);
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
                &format!("{:.0}/{:.0}", state.hp, state.max_hp),
                hp_bar_x + hp_bar_w + 10.0,
                dash_y + 68.0,
                13.0,
                WHITE,
            );

            let yaw_deg = camera_yaw.to_degrees().rem_euclid(360.0);
            let cardinal = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
                [((yaw_deg + 22.5) / 45.0) as usize % 8];
            draw_text(
                &format!(
                    "POS: ({:.1}, {:.1}, {:.1})",
                    state.camera_target[0], state.camera_target[1], state.camera_target[2]
                ),
                dash_x + 14.0,
                dash_y + 92.0,
                14.0,
                Color::new(0.75, 0.85, 0.95, 1.0),
            );
            draw_text(
                &format!(
                    "HDG: {:.0}° [{}] | SCORE: {}",
                    yaw_deg, cardinal, state.score
                ),
                dash_x + 14.0,
                dash_y + 112.0,
                14.0,
                Color::new(0.2, 0.85, 0.7, 1.0),
            );
            draw_text(
                "[WASD] Move | [Arrows/Drag] View | [Enter] Console",
                dash_x + 14.0,
                dash_y + 130.0,
                12.0,
                Color::new(0.45, 0.6, 0.75, 0.8),
            );

            // 6.4 右上角战术雷达 (Tactical Radar Minimap)
            let rad_center = vec2(screen_width() - 85.0, 85.0);
            let rad_r = 65.0;

            draw_circle(
                rad_center.x,
                rad_center.y,
                rad_r,
                Color::new(0.02, 0.04, 0.08, 0.85),
            );
            draw_circle_lines(
                rad_center.x,
                rad_center.y,
                rad_r * 0.33,
                1.0,
                Color::new(0.1, 0.35, 0.55, 0.45),
            );
            draw_circle_lines(
                rad_center.x,
                rad_center.y,
                rad_r * 0.66,
                1.0,
                Color::new(0.1, 0.35, 0.55, 0.45),
            );
            draw_circle_lines(
                rad_center.x,
                rad_center.y,
                rad_r,
                2.0,
                Color::new(0.0, 0.85, 1.0, 0.9),
            );

            draw_line(
                rad_center.x - rad_r,
                rad_center.y,
                rad_center.x + rad_r,
                rad_center.y,
                1.0,
                Color::new(0.1, 0.35, 0.55, 0.4),
            );
            draw_line(
                rad_center.x,
                rad_center.y - rad_r,
                rad_center.x,
                rad_center.y + rad_r,
                1.0,
                Color::new(0.1, 0.35, 0.55, 0.4),
            );

            // 旋转扫描波束
            let scan_ang = time * 2.8;
            let scan_end = rad_center + vec2(scan_ang.cos(), scan_ang.sin()) * rad_r;
            draw_line(
                rad_center.x,
                rad_center.y,
                scan_end.x,
                scan_end.y,
                1.5,
                Color::new(0.0, 1.0, 0.6, 0.6),
            );

            draw_text(
                "TACTICAL RADAR",
                rad_center.x - 48.0,
                rad_center.y - rad_r - 6.0,
                12.0,
                Color::new(0.0, 0.9, 1.0, 0.9),
            );

            // 玩家中心图标
            draw_circle(rad_center.x, rad_center.y, 3.5, Color::new(0.1, 1.0, 0.4, 1.0));
            draw_line(
                rad_center.x,
                rad_center.y,
                rad_center.x,
                rad_center.y - 7.0,
                2.0,
                Color::new(0.1, 1.0, 0.4, 1.0),
            );

            // 实体与中心信标雷达投影 (以视野朝向为正上方)
            let px = state.camera_target[0];
            let pz = state.camera_target[2];
            let radar_range = 35.0;

            // 中心信标雷达图标
            let bx = -px;
            let bz = -pz;
            let brad_x = (bx * camera_yaw.cos() - bz * camera_yaw.sin()) / radar_range * rad_r;
            let brad_y = (bx * camera_yaw.sin() + bz * camera_yaw.cos()) / radar_range * rad_r;
            if (brad_x * brad_x + brad_y * brad_y).sqrt() <= rad_r - 3.0 {
                let bp = rad_center + vec2(brad_x, brad_y);
                draw_circle(bp.x, bp.y, 4.0, Color::new(0.0, 0.9, 1.0, 1.0));
                draw_circle_lines(bp.x, bp.y, 6.0, 1.0, WHITE);
            }

            for ent in &state.entities {
                let ex = ent.pos[0] - px;
                let ez = ent.pos[2] - pz;
                let rx = (ex * camera_yaw.cos() - ez * camera_yaw.sin()) / radar_range * rad_r;
                let ry = (ex * camera_yaw.sin() + ez * camera_yaw.cos()) / radar_range * rad_r;
                let d = (rx * rx + ry * ry).sqrt();
                if d <= rad_r - 3.0 {
                    let ep = rad_center + vec2(rx, ry);
                    let col = Color::new(ent.color[0], ent.color[1], ent.color[2], 0.95);
                    draw_circle(ep.x, ep.y, 3.0, col);
                }
            }

            // 6.5 左侧/底部消息通知栏 (滚动显示最近 4 条系统日志与广播消息)
            let log_box_w = 460.0_f32.min(screen_width() - 32.0);
            let log_box_h = 100.0;
            let log_box_x = 16.0;
            let log_box_y = if command_mode {
                screen_height() - 65.0 - log_box_h - 10.0
            } else {
                screen_height() - 35.0 - log_box_h - 10.0
            };

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
                let line_y = log_box_y + 38.0 + (i as f32) * 16.0;
                draw_text(&entry.text, log_box_x + 14.0, line_y, 14.0, entry.color);
            }
        } else {
            // 未连接状态提示
            set_default_camera();
            let cx = screen_width() * 0.5;
            let cy = screen_height() * 0.5;
            let dots = match ((time * 2.5) as usize) % 4 {
                0 => "",
                1 => ".",
                2 => "..",
                _ => "...",
            };
            let title = format!("Connecting to Scheme Game Server{}", dots);
            let tdim = measure_text(&title, None, 28, 1.0);
            draw_text(
                &title,
                cx - tdim.width * 0.5,
                cy - 20.0,
                28.0,
                Color::new(0.0, 0.9, 1.0, 1.0),
            );
            let sub = format!("Target: {}", server_addr);
            let sdim = measure_text(&sub, None, 18, 1.0);
            draw_text(&sub, cx - sdim.width * 0.5, cy + 15.0, 18.0, LIGHTGRAY);
            let hint = "Ensure Server is running on port 8088 (Native) / 8089 (WASM WebSocket)";
            let hdim = measure_text(hint, None, 14, 1.0);
            draw_text(hint, cx - hdim.width * 0.5, cy + 45.0, 14.0, DARKGRAY);
        }

        // --- 7. 生产级命令控制台 ---
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

            let cursor = if ((time * 3.0) as usize) % 2 == 0 {
                "_"
            } else {
                " "
            };
            draw_text(
                &format!("COMMAND > {}{}", command_input, cursor),
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
                "Quick commands: heal, tp, spawn cube my_box, say hello",
                bar_x + 4.0,
                bar_y - 6.0,
                12.0,
                Color::new(0.35, 0.7, 0.9, 0.75),
            );
        } else {
            draw_text(
                "Press [Enter] to open command console",
                18.0,
                screen_height() - 14.0,
                13.0,
                Color::new(0.4, 0.6, 0.8, 0.7),
            );
        }

        next_frame().await;
    }
}
