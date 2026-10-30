//! # Roblox-style Client-Side Prediction & Soft Reconciliation
//!
//! Provides zero-latency client prediction for local player movement along with
//! soft reconciliation to smooth out minor network jitter without visual rubber-banding.

use lc_core::LispMotionInput;
use macroquad::math::{vec3, Vec3};

/// Manages client-side movement prediction and authoritative server reconciliation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClientPrediction {
    /// Current predicted local player position in 3D world space.
    pub predicted_pos: Vec3,
}

impl Default for ClientPrediction {
    fn default() -> Self {
        Self::new(Vec3::ZERO)
    }
}

impl ClientPrediction {
    /// Creates a new `ClientPrediction` initialized to the given starting position.
    pub fn new(initial_pos: Vec3) -> Self {
        Self {
            predicted_pos: initial_pos,
        }
    }

    /// Predicts local player movement from directional motion input.
    ///
    /// Applies velocity displacement along the X-Z horizontal plane and returns the updated position.
    pub fn predict_move(&mut self, input: &LispMotionInput, speed: f32, dt: f32) -> Vec3 {
        let delta = vec3(input.dx * speed * dt, 0.0, input.dz * speed * dt);
        self.predicted_pos += delta;
        self.predicted_pos
    }

    /// Reconciles local predicted position against authoritative server position.
    ///
    /// - If drift $|\vec{E}| < 5.0\text{m}$: soft blends `predicted_pos += drift * 0.2` to eliminate jitter.
    /// - If drift $|\vec{E}| \ge 5.0\text{m}$: hard snaps `predicted_pos = server_pos` (e.g. teleport, knockback, collision).
    pub fn reconcile(&mut self, server_pos: Vec3) {
        let drift = server_pos - self.predicted_pos;
        let dist = drift.length();
        if dist < 5.0 {
            self.predicted_pos += drift * 0.2;
        } else {
            self.predicted_pos = server_pos;
        }
    }

    /// Returns the current predicted position.
    pub fn pos(&self) -> Vec3 {
        self.predicted_pos
    }

    /// Resets the predicted position directly to the given coordinates.
    pub fn reset(&mut self, pos: Vec3) {
        self.predicted_pos = pos;
    }
}
