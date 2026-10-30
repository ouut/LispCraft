//! # Safe Mock & Testing Hooks
//!
//! Provides thread-local capture and execution interception for GPU drawing commands.
//! This allows deterministic headless testing of Scheme render functions without
//! requiring an active OpenGL window or Macroquad graphics context.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use macroquad::color::Color;
use macroquad::math::{Vec2, Vec3};

/// Captured GPU render commands for unit testing and headless validation.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderCommand {
    DrawCube {
        pos: Vec3,
        size: Vec3,
        color: Color,
    },
    DrawCubeWires {
        pos: Vec3,
        size: Vec3,
        color: Color,
    },
    DrawSphere {
        pos: Vec3,
        radius: f32,
        color: Color,
    },
    DrawSphereWires {
        pos: Vec3,
        radius: f32,
        color: Color,
    },
    DrawCylinder {
        pos: Vec3,
        r_top: f32,
        r_bot: f32,
        height: f32,
        sides: u8,
        color: Color,
    },
    DrawPlane {
        pos: Vec3,
        size: Vec2,
        color: Color,
    },
    DrawLine3D {
        start: Vec3,
        end: Vec3,
        color: Color,
    },
    DrawBillboardText {
        text: String,
        pos: Vec3,
        font_size: f32,
        color: Color,
    },
    DrawTextScreen {
        text: String,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
    },
    DrawRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    DrawRectLines {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        thickness: f32,
        color: Color,
    },
    DrawCircle {
        x: f32,
        y: f32,
        radius: f32,
        color: Color,
    },
    DrawCircleLines {
        x: f32,
        y: f32,
        radius: f32,
        thickness: f32,
        color: Color,
    },
    DrawLine2D {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        thickness: f32,
        color: Color,
    },
}

pub type RenderHook = Box<dyn FnMut(RenderCommand)>;

thread_local! {
    static MOCK_RENDER_HOOK: RefCell<Option<RenderHook>> = RefCell::new(None);
}

static HEADLESS_FALLBACK: AtomicBool = AtomicBool::new(false);

/// Enables or disables global headless fallback mode.
/// When enabled, actual Macroquad GPU calls are skipped without panicking if no OpenGL context exists.
pub fn enable_headless_fallback(enable: bool) {
    HEADLESS_FALLBACK.store(enable, Ordering::Relaxed);
}

/// Checks whether global headless fallback mode is enabled.
pub fn is_headless_fallback() -> bool {
    HEADLESS_FALLBACK.load(Ordering::Relaxed)
}

/// Sets a thread-local mock render hook to intercept render commands.
pub fn set_render_hook<F: FnMut(RenderCommand) + 'static>(f: F) {
    MOCK_RENDER_HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(f));
    });
}

/// Clears the thread-local mock render hook.
pub fn clear_render_hook() {
    MOCK_RENDER_HOOK.with(|h| {
        *h.borrow_mut() = None;
    });
}

/// Captures all render commands executed during closure `f`.
pub fn capture_render_commands<R, F: FnOnce() -> R>(f: F) -> (R, Vec<RenderCommand>) {
    let captured = Rc::new(RefCell::new(Vec::new()));
    let captured_clone = Rc::clone(&captured);
    set_render_hook(move |cmd| {
        captured_clone.borrow_mut().push(cmd);
    });
    let result = f();
    clear_render_hook();
    let cmds = Rc::try_unwrap(captured)
        .expect("multiple references to captured commands")
        .into_inner();
    (result, cmds)
}

/// Internal dispatcher: sends command to thread-local mock hook if registered,
/// otherwise performs the real GPU drawing function (or safe fallback in test mode).
pub(crate) fn dispatch_render<F: FnOnce()>(cmd: RenderCommand, draw_fn: F) {
    let intercepted = MOCK_RENDER_HOOK.with(|hook| {
        if let Some(ref mut h) = *hook.borrow_mut() {
            h(cmd);
            true
        } else {
            false
        }
    });

    if !intercepted {
        if is_headless_fallback() {
            return;
        }

        #[cfg(test)]
        {
            let prev_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(draw_fn));
            std::panic::set_hook(prev_hook);
        }

        #[cfg(not(test))]
        {
            draw_fn();
        }
    }
}
