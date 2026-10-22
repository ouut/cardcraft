use cardcraft::buffer::VirtualScreen;
use cardcraft::cad::CommandState;
use cardcraft::engine::GameEngine;

#[test]
fn test_initial_state() {
    let mut engine = GameEngine::new().expect("Failed to create engine");
    let snapshot = engine.get_snapshot().expect("Failed to get snapshot");

    assert_eq!(snapshot.meta.moon, 1);
    assert_eq!(snapshot.meta.gold, 5);
    assert_eq!(snapshot.meta.status, "RUNNING");
    assert_eq!(snapshot.meta.card_count, 5);
    assert_eq!(snapshot.meta.food_required, 1); // 1 Villager
    assert_eq!(snapshot.meta.food_current, 1);  // 1 Berry
    assert_eq!(snapshot.stacks.len(), 5);
}

#[test]
fn test_cad_stack_and_crafting_loop() {
    let mut engine = GameEngine::new().unwrap();

    // Stack Villager (1) onto BerryBush (2)
    let fb = engine.execute_command("s 1 2").unwrap();
    assert_eq!(fb, "OK");

    let snap = engine.get_snapshot().unwrap();
    let s2 = snap.stacks.iter().find(|s| s.id == 2).expect("Stack 2 should exist");
    assert_eq!(s2.recipe_name, Some("Harvest Berry".to_string()));
    assert_eq!(s2.status, "Crafting");

    // Tick 2 seconds (halfway through 4s recipe)
    engine.tick(2.0).unwrap();
    let snap2 = engine.get_snapshot().unwrap();
    let s2_ticked = snap2.stacks.iter().find(|s| s.id == 2).unwrap();
    let prog = s2_ticked.progress.expect("Progress should be present");
    assert!(prog >= 0.49 && prog <= 0.51, "Progress was: {}", prog);

    // Tick 2.5 more seconds (finish recipe)
    engine.tick(2.5).unwrap();
    let snap3 = engine.get_snapshot().unwrap();
    // A new Berry card should have spawned!
    let total_berries = snap3.stacks.iter()
        .flat_map(|s| s.cards.iter())
        .filter(|c| *c == "Berry")
        .count();
    assert_eq!(total_berries, 2, "Expected 2 berries on board (1 starting + 1 harvested)");
}

#[test]
fn test_cad_unstack() {
    let mut engine = GameEngine::new().unwrap();

    // First stack 1 onto 2 so stack 2 has 2 cards
    engine.execute_command("s 1 2").unwrap();
    let snap1 = engine.get_snapshot().unwrap();
    let s2 = snap1.stacks.iter().find(|s| s.id == 2).unwrap();
    assert_eq!(s2.cards.len(), 2);

    // Unstack stack 2
    let fb = engine.execute_command("u 2").unwrap();
    assert_eq!(fb, "OK");

    let snap2 = engine.get_snapshot().unwrap();
    let s2_after = snap2.stacks.iter().find(|s| s.id == 2).unwrap();
    assert_eq!(s2_after.cards.len(), 1);

    // Check that popped card exists in a new stack
    assert_eq!(snap2.stacks.len(), 5);
}

#[test]
fn test_cad_move() {
    let mut engine = GameEngine::new().unwrap();

    // Move stack 3 to empty slot 99
    let fb = engine.execute_command("m 3 99").unwrap();
    assert_eq!(fb, "OK");

    let snap = engine.get_snapshot().unwrap();
    assert!(snap.stacks.iter().any(|s| s.id == 99));
    assert!(!snap.stacks.iter().any(|s| s.id == 3));
}

#[test]
fn test_cad_buy_and_open_pack() {
    let mut engine = GameEngine::new().unwrap();

    // Starting gold: 5G. Basic pack costs 3G.
    let fb = engine.execute_command("b basic").unwrap();
    assert_eq!(fb, "OK");

    let snap = engine.get_snapshot().unwrap();
    assert_eq!(snap.meta.gold, 2);

    let pack_stack = snap.stacks.iter().find(|s| s.cards.contains(&"Pack:Basic".to_string()))
        .expect("Pack:Basic should be on board");
    let pack_id = pack_stack.id;

    // Open the pack
    let open_cmd = format!("o {}", pack_id);
    let fb_open = engine.execute_command(&open_cmd).unwrap();
    assert_eq!(fb_open, "OK");

    let snap_after = engine.get_snapshot().unwrap();
    // 3 cards dropped: Wood, Stone, Berry
    assert!(snap_after.logs.iter().any(|l| l.contains("Opened Basic Pack")));
}

