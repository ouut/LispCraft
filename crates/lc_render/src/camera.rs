//! # 3D Orbit Camera
//!
//! Provides `OrbitCamera` for smooth third-person spherical orbit around a target point,
//! with exponential smoothing, mouse drag, and scroll-wheel zoom support.

use macroquad::camera::{Camera3D, Projection};
use macroquad::math::{vec3, Vec2, Vec3};

/// Third-person orbit camera with smooth exponential interpolation.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitCamera {
    pub target_pos: Vec3,
    pub cam_pos: Vec3,
    pub camera_yaw: f32,
    pub target_camera_yaw: f32,
    pub camera_pitch: f32,
    pub target_camera_pitch: f32,
    pub camera_dist: f32,
    pub target_camera_dist: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new()
    }
}

impl OrbitCamera {
    /// Creates a new `OrbitCamera` at the origin with default orientation.
    pub fn new() -> Self {
        let mut cam = Self {
            target_pos: Vec3::ZERO,
            cam_pos: Vec3::ZERO,
            camera_yaw: 0.0,
            target_camera_yaw: 0.0,
            camera_pitch: 0.45,
            target_camera_pitch: 0.45,
            camera_dist: 11.0,
            target_camera_dist: 11.0,
        };
        cam.recompute_cam_pos();
        cam
    }

    /// Sets camera target position, yaw, pitch, and distance immediately.
    pub fn set_params(&mut self, target: Vec3, yaw: f32, pitch: f32, dist: f32) {
        self.target_pos = target;
        self.camera_yaw = yaw;
        self.target_camera_yaw = yaw;
        self.camera_pitch = pitch;
        self.target_camera_pitch = pitch;
        self.camera_dist = dist;
        self.target_camera_dist = dist;
        self.recompute_cam_pos();
    }

    /// Sets target parameters for smooth interpolation without snapping.
    pub fn set_target_params(&mut self, target: Vec3, yaw: f32, pitch: f32, dist: f32) {
        self.target_pos = target;
        self.target_camera_yaw = yaw;
        self.target_camera_pitch = pitch;
        self.target_camera_dist = dist;
    }

    /// Returns current camera target position, yaw, pitch, and distance.
    pub fn get_params(&self) -> (Vec3, f32, f32, f32) {
        (self.target_pos, self.camera_yaw, self.camera_pitch, self.camera_dist)
    }

    /// Smoothly updates camera position and orientation towards target point using exponential smoothing.
    pub fn update(&mut self, dt: f32, target: Vec3) {
        let smooth_yaw = 1.0 - (-16.0 * dt).exp();
        let smooth_pitch = 1.0 - (-16.0 * dt).exp();
        let smooth_dist = 1.0 - (-14.0 * dt).exp();
        let smooth_pos = 1.0 - (-12.0 * dt).exp();

        self.target_pos = self.target_pos.lerp(target, smooth_pos);
        self.camera_yaw += (self.target_camera_yaw - self.camera_yaw) * smooth_yaw;
        self.camera_pitch += (self.target_camera_pitch - self.camera_pitch) * smooth_pitch;
        self.camera_dist += (self.target_camera_dist - self.camera_dist) * smooth_dist;

        self.recompute_cam_pos();
    }

    /// Handles mouse drag rotation input.
    pub fn handle_mouse_orbit(&mut self, delta_x: f32, delta_y: f32) {
        self.target_camera_yaw += delta_x * 0.006;
        self.target_camera_pitch = (self.target_camera_pitch + delta_y * 0.005).clamp(0.12, 1.35);
    }

    /// Handles mouse wheel zoom input.
    pub fn handle_zoom(&mut self, wheel_delta: f32) {
        self.target_camera_dist = (self.target_camera_dist - wheel_delta * 0.8).clamp(4.0, 26.0);
    }

    /// Converts current orbit state to a Macroquad `Camera3D`.
    pub fn to_camera3d(&self) -> Camera3D {
        Camera3D {
            position: self.cam_pos,
            target: self.target_pos,
            up: vec3(0.0, 1.0, 0.0),
            fovy: 45.0_f32.to_radians(),
            aspect: None,
            projection: Projection::Perspective,
            render_target: None,
            viewport: None,
            z_near: 0.1,
            z_far: 1000.0,
        }
    }

    /// Returns the 2D forward vector in the horizontal X-Z plane.
    pub fn forward(&self) -> Vec2 {
        Vec2::new(-self.camera_yaw.sin(), -self.camera_yaw.cos())
    }

    /// Returns the 2D right vector in the horizontal X-Z plane.
    pub fn right(&self) -> Vec2 {
        Vec2::new(self.camera_yaw.cos(), -self.camera_yaw.sin())
    }

    /// Internal helper to recompute 3D position from spherical coordinates.
    fn recompute_cam_pos(&mut self) {
        let horiz_dist = self.camera_dist * self.camera_pitch.cos();
        let vert_dist = self.camera_dist * self.camera_pitch.sin();
        let cam_offset = vec3(
            self.camera_yaw.sin() * horiz_dist,
            vert_dist,
            self.camera_yaw.cos() * horiz_dist,
        );
        self.cam_pos = self.target_pos + cam_offset;
    }
}
