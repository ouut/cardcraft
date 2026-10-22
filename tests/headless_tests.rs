use cardcraft::buffer::VirtualScreen;
use cardcraft::cad::CommandState;
use cardcraft::engine::{GameEngine, NetEvent};
use cardcraft::p2p::{ticket_to_addr, P2pClient, P2pHost};
use cardcraft::server::{run_tcp_server, run_web_server, spawn_engine_actor};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::protocol::Message;

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

    // 3. Test system-file-write and system-file-read in sandboxed saves/
    let save_file = "test_save.txt";
    let cmd_write = format!("(system-file-write \"{}\" \"CardCraft Save Data 123\")", save_file);
    let fb_write = engine.execute_command(&cmd_write).unwrap();
    assert_eq!(fb_write, "#true");

    let cmd_read = format!("(system-file-read \"{}\")", save_file);
    let fb_read = engine.execute_command(&cmd_read).unwrap();
    assert!(fb_read.contains("CardCraft Save Data 123"));

    // 4. Test sandbox traversal prevention (writing outside sandbox is rejected)
    let cmd_escape = "(system-file-write \"../../escape_hack.txt\" \"pwn\")";
    let fb_escape = engine.execute_command(cmd_escape).unwrap();
    assert_eq!(fb_escape, "#false");
}

#[test]
fn test_network_primitives() {
    let mut engine = GameEngine::new().unwrap();

    // 1. Test system-time-now
    let time_str = engine.execute_command("(system-time-now)").unwrap();
    let time_val: f64 = time_str.parse().expect("Expected float from system-time-now");
    assert!(time_val > 1000000.0, "Timestamp was: {}", time_val);

    // 2. Test net-send!
    engine.execute_command("(net-send! 100 \"Hello Client 100\")").unwrap();
    let events = engine.drain_net_events();
    assert_eq!(events, vec![
        NetEvent::Send { client_id: 100, message: "Hello Client 100".to_string() }
    ]);

    // 3. Test net-broadcast!
    engine.execute_command("(net-broadcast! \"Server rebooting\")").unwrap();
    let events = engine.drain_net_events();
    assert_eq!(events, vec![
        NetEvent::Broadcast { message: "Server rebooting".to_string() }
    ]);

    // 4. Test net-disconnect!
    engine.execute_command("(net-disconnect! 100)").unwrap();
    let events = engine.drain_net_events();
    assert_eq!(events, vec![
        NetEvent::Disconnect { client_id: 100 }
    ]);
}

