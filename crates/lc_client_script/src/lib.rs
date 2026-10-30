//! # `lc_client_script`: Client-side Scheme Engine with Native GPU Rendering Bindings
//!
//! Provides the Layer 1 client-side Scheme runtime engine embedding Steel Scheme VM
//! with native GPU drawing bindings (`lc_render`) and in-memory VFS support.
//! Compatible with both Native and WASM32 compilation targets.

pub mod bindings;
pub mod helpers;
pub mod runtime;
pub mod vfs;

pub use bindings::{register_bindings, SCHEME_PRELUDE};
pub use helpers::{
    entity_to_raw_tuple, json_to_val, parse_color, parse_f32, parse_isize, parse_vec2, parse_vec3,
    slice3_to_steel, slice4_to_steel, val_to_json, vec3_to_steel,
};
pub use runtime::ClientSchemeRuntime;
pub use vfs::{scan_directory, sort_vfs_entries, vfs_sort_tier};

#[cfg(test)]
mod tests {
    use super::*;
    use lc_core::{ClientState, EntityDesc};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_vfs_sorting_order() {
        let mut vfs = HashMap::new();
        vfs.insert("client/main.scm".to_string(), "main".to_string());
        vfs.insert("client/hud.scm".to_string(), "hud".to_string());
        vfs.insert("shared/constants.scm".to_string(), "const".to_string());
        vfs.insert("shared/utils.scm".to_string(), "utils".to_string());
        vfs.insert("client/radar.scm".to_string(), "radar".to_string());

        let sorted = sort_vfs_entries(&vfs);
        let paths: Vec<&str> = sorted.into_iter().map(|(k, _)| k.as_str()).collect();

        assert_eq!(paths[0], "shared/constants.scm");
        assert_eq!(paths[1], "shared/utils.scm");
        assert!(paths[2] == "client/hud.scm" || paths[2] == "client/radar.scm");
        assert!(paths[3] == "client/hud.scm" || paths[3] == "client/radar.scm");
        assert_eq!(paths[4], "client/main.scm");
    }

