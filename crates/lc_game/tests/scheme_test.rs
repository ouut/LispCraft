use lc_game::{SchemeGame, VIEW_DISTANCE};

#[test]
fn test_space_arena_full_lifecycle() {
    let game = SchemeGame::new("games/space_arena.scm").expect("Failed to initialize space arena SchemeGame");

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
    let state1 = game.get_client_state("player_1").expect("Failed to get state for player_1");
    assert_eq!(state1.client_id, "player_1");
    // Verify player_2 is in player_1's visible entities
    let p2_ent = state1.entities.iter().find(|e| e.id == "player-player_2");
    assert!(p2_ent.is_some(), "player_2 should be visible to player_1");
    let p2_desc = p2_ent.unwrap();
    assert_eq!(p2_desc.shape, "cube");
    assert_eq!(p2_desc.label, "player_2");

    // Also verify player_1 is visible to player_2
    let state2 = game.get_client_state("player_2").expect("Failed to get state for player_2");
    let p1_ent = state2.entities.iter().find(|e| e.id == "player-player_1");
    assert!(p1_ent.is_some(), "player_1 should be visible to player_2");

    // 4. Test distance clipping: Move player_2 far away (e.g. tp 100 100)
    game.on_command("player_2", "tp 100 100");
    let state1_clipped = game.get_client_state("player_1").expect("Failed to get state for player_1");
    let p2_clipped = state1_clipped.entities.iter().find(|e| e.id == "player-player_2");
    assert!(p2_clipped.is_none(), "player_2 should NOT be visible when outside 25.0 units");

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
    assert!(state_after_speed.logs.iter().any(|l| l.to_lowercase().contains("speed multiplier")));

    // 5.4 spawn
    let spawn_out = game.on_command("player_1", "spawn cube StarBase");
    println!("spawn: {}", spawn_out);
    assert!(spawn_out.contains("Spawned cube [StarBase]"));
    let state_after_spawn = game.get_client_state("player_1").unwrap();
    assert!(state_after_spawn.entities.iter().any(|e| e.label == "StarBase"));

    // 5.5 say
    let say_out = game.on_command("player_1", "say Greetings galaxy!");
    assert!(say_out.contains("Broadcasted"));
    let state2_after_say = game.get_client_state("player_2").unwrap();
    assert!(state2_after_say.logs.iter().any(|l| l.contains("Greetings galaxy!")));

    // 5.6 list
    let list_out = game.on_command("player_1", "list");
    println!("list: {}", list_out);
    assert!(list_out.contains("player_1") && list_out.contains("player_2"));

    // 5.7 heal
    let heal_out = game.on_command("player_1", "heal");
    assert!(heal_out.contains("Health restored to 100%"));

    // 6. Test game-tick bobbing
    let st_before = game.get_client_state("player_1").unwrap();
    let beacon_before = st_before.entities.iter().find(|e| e.id == "beacon-center").unwrap();
    let y_before = beacon_before.pos[1];

    for _ in 0..20 {
        game.tick(0.05);
    }
    let st_after = game.get_client_state("player_1").unwrap();
    let beacon_after = st_after.entities.iter().find(|e| e.id == "beacon-center").unwrap();
    let y_after = beacon_after.pos[1];
    println!("Beacon y: before = {}, after = {}", y_before, y_after);
    assert!((y_before - y_after).abs() > 0.001, "Beacon y should bob over time");

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
    let game = SchemeGame::new("games/dungeon.scm").expect("Failed to initialize dungeon SchemeGame");

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
    assert!(hero2_ent.is_some(), "hero_2 should be visible to hero_1 in dungeon");
    assert_eq!(hero2_ent.unwrap().shape, "cube");

    // 4. Distance clipping outside 20.0
    game.on_command("hero_2", "tp 80 80");
    let state1_clipped = game.get_client_state("hero_1").expect("hero_1 state");
    let hero2_clipped = state1_clipped.entities.iter().find(|e| e.id == "player-hero_2");
    assert!(hero2_clipped.is_none(), "hero_2 should be culled beyond 20.0 units");

    // 5. Test Torch command
    let torch_out = game.on_command("hero_1", "torch");
    println!("torch_out: {}", torch_out);
    assert!(torch_out.contains("torch"));
    let state_torch = game.get_client_state("hero_1").unwrap();
    assert!(state_torch.entities.iter().any(|e| e.label == "Glowing Torch"));

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
    let core_before = state1.entities.iter().find(|e| e.id == "dungeon-core").unwrap();
    let y_core_before = core_before.pos[1];
    for _ in 0..20 {
        game.tick(0.05);
    }
    let state1_after_ticks = game.get_client_state("hero_1").unwrap();
    let core_after = state1_after_ticks.entities.iter().find(|e| e.id == "dungeon-core").unwrap();
    let y_core_after = core_after.pos[1];
    println!("Dungeon core y: before = {}, after = {}", y_core_before, y_core_after);
    assert!((y_core_before - y_core_after).abs() > 0.001);

    // 8. Test pickup collection in dungeon (Health potion at 4.0, 4.0)
    game.on_command("hero_1", "tp 4.0 4.0");
    let state_pickup = game.get_client_state("hero_1").unwrap();
    println!("Hero 1 logs after potion: {:?}", state_pickup.logs);
    assert!(state_pickup.logs.iter().any(|l| l.contains("Potion") || l.contains("Consumed")));
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
