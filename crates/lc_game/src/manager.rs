//! # GameManager: Multi-game dynamic mounting, client bundling, and switching
//!
//! Scans a games directory, bundles client/shared scripts into an in-memory VFS,
//! and mounts SchemeGame instances mapped by game ID.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lc_core::LispEvent;

use crate::world::SchemeGame;

/// Resolves candidate directory paths for the games root directory.
pub fn resolve_games_dir(dir: &str) -> PathBuf {
    let p = Path::new(dir);
    if p.is_absolute() && p.exists() && p.is_dir() {
        return p.to_path_buf();
    }

    let clean = dir.trim_end_matches('/');

    let mut candidates = vec![
        PathBuf::from(dir),
        PathBuf::from(clean),
        PathBuf::from(format!("../{}", clean)),
        PathBuf::from(format!("../../{}", clean)),
        PathBuf::from(format!("/workspace/LispCraft/{}", clean)),
    ];

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(manifest_dir);
        candidates.push(manifest_path.join(clean));
        candidates.push(manifest_path.join("../").join(clean));
        candidates.push(manifest_path.join("../../").join(clean));
    }

    for c in &candidates {
        if c.exists() && c.is_dir() {
            return c.clone();
        }
    }
    PathBuf::from(dir)
}

/// Recursively scans a directory for `.scm` files and records them in the VFS.
fn scan_and_bundle_files(current_dir: &Path, game_root: &Path, vfs: &mut HashMap<String, String>) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            scan_and_bundle_files(&p, game_root, vfs);
        } else if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("scm") {
            if let Ok(content) = fs::read_to_string(&p) {
                if let Ok(rel_path) = p.strip_prefix(game_root) {
                    let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                    vfs.insert(rel_str, content.clone());
                }
                if let Some(file_name) = p.file_name().and_then(|f| f.to_str()) {
                    vfs.entry(file_name.to_string()).or_insert(content);
                }
            }
        }
    }
}

/// Bundles all `.scm` files under `client/` and `shared/` of a game directory into an in-memory VFS.
pub fn bundle_directory(game_dir: &Path) -> HashMap<String, String> {
    let mut vfs = HashMap::new();
    let dirs_to_scan = [game_dir.join("client"), game_dir.join("shared")];
    for base_dir in dirs_to_scan {
        if base_dir.exists() && base_dir.is_dir() {
            scan_and_bundle_files(&base_dir, game_dir, &mut vfs);
        }
    }
    vfs
}

/// Multi-Game Manager providing dynamic mounting, script bundling, and lifecycle dispatch.
pub struct GameManager {
    games: HashMap<String, Arc<SchemeGame>>,
    bundles: HashMap<String, HashMap<String, String>>,
    game_ids: Vec<String>,
    default_game: String,
}

impl std::fmt::Debug for GameManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameManager")
            .field("game_ids", &self.game_ids)
            .field("default_game", &self.default_game)
            .finish()
    }
}

impl GameManager {
    /// Scans a directory (with fallback candidate path resolution) and mounts discovered games.
    pub fn new(games_dir: &str) -> Result<Self, String> {
        let resolved_dir = resolve_games_dir(games_dir);
        if !resolved_dir.exists() || !resolved_dir.is_dir() {
            return Err(format!(
                "Games directory does not exist or is not a directory: {}",
                resolved_dir.display()
            ));
        }

        let mut games = HashMap::new();
        let mut bundles = HashMap::new();
        let mut game_ids = Vec::new();

        let mut entries: Vec<_> = fs::read_dir(&resolved_dir)
            .map_err(|e| {
                format!(
                    "Failed to read games directory '{}': {}",
                    resolved_dir.display(),
                    e
                )
            })?
            .filter_map(|res| res.ok())
            .collect();

        entries.sort_by_key(|e| e.file_name());

        // 1. Process game subdirectories
        for entry in &entries {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.starts_with('.') {
                    continue;
                }
                let game_id = dir_name;

                // Bundle client and shared scripts into in-memory VFS
                let vfs = bundle_directory(&path);

                // Locate server entrypoint script
                let candidates = [
                    path.join("server/main.scm"),
                    path.join("server").join(format!("{}.scm", game_id)),
                    path.join(format!("{}.scm", game_id)),
                    path.join("main.scm"),
                    resolved_dir.join(format!("{}.scm", game_id)),
                ];

                let server_script = candidates.iter().find(|c| c.exists() && c.is_file());

                if let Some(script_path) = server_script {
                    let game = SchemeGame::new(&script_path.to_string_lossy())?;
                    games.insert(game_id.clone(), Arc::new(game));
                    bundles.insert(game_id.clone(), vfs);
                    game_ids.push(game_id);
                }
            }
        }

        // 2. Process standalone root `.scm` files (e.g. games/space_arena.scm)
        for entry in &entries {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("scm") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let game_id = stem.to_string();
                    if !games.contains_key(&game_id) {
                        let game = SchemeGame::new(&path.to_string_lossy())?;
                        games.insert(game_id.clone(), Arc::new(game));
                        bundles.insert(game_id.clone(), HashMap::new());
                        game_ids.push(game_id);
                    }
                }
            }
        }

        if games.is_empty() {
            return Err(format!(
                "No valid games found in '{}'",
                resolved_dir.display()
            ));
        }

        game_ids.sort();
        game_ids.dedup();

        let default_game = if games.contains_key("space_arena") {
            "space_arena".to_string()
        } else if let Some(first) = game_ids.first() {
            first.clone()
        } else {
            "space_arena".to_string()
        };

        Ok(Self {
            games,
            bundles,
            game_ids,
            default_game,
        })
    }

    /// Retrieve SchemeGame instance for a specific game ID.
    pub fn get_game(&self, game_id: &str) -> Option<Arc<SchemeGame>> {
        self.games.get(game_id).cloned()
    }

    /// Retrieve the in-memory VFS script bundle for a game ID.
    pub fn get_bundle(&self, game_id: &str) -> Option<&HashMap<String, String>> {
        self.bundles.get(game_id)
    }

    /// Retrieve `LispEvent::BundleScript` for client synchronization.
    pub fn get_bundle_event(&self, game_id: &str) -> Option<LispEvent> {
        self.get_bundle(game_id).map(|vfs| LispEvent::BundleScript {
            game_id: game_id.to_string(),
            vfs: vfs.clone(),
        })
    }

    /// Default game ID ("space_arena", or first discovered).
    pub fn default_game_id(&self) -> String {
        self.default_game.clone()
    }

    /// List all discovered game IDs.
    pub fn all_game_ids(&self) -> Vec<String> {
        self.game_ids.clone()
    }

    /// Ticks all mounted game simulations by `dt`.
    pub fn tick_all(&self, dt: f32) {
        for game in self.games.values() {
            game.tick(dt);
        }
    }

    /// Check if a game ID is mounted.
    pub fn has_game(&self, game_id: &str) -> bool {
        self.games.contains_key(game_id)
    }

    /// Total count of mounted games.
    pub fn game_count(&self) -> usize {
        self.games.len()
    }

    /// Manually mount a game instance and its bundle (useful for testing or dynamic loading).
    pub fn mount_game(
        &mut self,
        game_id: &str,
        game: Arc<SchemeGame>,
        bundle: HashMap<String, String>,
    ) {
        if !self.games.contains_key(game_id) {
            self.game_ids.push(game_id.to_string());
            self.game_ids.sort();
        }
        self.games.insert(game_id.to_string(), game);
        self.bundles.insert(game_id.to_string(), bundle);
    }
}
