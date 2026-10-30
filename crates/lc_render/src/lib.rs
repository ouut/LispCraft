//! # `lc_render`: Whiteboard GPU Atomic Rendering Library
//!
//! Provides decoupled, pure GPU drawing atoms, 3D orbit camera controls,
//! NDC screen projection, and client math utilities for Macroquad and Scheme script execution.
//!
//! ## Submodules
//! - [`camera`]: Spherical 3D orbit camera with smooth interpolation.
//! - [`primitives_3d`]: Low-level 3D GPU drawing primitives (cube, sphere, cylinder, plane, 3D line, billboard).
//! - [`primitives_2d`]: Low-level 2D GPU drawing primitives (text, rectangle, circle, 2D line, measurement).
//! - [`projection`]: Screen size query and 3D world-to-screen viewport projection.
//! - [`mock`]: Thread-local mock hook and command capture for headless testing.
//! - [`interpolation`]: Snapshot interpolation buffer for 60~144 FPS smooth rendering.
//! - [`prediction`]: Client-side dead-reckoning movement prediction.

pub mod camera;
pub mod interpolation;
pub mod mock;
pub mod prediction;
pub mod primitives_2d;
pub mod primitives_3d;
pub mod projection;

// Primary re-exports
pub use camera::OrbitCamera;
pub use interpolation::{SnapshotEntry, SnapshotInterpolationBuffer};
pub use mock::{
    capture_render_commands, clear_render_hook, enable_headless_fallback, is_headless_fallback,
    set_render_hook, RenderCommand,
};
pub use prediction::ClientPrediction;
pub use primitives_2d::{
    draw_circle, draw_circle_lines, draw_line_2d, draw_rect, draw_rect_lines, draw_text_screen,
    measure_text_width,
};
pub use primitives_3d::{
    draw_billboard_text, draw_cube, draw_cube_wires, draw_cylinder, draw_line_3d, draw_plane,
    draw_sphere, draw_sphere_wires,
};
pub use projection::{
    compute_camera_matrix, screen_size, world_to_screen, world_to_screen_with_size,
};

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::{ClientState, EntityDesc, LispMotionInput};
    use macroquad::camera::{Camera3D, Projection};
    use macroquad::color::{Color, RED, WHITE};
    use macroquad::math::{vec2, vec3, Vec3};

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
    fn test_orbit_camera_set_and_get_params() {
        let mut cam = OrbitCamera::new();
        let target = vec3(10.0, 2.5, -7.0);
        cam.set_params(target, 1.2, 0.6, 15.0);

        let (got_target, yaw, pitch, dist) = cam.get_params();
        assert_eq!(got_target, target);
        assert!((yaw - 1.2).abs() < 1e-5);
        assert!((pitch - 0.6).abs() < 1e-5);
        assert!((dist - 15.0).abs() < 1e-5);

        let cam3d = cam.to_camera3d();
        assert_eq!(cam3d.target, target);
        assert!((cam3d.position - cam.cam_pos).length() < 1e-5);
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

    #[test]
    fn test_orbit_camera_clamping() {
        let mut cam = OrbitCamera::new();
        // Pitch clamping
        cam.handle_mouse_orbit(0.0, 1000.0);
        assert!((cam.target_camera_pitch - 1.35).abs() < 1e-4);
        cam.handle_mouse_orbit(0.0, -2000.0);
        assert!((cam.target_camera_pitch - 0.12).abs() < 1e-4);

        // Zoom clamping
        cam.handle_zoom(100.0);
        assert!((cam.target_camera_dist - 4.0).abs() < 1e-4);
        cam.handle_zoom(-100.0);
        assert!((cam.target_camera_dist - 26.0).abs() < 1e-4);
    }

    // --- Projection Tests ---
    #[test]
    fn test_screen_size_fallback() {
        let (w, h) = screen_size();
        assert!(w > 0.0);
        assert!(h > 0.0);
    }

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

        // In headless testing, world_to_screen should gracefully use the screen_size fallback without panicking
        let screen = world_to_screen(&camera, vec3(0.0, 0.0, 0.0));
        assert!(screen.is_some());
    }

    // --- 3D and 2D Primitives Drawing Tests (Mock Hook) ---
    #[test]
    fn test_mock_capture_3d_primitives() {
        let camera = OrbitCamera::new().to_camera3d();

        let ((), cmds) = capture_render_commands(|| {
            draw_cube(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), RED);
            draw_cube_wires(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), WHITE);
            draw_sphere(vec3(0.0, 1.0, 0.0), 2.5, Color::new(0.0, 1.0, 0.0, 1.0));
            draw_sphere_wires(vec3(0.0, 1.0, 0.0), 2.5, WHITE);
            draw_cylinder(vec3(0.0, 0.0, 0.0), 1.0, 2.0, 3.0, 8, RED);
            draw_plane(vec3(0.0, 0.0, 0.0), vec2(10.0, 10.0), WHITE);
            draw_line_3d(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0), RED);
            draw_billboard_text("Test Label", vec3(0.0, 2.0, 0.0), 14.0, WHITE, &camera);
        });

        assert_eq!(cmds.len(), 8);
        assert_eq!(
            cmds[0],
            RenderCommand::DrawCube {
                pos: vec3(1.0, 2.0, 3.0),
                size: vec3(4.0, 5.0, 6.0),
                color: RED,
            }
        );
        assert_eq!(
            cmds[1],
            RenderCommand::DrawCubeWires {
                pos: vec3(1.0, 2.0, 3.0),
                size: vec3(4.0, 5.0, 6.0),
                color: WHITE,
            }
        );
        assert_eq!(
            cmds[2],
            RenderCommand::DrawSphere {
                pos: vec3(0.0, 1.0, 0.0),
                radius: 2.5,
                color: Color::new(0.0, 1.0, 0.0, 1.0),
            }
        );
        assert_eq!(
            cmds[3],
            RenderCommand::DrawSphereWires {
                pos: vec3(0.0, 1.0, 0.0),
                radius: 2.5,
                color: WHITE,
            }
        );
        assert_eq!(
            cmds[4],
            RenderCommand::DrawCylinder {
                pos: vec3(0.0, 0.0, 0.0),
                r_top: 1.0,
                r_bot: 2.0,
                height: 3.0,
                sides: 8,
                color: RED,
            }
        );
        assert_eq!(
            cmds[5],
            RenderCommand::DrawPlane {
                pos: vec3(0.0, 0.0, 0.0),
                size: vec2(10.0, 10.0),
                color: WHITE,
            }
        );
        assert_eq!(
            cmds[6],
            RenderCommand::DrawLine3D {
                start: vec3(0.0, 0.0, 0.0),
                end: vec3(1.0, 1.0, 1.0),
                color: RED,
            }
        );
        assert_eq!(
            cmds[7],
            RenderCommand::DrawBillboardText {
                text: "Test Label".to_string(),
                pos: vec3(0.0, 2.0, 0.0),
                font_size: 14.0,
                color: WHITE,
            }
        );
    }

    #[test]
    fn test_mock_capture_2d_primitives() {
        let ((), cmds) = capture_render_commands(|| {
            draw_text_screen("Hello Scheme", 10.0, 20.0, 16.0, WHITE);
            draw_rect(50.0, 60.0, 100.0, 80.0, RED);
            draw_rect_lines(50.0, 60.0, 100.0, 80.0, 2.0, WHITE);
            draw_circle(200.0, 200.0, 25.0, Color::new(0.0, 0.0, 1.0, 1.0));
            draw_circle_lines(200.0, 200.0, 25.0, 1.5, WHITE);
            draw_line_2d(0.0, 0.0, 100.0, 100.0, 3.0, RED);
        });

        assert_eq!(cmds.len(), 6);
        assert_eq!(
            cmds[0],
            RenderCommand::DrawTextScreen {
                text: "Hello Scheme".to_string(),
                x: 10.0,
                y: 20.0,
                font_size: 16.0,
                color: WHITE,
            }
        );
        assert_eq!(
            cmds[1],
            RenderCommand::DrawRect {
                x: 50.0,
                y: 60.0,
                w: 100.0,
                h: 80.0,
                color: RED,
            }
        );
        assert_eq!(
            cmds[2],
            RenderCommand::DrawRectLines {
                x: 50.0,
                y: 60.0,
                w: 100.0,
                h: 80.0,
                thickness: 2.0,
                color: WHITE,
            }
        );
        assert_eq!(
            cmds[3],
            RenderCommand::DrawCircle {
                x: 200.0,
                y: 200.0,
                radius: 25.0,
                color: Color::new(0.0, 0.0, 1.0, 1.0),
            }
        );
        assert_eq!(
            cmds[4],
            RenderCommand::DrawCircleLines {
                x: 200.0,
                y: 200.0,
                radius: 25.0,
                thickness: 1.5,
                color: WHITE,
            }
        );
        assert_eq!(
            cmds[5],
            RenderCommand::DrawLine2D {
                x1: 0.0,
                y1: 0.0,
                x2: 100.0,
                y2: 100.0,
                thickness: 3.0,
                color: RED,
            }
        );

        let w = measure_text_width("Hello", 16.0);
        assert!(w > 0.0);
    }

    #[test]
    fn test_headless_safe_drawing_without_panic() {
        // Direct drawing calls in headless test environment must not panic
        draw_cube(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0), RED);
        draw_rect(0.0, 0.0, 10.0, 10.0, WHITE);
        let cam = Camera3D::default();
        draw_billboard_text("Safe", vec3(0.0, 0.0, 0.0), 12.0, WHITE, &cam);
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
    fn test_client_prediction_moving_tolerance() {
        let mut pred = ClientPrediction::new(vec3(10.0, 0.0, 0.0));
        // Small drift 0.2m when actively moving (dist < 0.35m tolerance) -> ignored without jitter
        pred.reconcile_moving(vec3(9.8, 0.0, 0.0), true);
        assert_eq!(pred.predicted_pos, vec3(10.0, 0.0, 0.0));

        // Larger drift 1.0m when actively moving (dist >= 0.35m) -> smoothly reconciled
        pred.reconcile_moving(vec3(9.0, 0.0, 0.0), true);
        assert!((pred.predicted_pos.x - 9.8).abs() < 1e-4);
    }

    #[test]
    fn test_client_prediction_hard_snap_teleport() {
        let mut pred = ClientPrediction::new(vec3(10.0, 0.0, 0.0));
        // Server teleports to 50.0 (drift = 40.0 >= 5.0 -> hard snap)
        pred.reconcile(vec3(50.0, 0.0, 0.0));
        assert_eq!(pred.predicted_pos, vec3(50.0, 0.0, 0.0));
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
            properties: Default::default(),
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
}