#[test]
fn test_cad_sell_card() {
    let mut engine = GameEngine::new().unwrap();

    // Stack 4 contains "Wood" (val: 1G). Starting gold: 5G.
    let fb = engine.execute_command("x 4").unwrap();
    assert_eq!(fb, "OK");

    let snap = engine.get_snapshot().unwrap();
    assert_eq!(snap.meta.gold, 6);
    assert!(!snap.stacks.iter().any(|s| s.id == 4));
}

#[test]
fn test_cad_pause_and_wait() {
    let mut engine = GameEngine::new().unwrap();

    // Toggle pause
    let fb = engine.execute_command("p").unwrap();
    assert_eq!(fb, "PAUSED");

    let snap = engine.get_snapshot().unwrap();
    assert_eq!(snap.meta.status, "PAUSED");

    // Ticking while paused should not reduce day time
    let time_left = snap.meta.day_seconds_left;
    engine.tick(5.0).unwrap();
    let snap2 = engine.get_snapshot().unwrap();
    assert_eq!(snap2.meta.day_seconds_left, time_left);

    // Unpause
    let fb2 = engine.execute_command("p").unwrap();
    assert_eq!(fb2, "RUNNING");

    // Fast forward with wait command
    engine.execute_command("w 4").unwrap();
    let snap3 = engine.get_snapshot().unwrap();
    assert!(snap3.meta.day_seconds_left < time_left);
}

#[test]
fn test_moon_cycle_and_food_consumption() {
    let mut engine = GameEngine::new().unwrap();

    // Day duration is 30.0s. Let's wait 31 seconds to trigger Moon 2.
    engine.execute_command("w 31").unwrap();

    let snap = engine.get_snapshot().unwrap();
    assert_eq!(snap.meta.moon, 2);
    // Villager consumed 1 food (the starting Berry)
    assert!(snap.logs.iter().any(|l| l.contains("Moon ended! Consumed 1 food.")));
}

#[test]
fn test_headless_virtual_buffer_rendering() {
    let mut engine = GameEngine::new().unwrap();
    let mut screen = VirtualScreen::new(80, 24).unwrap();
    let mut cmd_state = CommandState::new();

    cmd_state.insert_char('s');
    cmd_state.insert_char(' ');
    cmd_state.insert_char('1');
    cmd_state.insert_char(' ');
    cmd_state.insert_char('2');
    cmd_state.set_feedback("Ready".to_string());

    let snap = engine.get_snapshot().unwrap();
    screen.render(&snap, &cmd_state).unwrap();

    let output = screen.to_string();
    assert!(output.contains("CardCraft"));
    assert!(output.contains("MOON 1"));
    assert!(output.contains("Villager"));
    assert!(output.contains("BerryBush"));
    assert!(output.contains("Command: s 1 2"));
    assert!(output.contains("[Ready]"));
}

#[test]
fn test_lisp_god_mode() {
    let mut engine = GameEngine::new().unwrap();

    // Directly invoke Scheme to spawn an Apple and add 100 gold
    let fb = engine.execute_command("(spawn-card! \"Apple\")").unwrap();
    assert!(fb.contains("void") || !fb.is_empty());

    let snap = engine.get_snapshot().unwrap();
    let apple_found = snap.stacks.iter().any(|s| s.cards.contains(&"Apple".to_string()));
    assert!(apple_found, "Apple should have been spawned directly by Scheme!");
}

#[test]
fn test_rust_interfaces_from_scheme() {
    let mut engine = GameEngine::new().unwrap();

    // 1. Test rust-random-int
    let res = engine.execute_command("(rust-random-int 10 20)").unwrap();
    let n: i64 = res.parse().expect("Expected integer from rust-random-int");
    assert!(n >= 10 && n <= 20, "Number was: {}", n);

    // 2. Test rust-millis
    let res_time = engine.execute_command("(rust-millis)").unwrap();
    let t: i64 = res_time.parse().expect("Expected timestamp from rust-millis");
    assert!(t > 0);

    // 3. Test rust-write-file and rust-read-file
    let tmp_path = "/tmp/cardcraft_test_save.txt";
    let cmd_write = format!("(rust-write-file \"{}\" \"CardCraft Save Data 123\")", tmp_path);
    let fb_write = engine.execute_command(&cmd_write).unwrap();
    assert_eq!(fb_write, "#true");

    let cmd_read = format!("(rust-read-file \"{}\")", tmp_path);
    let fb_read = engine.execute_command(&cmd_read).unwrap();
    assert!(fb_read.contains("CardCraft Save Data 123"));
}
