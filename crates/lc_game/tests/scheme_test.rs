use lc_game::{GameManager, SchemeGame, VIEW_DISTANCE};

#[test]
fn test_space_arena_full_lifecycle() {
    let game = SchemeGame::new("games/space_arena.scm")
        .expect("Failed to initialize space arena SchemeGame");

    // 1. Join two players
    game.on_join("player_1");
    game.on_join("player_2");

    // 2. Both players teleport to specific known locations
    let tp1 = game.on_command("player_1", "tp 0 0");
    println!("tp1: {}", tp1);
    assert!(tp1.contains("Teleported"));

    let tp2 = game.on_command("player_2", "tp 10 0");
    println!("tp2: {}", tp2);
    assert!(tp2.contains("Teleported"));

    // 3. Test mutual visibility: player_1 is at (0, 0), player_2 is at (10, 0).
    // Distance is 10.0, which is <= 25.0 view distance!
    let state1 = game
        .get_client_state("player_1")
        .expect("Failed to get state for player_1");
    assert_eq!(state1.client_id, "player_1");
    // Verify player_2 is in player_1's visible entities
    let p2_ent = state1.entities.iter().find(|e| e.id == "player-player_2");
    assert!(p2_ent.is_some(), "player_2 should be visible to player_1");
    let p2_desc = p2_ent.unwrap();
    assert_eq!(p2_desc.shape, "cube");
    assert_eq!(p2_desc.label, "player_2");

    // Also verify player_1 is visible to player_2
    let state2 = game
        .get_client_state("player_2")
        .expect("Failed to get state for player_2");
    let p1_ent = state2.entities.iter().find(|e| e.id == "player-player_1");
    assert!(p1_ent.is_some(), "player_1 should be visible to player_2");

    // 4. Test distance clipping: Move player_2 far away (e.g. tp 100 100)
    game.on_command("player_2", "tp 100 100");
    let state1_clipped = game
        .get_client_state("player_1")
        .expect("Failed to get state for player_1");
    let p2_clipped = state1_clipped
        .entities
        .iter()
        .find(|e| e.id == "player-player_2");
    assert!(
        p2_clipped.is_none(),
        "player_2 should NOT be visible when outside 25.0 units"
    );

    // 5. Test Commands
    // 5.1 help
    let help_out = game.on_command("player_1", "help");
    assert!(help_out.contains("Available commands"));

    // 5.2 stats
    let stats_out = game.on_command("player_1", "stats");
    println!("stats: {}", stats_out);
    assert!(stats_out.contains("HP: 100"));

    // 5.3 speed
    let spd_out = game.on_command("player_1", "speed 2.0");
    println!("spd_out: {}", spd_out);
    assert!(spd_out.contains("2"));
    let state_after_speed = game.get_client_state("player_1").unwrap();
    println!("state_after_speed logs: {:?}", state_after_speed.logs);
    assert!(state_after_speed
        .logs
        .iter()
        .any(|l| l.to_lowercase().contains("speed multiplier")));

    // 5.4 spawn
    let spawn_out = game.on_command("player_1", "spawn cube StarBase");
    println!("spawn: {}", spawn_out);
    assert!(spawn_out.contains("Spawned cube [StarBase]"));
    let state_after_spawn = game.get_client_state("player_1").unwrap();
    assert!(state_after_spawn
        .entities
        .iter()
        .any(|e| e.label == "StarBase"));

    // 5.5 say
    let say_out = game.on_command("player_1", "say Greetings galaxy!");
    assert!(say_out.contains("Broadcasted"));
    let state2_after_say = game.get_client_state("player_2").unwrap();
    assert!(state2_after_say
        .logs
        .iter()
        .any(|l| l.contains("Greetings galaxy!")));

    // 5.6 list
    let list_out = game.on_command("player_1", "list");
    println!("list: {}", list_out);
    assert!(list_out.contains("player_1") && list_out.contains("player_2"));

    // 5.7 heal
    let heal_out = game.on_command("player_1", "heal");
    assert!(heal_out.contains("Health restored to 100%"));

    // 6. Test game-tick bobbing
    let st_before = game.get_client_state("player_1").unwrap();
    let beacon_before = st_before
        .entities
        .iter()
        .find(|e| e.id == "beacon-center")
        .unwrap();
    let y_before = beacon_before.pos[1];

    for _ in 0..20 {
        game.tick(0.05);
    }
    let st_after = game.get_client_state("player_1").unwrap();
    let beacon_after = st_after
        .entities
        .iter()
        .find(|e| e.id == "beacon-center")
        .unwrap();
    let y_after = beacon_after.pos[1];
    println!("Beacon y: before = {}, after = {}", y_before, y_after);
    assert!(
        (y_before - y_after).abs() > 0.001,
        "Beacon y should bob over time"
    );

    // 7. Test interactive pickups
    // Teleport player_1 close to pickup-hp-1 at (6.0, 6.0)
    game.on_command("player_1", "tp 6.0 6.0");
    // Verify pickup is triggered
    let state_pickup = game.get_client_state("player_1").unwrap();
    println!("State logs after pickup: {:?}", state_pickup.logs);
    assert!(state_pickup.logs.iter().any(|l| l.contains("Health Orb")));
}

