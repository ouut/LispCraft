//! # 3D Scene & Entity Renderer
//!
//! Provides `SceneRenderer` to render the 3D ground grid, player avatar, entities
//! (`"cube"`, `"sphere"`, `"pyramid"`, `"beacon"`), and projected 2D billboard labels.

use lc_core::EntityDesc;
use macroquad::camera::{set_camera, set_default_camera, Camera3D};
use macroquad::color::{Color, WHITE};
use macroquad::math::{vec3, Vec2, Vec3};
use macroquad::models::{
    draw_cube, draw_cube_wires, draw_cylinder, draw_cylinder_ex, draw_cylinder_wires,
    draw_grid, draw_sphere, draw_sphere_wires, DrawCylinderParams,
};
use macroquad::prelude::DrawMode;
use macroquad::shapes::{draw_rectangle, draw_rectangle_lines};
use macroquad::text::{draw_text, measure_text};
use macroquad::time::get_time;
use macroquad::window::clear_background;

use crate::projection::world_to_screen;

fn safe_get_time() -> f32 {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(get_time));
    std::panic::set_hook(prev_hook);
    res.unwrap_or(0.0) as f32
}

fn safe_measure_text(text: &str, font_size: u16) -> (f32, f32) {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let dim = measure_text(text, None, font_size, 1.0);
        (dim.width, dim.height)
    }));
    std::panic::set_hook(prev_hook);
    res.unwrap_or((text.len() as f32 * (font_size as f32 * 0.55), font_size as f32))
}

/// Billboard projection tag for 2D overlay rendering.
#[derive(Debug, Clone)]
struct BillboardLabel {
    pos: Vec2,
    text: String,
    color: Color,
}

/// Renderer responsible for drawing the 3D game world and HUD billboards.
#[derive(Debug, Default, Clone)]
pub struct SceneRenderer;

impl SceneRenderer {
    pub fn new() -> Self {
        Self
    }

    /// Renders the complete 3D scene and 2D billboard overlay.
    pub fn draw_scene(&self, camera: &Camera3D, entities: &[EntityDesc], player_pos: Vec3) {
        let time = safe_get_time();

        // 1. Clear background (deep space navy)
        clear_background(Color::new(0.05, 0.07, 0.11, 1.0));

        // 2. Set 3D perspective camera
        set_camera(camera);

        // 3. Ground Grid
        draw_grid(
            60,
            1.0,
            Color::new(0.12, 0.22, 0.38, 0.6),
            Color::new(0.05, 0.08, 0.16, 0.35),
        );
        draw_grid(
            12,
            5.0,
            Color::new(0.0, 0.6, 0.9, 0.75),
            Color::new(0.08, 0.15, 0.28, 0.5),
        );

        // 4. Render 3D Entities
        let mut billboard_labels: Vec<BillboardLabel> = Vec::new();

        for ent in entities {
            let pos = vec3(ent.pos[0], ent.pos[1], ent.pos[2]);
            let size = vec3(ent.size[0], ent.size[1], ent.size[2]);
            let col = Color::new(ent.color[0], ent.color[1], ent.color[2], ent.color[3].max(0.1));
            let wire_col = Color::new(
                (ent.color[0] + 0.35).min(1.0),
                (ent.color[1] + 0.35).min(1.0),
                (ent.color[2] + 0.35).min(1.0),
                0.85,
            );

            let top_y = match ent.shape.as_str() {
                "sphere" => {
                    let radius = size.x * 0.5;
                    draw_sphere(pos, radius, None, col);
                    draw_sphere_wires(pos, radius, None, wire_col);
                    pos.y + radius
                }
                "pyramid" => {
                    let radius = (size.x.max(size.z)) * 0.5;
                    let height = size.y.max(0.1);
                    let params = DrawCylinderParams {
                        sides: 4,
                        draw_mode: DrawMode::Triangles,
                    };
                    draw_cylinder_ex(pos, 0.0, radius, height, None, col, params);
                    let wire_params = DrawCylinderParams {
                        sides: 4,
                        draw_mode: DrawMode::Lines,
                    };
                    draw_cylinder_ex(pos, 0.0, radius, height, None, wire_col, wire_params);
                    pos.y + height * 0.5
                }
                "beacon" => {
                    // Base platform
                    draw_cube(
                        pos + vec3(0.0, 0.1, 0.0),
                        vec3(size.x, 0.2, size.z),
                        None,
                        Color::new(0.08, 0.14, 0.25, 0.95),
                    );
                    draw_cube_wires(
                        pos + vec3(0.0, 0.1, 0.0),
                        vec3(size.x, 0.2, size.z),
                        wire_col,
                    );

                    // Central glowing core
                    let core_bob = (time * 2.2).sin() * 0.15;
                    let core_pos = pos + vec3(0.0, size.y * 0.5 + core_bob, 0.0);
                    draw_cube(
                        core_pos,
                        vec3(size.x * 0.45, size.y * 0.6, size.z * 0.45),
                        None,
                        col,
                    );
                    draw_cube_wires(
                        core_pos,
                        vec3(size.x * 0.55, size.y * 0.7, size.z * 0.55),
                        wire_col,
                    );

                    // Sky beacon beam
                    let beam_pulse = (time * 3.5).sin().abs();
                    draw_cylinder(
                        pos + vec3(0.0, 20.0, 0.0),
                        0.25,
                        0.4,
                        40.0,
                        None,
                        Color::new(col.r, col.g, col.b, 0.25 + 0.15 * beam_pulse),
                    );

                    pos.y + size.y * 0.5 + 0.5
                }
                _ => {
                    // Default: Cube
                    draw_cube(pos, size, None, col);
                    draw_cube_wires(pos, size, wire_col);
                    pos.y + size.y * 0.5
                }
            };

            // Ground shadow projection
            draw_cylinder_wires(
                vec3(pos.x, 0.02, pos.z),
                size.x * 0.55,
                size.z * 0.55,
                0.02,
                None,
                Color::new(0.0, 0.0, 0.0, 0.45),
            );

            // Collect 3D floating billboard label
            if !ent.label.is_empty() {
                let anchor_3d = vec3(pos.x, top_y + 0.55, pos.z);
                if let Some(screen_p) = world_to_screen(camera, anchor_3d) {
                    billboard_labels.push(BillboardLabel {
                        pos: screen_p,
                        text: ent.label.clone(),
                        color: col,
                    });
                }
            }
        }

        // 5. Render Local Player Avatar at player_pos
        self.draw_player_avatar(player_pos, time);

        // 6. Switch to Orthographic 2D Overlay for Billboard Labels
        set_default_camera();
        for label in &billboard_labels {
            let (text_w, _text_h) = safe_measure_text(&label.text, 14);

            let tag_w = text_w + 16.0;
            let tag_h = 20.0;
            let tag_x = label.pos.x - tag_w * 0.5;
            let tag_y = label.pos.y - tag_h;

            draw_rectangle(
                tag_x,
                tag_y,
                tag_w,
                tag_h,
                Color::new(0.03, 0.06, 0.12, 0.82),
            );
            draw_rectangle_lines(
                tag_x,
                tag_y,
                tag_w,
                tag_h,
                1.0,
                Color::new(
                    (label.color.r * 0.8 + 0.2).min(1.0),
                    (label.color.g * 0.8 + 0.2).min(1.0),
                    (label.color.b * 0.8 + 0.2).min(1.0),
                    0.85,
                ),
            );
            draw_text(&label.text, tag_x + 8.0, tag_y + 14.0, 14.0, WHITE);
        }
    }

