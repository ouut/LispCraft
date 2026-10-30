//! # 2D Tactical Radar Minimap HUD
//!
//! Provides `RadarRenderer` to render a circular radar display showing player heading,
//! cardinal direction indicators (N/E/S/W), distance range rings, and colored blips for nearby entities.

use lc_core::EntityDesc;
use macroquad::color::Color;
use macroquad::math::{vec2, Vec2, Vec3};
use macroquad::shapes::{draw_circle, draw_circle_lines, draw_line};
use macroquad::text::{measure_text, draw_text};
use macroquad::time::get_time;

/// Tactical radar minimap renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct RadarRenderer {
    /// World radius covered by the radar minimap (in meters). Default: 35.0m.
    pub radar_range: f32,
}

impl Default for RadarRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn safe_get_time() -> f32 {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(get_time));
    std::panic::set_hook(prev_hook);
    res.unwrap_or(0.0) as f32
}

fn safe_measure_width(text: &str, font_size: u16) -> f32 {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        measure_text(text, None, font_size, 1.0).width
    }));
    std::panic::set_hook(prev_hook);
    res.unwrap_or(text.len() as f32 * (font_size as f32 * 0.55))
}

impl RadarRenderer {
    /// Creates a new `RadarRenderer` with default 35.0m range.
    pub fn new() -> Self {
        Self { radar_range: 35.0 }
    }

    /// Creates a `RadarRenderer` with a custom detection range.
    pub fn with_range(radar_range: f32) -> Self {
        Self { radar_range }
    }

    /// Draws the circular radar minimap at the given 2D screen center.
    pub fn draw_radar(
        &self,
        center: Vec2,
        radius: f32,
        player_pos: Vec3,
        player_yaw: f32,
        entities: &[EntityDesc],
    ) {
        // 1. Circular radar background
        draw_circle(
            center.x,
            center.y,
            radius,
            Color::new(0.02, 0.04, 0.08, 0.85),
        );

        // 2. Distance range rings
        draw_circle_lines(
            center.x,
            center.y,
            radius * 0.33,
            1.0,
            Color::new(0.1, 0.35, 0.55, 0.45),
        );
        draw_circle_lines(
            center.x,
            center.y,
            radius * 0.66,
            1.0,
            Color::new(0.1, 0.35, 0.55, 0.45),
        );
        draw_circle_lines(
            center.x,
            center.y,
            radius,
            2.0,
            Color::new(0.0, 0.85, 1.0, 0.9),
        );

        // 3. Grid crosshair lines
        draw_line(
            center.x - radius,
            center.y,
            center.x + radius,
            center.y,
            1.0,
            Color::new(0.1, 0.35, 0.55, 0.4),
        );
        draw_line(
            center.x,
            center.y - radius,
            center.x,
            center.y + radius,
            1.0,
            Color::new(0.1, 0.35, 0.55, 0.4),
        );

        // 4. Rotating sweep beam (safe fallback outside Macroquad context)
        let time = safe_get_time();
        let scan_ang = time * 2.8;
        let scan_end = center + vec2(scan_ang.cos(), scan_ang.sin()) * radius;
        draw_line(
            center.x,
            center.y,
            scan_end.x,
            scan_end.y,
            1.5,
            Color::new(0.0, 1.0, 0.6, 0.6),
        );

        // Header label
        draw_text(
            "TACTICAL RADAR",
            center.x - 48.0,
            center.y - radius - 6.0,
            12.0,
            Color::new(0.0, 0.9, 1.0, 0.9),
        );

        // 5. Cardinal direction markers (Heading-up display: forward is top)
        // North is world vector (0, -1)
        // East is world vector (1, 0)
        // South is world vector (0, 1)
        // West is world vector (-1, 0)
        let directions = [
            ("N", 0.0_f32, -1.0_f32, Color::new(1.0, 0.35, 0.35, 1.0)),
            ("E", 1.0_f32, 0.0_f32, Color::new(0.6, 0.8, 1.0, 0.85)),
            ("S", 0.0_f32, 1.0_f32, Color::new(0.6, 0.8, 1.0, 0.85)),
            ("W", -1.0_f32, 0.0_f32, Color::new(0.6, 0.8, 1.0, 0.85)),
        ];

        let marker_dist = radius - 10.0;
        for (name, dx, dz, color) in directions {
            let rx = dx * player_yaw.cos() - dz * player_yaw.sin();
            let ry = dx * player_yaw.sin() + dz * player_yaw.cos();
            let marker_pos = center + vec2(rx, ry) * marker_dist;

            let text_w = safe_measure_width(name, 11);

            draw_text(
                name,
                marker_pos.x - text_w * 0.5,
                marker_pos.y + 4.0,
                11.0,
                color,
            );
        }

        // 6. Player center icon
        draw_circle(center.x, center.y, 3.5, Color::new(0.1, 1.0, 0.4, 1.0));
        draw_line(
            center.x,
            center.y,
            center.x,
            center.y - 7.0,
            2.0,
            Color::new(0.1, 1.0, 0.4, 1.0),
        );

        // 7. Entity & Player blips within radar radius
        let px = player_pos.x;
        let pz = player_pos.z;
        let r_range = self.radar_range.max(1.0);

        for ent in entities {
            let ex = ent.pos[0] - px;
            let ez = ent.pos[2] - pz;
            let rx = (ex * player_yaw.cos() - ez * player_yaw.sin()) / r_range * radius;
            let ry = (ex * player_yaw.sin() + ez * player_yaw.cos()) / r_range * radius;
            let d = (rx * rx + ry * ry).sqrt();

            if d <= radius - 3.0 {
                let ep = center + vec2(rx, ry);
                let col = Color::new(ent.color[0], ent.color[1], ent.color[2], 0.95);
                draw_circle(ep.x, ep.y, 3.0, col);

                // Differentiate players with outer ring
                if ent.id.starts_with("player") {
                    draw_circle_lines(ep.x, ep.y, 5.0, 1.0, col);
                }
            }
        }
    }
}