#[test]
fn test_dungeon_full_lifecycle() {
    let game =
        SchemeGame::new("games/dungeon.scm").expect("Failed to initialize dungeon SchemeGame");

    // 1. Join two dungeon adventurers
    game.on_join("hero_1");
    game.on_join("hero_2");

    // 2. Teleport to known positions
    game.on_command("hero_1", "tp 0 -4");
    game.on_command("hero_2", "tp 3 -4");

    // 3. Mutual visibility in dungeon (distance is 3.0, within 20.0)
    let state1 = game.get_client_state("hero_1").expect("hero_1 state");
    assert_eq!(state1.client_id, "hero_1");
    let hero2_ent = state1.entities.iter().find(|e| e.id == "player-hero_2");
    assert!(
        hero2_ent.is_some(),
        "hero_2 should be visible to hero_1 in dungeon"
    );
    assert_eq!(hero2_ent.unwrap().shape, "cube");

    // 4. Distance clipping outside 20.0
    game.on_command("hero_2", "tp 80 80");
    let state1_clipped = game.get_client_state("hero_1").expect("hero_1 state");
    let hero2_clipped = state1_clipped
        .entities
        .iter()
        .find(|e| e.id == "player-hero_2");
    assert!(
        hero2_clipped.is_none(),
        "hero_2 should be culled beyond 20.0 units"
    );

    // 5. Test Torch command
    let torch_out = game.on_command("hero_1", "torch");
    println!("torch_out: {}", torch_out);
    assert!(torch_out.contains("torch"));
    let state_torch = game.get_client_state("hero_1").unwrap();
    assert!(state_torch
        .entities
        .iter()
        .any(|e| e.label == "Glowing Torch"));

    // 6. Test Help, Stats, Heal, Speed, Say, List
    let help_out = game.on_command("hero_1", "help");
    assert!(help_out.contains("Dungeon commands"));

    let stats_out = game.on_command("hero_1", "stats");
    assert!(stats_out.contains("HP: 60"));

    let heal_out = game.on_command("hero_1", "heal");
    assert!(heal_out.contains("Health restored to 100%"));

    let spd_out = game.on_command("hero_1", "speed 1.5");
    assert!(spd_out.contains("1.5"));

    let say_out = game.on_command("hero_1", "say Darkness awaits!");
    assert!(say_out.contains("Broadcasted"));

    let list_out = game.on_command("hero_1", "list");
    assert!(list_out.contains("hero_1") && list_out.contains("hero_2"));

    // 7. Test Dungeon Core Tick Bobbing
    let core_before = state1
        .entities
        .iter()
        .find(|e| e.id == "dungeon-core")
        .unwrap();
    let y_core_before = core_before.pos[1];
    for _ in 0..20 {
        game.tick(0.05);
    }
    let state1_after_ticks = game.get_client_state("hero_1").unwrap();
    let core_after = state1_after_ticks
        .entities
        .iter()
        .find(|e| e.id == "dungeon-core")
        .unwrap();
    let y_core_after = core_after.pos[1];
    println!(
        "Dungeon core y: before = {}, after = {}",
        y_core_before, y_core_after
    );
    assert!((y_core_before - y_core_after).abs() > 0.001);

    // 8. Test pickup collection in dungeon (Health potion at 4.0, 4.0)
    game.on_command("hero_1", "tp 4.0 4.0");
    let state_pickup = game.get_client_state("hero_1").unwrap();
    println!("Hero 1 logs after potion: {:?}", state_pickup.logs);
    assert!(state_pickup
        .logs
        .iter()
        .any(|l| l.contains("Potion") || l.contains("Consumed")));
}

