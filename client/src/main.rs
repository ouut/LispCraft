use macroquad::prelude::*;
use quad_net::quad_socket::client::QuadSocket;
use shared::{ClientMsg, ClientState, ServerMsg};

#[macroquad::main("LispCraft 3D")]
async fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let server_addr = "127.0.0.1:8088";
    #[cfg(target_arch = "wasm32")]
    let server_addr = "ws://127.0.0.1:8089";

    println!("[Client] Connecting to {}", server_addr);
    let mut socket = match QuadSocket::connect(server_addr) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("[Client] Connect error: {:?}", e);
            None
        }
    };

    let mut current_state: Option<ClientState> = None;
    let mut command_input = String::new();
    let mut command_mode = false;
    let mut camera_yaw: f32 = 0.0;
    let mut last_echo: Option<String> = None;

    loop {
        // --- 1. 网络收包 ---
        if let Some(ref mut sock) = socket {
            while let Some(bytes) = sock.try_recv() {
                // 尝试解析状态同步包
                if let Ok(state) = serde_json::from_slice::<ClientState>(&bytes) {
                    current_state = Some(state);
                } else if let Ok(srv_msg) = serde_json::from_slice::<ServerMsg>(&bytes) {
                    match srv_msg {
                        ServerMsg::Welcome { client_id } => {
                            println!("[Client] Joined world as {}", client_id);
                        }
                        ServerMsg::SyncState(st) => {
                            current_state = Some(st);
                        }
                        ServerMsg::CommandEcho { success: _, output } => {
                            last_echo = Some(output);
                        }
                    }
                }
            }
        }

        // --- 2. 键盘与输入处理 ---
        if is_key_pressed(KeyCode::Enter) {
            command_mode = !command_mode;
            if !command_mode && !command_input.trim().is_empty() {
                let cmd_text = command_input.trim().to_string();
                if let Some(ref mut sock) = socket {
                    let msg = ClientMsg::Command(cmd_text);
                    if let Ok(bytes) = serde_json::to_vec(&msg) {
                        sock.send(&bytes);
                    }
                }
                command_input.clear();
            }
        }

        if command_mode {
            while let Some(c) = get_char_pressed() {
                if c >= ' ' && c <= '~' {
                    command_input.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                command_input.pop();
            }
        } else {
            let mut dx = 0.0;
            let mut dz = 0.0;
            if is_key_down(KeyCode::W) { dz -= 1.0; }
            if is_key_down(KeyCode::S) { dz += 1.0; }
            if is_key_down(KeyCode::A) { dx -= 1.0; }
            if is_key_down(KeyCode::D) { dx += 1.0; }
            if is_key_down(KeyCode::Left) { camera_yaw -= 0.03; }
            if is_key_down(KeyCode::Right) { camera_yaw += 0.03; }

            if dx != 0.0 || dz != 0.0 {
                if let Some(ref mut sock) = socket {
                    let msg = ClientMsg::Move { dx, dz, yaw: camera_yaw };
                    if let Ok(bytes) = serde_json::to_vec(&msg) {
                        sock.send(&bytes);
                    }
                }
            }
        }

        // --- 3. 3D 渲染画面 ---
        clear_background(Color::new(0.06, 0.08, 0.12, 1.0));

        if let Some(ref state) = current_state {
            let target = vec3(state.camera_target[0], state.camera_target[1], state.camera_target[2]);
            let cam_offset = vec3(camera_yaw.sin() * 10.0, 6.0, camera_yaw.cos() * 10.0);
            let cam_pos = target + cam_offset;

            set_camera(&Camera3D {
                position: cam_pos,
                target,
                up: vec3(0.0, 1.0, 0.0),
                fovy: 45.0,
                aspect: None,
                projection: Projection::Perspective,
                render_target: None,
                viewport: None,
                z_near: 0.1,
                z_far: 1000.0,
            });

            // 绘制网格与坐标原点
            draw_grid(40, 1.0, Color::new(0.2, 0.25, 0.35, 1.0), Color::new(0.1, 0.12, 0.18, 1.0));

            // 根据服务端下发的数据渲染场景实体 (纯渲染，无业务规则)
            for ent in &state.entities {
                let pos = vec3(ent.pos[0], ent.pos[1], ent.pos[2]);
                let size = vec3(ent.size[0], ent.size[1], ent.size[2]);
                let col = Color::new(ent.color[0], ent.color[1], ent.color[2], ent.color[3]);

                if ent.shape == "sphere" {
                    draw_sphere(pos, size.x * 0.5, None, col);
                    draw_sphere_wires(pos, size.x * 0.5, None, Color::new(1.0, 1.0, 1.0, 0.2));
                } else {
                    draw_cube(pos, size, None, col);
                    draw_cube_wires(pos, size, Color::new(0.0, 0.0, 0.0, 0.4));
                }
            }

            // 渲染自己的机体
            draw_cube(target, vec3(0.8, 1.6, 0.8), None, GOLD);
            draw_cube_wires(target, vec3(0.8, 1.6, 0.8), ORANGE);

            set_default_camera();

            // --- 4. 2D HUD ---
            draw_rectangle(10.0, 10.0, 420.0, 90.0, Color::new(0.0, 0.0, 0.0, 0.65));
            draw_rectangle_lines(10.0, 10.0, 420.0, 90.0, 1.5, SKYBLUE);
            draw_text(&format!("Player ID: {}", state.client_id), 20.0, 32.0, 20.0, GREEN);
            draw_text(&format!("Pos: ({:.1}, {:.1}) | HP: {:.0}/{:.0}", state.camera_target[0], state.camera_target[2], state.hp, state.max_hp), 20.0, 56.0, 18.0, WHITE);
            draw_text("[W/A/S/D] Move | [<-/->] Orbit | [Enter] Command Bar", 20.0, 80.0, 15.0, LIGHTGRAY);

            if let Some(ref echo) = last_echo {
                draw_text(&format!("Notice: {}", echo), 20.0, 120.0, 18.0, YELLOW);
            }
        } else {
            set_default_camera();
            draw_text("Connecting to Scheme Game Server...", 30.0, 60.0, 28.0, YELLOW);
            draw_text("Ensure Server is running on 127.0.0.1:8088 / 8089", 30.0, 95.0, 18.0, LIGHTGRAY);
        }

        // 命令输入栏
        if command_mode {
            let bar_y = screen_height() - 55.0;
            draw_rectangle(15.0, bar_y, screen_width() - 30.0, 40.0, Color::new(0.0, 0.0, 0.0, 0.88));
            draw_rectangle_lines(15.0, bar_y, screen_width() - 30.0, 40.0, 2.0, GREEN);
            draw_text(&format!("COMMAND > {}_", command_input), 25.0, bar_y + 26.0, 20.0, GREEN);
        }

        next_frame().await;
    }
}
