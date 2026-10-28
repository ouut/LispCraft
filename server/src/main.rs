use std::env;
use std::fs;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use quad_net::quad_socket::server::{listen, Settings, SocketHandle};
use shared::{ClientMsg, ServerMsg};
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;

static SEED: AtomicU64 = AtomicU64::new(88172645463325252);
fn prng() -> f64 {
    let mut x = SEED.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, Ordering::Relaxed);
    (x as f64) / (u64::MAX as f64)
}

#[derive(Default)]
struct ClientSession {
    id: Option<String>,
}

struct SchemeGame {
    engine: Mutex<Engine>,
}

impl SchemeGame {
    pub fn new(script_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let script = fs::read_to_string(script_path)?;
        let mut engine = Engine::new();
        engine.register_fn("random", prng);
        engine.run(script)?;
        engine.run("(init-game)".to_string())?;
        println!("[SchemeGame] Successfully loaded script '{}' and initialized world.", script_path);
        Ok(Self {
            engine: Mutex::new(engine),
        })
    }

    pub fn on_join(&self, id: &str) {
        let mut engine = self.engine.lock().unwrap();
        let code = format!(r#"(on-player-join "{}")"#, id);
        let _ = engine.run(code);
        println!("[SchemeGame] Player joined: {}", id);
    }

    pub fn on_leave(&self, id: &str) {
        let mut engine = self.engine.lock().unwrap();
        let code = format!(r#"(on-player-leave "{}")"#, id);
        let _ = engine.run(code);
        println!("[SchemeGame] Player left: {}", id);
    }

    pub fn on_move(&self, id: &str, dx: f32, dz: f32, yaw: f32) {
        let mut engine = self.engine.lock().unwrap();
        let code = format!(r#"(on-player-move "{}" {} {} {})"#, id, dx, dz, yaw);
        let _ = engine.run(code);
    }

    pub fn on_command(&self, id: &str, cmd: &str) -> String {
        let mut engine = self.engine.lock().unwrap();
        let safe_cmd = cmd.replace('\\', "\\\\").replace('"', "\\\"");
        let code = format!(r#"(on-player-command "{}" "{}")"#, id, safe_cmd);
        match engine.run(code) {
            Ok(res) => {
                if let Some(first) = res.first() {
                    first.to_string().trim_matches('"').to_string()
                } else {
                    "OK".to_string()
                }
            }
            Err(e) => format!("Scheme Error: {:?}", e),
        }
    }

    pub fn get_client_state_json(&self, id: &str) -> String {
        let mut engine = self.engine.lock().unwrap();
        let code = format!(r#"(get-client-state-json "{}")"#, id);
        match engine.run(code) {
            Ok(res) => {
                if let Some(first) = res.first() {
                    first.to_string().trim_matches('"').to_string()
                } else {
                    "{}".to_string()
                }
            }
            Err(_) => "{}".to_string(),
        }
    }

    pub fn tick(&self, dt: f32) {
        let mut engine = self.engine.lock().unwrap();
        let code = format!(r#"(game-tick {})"#, dt);
        let _ = engine.run(code);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let script_path = args.get(1).map(|s| s.as_str()).unwrap_or("games/space_arena.scm");

    println!("====================================================");
    println!("      LispCraft Game Server (Rust + Steel Scheme)   ");
    println!("====================================================");
    println!("Loading Scheme game definition from: {}", script_path);

    let game = Arc::new(SchemeGame::new(script_path).expect("Failed to initialize Scheme game"));
    let next_id = Arc::new(AtomicUsize::new(1));

    // 启动物理与逻辑 Tick 线程
    let tick_game = game.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(50));
            tick_game.tick(0.05);
        }
    });

    // quad-net 服务端事件配置
    let game_for_msg = game.clone();
    let game_for_timer = game.clone();
    let game_for_disc = game.clone();
    let next_id_for_msg = next_id.clone();

    let settings = Settings {
        on_message: move |handle: &mut SocketHandle, session: &mut ClientSession, data: Vec<u8>| {
            if session.id.is_none() {
                let id = format!("player_{}", next_id_for_msg.fetch_add(1, Ordering::SeqCst));
                session.id = Some(id.clone());
                game_for_msg.on_join(&id);

                // 发送欢迎包
                let welcome = ServerMsg::Welcome { client_id: id };
                if let Ok(bytes) = serde_json::to_vec(&welcome) {
                    let _ = handle.send(&bytes);
                }
            }

            let client_id = session.id.as_ref().unwrap();

            if let Ok(msg) = serde_json::from_slice::<ClientMsg>(&data) {
                match msg {
                    ClientMsg::Move { dx, dz, yaw } => {
                        game_for_msg.on_move(client_id, dx, dz, yaw);
                    }
                    ClientMsg::Action { action_id: _ } => {}
                    ClientMsg::Command(cmd) => {
                        println!("[Server] Received command from {}: {}", client_id, cmd);
                        let result = game_for_msg.on_command(client_id, &cmd);
                        let echo = ServerMsg::CommandEcho {
                            success: true,
                            output: result,
                        };
                        if let Ok(bytes) = serde_json::to_vec(&echo) {
                            let _ = handle.send(&bytes);
                        }
                    }
                }
            }
        },
        on_timer: move |handle: &mut SocketHandle, session: &ClientSession| {
            if let Some(ref client_id) = session.id {
                let json_str = game_for_timer.get_client_state_json(client_id);
                if !json_str.is_empty() && json_str != "{}" {
                    let _ = handle.send(json_str.as_bytes());
                }
            }
        },
        on_disconnect: move |session: &ClientSession| {
            if let Some(ref client_id) = session.id {
                game_for_disc.on_leave(client_id);
            }
        },
        timer: Some(Duration::from_millis(40)), // ~25 TPS 状态同步
        _marker: std::marker::PhantomData,
    };

    let tcp_addr = "0.0.0.0:8088";
    let ws_addr = "0.0.0.0:8089";
    println!("[Server] Listening on TCP: {} (Native) & WebSocket: {} (WASM)", tcp_addr, ws_addr);
    listen(tcp_addr, ws_addr, settings);
}