#[test]
fn test_network_hooks_and_chat_and_whisper() {
    let mut engine = GameEngine::new().unwrap();

    // 1. Client 1 connects
    engine.net_connect(1).unwrap();
    let ev1 = engine.drain_net_events();
    assert!(ev1.iter().any(|e| matches!(e, NetEvent::Send { client_id: 1, message } if message.contains("Welcome"))));
    assert!(ev1.iter().any(|e| matches!(e, NetEvent::Broadcast { message } if message.contains("Player_1 connected"))));

    // 2. Client 2 connects
    engine.net_connect(2).unwrap();
    let ev2 = engine.drain_net_events();
    assert!(ev2.iter().any(|e| matches!(e, NetEvent::Send { client_id: 2, message } if message.contains("Welcome"))));
    assert!(ev2.iter().any(|e| matches!(e, NetEvent::Broadcast { message } if message.contains("Player_2 connected"))));

    // 3. Client 1 and Client 2 set nicknames
    engine.net_message(1, "nick Alice").unwrap();
    let ev_nick1 = engine.drain_net_events();
    assert!(ev_nick1.iter().any(|e| matches!(e, NetEvent::Broadcast { message } if message.contains("Alice"))));

    engine.net_message(2, "nick Bob").unwrap();
    let ev_nick2 = engine.drain_net_events();
    assert!(ev_nick2.iter().any(|e| matches!(e, NetEvent::Broadcast { message } if message.contains("Bob"))));

    // 4. Client 1 speaks in public chat
    engine.net_message(1, "say Good morning villagers!").unwrap();
    let ev_chat = engine.drain_net_events();
    assert_eq!(ev_chat, vec![
        NetEvent::Broadcast { message: "[Alice]: Good morning villagers!".to_string() }
    ]);

    // 5. Client 1 sends private message (whisper) to Bob
    engine.net_message(1, "w Bob Secret trade offer").unwrap();
    let ev_whisper = engine.drain_net_events();
    assert_eq!(ev_whisper, vec![
        NetEvent::Send { client_id: 2, message: "[Private from Alice]: Secret trade offer".to_string() },
        NetEvent::Send { client_id: 1, message: "[Private to Bob]: Secret trade offer".to_string() }
    ]);

    // 6. Client 2 queries who is online
    engine.net_message(2, "who").unwrap();
    let ev_who = engine.drain_net_events();
    assert!(ev_who.iter().any(|e| matches!(e, NetEvent::Send { client_id: 2, message } if message.contains("Alice") && message.contains("Bob"))));

    // 7. Client 2 executes CAD stack command through network
    engine.net_message(2, "s 1 2").unwrap();
    let ev_cad = engine.drain_net_events();
    assert!(ev_cad.iter().any(|e| matches!(e, NetEvent::Send { client_id: 2, message } if message.contains("OK"))));

    // Verify stack 2 is now Crafting
    let snap = engine.get_snapshot().unwrap();
    let s2 = snap.stacks.iter().find(|s| s.id == 2).unwrap();
    assert_eq!(s2.status, "Crafting");

    // 8. Client 1 disconnects
    engine.net_disconnect(1).unwrap();
    let ev_dc = engine.drain_net_events();
    assert!(ev_dc.iter().any(|e| matches!(e, NetEvent::Broadcast { message } if message.contains("Alice disconnected"))));
}

#[tokio::test]
async fn test_p2p_ticket_serialization_roundtrip() {
    let host = P2pHost::bind().await.expect("Failed to bind P2P Host");
    let ticket = &host.ticket;
    assert!(ticket.starts_with("cardcraft://"));

    let parsed_addr = ticket_to_addr(ticket).expect("Failed to parse ticket");
    assert_eq!(host.addr.id, parsed_addr.id);
}

#[tokio::test]
async fn test_iroh_p2p_connection_and_game_messaging() {
    let host = P2pHost::bind().await.expect("Host bind failed");
    let ticket = host.ticket.clone();

    // Client task: P2pClient is Send + Sync, so it runs cleanly in tokio::spawn
    let client_task = tokio::spawn(async move {
        let client = P2pClient::new().await.expect("Client init failed");
        let conn = client.connect_to_ticket(&ticket).await.expect("Client connect failed");
        let (mut send_stream, recv_stream) = conn.open_bi().await.expect("open_bi failed");
        
        // Send CAD/Chat message
        send_stream.write_all(b"say Hello P2P World\n").await.unwrap();
        send_stream.flush().await.unwrap();

        // Read Host reply
        let mut reader = BufReader::new(recv_stream);
        let mut reply = String::new();
        reader.read_line(&mut reply).await.unwrap();
        reply
    });

    // Host runs on the local thread with GameEngine
    let mut engine = GameEngine::new().unwrap();
    let incoming = host.endpoint.accept().await.expect("Expected connection");
    let conn = incoming.await.expect("Host connection handshake failed");
    
    let (mut send_stream, recv_stream) = conn.accept_bi().await.expect("accept_bi failed");
    let mut reader = BufReader::new(recv_stream);
    let mut line = String::new();
    
    // Register peer as Client 2
    engine.net_connect(2).unwrap();
    
    // Read command from P2P peer
    reader.read_line(&mut line).await.expect("Read line failed");
    let trimmed = line.trim();
    assert_eq!(trimmed, "say Hello P2P World");
    
    // Execute in Scheme engine
    engine.net_message(2, trimmed).unwrap();
    let events = engine.drain_net_events();
    
    // Reply back to peer
    let reply = format!("HOST_ACK: {}\n", events.len());
    send_stream.write_all(reply.as_bytes()).await.unwrap();
    send_stream.flush().await.unwrap();

    let client_reply = client_task.await.unwrap();
    assert!(client_reply.contains("HOST_ACK"));
}