    #[test]
    fn test_vfs_loading_and_lifecycle() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        let mut vfs = HashMap::new();
        vfs.insert(
            "shared/constants.scm".to_string(),
            "(define BASE_SPEED 10.0)".to_string(),
        );
        vfs.insert(
            "client/main.scm".to_string(),
            r#"
            (define *initialized* #f)
            (define *total-dt* 0.0)

            (define (client-init)
              (set! *initialized* #t))

            (define (client-render dt)
              (set! *total-dt* (+ *total-dt* dt)))
            "#
            .to_string(),
        );

        runtime.load_vfs(&vfs).expect("Failed to load VFS");
        assert_eq!(runtime.vfs().len(), 2);

        // Before init
        let init_val = runtime.eval("*initialized*").unwrap();
        assert_eq!(init_val[0], game_script::SteelVal::BoolV(false));

        // Call init
        runtime.init().expect("client-init failed");
        let init_after = runtime.eval("*initialized*").unwrap();
        assert_eq!(init_after[0], game_script::SteelVal::BoolV(true));

        // Call render twice
        runtime.render(0.016).expect("client-render failed");
        runtime.render(0.016).expect("client-render failed");

        let dt_val = runtime.eval("*total-dt*").unwrap();
        if let game_script::SteelVal::NumV(dt) = dt_val[0] {
            assert!((dt - 0.032).abs() < 1e-4);
        } else {
            panic!("Expected NumV for *total-dt*");
        }
    }

    #[test]
    fn test_replicated_state_alists() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        let state = ClientState {
            client_id: "player_1".to_string(),
            camera_target: [5.0, 1.0, -10.0],
            camera_yaw: 0.785,
            hp: 85.0,
            max_hp: 100.0,
            score: 1250,
            entities: vec![
                EntityDesc {
                    id: "cube_1".to_string(),
                    shape: "cube".to_string(),
                    pos: [2.0, 0.0, 3.0],
                    size: [1.0, 1.0, 1.0],
                    color: [1.0, 0.0, 0.0, 1.0],
                    label: "Target Alpha".to_string(),
                },
                EntityDesc {
                    id: "sphere_2".to_string(),
                    shape: "sphere".to_string(),
                    pos: [-4.0, 2.0, 8.0],
                    size: [2.0, 2.0, 2.0],
                    color: [0.0, 1.0, 0.5, 0.9],
                    label: "Beacon Beta".to_string(),
                },
            ],
            logs: vec![],
            properties: HashMap::new(),
        };

        runtime.set_replicated_state(state);

        // Test (get-local-player) string keys & symbol keys
        let lp_pos = runtime
            .eval(r#"(cdr (assoc "pos" (get-local-player)))"#)
            .unwrap();
        assert_eq!(format!("{:?}", lp_pos), "['(5.0 1.0 -10.0)]");

        let lp_sym_pos = runtime
            .eval(r#"(cdr (assoc 'pos (get-local-player)))"#)
            .unwrap();
        assert_eq!(format!("{:?}", lp_sym_pos), "['(5.0 1.0 -10.0)]");

        let lp_hp = runtime
            .eval(r#"(cdr (assoc "hp" (get-local-player)))"#)
            .unwrap();
        assert_eq!(lp_hp[0], game_script::SteelVal::NumV(85.0));

        let lp_score = runtime
            .eval(r#"(cdr (assoc "score" (get-local-player)))"#)
            .unwrap();
        assert_eq!(lp_score[0], game_script::SteelVal::IntV(1250));

        // Test (get-replicated-entities)
        let ent_count = runtime
            .eval(r#"(length (get-replicated-entities))"#)
            .unwrap();
        assert_eq!(ent_count[0], game_script::SteelVal::IntV(2));

        let ent0_id = runtime
            .eval(r#"(cdr (assoc "id" (car (get-replicated-entities))))"#)
            .unwrap();
        assert_eq!(ent0_id[0], game_script::SteelVal::StringV("cube_1".into()));

        let ent0_shape = runtime
            .eval(r#"(cdr (assoc "shape" (car (get-replicated-entities))))"#)
            .unwrap();
        assert_eq!(
            ent0_shape[0],
            game_script::SteelVal::StringV("cube".into())
        );

        let ent0_pos = runtime
            .eval(r#"(cdr (assoc "pos" (car (get-replicated-entities))))"#)
            .unwrap();
        assert_eq!(format!("{:?}", ent0_pos), "['(2.0 0.0 3.0)]");

        let ent0_label = runtime
            .eval(r#"(cdr (assoc "label" (car (get-replicated-entities))))"#)
            .unwrap();
        assert_eq!(
            ent0_label[0],
            game_script::SteelVal::StringV("Target Alpha".into())
        );
    }

    #[test]
    fn test_fire_server_and_drain() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        runtime
            .eval(r#"(fire-server! "shoot" "{\"power\": 100, \"target\": \"alpha\"}")"#)
            .unwrap();
        runtime
            .eval(r#"(fire-server! "ping" (hash "seq" 1))"#)
            .unwrap();

        let events = runtime.drain_outgoing_events();
        assert_eq!(events.len(), 2);

        assert_eq!(events[0].0, "shoot");
        assert_eq!(events[0].1["power"], 100);
        assert_eq!(events[0].1["target"], "alpha");

        assert_eq!(events[1].0, "ping");
        assert_eq!(events[1].1["seq"], 1);

        // Queue should now be empty
        let events_after = runtime.drain_outgoing_events();
        assert!(events_after.is_empty());
    }

    #[test]
    fn test_server_event_dispatch() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        runtime
            .eval(
                r#"
            (define *chat-log* '())
            (on-server-event "chat" (lambda (p)
                                      (set! *chat-log* (cons (hash-try-get p "msg") *chat-log*))))
            "#,
            )
            .unwrap();

        runtime.handle_server_event("chat", &json!({"msg": "Welcome to LispCraft!"}));
        runtime.handle_server_event("chat", &json!({"msg": "Second Message"}));

        let log_res = runtime.eval("*chat-log*").unwrap();
        assert_eq!(
            format!("{:?}", log_res),
            r#"['("Second Message" "Welcome to LispCraft!")]"#
        );
    }

    #[test]
    fn test_camera_bindings() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        runtime
            .eval(r#"(set-camera! (list 12.0 5.0 -8.0) 1.25 0.65 14.0)"#)
            .unwrap();

        let cam = runtime.camera();
        let (pos, yaw, pitch, dist) = cam.get_params();
        assert!((pos.x - 12.0).abs() < 1e-4);
        assert!((pos.y - 5.0).abs() < 1e-4);
        assert!((pos.z - -8.0).abs() < 1e-4);
        assert!((yaw - 1.25).abs() < 1e-4);
        assert!((pitch - 0.65).abs() < 1e-4);
        assert!((dist - 14.0).abs() < 1e-4);

        let query = runtime.eval("(get-camera)").unwrap();
        assert!(matches!(query[0], game_script::SteelVal::ListV(_)));
    }

    #[test]
    fn test_native_gpu_drawing_captured() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        let (_, cmds) = lc_render::capture_render_commands(|| {
            runtime
                .eval(
                    r#"
                (draw-cube! (list 1 2 3) (list 2 2 2) (list 1 0 0 1))
                (draw-cube-wires! (list 1 2 3) (list 2 2 2) (list 1 1 0 1))
                (draw-sphere! (list 0 5 0) 2.5 (list 0 1 0 1))
                (draw-sphere-wires! (list 0 5 0) 2.5 (list 0 1 1 1))
                (draw-cylinder! (list 0 0 0) 1.0 1.5 4.0 16 (list 1 0 1 1))
                (draw-plane! (list 0 0 0) (list 20 20) (list 0.2 0.2 0.2 1.0))
                (draw-line-3d! (list 0 0 0) (list 10 10 10) (list 1 1 1 1))
                (draw-billboard-text! "Enemy Mech" (list 0 2 0) 16.0 (list 1 0 0 1))
                (draw-text-screen! "HUD HP: 100" 20 30 18 (list 0 1 0 1))
                (draw-rect! 10 10 200 40 (list 0 0 0 0.8))
                (draw-rect-lines! 10 10 200 40 2.0 (list 0 1 1 1))
                (draw-circle! 100 100 30 (list 1 1 0 1))
                (draw-circle-lines! 100 100 30 1.5 (list 1 1 1 1))
                (draw-line-2d! 10 10 100 100 2.0 (list 1 0 0 1))
                "#,
                )
                .unwrap();
        });

        assert_eq!(cmds.len(), 14);
        assert!(matches!(cmds[0], lc_render::RenderCommand::DrawCube { .. }));
        assert!(matches!(
            cmds[1],
            lc_render::RenderCommand::DrawCubeWires { .. }
        ));
        assert!(matches!(
            cmds[2],
            lc_render::RenderCommand::DrawSphere { .. }
        ));
        assert!(matches!(
            cmds[3],
            lc_render::RenderCommand::DrawSphereWires { .. }
        ));
        assert!(matches!(
            cmds[4],
            lc_render::RenderCommand::DrawCylinder { .. }
        ));
        assert!(matches!(
            cmds[5],
            lc_render::RenderCommand::DrawPlane { .. }
        ));
        assert!(matches!(
            cmds[6],
            lc_render::RenderCommand::DrawLine3D { .. }
        ));
        assert!(matches!(
            cmds[7],
            lc_render::RenderCommand::DrawBillboardText { .. }
        ));
        assert!(matches!(
            cmds[8],
            lc_render::RenderCommand::DrawTextScreen { .. }
        ));
        assert!(matches!(cmds[9], lc_render::RenderCommand::DrawRect { .. }));
        assert!(matches!(
            cmds[10],
            lc_render::RenderCommand::DrawRectLines { .. }
        ));
        assert!(matches!(
            cmds[11],
            lc_render::RenderCommand::DrawCircle { .. }
        ));
        assert!(matches!(
            cmds[12],
            lc_render::RenderCommand::DrawCircleLines { .. }
        ));
        assert!(matches!(
            cmds[13],
            lc_render::RenderCommand::DrawLine2D { .. }
        ));
    }

    #[test]
    fn test_screen_projection_and_metrics() {
        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");

        let sw = runtime.eval("(screen-width)").unwrap();
        assert!(matches!(sw[0], game_script::SteelVal::NumV(_)));

        let sh = runtime.eval("(screen-height)").unwrap();
        assert!(matches!(sh[0], game_script::SteelVal::NumV(_)));

        let mw = runtime
            .eval(r#"(measure-text-width "HELLO" 14)"#)
            .unwrap();
        if let game_script::SteelVal::NumV(w) = mw[0] {
            assert!(w > 0.0);
        } else {
            panic!("Expected NumV for measure-text-width");
        }

        // Test world-to-screen (origin in front of camera should project)
        let w2s = runtime.eval("(world-to-screen (list 0 0 0))").unwrap();
        assert!(matches!(w2s[0], game_script::SteelVal::ListV(_)));
    }

    #[test]
    fn test_directory_loading_standalone() {
        let temp_dir = std::env::temp_dir().join("lc_test_vfs_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        std::fs::write(
            temp_dir.join("constants.scm"),
            "(define TEST_CONST 999.0)",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("main.scm"),
            "(define (client-init) (void))",
        )
        .unwrap();

        let mut runtime = ClientSchemeRuntime::new().expect("Failed to create runtime");
        runtime
            .load_directory(&temp_dir)
            .expect("Failed to load directory");

        assert_eq!(runtime.vfs().len(), 2);
        let res = runtime.eval("TEST_CONST").unwrap();
        assert_eq!(res[0], game_script::SteelVal::NumV(999.0));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
