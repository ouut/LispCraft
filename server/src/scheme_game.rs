use steel::rvals::SteelVal;
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static SEED: AtomicU64 = AtomicU64::new(88172645463325252);

fn prng() -> f64 {
    let mut x = SEED.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, Ordering::Relaxed);
    (x as f64) / (u64::MAX as f64)
}

fn resolve_script_path(path: &str) -> PathBuf {
    let candidates = [
        PathBuf::from(path),
        PathBuf::from(format!("../{}", path)),
        PathBuf::from(format!("/workspace/LispCraft/{}", path)),
    ];
    for c in &candidates {
        if c.exists() && c.is_file() {
            return c.clone();
        }
    }
    PathBuf::from(path)
}

pub struct SchemeGame {
    engine: Mutex<Engine>,
}

impl SchemeGame {
    pub fn new(script_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let resolved = resolve_script_path(script_path);
        let script = fs::read_to_string(&resolved)?;

        let mut engine = Engine::new();
        engine.register_fn("random", prng);
        engine.run(script)?;

        // 初始化游戏世界
        let _ = engine.call_function_by_name_with_args("init-game", vec![])
            .or_else(|_| engine.run("(init-game)".to_string()).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void)));

        println!(
            "[SchemeGame] Successfully loaded script '{}' and initialized world.",
            resolved.display()
        );

        Ok(Self {
            engine: Mutex::new(engine),
        })
    }

    pub fn on_join(&self, id: &str) {
        let mut engine = self.engine.lock().unwrap();
        let _ = engine.call_function_by_name_with_args(
            "on-player-join",
            vec![SteelVal::StringV(id.into())],
        ).or_else(|_| {
            let code = format!(r#"(on-player-join "{}")"#, id);
            engine.run(code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
        println!("[SchemeGame] Player joined: {}", id);
    }

    pub fn on_leave(&self, id: &str) {
        let mut engine = self.engine.lock().unwrap();
        let _ = engine.call_function_by_name_with_args(
            "on-player-leave",
            vec![SteelVal::StringV(id.into())],
        ).or_else(|_| {
            let code = format!(r#"(on-player-leave "{}")"#, id);
            engine.run(code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
        println!("[SchemeGame] Player left: {}", id);
    }

    pub fn on_move(&self, id: &str, dx: f32, dz: f32, yaw: f32) {
        let mut engine = self.engine.lock().unwrap();
        let _ = engine.call_function_by_name_with_args(
            "on-player-move",
            vec![
                SteelVal::StringV(id.into()),
                SteelVal::NumV(dx as f64),
                SteelVal::NumV(dz as f64),
                SteelVal::NumV(yaw as f64),
            ],
        ).or_else(|_| {
            let code = format!(r#"(on-player-move "{}" {} {} {})"#, id, dx, dz, yaw);
            engine.run(code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
    }

    pub fn on_command(&self, id: &str, cmd: &str) -> String {
        let mut engine = self.engine.lock().unwrap();
        match engine.call_function_by_name_with_args(
            "on-player-command",
            vec![
                SteelVal::StringV(id.into()),
                SteelVal::StringV(cmd.into()),
            ],
        ) {
            Ok(SteelVal::StringV(s)) => s.as_str().to_string(),
            Ok(other) => other.to_string().trim_matches('"').to_string(),
            Err(_) => {
                // 回退到字符串转义评估
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
        }
    }

    pub fn get_client_state_json(&self, id: &str) -> String {
        let mut engine = self.engine.lock().unwrap();
        match engine.call_function_by_name_with_args(
            "get-client-state-json",
            vec![SteelVal::StringV(id.into())],
        ) {
            Ok(SteelVal::StringV(s)) => s.as_str().to_string(),
            Ok(other) => {
                let s = other.to_string();
                s.trim_matches('"').to_string()
            }
            Err(_) => {
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
        }
    }

    pub fn tick(&self, dt: f32) {
        let mut engine = self.engine.lock().unwrap();
        let _ = engine.call_function_by_name_with_args(
            "game-tick",
            vec![SteelVal::NumV(dt as f64)],
        ).or_else(|_| {
            let code = format!(r#"(game-tick {})"#, dt);
            engine.run(code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
    }
}
