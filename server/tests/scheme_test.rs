use shared::ClientState;
use std::sync::atomic::{AtomicU64, Ordering};
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;

static SEED: AtomicU64 = AtomicU64::new(88172645463325252);
fn prng() -> f64 {
    let mut x = SEED.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, Ordering::Relaxed);
    (x as f64) / (u64::MAX as f64)
}

fn create_engine_with_script(path: &str) -> Engine {
    let script = std::fs::read_to_string(path)
        .or_else(|_| std::fs::read_to_string(format!("../{}", path)))
        .expect("Cannot read script");
    let mut engine = Engine::new();
    engine.register_fn("random", prng);
    engine.run(script).expect("Failed to load script into Steel");
    engine.run("(init-game)".to_string()).expect("Failed to init game");
    engine
}

fn get_state(engine: &mut Engine, client_id: &str) -> ClientState {
    let code = format!(r#"(get-client-state-json "{}")"#, client_id);
    let res = engine.run(code).expect("get-client-state-json failed");
    let val = res.first().expect("empty result");
    let json_str = match val {
        steel::SteelVal::StringV(s) => s.to_string(),
        other => other.to_string().trim_matches('"').to_string(),
    };
    serde_json::from_str::<ClientState>(&json_str)
        .unwrap_or_else(|e| panic!("Failed to parse JSON: {}\nErr: {:?}", json_str, e))
}

fn run_command(engine: &mut Engine, client_id: &str, cmd: &str) -> String {
    let code = format!(r#"(on-player-command "{}" "{}")"#, client_id, cmd);
    let res = engine.run(code).expect("on-player-command failed");
    let val = res.first().expect("empty result");
    match val {
        steel::SteelVal::StringV(s) => s.to_string(),
        other => other.to_string(),
    }
}

#[test]
fn test_space_arena_full_lifecycle() {
    let mut engine = create_engine_with_script("games/space_arena.scm");

    // 1. Join two players
    engine.run(r#"(on-player-join "player_1")"#.to_string()).unwrap();
    engine.run(r#"(on-player-join "player_2")"#.to_string()).unwrap();

    // 2. Both players teleport to specific known locations
    let tp1 = run_command(&mut engine, "player_1", "tp 0 0");
    println!("tp1: {}", tp1);
    assert!(tp1.contains("Teleported"));

    let tp2 = run_command(&mut engine, "player_2", "tp 10 0");
    println!("tp2: {}", tp2);
    assert!(tp2.contains("Teleported"));

    // 3. Test mutual visibility: player_1 is at (0, 0), player_2 is at (10, 0).
    // Distance is 10.0, which is <= 25.0 view distance!
    let state1 = get_state(&mut engine, "player_1");
    assert_eq!(state1.client_id, "player_1");
    // Verify player_2 is in player_1's visible entities
    let p2_ent = state1.entities.iter().find(|e| e.id == "player-player_2");
    assert!(p2_ent.is_some(), "player_2 should be visible to player_1");
    let p2_desc = p2_ent.unwrap();
    assert_eq!(p2_desc.shape, "cube");
    assert_eq!(p2_desc.label, "player_2");

    // Also verify player_1 is visible to player_2
    let state2 = get_state(&mut engine, "player_2");
    let p1_ent = state2.entities.iter().find(|e| e.id == "player-player_1");
    assert!(p1_ent.is_some(), "player_1 should be visible to player_2");

    // 4. Test distance clipping: Move player_2 far away (e.g. tp 100 100)
    run_command(&mut engine, "player_2", "tp 100 100");
    let state1_clipped = get_state(&mut engine, "player_1");
    let p2_clipped = state1_clipped.entities.iter().find(|e| e.id == "player-player_2");
    assert!(p2_clipped.is_none(), "player_2 should NOT be visible when outside 25.0 units");

    // 5. Test Commands
    // 5.1 help
    let help_out = run_command(&mut engine, "player_1", "help");
    assert!(help_out.contains("Available commands"));

    // 5.2 stats
    let stats_out = run_command(&mut engine, "player_1", "stats");
    println!("stats: {}", stats_out);
    assert!(stats_out.contains("HP: 100"));

    // 5.3 speed
    let spd_out = run_command(&mut engine, "player_1", "speed 2.0");
    println!("spd_out: {}", spd_out);
    assert!(spd_out.contains("2"));
    let state_after_speed = get_state(&mut engine, "player_1");
    println!("state_after_speed logs: {:?}", state_after_speed.logs);
    assert!(state_after_speed.logs.iter().any(|l| l.to_lowercase().contains("speed multiplier")));

    // 5.4 spawn
    let spawn_out = run_command(&mut engine, "player_1", "spawn cube StarBase");
    println!("spawn: {}", spawn_out);
    assert!(spawn_out.contains("Spawned cube [StarBase]"));
    let state_after_spawn = get_state(&mut engine, "player_1");
    assert!(state_after_spawn.entities.iter().any(|e| e.label == "StarBase"));

    // 5.5 say
    let say_out = run_command(&mut engine, "player_1", "say Greetings galaxy!");
    assert!(say_out.contains("Broadcasted"));
    let state2_after_say = get_state(&mut engine, "player_2");
    assert!(state2_after_say.logs.iter().any(|l| l.contains("Greetings galaxy!")));

    // 5.6 list
    let list_out = run_command(&mut engine, "player_1", "list");
    println!("list: {}", list_out);
    assert!(list_out.contains("player_1") && list_out.contains("player_2"));

    // 5.7 heal
    let heal_out = run_command(&mut engine, "player_1", "heal");
    assert!(heal_out.contains("Health restored to 100%"));

    // 6. Test game-tick bobbing
    let st_before = get_state(&mut engine, "player_1");
    let beacon_before = st_before.entities.iter().find(|e| e.id == "beacon-center").unwrap();
    let y_before = beacon_before.pos[1];

    for _ in 0..20 {
        engine.run("(game-tick 0.05)".to_string()).unwrap();
    }
    let st_after = get_state(&mut engine, "player_1");
    let beacon_after = st_after.entities.iter().find(|e| e.id == "beacon-center").unwrap();
    let y_after = beacon_after.pos[1];
    println!("Beacon y: before = {}, after = {}", y_before, y_after);
    assert!((y_before - y_after).abs() > 0.001, "Beacon y should bob over time");

    // 7. Test interactive pickups
    // Teleport player_1 close to pickup-hp-1 at (6.0, 6.0)
    run_command(&mut engine, "player_1", "tp 6.0 6.0");
    // Verify pickup is triggered
    let state_pickup = get_state(&mut engine, "player_1");
    println!("State logs after pickup: {:?}", state_pickup.logs);
    assert!(state_pickup.logs.iter().any(|l| l.contains("Health Orb")));
}

#[test]
fn test_dungeon_full_lifecycle() {
    let mut engine = create_engine_with_script("games/dungeon.scm");

    // 1. Join two dungeon adventurers
    engine.run(r#"(on-player-join "hero_1")"#.to_string()).unwrap();
    engine.run(r#"(on-player-join "hero_2")"#.to_string()).unwrap();

    // 2. Teleport to known positions
    run_command(&mut engine, "hero_1", "tp 0 -4");
    run_command(&mut engine, "hero_2", "tp 3 -4");

    // 3. Mutual visibility in dungeon (distance is 3.0, within 20.0)
    let state1 = get_state(&mut engine, "hero_1");
    assert_eq!(state1.client_id, "hero_1");
    let hero2_ent = state1.entities.iter().find(|e| e.id == "player-hero_2");
    assert!(hero2_ent.is_some(), "hero_2 should be visible to hero_1 in dungeon");
    assert_eq!(hero2_ent.unwrap().shape, "cube");

    // 4. Distance clipping outside 20.0
    run_command(&mut engine, "hero_2", "tp 80 80");
    let state1_clipped = get_state(&mut engine, "hero_1");
    let hero2_clipped = state1_clipped.entities.iter().find(|e| e.id == "player-hero_2");
    assert!(hero2_clipped.is_none(), "hero_2 should be culled beyond 20.0 units");

    // 5. Test Torch command
    let torch_out = run_command(&mut engine, "hero_1", "torch");
    println!("torch_out: {}", torch_out);
    assert!(torch_out.contains("torch"));
    let state_torch = get_state(&mut engine, "hero_1");
    assert!(state_torch.entities.iter().any(|e| e.label == "Glowing Torch"));

    // 6. Test Help, Stats, Heal, Speed, Say, List
    let help_out = run_command(&mut engine, "hero_1", "help");
    assert!(help_out.contains("Dungeon commands"));

    let stats_out = run_command(&mut engine, "hero_1", "stats");
    assert!(stats_out.contains("HP: 60"));

    let heal_out = run_command(&mut engine, "hero_1", "heal");
    assert!(heal_out.contains("Health restored to 100%"));

    let spd_out = run_command(&mut engine, "hero_1", "speed 1.5");
    assert!(spd_out.contains("1.5"));

    let say_out = run_command(&mut engine, "hero_1", "say Darkness awaits!");
    assert!(say_out.contains("Broadcasted"));

    let list_out = run_command(&mut engine, "hero_1", "list");
    assert!(list_out.contains("hero_1") && list_out.contains("hero_2"));

    // 7. Test Dungeon Core Tick Bobbing
    let core_before = state1.entities.iter().find(|e| e.id == "dungeon-core").unwrap();
    let y_core_before = core_before.pos[1];
    for _ in 0..20 {
        engine.run("(game-tick 0.05)".to_string()).unwrap();
    }
    let state1_after_ticks = get_state(&mut engine, "hero_1");
    let core_after = state1_after_ticks.entities.iter().find(|e| e.id == "dungeon-core").unwrap();
    let y_core_after = core_after.pos[1];
    println!("Dungeon core y: before = {}, after = {}", y_core_before, y_core_after);
    assert!((y_core_before - y_core_after).abs() > 0.001);

    // 8. Test pickup collection in dungeon (Health potion at 4.0, 4.0)
    run_command(&mut engine, "hero_1", "tp 4.0 4.0");
    let state_pickup = get_state(&mut engine, "hero_1");
    println!("Hero 1 logs after potion: {:?}", state_pickup.logs);
    assert!(state_pickup.logs.iter().any(|l| l.contains("Potion") || l.contains("Consumed")));
}
