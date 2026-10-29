use game_script::{PluginContext, PluginError, RegisterFn, ScriptApp, ScriptError, ScriptPlugin};
use shared::ClientState;
use steel::rvals::SteelVal;
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

/// LispCraft plugin mounting native deterministic PRNG into Steel Scheme VM.
pub struct LispCraftPlugin;

impl ScriptPlugin for LispCraftPlugin {
    fn name(&self) -> &'static str {
        "LispCraftPlugin"
    }

    fn build(&self, ctx: &mut PluginContext) -> Result<(), PluginError> {
        ctx.engine().register_fn("random", prng);
        Ok(())
    }
}

/// Upgraded Scheme simulation engine powered by game_script::ScriptApp (v0.2.0)
pub struct SchemeGame {
    app: Mutex<ScriptApp>,
}

impl SchemeGame {
    /// Initialize SchemeGame with the specified .scm script file.
    pub fn new(script_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let resolved = resolve_script_path(script_path);
        let script = fs::read_to_string(&resolved)?;

        let mut app = ScriptApp::new();
        app.add_plugin(LispCraftPlugin)?;
        app.run(&script)?;

        // Ensure tick-game alias is bound for zero-overhead direct function calls
        let _ = app.run("(if (and (defined? 'game-tick) (not (defined? 'tick-game))) (define tick-game game-tick) (void))");

        // Initialize game world
        let _ = app.safe_call_function("init-game", vec![], 100)
            .or_else(|_| app.run("(init-game)").map(|v| v.into_iter().next().unwrap_or(SteelVal::Void)));

        println!(
            "[SchemeGame] Successfully loaded script '{}' and initialized world via game_script (v0.2.0).",
            resolved.display()
        );

        Ok(Self {
            app: Mutex::new(app),
        })
    }

    /// Player join lifecycle handler
    pub fn on_join(&self, id: &str) {
        let mut app = self.app.lock().unwrap();
        let _ = app.safe_call_function(
            "on-player-join",
            vec![SteelVal::StringV(id.into())],
            100,
        ).or_else(|_| {
            let code = format!(r#"(on-player-join "{}")"#, id);
            app.run(&code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
        println!("[SchemeGame] Player joined: {}", id);
    }

    /// Player leave lifecycle handler
    pub fn on_leave(&self, id: &str) {
        let mut app = self.app.lock().unwrap();
        let _ = app.safe_call_function(
            "on-player-leave",
            vec![SteelVal::StringV(id.into())],
            100,
        ).or_else(|_| {
            let code = format!(r#"(on-player-leave "{}")"#, id);
            app.run(&code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
        println!("[SchemeGame] Player left: {}", id);
    }

    /// Player movement handler
    pub fn on_move(&self, id: &str, dx: f32, dz: f32, yaw: f32) {
        let mut app = self.app.lock().unwrap();
        let _ = app.safe_call_function(
            "on-player-move",
            vec![
                SteelVal::StringV(id.into()),
                SteelVal::NumV(dx as f64),
                SteelVal::NumV(dz as f64),
                SteelVal::NumV(yaw as f64),
            ],
            100,
        ).or_else(|_| {
            let code = format!(r#"(on-player-move "{}" {} {} {})"#, id, dx, dz, yaw);
            app.run(&code).map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
        });
    }

    /// Execute player console Scheme command or high-level command with watchdog timeout protection
    pub fn on_command(&self, id: &str, cmd: &str) -> String {
        let mut app = self.app.lock().unwrap();

        // If the command is a raw Scheme S-expression, evaluate directly via safe_eval
        if cmd.trim().starts_with('(') {
            return match app.safe_eval(cmd, 100) {
                Ok(out) => out.trim().trim_start_matches("=> ").trim_matches('"').to_string(),
                Err(e) => format!("Scheme Error: {:?}", e),
            };
        }

        // Otherwise invoke on-player-command via precompiled call
        match app.safe_call_function(
            "on-player-command",
            vec![
                SteelVal::StringV(id.into()),
                SteelVal::StringV(cmd.into()),
            ],
            100,
        ) {
            Ok(SteelVal::StringV(s)) => s.as_str().to_string(),
            Ok(other) => other.to_string().trim_matches('"').to_string(),
            Err(_) => {
                // Fallback to safe_eval with string escape
                let safe_cmd = cmd.replace('\\', "\\\\").replace('"', "\\\"");
                let code = format!(r#"(on-player-command "{}" "{}")"#, id, safe_cmd);
                match app.safe_eval(&code, 100) {
                    Ok(res) => res.trim().trim_start_matches("=> ").trim_matches('"').to_string(),
                    Err(e) => format!("Scheme Error: {:?}", e),
                }
            }
        }
    }

    /// High-frequency simulation tick executing precompiled tick function
    pub fn tick(&self, dt: f32) {
        let mut app = self.app.lock().unwrap();
        let dt_val = SteelVal::NumV(dt as f64);
        let _ = app.safe_call_function("tick-game", vec![dt_val.clone()], 100)
            .or_else(|_| app.safe_call_function("game-tick", vec![dt_val], 100));
    }

    /// Retrieve spatial-culled ClientState JSON string for a specific player
    pub fn get_client_state_json(&self, id: &str) -> String {
        let mut app = self.app.lock().unwrap();
        match app.safe_call_function(
            "get-client-state-json",
            vec![SteelVal::StringV(id.into())],
            100,
        ) {
            Ok(SteelVal::StringV(s)) => s.as_str().to_string(),
            Ok(other) => other.to_string().trim_matches('"').to_string(),
            Err(_) => {
                let code = format!(r#"(get-client-state-json "{}")"#, id);
                match app.safe_eval(&code, 100) {
                    Ok(s) => s.trim_matches('"').to_string(),
                    Err(_) => "{}".to_string(),
                }
            }
        }
    }

    /// Retrieve strongly-typed ClientState for a player
    pub fn get_client_state(&self, id: &str) -> Option<ClientState> {
        let json_str = self.get_client_state_json(id);
        if json_str.is_empty() || json_str == "{}" {
            return None;
        }
        serde_json::from_str::<ClientState>(&json_str).ok()
    }

    /// Direct Scheme evaluation with 100ms watchdog protection
    pub fn safe_eval(&self, expr: &str, timeout_ms: u64) -> Result<String, ScriptError> {
        let mut app = self.app.lock().unwrap();
        app.safe_eval(expr, timeout_ms)
    }
}
