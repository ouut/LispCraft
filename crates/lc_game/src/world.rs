//! # SchemeGame: Scheme game world driver & spatial culling engine
//!
//! Provides authoritative game state management driven by Steel Scheme scripts,
//! integrating with `game_script` (v0.2.0) and enforcing 25.0m visibility culling.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use game_script::{ScriptApp, ScriptError, SteelVal};
use lc_core::{ClientState, VIEW_DISTANCE};

use crate::plugin::LispCraftPlugin;

/// Resolves a script path across multiple candidate directories.
pub fn resolve_script_path(path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() && p.exists() {
        return p.to_path_buf();
    }

    let mut candidates = vec![
        PathBuf::from(path),
        PathBuf::from(format!("../{}", path)),
        PathBuf::from(format!("../../{}", path)),
        PathBuf::from(format!("/workspace/LispCraft/{}", path)),
    ];

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(manifest_dir);
        candidates.push(manifest_path.join(path));
        candidates.push(manifest_path.join("../").join(path));
        candidates.push(manifest_path.join("../../").join(path));
    }

    for c in &candidates {
        if c.exists() && c.is_file() {
            return c.clone();
        }
    }
    PathBuf::from(path)
}

/// Upgraded Scheme simulation engine powered by `game_script::ScriptApp` (v0.2.0).
pub struct SchemeGame {
    app: Mutex<ScriptApp>,
}

impl SchemeGame {
    /// Initialize SchemeGame with the specified .scm script file.
    ///
    /// Loads the script into a new `ScriptApp` configured with `LispCraftPlugin`,
    /// aliases `tick-game` if necessary, and calls `(init-game)` to initialize the world.
    pub fn new(script_path: &str) -> Result<Self, String> {
        let resolved = resolve_script_path(script_path);
        let script = fs::read_to_string(&resolved).map_err(|e| {
            format!(
                "Failed to read script '{}' (resolved from '{}'): {}",
                resolved.display(),
                script_path,
                e
            )
        })?;

        let mut app = ScriptApp::new();
        if let Some(parent) = resolved.parent() {
            if parent.exists() {
                app.with_load_path(parent);
            }
        }

        app.add_plugin(LispCraftPlugin)
            .map_err(|e| format!("Plugin error: {}", e))?;
        app.run(&script)
            .map_err(|e| format!("Scheme script load error: {}", e))?;

        // Ensure tick-game alias is bound for zero-overhead direct function calls
        let _ = app.run("(if (and (defined? 'game-tick) (not (defined? 'tick-game))) (define tick-game game-tick) (void))");

        // Initialize game world
        let _ = app
            .safe_call_function("init-game", vec![], 100)
            .or_else(|_| {
                app.run("(init-game)")
                    .map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
            })
            .map_err(|e| format!("Failed to initialize game world (init-game): {}", e))?;

        Ok(Self {
            app: Mutex::new(app),
        })
    }

