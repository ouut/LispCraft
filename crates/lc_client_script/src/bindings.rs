//! # Native Drawing & State Bindings for Steel Scheme VM

use std::sync::{Arc, Mutex};
use lc_core::ClientState;
use lc_render::OrbitCamera;
use steel::rvals::SteelVal;
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;

use crate::helpers::*;

/// Prelude script that defines ergonomic Scheme wrappers for state and events.
pub const SCHEME_PRELUDE: &str = r#"
;; --- Replicated State Helpers ---

;; Converts raw entity arrays from Rust into Scheme association lists
(define (get-replicated-entities)
  (map (lambda (e)
         (let ((id (list-ref e 0))
               (shape (list-ref e 1))
               (pos (list-ref e 2))
               (size (list-ref e 3))
               (color (list-ref e 4))
               (label (list-ref e 5)))
           (list (cons "id" id)
                 (cons 'id id)
                 (cons "shape" shape)
                 (cons 'shape shape)
                 (cons "pos" pos)
                 (cons 'pos pos)
                 (cons "size" size)
                 (cons 'size size)
                 (cons "color" color)
                 (cons 'color color)
                 (cons "label" label)
                 (cons 'label label))))
       (%internal-get-replicated-entities)))

;; Converts raw player state into a Scheme association list
(define (get-local-player)
  (let ((p (%internal-get-local-player)))
    (if (and p (not (null? p)))
        (let ((pos (list-ref p 0))
              (yaw (list-ref p 1))
              (hp (list-ref p 2))
              (score (list-ref p 3))
              (max-hp (if (> (length p) 4) (list-ref p 4) 100.0)))
          (list (cons "pos" pos)
                (cons 'pos pos)
                (cons "yaw" yaw)
                (cons 'yaw yaw)
                (cons "hp" hp)
                (cons 'hp hp)
                (cons "score" score)
                (cons 'score score)
                (cons "max_hp" max-hp)
                (cons 'max_hp max-hp)))
        '())))

;; --- RemoteEvent Callback Registry ---
(define *server-event-handlers* (hash))

(define (on-server-event event-name callback)
  (set! *server-event-handlers* (hash-insert *server-event-handlers* event-name callback)))

(define (%dispatch-server-event name payload)
  (let ((handler (hash-try-get *server-event-handlers* name)))
    (if handler
        (begin
          (handler payload)
          #t)
        #f)))
"#;

/// Registers all native GPU drawing atoms, camera controls, state query atoms,
/// and RemoteEvent functions into the Steel Scheme engine.
pub fn register_bindings(
    engine: &mut Engine,
    state_cache: Arc<Mutex<ClientState>>,
    camera: Arc<Mutex<OrbitCamera>>,
    outgoing_events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
) -> Result<(), String> {
    // -------------------------------------------------------------------------
    // 3D GPU Drawing Atoms
    // -------------------------------------------------------------------------

    // (draw-cube! (list x y z) (list w h d) (list r g b a))
    engine.register_fn(
        "draw-cube!",
        |pos_val: SteelVal, size_val: SteelVal, color_val: SteelVal| {
            if let (Ok(pos), Ok(size), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_vec3(&size_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_cube(pos, size, col);
            }
        },
    );

    // (draw-cube-wires! (list x y z) (list w h d) (list r g b a))
    engine.register_fn(
        "draw-cube-wires!",
        |pos_val: SteelVal, size_val: SteelVal, color_val: SteelVal| {
            if let (Ok(pos), Ok(size), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_vec3(&size_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_cube_wires(pos, size, col);
            }
        },
    );

    // (draw-sphere! (list x y z) radius (list r g b a))
    engine.register_fn(
        "draw-sphere!",
        |pos_val: SteelVal, radius_val: SteelVal, color_val: SteelVal| {
            if let (Ok(pos), Ok(radius), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_f32(&radius_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_sphere(pos, radius, col);
            }
        },
    );

    // (draw-sphere-wires! (list x y z) radius (list r g b a))
    engine.register_fn(
        "draw-sphere-wires!",
        |pos_val: SteelVal, radius_val: SteelVal, color_val: SteelVal| {
            if let (Ok(pos), Ok(radius), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_f32(&radius_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_sphere_wires(pos, radius, col);
            }
        },
    );

    // (draw-cylinder! (list x y z) r-top r-bot height sides (list r g b a))
    engine.register_fn(
        "draw-cylinder!",
        |pos_val: SteelVal,
         rtop_val: SteelVal,
         rbot_val: SteelVal,
         h_val: SteelVal,
         sides_val: SteelVal,
         color_val: SteelVal| {
            if let (Ok(pos), Ok(r_top), Ok(r_bot), Ok(height), Ok(sides), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_f32(&rtop_val),
                parse_f32(&rbot_val),
                parse_f32(&h_val),
                parse_isize(&sides_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_cylinder(pos, r_top, r_bot, height, sides.max(3) as u8, col);
            }
        },
    );

    // (draw-plane! (list x y z) (list w d) (list r g b a))
    engine.register_fn(
        "draw-plane!",
        |pos_val: SteelVal, size_val: SteelVal, color_val: SteelVal| {
            if let (Ok(pos), Ok(size), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_vec2(&size_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_plane(pos, size, col);
            }
        },
    );

    // (draw-line-3d! (list x1 y1 z1) (list x2 y2 z2) (list r g b a))
    engine.register_fn(
        "draw-line-3d!",
        |start_val: SteelVal, end_val: SteelVal, color_val: SteelVal| {
            if let (Ok(start), Ok(end), Ok(col)) = (
                parse_vec3(&start_val),
                parse_vec3(&end_val),
                parse_color(&color_val),
            ) {
                lc_render::draw_line_3d(start, end, col);
            }
        },
    );

    // (draw-billboard-text! text (list x y z) font-size (list r g b a))
    let cam_for_billboard = Arc::clone(&camera);
    engine.register_fn(
        "draw-billboard-text!",
        move |text_val: SteelVal,
              pos_val: SteelVal,
              size_val: SteelVal,
              color_val: SteelVal| {
            let text = match text_val {
                SteelVal::StringV(s) => s.to_string(),
                other => other.to_string(),
            };
            if let (Ok(pos), Ok(font_size), Ok(col)) = (
                parse_vec3(&pos_val),
                parse_f32(&size_val),
                parse_color(&color_val),
            ) {
                if let Ok(cam) = cam_for_billboard.lock() {
                    let cam3d = cam.to_camera3d();
                    lc_render::draw_billboard_text(&text, pos, font_size, col, &cam3d);
                }
            }
        },
    );

    // -------------------------------------------------------------------------
    // 2D & UI GPU Drawing Atoms
    // -------------------------------------------------------------------------

    // (draw-text-screen! text x y font-size (list r g b a))
    engine.register_fn(
        "draw-text-screen!",
        |text_val: SteelVal,
         x_val: SteelVal,
         y_val: SteelVal,
         font_val: SteelVal,
         col_val: SteelVal| {
            let text = match text_val {
                SteelVal::StringV(s) => s.to_string(),
                other => other.to_string(),
            };
            if let (Ok(x), Ok(y), Ok(font_size), Ok(col)) = (
                parse_f32(&x_val),
                parse_f32(&y_val),
                parse_f32(&font_val),
                parse_color(&col_val),
            ) {
                lc_render::draw_text_screen(&text, x, y, font_size, col);
            }
        },
    );

    // (draw-rect! x y w h (list r g b a))
    engine.register_fn(
        "draw-rect!",
        |x_val: SteelVal,
         y_val: SteelVal,
         w_val: SteelVal,
         h_val: SteelVal,
         col_val: SteelVal| {
            if let (Ok(x), Ok(y), Ok(w), Ok(h), Ok(col)) = (
                parse_f32(&x_val),
                parse_f32(&y_val),
                parse_f32(&w_val),
                parse_f32(&h_val),
                parse_color(&col_val),
            ) {
                lc_render::draw_rect(x, y, w, h, col);
            }
        },
    );

    // (draw-rect-lines! x y w h thickness (list r g b a))
    engine.register_fn(
        "draw-rect-lines!",
        |x_val: SteelVal,
         y_val: SteelVal,
         w_val: SteelVal,
         h_val: SteelVal,
         thick_val: SteelVal,
         col_val: SteelVal| {
            if let (Ok(x), Ok(y), Ok(w), Ok(h), Ok(thickness), Ok(col)) = (
                parse_f32(&x_val),
                parse_f32(&y_val),
                parse_f32(&w_val),
                parse_f32(&h_val),
                parse_f32(&thick_val),
                parse_color(&col_val),
            ) {
                lc_render::draw_rect_lines(x, y, w, h, thickness, col);
            }
        },
    );

    // (draw-circle! x y radius (list r g b a))
    engine.register_fn(
        "draw-circle!",
        |x_val: SteelVal, y_val: SteelVal, radius_val: SteelVal, col_val: SteelVal| {
            if let (Ok(x), Ok(y), Ok(radius), Ok(col)) = (
                parse_f32(&x_val),
                parse_f32(&y_val),
                parse_f32(&radius_val),
                parse_color(&col_val),
            ) {
                lc_render::draw_circle(x, y, radius, col);
            }
        },
    );

    // (draw-circle-lines! x y radius thickness (list r g b a))
    engine.register_fn(
        "draw-circle-lines!",
        |x_val: SteelVal,
         y_val: SteelVal,
         radius_val: SteelVal,
         thick_val: SteelVal,
         col_val: SteelVal| {
            if let (Ok(x), Ok(y), Ok(radius), Ok(thickness), Ok(col)) = (
                parse_f32(&x_val),
                parse_f32(&y_val),
                parse_f32(&radius_val),
                parse_f32(&thick_val),
                parse_color(&col_val),
            ) {
                lc_render::draw_circle_lines(x, y, radius, thickness, col);
            }
        },
    );

    // (draw-line-2d! x1 y1 x2 y2 thickness (list r g b a))
    engine.register_fn(
        "draw-line-2d!",
        |x1_val: SteelVal,
         y1_val: SteelVal,
         x2_val: SteelVal,
         y2_val: SteelVal,
         thick_val: SteelVal,
         col_val: SteelVal| {
            if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2), Ok(thickness), Ok(col)) = (
                parse_f32(&x1_val),
                parse_f32(&y1_val),
                parse_f32(&x2_val),
                parse_f32(&y2_val),
                parse_f32(&thick_val),
                parse_color(&col_val),
            ) {
                lc_render::draw_line_2d(x1, y1, x2, y2, thickness, col);
            }
        },
    );

    // (measure-text-width text font-size)
    engine.register_fn(
        "measure-text-width",
        |text_val: SteelVal, font_val: SteelVal| -> SteelVal {
            let text = match text_val {
                SteelVal::StringV(s) => s.to_string(),
                other => other.to_string(),
            };
            let font_size = parse_f32(&font_val).unwrap_or(14.0);
            let w = lc_render::measure_text_width(&text, font_size);
            SteelVal::NumV(w as f64)
        },
    );

    // (screen-width)
    engine.register_fn("screen-width", || -> SteelVal {
        let (sw, _) = lc_render::screen_size();
        SteelVal::NumV(sw as f64)
    });

    // (screen-height)
    engine.register_fn("screen-height", || -> SteelVal {
        let (_, sh) = lc_render::screen_size();
        SteelVal::NumV(sh as f64)
    });

    // (world-to-screen (list x y z)) -> (list sx sy) or #f
    let cam_for_projection = Arc::clone(&camera);
    engine.register_fn("world-to-screen", move |pos_val: SteelVal| -> SteelVal {
        let pos = match parse_vec3(&pos_val) {
            Ok(p) => p,
            Err(_) => return SteelVal::BoolV(false),
        };
        if let Ok(cam) = cam_for_projection.lock() {
            let cam3d = cam.to_camera3d();
            match lc_render::world_to_screen(&cam3d, pos) {
                Some(screen_pt) => SteelVal::ListV(
                    vec![
                        SteelVal::NumV(screen_pt.x as f64),
                        SteelVal::NumV(screen_pt.y as f64),
                    ]
                    .into(),
                ),
                None => SteelVal::BoolV(false),
            }
        } else {
            SteelVal::BoolV(false)
        }
    });

    // -------------------------------------------------------------------------
    // Camera Control & Queries
    // -------------------------------------------------------------------------

    // (set-camera! (list tx ty tz) yaw pitch dist)
    let cam_for_set = Arc::clone(&camera);
    engine.register_fn(
        "set-camera!",
        move |target_val: SteelVal,
              yaw_val: SteelVal,
              pitch_val: SteelVal,
              dist_val: SteelVal| {
            if let (Ok(target), Ok(yaw), Ok(pitch), Ok(dist)) = (
                parse_vec3(&target_val),
                parse_f32(&yaw_val),
                parse_f32(&pitch_val),
                parse_f32(&dist_val),
            ) {
                if let Ok(mut cam) = cam_for_set.lock() {
                    cam.set_params(target, yaw, pitch, dist);
                }
            }
        },
    );

    // (get-camera) -> (list (list tx ty tz) yaw pitch dist)
    let cam_for_get = Arc::clone(&camera);
    engine.register_fn("get-camera", move || -> SteelVal {
        if let Ok(cam) = cam_for_get.lock() {
            let (target, yaw, pitch, dist) = cam.get_params();
            SteelVal::ListV(
                vec![
                    slice3_to_steel([target.x, target.y, target.z]),
                    SteelVal::NumV(yaw as f64),
                    SteelVal::NumV(pitch as f64),
                    SteelVal::NumV(dist as f64),
                ]
                .into(),
            )
        } else {
            SteelVal::BoolV(false)
        }
    });

    // -------------------------------------------------------------------------
    // Replicated State Queries
    // -------------------------------------------------------------------------

    // %internal-get-replicated-entities
    let state_for_entities = Arc::clone(&state_cache);
    engine.register_fn("%internal-get-replicated-entities", move || -> SteelVal {
        if let Ok(state) = state_for_entities.lock() {
            let raw_list: Vec<SteelVal> =
                state.entities.iter().map(entity_to_raw_tuple).collect();
            SteelVal::ListV(raw_list.into())
        } else {
            SteelVal::ListV(vec![].into())
        }
    });

    // %internal-get-local-player
    let state_for_player = Arc::clone(&state_cache);
    engine.register_fn("%internal-get-local-player", move || -> SteelVal {
        if let Ok(state) = state_for_player.lock() {
            SteelVal::ListV(
                vec![
                    slice3_to_steel(state.camera_target),
                    SteelVal::NumV(state.camera_yaw as f64),
                    SteelVal::NumV(state.hp as f64),
                    SteelVal::IntV(state.score as isize),
                    SteelVal::NumV(state.max_hp as f64),
                ]
                .into(),
            )
        } else {
            SteelVal::BoolV(false)
        }
    });

    // -------------------------------------------------------------------------
    // RemoteEvents
    // -------------------------------------------------------------------------

    // (fire-server! event-name payload)
    let events_for_fire = Arc::clone(&outgoing_events);
    engine.register_fn(
        "fire-server!",
        move |event_val: SteelVal, payload_val: SteelVal| {
            let name = match event_val {
                SteelVal::StringV(s) => s.to_string(),
                SteelVal::SymbolV(s) => s.to_string(),
                other => other.to_string(),
            };

            let json_payload = match payload_val {
                SteelVal::StringV(ref s) => {
                    serde_json::from_str::<serde_json::Value>(s)
                        .unwrap_or_else(|_| serde_json::Value::String(s.to_string()))
                }
                ref other => val_to_json(other),
            };

            if let Ok(mut q) = events_for_fire.lock() {
                q.push((name, json_payload));
            }
        },
    );

    // Evaluate Scheme prelude
    engine
        .run(SCHEME_PRELUDE)
        .map_err(|e| format!("Failed to evaluate Scheme prelude: {}", e))?;

    Ok(())
}
