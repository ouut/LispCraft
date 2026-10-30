//! # NDC Screen Projection Math & Viewport Metrics
//!
//! Provides `world_to_screen`, `world_to_screen_with_size`, and `screen_size`
//! to transform 3D world coordinates into 2D viewport coordinates and query screen metrics.

use macroquad::camera::{Camera3D, Projection};
use macroquad::math::{vec2, vec4, Mat4, Vec2, Vec3};
use macroquad::window::{screen_height, screen_width};

/// Safely queries the current window size `(width, height)` in pixels.
///
/// In headless or non-window environments (e.g. unit tests), gracefully falls back to `(800.0, 600.0)`.
pub fn screen_size() -> (f32, f32) {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (screen_width(), screen_height())
    }));
    std::panic::set_hook(prev_hook);

    let (sw, sh) = res.unwrap_or((800.0, 600.0));
    if sw <= 1.0 || sh <= 1.0 {
        (800.0, 600.0)
    } else {
        (sw, sh)
    }
}

/// Computes the view-projection matrix for a `Camera3D` given a specific viewport size.
pub fn compute_camera_matrix(camera: &Camera3D, screen_w: f32, screen_h: f32) -> Mat4 {
    let aspect = camera.aspect.unwrap_or(screen_w / screen_h.max(1.0));
    match camera.projection {
        Projection::Perspective => {
            Mat4::perspective_rh_gl(camera.fovy, aspect, camera.z_near, camera.z_far)
                * Mat4::look_at_rh(camera.position, camera.target, camera.up)
        }
        Projection::Orthographics => {
            let top = camera.fovy / 2.0;
            let right = top * aspect;
            Mat4::orthographic_rh_gl(-right, right, -top, top, camera.z_near, camera.z_far)
                * Mat4::look_at_rh(camera.position, camera.target, camera.up)
        }
    }
}

/// Projects a 3D world coordinate to 2D screen coordinates with explicit screen dimensions.
///
/// Returns `None` if the point is behind the camera or outside the viewing frustum.
pub fn world_to_screen_with_size(
    camera: &Camera3D,
    point: Vec3,
    screen_w: f32,
    screen_h: f32,
) -> Option<Vec2> {
    let mat = compute_camera_matrix(camera, screen_w, screen_h);
    let p = mat * vec4(point.x, point.y, point.z, 1.0);
    if p.w <= 0.05 {
        return None;
    }
    let ndc_x = p.x / p.w;
    let ndc_y = p.y / p.w;
    let ndc_z = p.z / p.w;

    // Clip behind near plane and past far plane
    if ndc_z < -1.0 || ndc_z > 1.0 {
        return None;
    }

    let sx = (ndc_x + 1.0) * 0.5 * screen_w;
    let sy = (1.0 - ndc_y) * 0.5 * screen_h;

    // Screen edge tolerance
    if sx < -100.0 || sx > screen_w + 100.0 || sy < -100.0 || sy > screen_h + 100.0 {
        return None;
    }

    Some(vec2(sx, sy))
}

/// Projects a 3D world coordinate to 2D screen coordinates using current window dimensions.
///
/// In headless or non-window environments (e.g. unit tests), gracefully falls back to 800x600.
pub fn world_to_screen(camera: &Camera3D, point: Vec3) -> Option<Vec2> {
    let (sw, sh) = screen_size();
    world_to_screen_with_size(camera, point, sw, sh)
}