    /// Renders the local player avatar mech model.
    pub fn draw_player_avatar(&self, av_pos: Vec3, time: f32) {
        // Main armor torso
        let torso_pos = av_pos + vec3(0.0, 0.85, 0.0);
        draw_cube(
            torso_pos,
            vec3(0.85, 0.95, 0.65),
            None,
            Color::new(0.18, 0.55, 0.92, 1.0),
        );
        draw_cube_wires(
            torso_pos,
            vec3(0.85, 0.95, 0.65),
            Color::new(0.5, 0.85, 1.0, 0.9),
        );

        // Glowing chest reactor core
        let core_pulse = (time * 6.0).sin() * 0.03;
        draw_sphere(
            torso_pos + vec3(0.0, 0.0, -0.34),
            0.16 + core_pulse,
            None,
            Color::new(0.0, 1.0, 0.85, 1.0),
        );

        // Cockpit helmet and visor
        let head_pos = av_pos + vec3(0.0, 1.5, 0.0);
        draw_cube(
            head_pos,
            vec3(0.5, 0.38, 0.5),
            None,
            Color::new(0.12, 0.18, 0.28, 1.0),
        );
        draw_cube_wires(
            head_pos,
            vec3(0.5, 0.38, 0.5),
            Color::new(0.35, 0.65, 0.9, 0.85),
        );
        draw_cube(
            head_pos + vec3(0.0, 0.0, -0.26),
            vec3(0.35, 0.12, 0.04),
            None,
            Color::new(0.0, 1.0, 0.6, 1.0),
        );

        // Tactical side wings
        let wing_l = av_pos + vec3(-0.62, 0.95, 0.0);
        let wing_r = av_pos + vec3(0.62, 0.95, 0.0);
        draw_cube(
            wing_l,
            vec3(0.28, 0.55, 0.45),
            None,
            Color::new(0.14, 0.32, 0.58, 1.0),
        );
        draw_cube_wires(
            wing_l,
            vec3(0.28, 0.55, 0.45),
            Color::new(0.4, 0.7, 0.9, 0.8),
        );
        draw_cube(
            wing_r,
            vec3(0.28, 0.55, 0.45),
            None,
            Color::new(0.14, 0.32, 0.58, 1.0),
        );
        draw_cube_wires(
            wing_r,
            vec3(0.28, 0.55, 0.45),
            Color::new(0.4, 0.7, 0.9, 0.8),
        );

        // Rear thruster booster and flame
        let booster_pos = av_pos + vec3(0.0, 0.85, 0.35);
        draw_cylinder(
            booster_pos,
            0.16,
            0.20,
            0.30,
            None,
            Color::new(0.18, 0.24, 0.36, 1.0),
        );
        let flame_len = 0.25 + 0.12 * (time * 16.0).sin().abs();
        draw_cylinder(
            booster_pos + vec3(0.0, 0.0, 0.22),
            0.14,
            0.02,
            flame_len,
            None,
            Color::new(0.1, 0.75, 1.0, 0.85),
        );

        // Ground energy aura ring
        let shadow_r = 0.8 + 0.04 * (time * 4.0).sin();
        draw_cylinder_wires(
            av_pos + vec3(0.0, 0.03, 0.0),
            shadow_r,
            shadow_r,
            0.03,
            None,
            Color::new(0.0, 0.85, 1.0, 0.5),
        );
    }
}