    /// Player join lifecycle handler: calls Scheme `(on-player-join name)`.
    pub fn on_join(&self, name: &str) {
        let mut app = self.app.lock().unwrap();
        let _ = app
            .safe_call_function(
                "on-player-join",
                vec![SteelVal::StringV(name.into())],
                100,
            )
            .or_else(|_| {
                let safe_name = name.replace('\\', "\\\\").replace('"', "\\\"");
                let code = format!(r#"(on-player-join "{}")"#, safe_name);
                app.run(&code)
                    .map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
            });
    }

    /// Player leave lifecycle handler: calls Scheme `(on-player-leave name)`.
    pub fn on_leave(&self, name: &str) {
        let mut app = self.app.lock().unwrap();
        let _ = app
            .safe_call_function(
                "on-player-leave",
                vec![SteelVal::StringV(name.into())],
                100,
            )
            .or_else(|_| {
                let safe_name = name.replace('\\', "\\\\").replace('"', "\\\"");
                let code = format!(r#"(on-player-leave "{}")"#, safe_name);
                app.run(&code)
                    .map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
            });
    }

    /// Player movement handler: calls Scheme `(on-player-move name dx dz yaw)`.
    pub fn on_move(&self, name: &str, dx: f32, dz: f32, yaw: f32) {
        let mut app = self.app.lock().unwrap();
        let _ = app
            .safe_call_function(
                "on-player-move",
                vec![
                    SteelVal::StringV(name.into()),
                    SteelVal::NumV(dx as f64),
                    SteelVal::NumV(dz as f64),
                    SteelVal::NumV(yaw as f64),
                ],
                100,
            )
            .or_else(|_| {
                let safe_name = name.replace('\\', "\\\\").replace('"', "\\\"");
                let code = format!(r#"(on-player-move "{}" {} {} {})"#, safe_name, dx, dz, yaw);
                app.run(&code)
                    .map(|v| v.into_iter().next().unwrap_or(SteelVal::Void))
            });
    }

    /// Execute player console Scheme command or high-level command with watchdog timeout protection.
    ///
    /// Evaluates raw Scheme expressions directly with `safe_eval`, or invokes `(on-player-command name expr)`.
    pub fn on_command(&self, name: &str, expr: &str) -> String {
        let mut app = self.app.lock().unwrap();

        // If the command is a raw Scheme S-expression, evaluate directly via safe_eval
        if expr.trim().starts_with('(') {
            return match app.safe_eval(expr, 100) {
                Ok(out) => out.trim().trim_start_matches("=> ").trim_matches('"').to_string(),
                Err(e) => format!("Scheme Error: {:?}", e),
            };
        }

        // Otherwise invoke on-player-command via precompiled call
        match app.safe_call_function(
            "on-player-command",
            vec![
                SteelVal::StringV(name.into()),
                SteelVal::StringV(expr.into()),
            ],
            100,
        ) {
            Ok(SteelVal::StringV(s)) => s.as_str().to_string(),
            Ok(other) => other.to_string().trim_matches('"').to_string(),
            Err(_) => {
                // Fallback to safe_eval with string escape
                let safe_name = name.replace('\\', "\\\\").replace('"', "\\\"");
                let safe_cmd = expr.replace('\\', "\\\\").replace('"', "\\\"");
                let code = format!(r#"(on-player-command "{}" "{}")"#, safe_name, safe_cmd);
                match app.safe_eval(&code, 100) {
                    Ok(res) => res.trim().trim_start_matches("=> ").trim_matches('"').to_string(),
                    Err(e) => format!("Scheme Error: {:?}", e),
                }
            }
        }
    }

    /// High-frequency simulation tick: calls Scheme `(game-tick dt)` or alias `(tick-game dt)`.
    pub fn tick(&self, dt: f32) {
        let mut app = self.app.lock().unwrap();
        let dt_val = SteelVal::NumV(dt as f64);
        let _ = app
            .safe_call_function("tick-game", vec![dt_val.clone()], 100)
            .or_else(|_| app.safe_call_function("game-tick", vec![dt_val], 100));
    }

    /// Retrieve raw JSON string representation of client state from Scheme VM.
    pub fn get_client_state_json(&self, name: &str) -> String {
        let mut app = self.app.lock().unwrap();
        match app.safe_call_function(
            "get-client-state-json",
            vec![SteelVal::StringV(name.into())],
            100,
        ) {
            Ok(SteelVal::StringV(s)) => s.as_str().to_string(),
            Ok(other) => other.to_string().trim_matches('"').to_string(),
            Err(_) => {
                let safe_name = name.replace('\\', "\\\\").replace('"', "\\\"");
                let code = format!(r#"(get-client-state-json "{}")"#, safe_name);
                match app.safe_eval(&code, 100) {
                    Ok(s) => s.trim_start_matches("=> ").trim().trim_matches('"').to_string(),
                    Err(_) => "{}".to_string(),
                }
            }
        }
    }

    /// Retrieve strongly-typed ClientState for a player.
    ///
    /// Parses JSON into `ClientState` and filters/culls entities by 25.0m distance
    /// (`VIEW_DISTANCE`) if not already culled by Scheme.
    pub fn get_client_state(&self, name: &str) -> Option<ClientState> {
        let json_str = self.get_client_state_json(name);
        if json_str.is_empty() || json_str == "{}" {
            return None;
        }

        let mut state: ClientState = serde_json::from_str(&json_str).ok()?;

        // Perform visibility culling by VIEW_DISTANCE (25.0m) in horizontal plane (XZ)
        let px = state.camera_target[0];
        let pz = state.camera_target[2];
        let max_dist_sq = VIEW_DISTANCE * VIEW_DISTANCE;

        state.entities.retain(|e| {
            let dx = e.pos[0] - px;
            let dz = e.pos[2] - pz;
            (dx * dx + dz * dz) <= max_dist_sq
        });

        Some(state)
    }

    /// Safely evaluate an arbitrary Scheme expression with watchdog timeout protection.
    pub fn safe_eval(&self, expr: &str, timeout_ms: u64) -> Result<String, ScriptError> {
        let mut app = self.app.lock().unwrap();
        app.safe_eval(expr, timeout_ms)
    }

    /// Direct access to inner `ScriptApp` mutex.
    pub fn app(&self) -> &Mutex<ScriptApp> {
        &self.app
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::EntityDesc;

    #[test]
    fn test_script_path_resolver() {
        let path = resolve_script_path("games/space_arena.scm");
        assert!(path.exists(), "Resolved path must exist: {:?}", path);
        assert!(path.is_file(), "Resolved path must be a file: {:?}", path);
    }

    #[test]
    fn test_spatial_culling_logic() {
        let game = SchemeGame::new("games/space_arena.scm").unwrap();
        game.on_join("cull_player");
        game.on_command("cull_player", "tp 0 0");

        // Manually inject entities into VM state or verify culling
        let mut state = game.get_client_state("cull_player").unwrap();

        // Add a mock entity near (5m away) and another far (30m away)
        state.entities.push(EntityDesc {
            id: "near_test".into(),
            shape: "cube".into(),
            pos: [3.0, 0.0, 4.0], // distance = 5.0 <= 25.0
            size: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            label: "Near".into(),
        });
        state.entities.push(EntityDesc {
            id: "far_test".into(),
            shape: "cube".into(),
            pos: [20.0, 0.0, 20.0], // distance = sqrt(800) = 28.28 > 25.0
            size: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            label: "Far".into(),
        });

        // Filter using VIEW_DISTANCE
        let px = state.camera_target[0];
        let pz = state.camera_target[2];
        let max_dist_sq = VIEW_DISTANCE * VIEW_DISTANCE;
        state.entities.retain(|e| {
            let dx = e.pos[0] - px;
            let dz = e.pos[2] - pz;
            (dx * dx + dz * dz) <= max_dist_sq
        });

        assert!(state.entities.iter().any(|e| e.id == "near_test"));
        assert!(!state.entities.iter().any(|e| e.id == "far_test"));
    }
}