#[test]
fn test_game_dir_loading_and_sandboxing() {
    let custom_dir = std::path::PathBuf::from("/tmp/test_cardcraft_dungeon");
    std::fs::create_dir_all(&custom_dir).unwrap();

    // Write a custom game core.scm
    let custom_script = r#"
(define *game-title* "DungeonQuest")
(define (game-start!) "Dungeon started!")
(define (game-update! dt) "Ticked")
(define (game-input . rest) "OK")
(define (game-render) "{}")
"#;
    std::fs::write(custom_dir.join("core.scm"), custom_script).unwrap();

    // Initialize engine with custom game dir
    let mut engine = GameEngine::with_game_dir(&custom_dir).expect("Failed to load custom game");
    assert_eq!(engine.game_dir(), custom_dir.as_path());
    assert_eq!(engine.sandbox_dir(), custom_dir.join("saves").as_path());

    // Verify custom game evaluated
    let res = engine.eval_raw("*game-title*").unwrap();
    assert_eq!(res, "DungeonQuest");

    // Verify file write inside custom game saves/
    let write_res = engine.execute_command("(system-file-write \"hero_slot.json\" \"hero data\")").unwrap();
    assert_eq!(write_res, "#true");
    assert!(custom_dir.join("saves/hero_slot.json").exists());

    let content = std::fs::read_to_string(custom_dir.join("saves/hero_slot.json")).unwrap();
    assert_eq!(content, "hero data");

    // Clean up
    std::fs::remove_dir_all(&custom_dir).ok();
}

#[tokio::test]
async fn test_tcp_server_connection_and_command() {
    let local = tokio::task::LocalSet::new();

    local.run_until(async {
        let engine = GameEngine::new().unwrap();
        let (handle, engine_task) = spawn_engine_actor(engine);

        // Pick a free local port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let h_server = handle.clone();
        tokio::spawn(async move {
            run_tcp_server(h_server, port).await.ok();
        });

        // Wait for server to bind
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect via TCP
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await.expect("TCP connect failed");
        
        // Send CAD command over TCP
        stream.write_all(b"say Hello from Netcat\n").await.unwrap();
        stream.flush().await.unwrap();

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader.read_line(&mut response).await.unwrap();
        assert!(response.contains("OK"));

        drop(handle);
        engine_task.abort();
    }).await;
}

#[tokio::test]
async fn test_web_server_static_file_and_websocket() {
    let local = tokio::task::LocalSet::new();

    local.run_until(async {
        let engine = GameEngine::new().unwrap();
        let (handle, engine_task) = spawn_engine_actor(engine);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let h_server = handle.clone();
        let web_dir = std::path::PathBuf::from("web");
        tokio::spawn(async move {
            run_web_server(h_server, port, web_dir).await.ok();
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 1. Test Static File Server (HTTP GET /)
        let mut http_stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
        http_stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.unwrap();
        http_stream.flush().await.unwrap();

        let mut http_resp = Vec::new();
        http_stream.read_to_end(&mut http_resp).await.unwrap();
        let http_str = String::from_utf8_lossy(&http_resp);
        assert!(http_str.contains("HTTP/1.1 200 OK"));
        assert!(http_str.contains("CardCraft Web Console"));

        // 2. Test WebSocket connection on the SAME port!
        let ws_url = format!("ws://127.0.0.1:{}", port);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.expect("WS connect failed");

        // Read initial snapshot
        let init_msg = ws_stream.next().await.unwrap().unwrap();
        if let Message::Text(txt) = init_msg {
            assert!(txt.contains("moon"));
            assert!(txt.contains("stacks"));
        } else {
            panic!("Expected text snapshot message");
        }

        // Send command over WebSocket
        ws_stream.send(Message::Text("s 1 2".into())).await.unwrap();

        // Read response
        let resp_msg = ws_stream.next().await.unwrap().unwrap();
        if let Message::Text(txt) = resp_msg {
            assert!(txt.contains("response"));
            assert!(txt.contains("OK"));
        } else {
            panic!("Expected response message");
        }

        drop(handle);
        engine_task.abort();
    }).await;
}
