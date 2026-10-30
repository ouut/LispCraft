//! # 2D Drawing Primitives
//!
//! Provides pure, decoupled GPU 2D and UI rendering atoms for Macroquad.

use macroquad::color::Color;

use crate::mock::{dispatch_render, RenderCommand};

/// Draws 2D screen text at pixel coordinates `(x, y)` with `font_size` and `color`.
pub fn draw_text_screen(text: &str, x: f32, y: f32, font_size: f32, color: Color) {
    let text_owned = text.to_string();
    dispatch_render(
        RenderCommand::DrawTextScreen {
            text: text_owned.clone(),
            x,
            y,
            font_size,
            color,
        },
        || {
            macroquad::text::draw_text(&text_owned, x, y, font_size, color);
        },
    );
}

/// Draws a filled 2D rectangle at `(x, y)` with width `w`, height `h`, and `color`.
pub fn draw_rect(x: f32, y: f32, w: f32, h: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawRect { x, y, w, h, color },
        || macroquad::shapes::draw_rectangle(x, y, w, h, color),
    );
}

/// Draws a wireframe 2D rectangle at `(x, y)` with width `w`, height `h`, border `thickness`, and `color`.
pub fn draw_rect_lines(x: f32, y: f32, w: f32, h: f32, thickness: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawRectLines {
            x,
            y,
            w,
            h,
            thickness,
            color,
        },
        || macroquad::shapes::draw_rectangle_lines(x, y, w, h, thickness, color),
    );
}

/// Draws a filled 2D circle at center `(x, y)` with `radius` and `color`.
pub fn draw_circle(x: f32, y: f32, radius: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawCircle { x, y, radius, color },
        || macroquad::shapes::draw_circle(x, y, radius, color),
    );
}

/// Draws a wireframe 2D circle at center `(x, y)` with `radius`, border `thickness`, and `color`.
pub fn draw_circle_lines(x: f32, y: f32, radius: f32, thickness: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawCircleLines {
            x,
            y,
            radius,
            thickness,
            color,
        },
        || macroquad::shapes::draw_circle_lines(x, y, radius, thickness, color),
    );
}

/// Draws a 2D line segment from `(x1, y1)` to `(x2, y2)` with `thickness` and `color`.
pub fn draw_line_2d(x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: Color) {
    dispatch_render(
        RenderCommand::DrawLine2D {
            x1,
            y1,
            x2,
            y2,
            thickness,
            color,
        },
        || macroquad::shapes::draw_line(x1, y1, x2, y2, thickness, color),
    );
}

/// Measures the rendered pixel width of a text string at the specified `font_size`.
///
/// In headless or non-window environments (e.g. unit tests), gracefully falls back to an estimate.
pub fn measure_text_width(text: &str, font_size: f32) -> f32 {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        macroquad::text::measure_text(text, None, font_size.max(1.0) as u16, 1.0).width
    }));
    std::panic::set_hook(prev_hook);

    res.unwrap_or(text.len() as f32 * font_size * 0.55)
}
