//! # LispCraftPlugin: Deterministic PRNG plugin for game_script
//!
//! Mounts native deterministic PRNG (`random`) into the Steel Scheme VM.

use game_script::{PluginContext, PluginError, RegisterFn, ScriptPlugin};
use std::sync::atomic::{AtomicU64, Ordering};

static SEED: AtomicU64 = AtomicU64::new(88172645463325252);

/// Deterministic pseudo-random number generator (xorshift64).
pub fn prng() -> f64 {
    let mut x = SEED.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, Ordering::Relaxed);
    (x as f64) / (u64::MAX as f64)
}

/// Sets the seed for the deterministic PRNG.
pub fn set_prng_seed(seed: u64) {
    SEED.store(seed, Ordering::Relaxed);
}

/// LispCraft plugin mounting native deterministic PRNG into Steel Scheme VM.
#[derive(Debug, Default, Clone, Copy)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use game_script::ScriptApp;

    #[test]
    fn test_prng_reproducibility() {
        set_prng_seed(123456789);
        let val1 = prng();
        let val2 = prng();
        assert_ne!(val1, val2);
        assert!((0.0..=1.0).contains(&val1));
        assert!((0.0..=1.0).contains(&val2));

        set_prng_seed(123456789);
        let val1_again = prng();
        let val2_again = prng();
        assert_eq!(val1, val1_again);
        assert_eq!(val2, val2_again);
    }

    #[test]
    fn test_plugin_registration() {
        let mut app = ScriptApp::new();
        app.add_plugin(LispCraftPlugin).unwrap();
        let res = app.run("(random)").unwrap();
        assert_eq!(res.len(), 1);
        if let game_script::SteelVal::NumV(n) = res[0] {
            assert!((0.0..=1.0).contains(&n));
        } else {
            panic!("Expected NumV for (random)");
        }
    }
}
