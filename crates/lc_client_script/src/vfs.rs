//! # Virtual File System (VFS) loader & file ordering
//!
//! Provides deterministic dependency sorting for `.scm` files:
//! 1. `constants.scm` / `shared/*.scm` first
//! 2. Utility & client sub-modules (`hud.scm`, `radar.scm`, `avatar.scm`, etc.)
//! 3. `main.scm` / `client/main.scm` last

use std::collections::HashMap;
use std::path::Path;

/// Determines execution sorting priority for a VFS file path.
/// Lower integer means earlier evaluation.
pub fn vfs_sort_tier(path: &str) -> u8 {
    let lower = path.to_lowercase().replace('\\', "/");
    let filename = lower.rsplit('/').next().unwrap_or(&lower);

    if filename == "constants.scm" || lower.ends_with("/constants.scm") {
        0
    } else if lower.starts_with("shared/") || lower.contains("/shared/") {
        1
    } else if filename == "main.scm" || lower.ends_with("/main.scm") {
        3
    } else {
        2
    }
}

/// Sorts VFS entries in deterministic dependency order.
pub fn sort_vfs_entries<'a>(vfs: &'a HashMap<String, String>) -> Vec<(&'a String, &'a String)> {
    let mut entries: Vec<(&'a String, &'a String)> = vfs.iter().collect();
    entries.sort_by(|a, b| {
        let tier_a = vfs_sort_tier(a.0);
        let tier_b = vfs_sort_tier(b.0);
        tier_a.cmp(&tier_b).then_with(|| a.0.cmp(b.0))
    });
    entries
}

/// Recursively scans a local filesystem directory for all `.scm` files and collects them into a VFS map.
pub fn scan_directory(dir: &Path) -> Result<HashMap<String, String>, String> {
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }
    let mut vfs = HashMap::new();
    scan_dir_recursive(dir, dir, &mut vfs)?;

    // If dir is a `client/` subdirectory, also scan sibling `shared/` if present
    if let Some(parent) = dir.parent() {
        let shared_dir = parent.join("shared");
        if shared_dir.is_dir() && !dir.ends_with("shared") {
            let _ = scan_dir_recursive(&shared_dir, parent, &mut vfs);
        }
    }

    Ok(vfs)
}

fn scan_dir_recursive(
    current: &Path,
    base: &Path,
    vfs: &mut HashMap<String, String>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read directory {}: {}", current.display(), e))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive(&path, base, vfs)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("scm") {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
            vfs.insert(rel, content);
        }
    }
    Ok(())
}
