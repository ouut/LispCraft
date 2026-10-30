//! # lc_game: Scheme game world driver & LispCraftPlugin
//!
//! Layer 1 game logic driver for LispCraft 3D, integrating the Steel Scheme VM
//! through `game_script` (v0.2.0) and managing game state, player lifecycle,
//! ticks, commands, spatial culling, multi-game management, and remote events.

pub mod manager;
pub mod plugin;
pub mod world;

pub use manager::{bundle_directory, resolve_games_dir, GameManager};
pub use plugin::{prng, set_prng_seed, LispCraftPlugin};
pub use world::{resolve_script_path, SchemeGame};

// Re-export lc_core domain contracts for convenience
pub use lc_core::*;
