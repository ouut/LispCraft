//! # `lc_render`: 3D Graphics, Camera, Projection, Radar HUD & Interpolation
//!
//! High-performance decoupled rendering and math crate for LispCraft 3D.
//! Completely removes graphics and vector math complexity from the client main loop:
//! - [`OrbitCamera`]: Third-person spherical camera with exponential dampening.
//! - [`world_to_screen`]: NDC screen projection math with frustum clipping.
//! - [`RadarRenderer`]: Circular 2D minimap with player heading, cardinal directions, and entity blips.
//! - [`SceneRenderer`]: 3D ground grid, player mech avatar, entity primitives (cube/sphere/pyramid/beacon), and floating billboards.
//! - [`SnapshotInterpolationBuffer`]: Roblox-style 60~144 FPS smooth entity interpolation from 20 TPS snapshots.
//! - [`ClientPrediction`]: Roblox-style client-side movement prediction with soft error reconciliation.

pub mod camera;
pub mod interpolation;
pub mod prediction;
pub mod projection;
pub mod radar;
pub mod scene;

pub use camera::OrbitCamera;
pub use interpolation::{SnapshotEntry, SnapshotInterpolationBuffer};
pub use prediction::ClientPrediction;
pub use projection::{compute_camera_matrix, world_to_screen, world_to_screen_with_size};
pub use radar::RadarRenderer;
pub use scene::SceneRenderer;

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::{ClientState, EntityDesc, LispMotionInput};
    use macroquad::camera::{Camera3D, Projection};
    use macroquad::math::{vec3, Vec3};

    // --- Camera Tests ---
    #[test]
    fn test_orbit_camera_initialization() {
        let cam = OrbitCamera::new();
        assert_eq!(cam.target_pos, Vec3::ZERO);
        assert_eq!(cam.camera_yaw, 0.0);
        assert_eq!(cam.camera_pitch, 0.45);
        assert_eq!(cam.camera_dist, 11.0);
        assert!(cam.cam_pos.y > 0.0);
    }

    #[test]
    fn test_orbit_camera_update_and_zoom() {
        let mut cam = OrbitCamera::new();
        cam.handle_zoom(2.0); // Zoom in
        assert!(cam.target_camera_dist < 11.0);

        cam.handle_mouse_orbit(100.0, 50.0);
        assert!(cam.target_camera_yaw > 0.0);
        assert!(cam.target_camera_pitch > 0.45);

        // Update with target
        let target = vec3(5.0, 0.0, -5.0);
        cam.update(0.1, target);
        assert!(cam.target_pos.length() > 0.0);

        let cam3d = cam.to_camera3d();
        assert_eq!(cam3d.target, cam.target_pos);
        assert_eq!(cam3d.position, cam.cam_pos);
    }

    #[test]
    fn test_camera_directional_vectors() {
        let mut cam = OrbitCamera::new();
        cam.camera_yaw = 0.0;
        let fwd = cam.forward();
        let right = cam.right();
        // Yaw = 0: forward is (0, -1), right is (1, 0)
        assert!((fwd.x).abs() < 1e-4);
        assert!((fwd.y - (-1.0)).abs() < 1e-4);
        assert!((right.x - 1.0).abs() < 1e-4);
        assert!((right.y).abs() < 1e-4);
    }

    // --- Projection Tests ---
    #[test]
    fn test_projection_center() {
        let camera = Camera3D {
            position: vec3(0.0, 0.0, 10.0),
            target: vec3(0.0, 0.0, 0.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: 45.0_f32.to_radians(),
            aspect: Some(800.0 / 600.0),
            projection: Projection::Perspective,
            render_target: None,
            viewport: None,
            z_near: 0.1,
            z_far: 1000.0,
        };

        // Origin (0,0,0) in front of camera should project precisely to viewport center (400, 300)
        let screen = world_to_screen_with_size(&camera, vec3(0.0, 0.0, 0.0), 800.0, 600.0);
        assert!(screen.is_some());
        let pt = screen.unwrap();
        assert!((pt.x - 400.0).abs() < 1.0);
        assert!((pt.y - 300.0).abs() < 1.0);
    }

    #[test]
    fn test_projection_clipping_behind_camera() {
        let camera = Camera3D {
            position: vec3(0.0, 0.0, 10.0),
            target: vec3(0.0, 0.0, 0.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: 45.0_f32.to_radians(),
            aspect: Some(800.0 / 600.0),
            projection: Projection::Perspective,
            render_target: None,
            viewport: None,
            z_near: 0.1,
            z_far: 1000.0,
        };

        // Point behind camera (z = 20.0, camera at z = 10 looking at z = 0)
        let behind = world_to_screen_with_size(&camera, vec3(0.0, 0.0, 20.0), 800.0, 600.0);
        assert!(behind.is_none());
    }

    #[test]
    fn test_world_to_screen_fallback() {
        let camera = Camera3D {
            position: vec3(0.0, 5.0, 10.0),
            target: vec3(0.0, 0.0, 0.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: 45.0_f32.to_radians(),
            aspect: None,
            projection: Projection::Perspective,
            render_target: None,
            viewport: None,
            z_near: 0.1,
            z_far: 1000.0,
        };

        // In headless testing, world_to_screen should gracefully use the 800x600 fallback without panicking
        let screen = world_to_screen(&camera, vec3(0.0, 0.0, 0.0));
        assert!(screen.is_some());
    }

    // --- Prediction Tests ---
    #[test]
    fn test_client_prediction_movement() {
        let mut pred = ClientPrediction::new(Vec3::ZERO);
        let input = LispMotionInput {
            dx: 1.0,
            dz: 0.0,
            yaw: 0.0,
        };
        // Move at speed 10.0 for dt = 0.5s -> delta = 5.0 in X
        let pos = pred.predict_move(&input, 10.0, 0.5);
        assert!((pos.x - 5.0).abs() < 1e-4);
        assert!((pos.y).abs() < 1e-4);
        assert!((pos.z).abs() < 1e-4);
    }

    #[test]
    fn test_client_prediction_soft_reconciliation() {
        let mut pred = ClientPrediction::new(vec3(10.0, 0.0, 0.0));
        // Server says 12.0 (drift = 2.0 < 5.0 -> soft blend by 20%)
        pred.reconcile(vec3(12.0, 0.0, 0.0));
        // New pos: 10.0 + (12.0 - 10.0) * 0.2 = 10.4
        assert!((pred.predicted_pos.x - 10.4).abs() < 1e-4);
    }

    #[test]
    fn test_client_prediction_hard_snap_teleport() {
        let mut pred = ClientPrediction::new(vec3(10.0, 0.0, 0.0));
        // Server teleports to 50.0 (drift = 40.0 >= 5.0 -> hard snap)
        pred.reconcile(vec3(50.0, 0.0, 0.0));
        assert_eq!(pred.predicted_pos, vec3(50.0, 0.0, 0.0));
    }

    // --- Interpolation Tests ---
    fn make_test_state(id: &str, pos: [f32; 3], cam_target: [f32; 3]) -> ClientState {
        ClientState {
            client_id: "test_player".to_string(),
            camera_target: cam_target,
            camera_yaw: 0.0,
            hp: 100.0,
            max_hp: 100.0,
            score: 0,
            entities: vec![EntityDesc {
                id: id.to_string(),
                shape: "cube".to_string(),
                pos,
                size: [1.0, 1.0, 1.0],
                color: [1.0, 0.0, 0.0, 1.0],
                label: "Target".to_string(),
            }],
            logs: vec![],
        }
    }

    #[test]
    fn test_snapshot_interpolation_lerp() {
        let mut buffer = SnapshotInterpolationBuffer::new();

        let s0 = make_test_state("bot_1", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        let s1 = make_test_state("bot_1", [10.0, 20.0, 30.0], [10.0, 20.0, 30.0]);

        buffer.push(1.0, s0);
        buffer.push(2.0, s1);

        // Sample at 1.5s (exact midpoint, alpha = 0.5)
        let sampled = buffer.sample(1.5);
        assert!(sampled.is_some());
        let entities = sampled.unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].id, "bot_1");
        assert!((entities[0].pos[0] - 5.0).abs() < 1e-4);
        assert!((entities[0].pos[1] - 10.0).abs() < 1e-4);
        assert!((entities[0].pos[2] - 15.0).abs() < 1e-4);

        // Also test camera target interpolation
        let cam_target = buffer.sample_camera_target(1.5).unwrap();
        assert!((cam_target[0] - 5.0).abs() < 1e-4);
        assert!((cam_target[1] - 10.0).abs() < 1e-4);
        assert!((cam_target[2] - 15.0).abs() < 1e-4);
    }

    #[test]
    fn test_snapshot_interpolation_clamping() {
        let mut buffer = SnapshotInterpolationBuffer::new();
        let s0 = make_test_state("bot_1", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        let s1 = make_test_state("bot_1", [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        buffer.push(1.0, s0);
        buffer.push(2.0, s1);

        // Sample before oldest snapshot -> clamps to oldest
        let early = buffer.sample(0.5).unwrap();
        assert!((early[0].pos[0] - 0.0).abs() < 1e-4);

        // Sample after newest snapshot -> clamps to newest
        let late = buffer.sample(3.0).unwrap();
        assert!((late[0].pos[0] - 10.0).abs() < 1e-4);
    }

    #[test]
    fn test_orbit_camera_clamping() {
        let mut cam = OrbitCamera::new();
        // Test pitch clamping
        cam.handle_mouse_orbit(0.0, 1000.0);
        assert!((cam.target_camera_pitch - 1.35).abs() < 1e-4);
        cam.handle_mouse_orbit(0.0, -2000.0);
        assert!((cam.target_camera_pitch - 0.12).abs() < 1e-4);

        // Test zoom clamping
        cam.handle_zoom(100.0);
        assert!((cam.target_camera_dist - 4.0).abs() < 1e-4);
        cam.handle_zoom(-100.0);
        assert!((cam.target_camera_dist - 26.0).abs() < 1e-4);
    }

    #[test]
    fn test_snapshot_interpolation_out_of_order_and_pruning() {
        let mut buffer = SnapshotInterpolationBuffer::with_history_seconds(2.5);
        let s0 = make_test_state("bot_1", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        let s1 = make_test_state("bot_1", [10.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        let s2 = make_test_state("bot_1", [20.0, 0.0, 0.0], [20.0, 0.0, 0.0]);

        // Push out of order: 2.0 then 1.0 then 3.0
        buffer.push(2.0, s1);
        buffer.push(1.0, s0);
        buffer.push(3.0, s2);

        // Sample at 1.5s should interpolate between s0 (1.0) and s1 (2.0)
        let sampled = buffer.sample(1.5).unwrap();
        assert!((sampled[0].pos[0] - 5.0).abs() < 1e-4);

        // Push s3 at 10.0s -> older snapshots beyond 2.5s window should be pruned
        let s3 = make_test_state("bot_1", [40.0, 0.0, 0.0], [40.0, 0.0, 0.0]);
        buffer.push(10.0, s3);
        assert!(buffer.len() <= 3);
    }

    #[test]
    fn test_client_prediction_reset_and_diagonal() {
        let mut pred = ClientPrediction::new(Vec3::ZERO);
        let input = LispMotionInput {
            dx: 0.70710677,
            dz: 0.70710677,
            yaw: 0.0,
        };
        pred.predict_move(&input, 10.0, 1.0);
        assert!((pred.pos().x - 7.071_068).abs() < 1e-4);
        assert!((pred.pos().z - 7.071_068).abs() < 1e-4);

        pred.reset(vec3(100.0, 20.0, 30.0));
        assert_eq!(pred.pos(), vec3(100.0, 20.0, 30.0));
    }

    #[test]
    fn test_radar_renderer_construction() {
        let radar = RadarRenderer::new();
        assert_eq!(radar.radar_range, 35.0);

        let custom = RadarRenderer::with_range(50.0);
        assert_eq!(custom.radar_range, 50.0);
    }
}