#[test]
fn test_movement_and_leave() {
    let game = SchemeGame::new("games/space_arena.scm").unwrap();
    game.on_join("mover_1");

    // Teleport to a known location
    game.on_command("mover_1", "tp 10 10");
    let init_state = game.get_client_state("mover_1").unwrap();
    assert_eq!(init_state.camera_target[0], 10.0);
    assert_eq!(init_state.camera_target[2], 10.0);

    // Move forward dx=2.0, dz=4.0 (speed factor = 0.25 => dx*0.25 = 0.5, dz*0.25 = 1.0)
    game.on_move("mover_1", 2.0, 4.0, 1.57);
    let moved_state = game.get_client_state("mover_1").unwrap();
    assert!((moved_state.camera_target[0] - 10.5).abs() < 1e-3);
    assert!((moved_state.camera_target[2] - 11.0).abs() < 1e-3);
    assert_eq!(moved_state.camera_yaw, 1.57);

    // Leave
    game.on_leave("mover_1");
    // After leave, player hash is removed in space_arena
    let after_leave = game.get_client_state("mover_1");
    assert!(after_leave.is_none());
}

#[test]
fn test_raw_scheme_eval_and_watchdog() {
    let game = SchemeGame::new("games/space_arena.scm").unwrap();

    // 1. Raw Scheme evaluation via on_command
    let eval_res = game.on_command("nobody", "(+ 123 456)");
    assert_eq!(eval_res, "579");

    // 2. Safe eval with watchdog protection against infinite loops
    let infinite_expr = "(letrec ((loop (lambda () (loop)))) (loop))";
    let res = game.on_command("nobody", infinite_expr);
    println!("Watchdog timeout result: {}", res);
    assert!(res.contains("Timeout") || res.contains("Interrupted") || res.contains("Scheme Error"));
}

#[test]
fn test_view_distance_constant() {
    assert_eq!(VIEW_DISTANCE, 25.0);
}

#[test]
fn test_remote_event_handling_and_emission() {
    let game = SchemeGame::new("games/space_arena.scm").unwrap();

    // Register a custom on-remote-event handler in the VM
    let register_script = r#"
(define (on-remote-event player event payload)
  (if (string=? event "ping")
      (begin
        (fire-client! player "pong" payload)
        (broadcast-client! "system_announcement" "{\"status\":\"active\"}"))
      (void)))
"#;
    game.safe_eval(register_script, 100)
        .expect("Define handler");

    // Rust triggers on_remote_event
    let payload = serde_json::json!({"seq": 42, "msg": "hello"});
    game.on_remote_event("player_1", "ping", &payload);

    // Drain outgoing events
    let outgoing = game.drain_outgoing_events();
    assert_eq!(
        outgoing.len(),
        2,
        "Expected 2 outgoing events, got: {:?}",
        outgoing
    );

    // Verify unicast fire-client!
    let (target, ev_name, ev_payload) = &outgoing[0];
    assert_eq!(target.as_deref(), Some("player_1"));
    assert_eq!(ev_name, "pong");
    assert_eq!(ev_payload["seq"], 42);
    assert_eq!(ev_payload["msg"], "hello");

    // Verify broadcast broadcast-client!
    let (target_b, ev_name_b, ev_payload_b) = &outgoing[1];
    assert!(target_b.is_none(), "Broadcast event target must be None");
    assert_eq!(ev_name_b, "system_announcement");
    assert_eq!(ev_payload_b["status"], "active");

    // Second drain must be empty
    assert!(game.drain_outgoing_events().is_empty());

    // Trigger undefined remote event - must not crash
    game.on_remote_event("player_2", "unknown_event", &serde_json::json!({}));
    assert!(game.drain_outgoing_events().is_empty());
}

#[test]
fn test_game_manager_discovery_and_default() {
    let manager = GameManager::new("games").expect("GameManager discovery failed");

    // Verify default game ID
    assert_eq!(manager.default_game_id(), "space_arena");

    // Verify discovered games
    let ids = manager.all_game_ids();
    assert!(ids.contains(&"space_arena".to_string()));
    assert!(ids.contains(&"dungeon".to_string()));

    // Verify retrieving game instances
    let space_arena = manager.get_game("space_arena");
    assert!(space_arena.is_some());

    let dungeon = manager.get_game("dungeon");
    assert!(dungeon.is_some());

    let missing = manager.get_game("nonexistent_game_xyz");
    assert!(missing.is_none());

    // Verify tick_all runs without panicking
    manager.tick_all(0.05);
}

