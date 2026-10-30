//! # 3D Orbit Camera
//!
//! Provides `OrbitCamera` for smooth third-person spherical orbit around a target point,
//! with exponential smoothing, mouse drag, and scroll-wheel zoom support.

use macroquad::camera::{Camera3D, Projection};
use macroquad::math::{vec3, Vec2, Vec3};

/// Third-person orbit camera with exponential smoothing.
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
        let yaw = 0.0_f32;
        let pitch = 0.45_f32;
        let dist = 11.0_f32;
        let target = Vec3::ZERO;

        let horiz_dist = dist * pitch.cos();
        let vert_dist = dist * pitch.sin();
        let cam_offset = vec3(
            yaw.sin() * horiz_dist,
            vert_dist,
            yaw.cos() * horiz_dist,
        );

        Self {
            target_pos: target,
            cam_pos: target + cam_offset,
            camera_yaw: yaw,
            target_camera_yaw: yaw,
            camera_pitch: pitch,
            target_camera_pitch: pitch,
            camera_dist: dist,
            target_camera_dist: dist,
        }
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

        let horiz_dist = self.camera_dist * self.camera_pitch.cos();
        let vert_dist = self.camera_dist * self.camera_pitch.sin();
        let cam_offset = vec3(
            self.camera_yaw.sin() * horiz_dist,
            vert_dist,
            self.camera_yaw.cos() * horiz_dist,
        );
        self.cam_pos = self.target_pos + cam_offset;
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
}
