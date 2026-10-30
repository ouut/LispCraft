//! # ClientSchemeRuntime: Client-side Scheme runtime engine
//!
//! Embeds Steel Scheme VM via `steel::steel_vm::engine::Engine`, providing:
//! - Full Macroquad 3D/2D GPU rendering bindings
//! - In-memory Virtual File System (VFS) modular loading and hot-reload
//! - Dual-way RemoteEvent queue and event callback registration
//! - Replicated state caching (`ClientState`) and camera synchronization

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use game_script::{json_to_steel_val, SteelVal};
use lc_core::ClientState;
use lc_render::OrbitCamera;
use steel::steel_vm::engine::Engine;

use crate::bindings::register_bindings;
use crate::vfs::{scan_directory, sort_vfs_entries};

/// Client-side Scheme scripting and execution runtime.
pub struct ClientSchemeRuntime {
    engine: Engine,
    vfs: HashMap<String, String>,
    state_cache: Arc<Mutex<ClientState>>,
    camera: Arc<Mutex<OrbitCamera>>,
    outgoing_events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    has_client_init: bool,
    has_client_render: bool,
}

impl Default for ClientSchemeRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to initialize default ClientSchemeRuntime")
    }
}

impl ClientSchemeRuntime {
    /// Creates a new `ClientSchemeRuntime` with an initialized Steel Scheme engine,
    /// empty VFS, default camera, and all native GPU drawing bindings registered.
    pub fn new() -> Result<Self, String> {
        let mut engine = Engine::new();

        let state_cache = Arc::new(Mutex::new(ClientState {
            client_id: "local".to_string(),
            camera_target: [0.0, 0.0, 0.0],
            camera_yaw: 0.0,
            hp: 100.0,
            max_hp: 100.0,
            score: 0,
            entities: Vec::new(),
            logs: Vec::new(),
            properties: HashMap::new(),
        }));

        let camera = Arc::new(Mutex::new(OrbitCamera::new()));
        let outgoing_events = Arc::new(Mutex::new(Vec::new()));

        register_bindings(
            &mut engine,
            Arc::clone(&state_cache),
            Arc::clone(&camera),
            Arc::clone(&outgoing_events),
        )?;

        Ok(Self {
            engine,
            vfs: HashMap::new(),
            state_cache,
            camera,
            outgoing_events,
            has_client_init: false,
            has_client_render: false,
        })
    }

    /// Loads and evaluates an in-memory Virtual File System (VFS).
    ///
    /// Files are loaded in strict dependency tier order:
    /// 1. `constants.scm` / `shared/*.scm`
    /// 2. Utility scripts (`hud.scm`, `radar.scm`, `avatar.scm`, etc.)
    /// 3. `main.scm` / `client/main.scm`
    pub fn load_vfs(&mut self, vfs: &HashMap<String, String>) -> Result<(), String> {
        let sorted = sort_vfs_entries(vfs);
        for (path, content) in sorted {
            self.vfs.insert(path.clone(), content.clone());
            self.engine.run(content.clone()).map_err(|e| {
                format!("Failed to execute VFS script '{}': {}", path, e)
            })?;
        }
        self.refresh_lifecycle_hooks();
        Ok(())
    }

    /// Reads all `.scm` files from a local directory into VFS and runs them.
    ///
    /// Ideal for standalone offline development mode and hot-reloading.
    pub fn load_directory(&mut self, dir: &Path) -> Result<(), String> {
        let scanned = scan_directory(dir)?;
        self.load_vfs(&scanned)
    }

    /// Lifecycle hook: calls Scheme `(client-init)` if defined.
    pub fn init(&mut self) -> Result<(), String> {
        if self.has_client_init {
            self.engine
                .call_function_by_name_with_args("client-init", vec![])
                .map_err(|e| format!("client-init failed: {}", e))?;
        }
        Ok(())
    }

    /// Lifecycle hook: calls Scheme `(client-render dt)` if defined.
    pub fn render(&mut self, dt: f32) -> Result<(), String> {
        if self.has_client_render {
            self.engine
                .call_function_by_name_with_args("client-render", vec![SteelVal::NumV(dt as f64)])
                .map_err(|e| format!("client-render failed: {}", e))?;
        }
        Ok(())
    }