#[test]
fn test_game_manager_directory_bundling() {
    // Create a temporary modular game directory structure
    let temp_root = std::env::temp_dir().join(format!(
        "lc_test_games_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let game_dir = temp_root.join("cyber_arena");
    let server_dir = game_dir.join("server");
    let client_dir = game_dir.join("client");
    let client_ui_dir = client_dir.join("ui");
    let shared_dir = game_dir.join("shared");

    std::fs::create_dir_all(&server_dir).unwrap();
    std::fs::create_dir_all(&client_ui_dir).unwrap();
    std::fs::create_dir_all(&shared_dir).unwrap();

    // Write server script
    std::fs::write(
        server_dir.join("main.scm"),
        r#"
(define (init-game) "Cyber Arena Initialized")
(define (on-remote-event player event payload)
  (fire-client! player "welcome_reply" payload))
"#,
    )
    .unwrap();

    // Write client scripts
    std::fs::write(
        client_dir.join("main.scm"),
        ";; Client main script\n(define client-version 2.0)",
    )
    .unwrap();
    std::fs::write(
        client_ui_dir.join("radar.scm"),
        ";; Client radar script\n(define (render-radar) #t)",
    )
    .unwrap();

    // Write shared script
    std::fs::write(
        shared_dir.join("physics.scm"),
        ";; Shared physics\n(define GRAVITY -9.8)",
    )
    .unwrap();

    // Load GameManager pointing to temp_root
    let manager = GameManager::new(temp_root.to_str().unwrap())
        .expect("Failed to initialize GameManager on temp_root");

    assert_eq!(manager.all_game_ids(), vec!["cyber_arena"]);
    assert_eq!(manager.default_game_id(), "cyber_arena");

    // Verify bundled VFS
    let bundle = manager
        .get_bundle("cyber_arena")
        .expect("cyber_arena bundle must exist");

    // Check relative paths and filenames
    assert!(
        bundle.contains_key("client/main.scm"),
        "Missing client/main.scm"
    );
    assert!(bundle.contains_key("main.scm"), "Missing main.scm alias");
    assert!(
        bundle.contains_key("client/ui/radar.scm"),
        "Missing client/ui/radar.scm"
    );
    assert!(bundle.contains_key("radar.scm"), "Missing radar.scm alias");
    assert!(
        bundle.contains_key("shared/physics.scm"),
        "Missing shared/physics.scm"
    );
    assert!(
        bundle.contains_key("physics.scm"),
        "Missing physics.scm alias"
    );

    // Verify contents
    assert!(bundle
        .get("client/main.scm")
        .unwrap()
        .contains("client-version 2.0"));
    assert!(bundle
        .get("shared/physics.scm")
        .unwrap()
        .contains("GRAVITY -9.8"));

    // Verify get_bundle_event returns LispEvent::BundleScript
    let bundle_event = manager
        .get_bundle_event("cyber_arena")
        .expect("Bundle event must be Some");
    if let lc_core::LispEvent::BundleScript { game_id, vfs } = bundle_event {
        assert_eq!(game_id, "cyber_arena");
        assert_eq!(vfs.len(), bundle.len());
        assert_eq!(vfs.get("client/main.scm"), bundle.get("client/main.scm"));
    } else {
        panic!("Expected BundleScript event");
    }

    // Verify game instance execution
    let game = manager.get_game("cyber_arena").unwrap();
    game.on_remote_event("neon_rider", "connect", &serde_json::json!({"level": 99}));
    let outgoing = game.drain_outgoing_events();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].0.as_deref(), Some("neon_rider"));
    assert_eq!(outgoing[0].1, "welcome_reply");
    assert_eq!(outgoing[0].2["level"], 99);

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn test_multi_game_isolation() {
    let manager = GameManager::new("games").unwrap();
    let arena = manager.get_game("space_arena").unwrap();
    let dungeon = manager.get_game("dungeon").unwrap();

    // Join player in space_arena
    arena.on_join("astro_bob");
    assert!(arena.get_client_state("astro_bob").is_some());

    // astro_bob should NOT exist in dungeon
    assert!(dungeon.get_client_state("astro_bob").is_none());

    // Join adventurer in dungeon
    dungeon.on_join("knight_alice");
    assert!(dungeon.get_client_state("knight_alice").is_some());

    // knight_alice should NOT exist in space_arena
    assert!(arena.get_client_state("knight_alice").is_none());

    // Commands in arena should not affect dungeon
    arena.on_command("astro_bob", "say Ground control to Major Tom");
    let arena_state = arena.get_client_state("astro_bob").unwrap();
    assert!(arena_state.logs.iter().any(|l| l.contains("Major Tom")));

    let dungeon_state = dungeon.get_client_state("knight_alice").unwrap();
    assert!(!dungeon_state.logs.iter().any(|l| l.contains("Major Tom")));
}
