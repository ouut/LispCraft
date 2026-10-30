//! # 3D Drawing Primitives
//!
//! Provides pure, decoupled GPU 3D rendering atoms for Macroquad.

use macroquad::camera::Camera3D;
use macroquad::color::Color;
use macroquad::math::{Vec2, Vec3};
use macroquad::models::DrawCylinderParams;
use macroquad::prelude::DrawMode;

use crate::mock::{dispatch_render, RenderCommand};

/// Draws a solid 3D cube centered at `pos` with the given `size` and `color`.
pub fn draw_cube(pos: Vec3, size: Vec3, color: Color) {
    dispatch_render(
        RenderCommand::DrawCube { pos, size, color },
        || macroquad::models::draw_cube(pos, size, None, color),
    );
}

/// Draws a wireframe 3D cube centered at `pos` with the given `size` and `color`.
pub fn draw_cube_wires(pos: Vec3, size: Vec3, color: Color) {
    dispatch_render(
        RenderCommand::DrawCubeWires { pos, size, color },
        || macroquad::models::draw_cube_wires(pos, size, color),
    );
}

/// Draws a solid 3D sphere centered at `pos` with the given `radius` and `color`.
pub fn draw_sphere(pos: Vec3, radius: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawSphere { pos, radius, color },
        || macroquad::models::draw_sphere(pos, radius, None, color),
    );
}

/// Draws a wireframe 3D sphere centered at `pos` with the given `radius` and `color`.
pub fn draw_sphere_wires(pos: Vec3, radius: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawSphereWires { pos, radius, color },
        || macroquad::models::draw_sphere_wires(pos, radius, None, color),
    );
}

/// Draws a 3D cylinder or cone centered at `pos` with top/bottom radii, height, number of sides, and `color`.
pub fn draw_cylinder(pos: Vec3, r_top: f32, r_bot: f32, height: f32, sides: u8, color: Color) {
    dispatch_render(
        RenderCommand::DrawCylinder {
            pos,
            r_top,
            r_bot,
            height,
            sides,
            color,
        },
        || {
            let params = DrawCylinderParams {
                sides: sides.max(3) as usize,
                draw_mode: DrawMode::Triangles,
            };
            macroquad::models::draw_cylinder_ex(pos, r_top, r_bot, height, None, color, params);
        },
    );
}

/// Draws a horizontal 3D plane centered at `pos` with 2D `size` (width, depth) and `color`.
pub fn draw_plane(pos: Vec3, size: Vec2, color: Color) {
    dispatch_render(
        RenderCommand::DrawPlane { pos, size, color },
        || macroquad::models::draw_plane(pos, size, None, color),
    );
}

/// Draws a 3D line segment from `start` to `end` with `color`.
pub fn draw_line_3d(start: Vec3, end: Vec3, color: Color) {
    dispatch_render(
        RenderCommand::DrawLine3D { start, end, color },
        || macroquad::models::draw_line_3d(start, end, color),
    );
}

/// Draws 2D text billboarded at 3D world coordinate `pos` facing the `camera`.
pub fn draw_billboard_text(
    text: &str,
    pos: Vec3,
    font_size: f32,
    color: Color,
    camera: &Camera3D,
) {
    let text_owned = text.to_string();
    dispatch_render(
        RenderCommand::DrawBillboardText {
            text: text_owned.clone(),
            pos,
            font_size,
            color,
        },
        || {
            if let Some(screen_pos) = crate::projection::world_to_screen(camera, pos) {
                let text_w = crate::primitives_2d::measure_text_width(&text_owned, font_size);
                macroquad::camera::set_default_camera();
                crate::primitives_2d::draw_text_screen(
                    &text_owned,
                    screen_pos.x - text_w * 0.5,
                    screen_pos.y,
                    font_size,
                    color,
                );
                macroquad::camera::set_camera(camera);
            }
        },
    );
}