    /// Dispatches an incoming server event to Scheme event handlers.
    ///
    /// Checks for callbacks registered via `(on-server-event event-name callback)`,
    /// and falls back to `(handle-server-event name payload)` if defined.
    pub fn handle_server_event(&mut self, name: &str, payload: &serde_json::Value) {
        let name_val = SteelVal::StringV(name.into());
        let payload_val = json_to_steel_val(payload);

        let handled = self
            .engine
            .call_function_by_name_with_args(
                "%dispatch-server-event",
                vec![name_val.clone(), payload_val.clone()],
            )
            .map(|v| v.is_truthy())
            .unwrap_or(false);

        if !handled && self.engine.global_exists("handle-server-event") {
            let _ = self.engine.call_function_by_name_with_args(
                "handle-server-event",
                vec![name_val, payload_val],
            );
        }
    }

    /// Sets the authoritative replicated state snapshot received from server.
    pub fn set_replicated_state(&mut self, state: ClientState) {
        if let Ok(mut c) = self.state_cache.lock() {
            *c = state;
        }
    }

    /// Returns a clone of the current replicated state snapshot.
    pub fn replicated_state(&self) -> ClientState {
        self.state_cache.lock().unwrap().clone()
    }

    /// Drains and returns all queued outgoing RemoteEvents emitted via `(fire-server!)`.
    pub fn drain_outgoing_events(&mut self) -> Vec<(String, serde_json::Value)> {
        if let Ok(mut q) = self.outgoing_events.lock() {
            q.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    /// Executes arbitrary Scheme source code in the runtime environment.
    pub fn eval(&mut self, expr: &str) -> Result<Vec<SteelVal>, String> {
        let res = self.engine.run(expr.to_string()).map_err(|e| e.to_string())?;
        self.refresh_lifecycle_hooks();
        Ok(res)
    }

    /// Evaluates a Scheme expression in REPL mode, returning formatted string output.
    pub fn eval_repl(&mut self, expr: &str) -> Result<String, String> {
        let vals = self.engine.run(expr.to_string()).map_err(|e| e.to_string())?;
        self.refresh_lifecycle_hooks();
        if vals.is_empty() {
            Ok("=> ()".to_string())
        } else {
            let out = vals
                .iter()
                .map(|v| format!("{}", v))
                .collect::<Vec<_>>()
                .join(" ");
            Ok(format!("=> {}", out))
        }
    }

    /// Accesses the underlying Steel Scheme engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Mutably accesses the underlying Steel Scheme engine.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Returns a reference to the loaded Virtual File System.
    pub fn vfs(&self) -> &HashMap<String, String> {
        &self.vfs
    }

    /// Clones the current `OrbitCamera` state.
    pub fn camera(&self) -> OrbitCamera {
        self.camera.lock().unwrap().clone()
    }

    /// Updates the camera with the given `OrbitCamera`.
    pub fn set_camera(&mut self, cam: OrbitCamera) {
        if let Ok(mut c) = self.camera.lock() {
            *c = cam;
        }
    }

    /// Returns the shared `Arc<Mutex<OrbitCamera>>` handle.
    pub fn camera_arc(&self) -> Arc<Mutex<OrbitCamera>> {
        Arc::clone(&self.camera)
    }

    /// Executes closure `f` with an immutable reference to the camera.
    pub fn with_camera<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&OrbitCamera) -> R,
    {
        let cam = self.camera.lock().unwrap();
        f(&cam)
    }

    /// Executes closure `f` with a mutable reference to the camera.
    pub fn with_camera_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut OrbitCamera) -> R,
    {
        let mut cam = self.camera.lock().unwrap();
        f(&mut cam)
    }

    /// Refreshes whether lifecycle functions (`client-init`, `client-render`) are defined.
    fn refresh_lifecycle_hooks(&mut self) {
        self.has_client_init = self.engine.global_exists("client-init");
        self.has_client_render = self.engine.global_exists("client-render");
    }
}
