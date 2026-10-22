use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use cardcraft::buffer::VirtualScreen;
use cardcraft::cad::CommandState;
use cardcraft::engine::GameEngine;
use cardcraft::render::render_ui;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--version" || arg == "-v") {
        println!("cardcraft v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // 1. Parse game dir (-g / --game)
    let game_dir = if let Some(pos) = args.iter().position(|a| a == "-g" || a == "--game") {
        args.get(pos + 1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(".")
    };

    // 2. Parse web dir (--serve / --web-dir)
    let web_dir = if let Some(pos) = args.iter().position(|a| a == "--serve" || a == "--web-dir") {
        args.get(pos + 1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("web"))
    } else {
        PathBuf::from("web")
    };

    // 3. Parse WS port (--ws [PORT])
    let ws_port = if let Some(pos) = args.iter().position(|a| a == "--ws" || a == "--ws-server") {
        let p = args.get(pos + 1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(8080);
        Some(p)
    } else {
        None
    };

    // 4. Parse TCP port (--tcp [PORT])
    let tcp_port = if let Some(pos) = args.iter().position(|a| a == "--tcp" || a == "--tcp-server") {
        let p = args.get(pos + 1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(9000);
        Some(p)
    } else {
        None
    };

    // Start network servers if requested
    if ws_port.is_some() || tcp_port.is_some() {
        return run_network_servers(game_dir, ws_port, tcp_port, web_dir);
    }

    if args.iter().any(|arg| arg == "--test") {
        return run_self_test();
    }

    if args.iter().any(|arg| arg == "--headless") {
        return run_headless_demo(&game_dir);
    }

    if args.iter().any(|arg| arg == "--p2p-host" || arg == "host") {
        return run_p2p_host(&game_dir);
    }

    if let Some(pos) = args.iter().position(|arg| arg == "--p2p-join" || arg == "join") {
        if let Some(ticket) = args.get(pos + 1) {
            return run_p2p_join(ticket);
        } else {
            eprintln!("Usage: cardcraft --p2p-join <ticket>");
            return Ok(());
        }
    }

    // Default: run interactive TUI
    run_interactive_tui(&game_dir)
}

fn run_network_servers(
    game_dir: PathBuf,
    ws_port: Option<u16>,
    tcp_port: Option<u16>,
    web_dir: PathBuf,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let local = tokio::task::LocalSet::new();

    local.block_on(&rt, async {
        println!("=== Starting CardCraft Multi-Protocol Server ===");
        println!("Game Directory: {:?}", game_dir);
        let engine = GameEngine::with_game_dir(&game_dir)?;
        let (handle, engine_task) = cardcraft::server::spawn_engine_actor(engine);

        let mut tasks = Vec::new();

        if let Some(port) = tcp_port {
            let h = handle.clone();
            tasks.push(tokio::spawn(async move {
                cardcraft::server::run_tcp_server(h, port).await
            }));
        }

        if let Some(port) = ws_port {
            let h = handle.clone();
            let wdir = web_dir.clone();
            tasks.push(tokio::spawn(async move {
                cardcraft::server::run_web_server(h, port, wdir).await
            }));
        }

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutdown signal received. Exiting server...");
            }
            res = async {
                for t in tasks {
                    t.await??;
                }
                Ok::<(), anyhow::Error>(())
            } => {
                res?;
            }
        }

        drop(handle);
        engine_task.await.ok();
        Ok(())
    })
}

fn print_help() {
    println!(r#"CardCraft - A CAD-Driven Stacking Card Game in Steel Scheme & Rust

USAGE:
    cardcraft [OPTIONS] [COMMAND]

GAME & CONFIG:
    -g, --game <DIR>            Specify game directory (default: current directory .)
                                Loads <DIR>/core.scm, sandbox jailed inside <DIR>/saves/
    --serve, --web-dir <DIR>    Specify directory for static web files (default: ./web)

NETWORK SERVERS (Can run concurrently):
    --ws [PORT]                 Start WebSocket + HTTP Static File server (default port: 8080)
                                Browse http://localhost:8080 to play via web browser!
    --tcp [PORT]                Start raw TCP line-based server (default port: 9000)
                                Connect with: nc 127.0.0.1 9000

P2P SERVERLESS MULTIPLAYER (Iroh QUIC):
    host, --p2p-host            Start as an Iroh P2P Host (generates a NAT-traversing direct ticket)
    join, --p2p-join <TICKET>   Join an Iroh P2P Host session using its ticket

SINGLE-PLAYER & DEBUG:
    (no arguments)              Start full interactive terminal TUI game
    --headless                  Output initial single ASCII frame to stdout and exit
    --test                      Run internal headless self-test loop with state assertions

INFORMATION:
    -h, --help                  Print this help message
    -v, --version               Print version information
"#);
}

fn run_p2p_host(game_dir: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let gd = game_dir.to_path_buf();
    rt.block_on(async {
        println!("=== CardCraft Iroh P2P Host Starting ===");
        println!("Game Directory: {:?}", gd);
        let host = cardcraft::p2p::P2pHost::bind().await?;
        println!("\n[P2P Host Ready!]");
        println!("Endpoint ID: {}", host.addr.id);
        println!("\nShare this ticket with your friend to connect without any central server or public IP:\n");
        println!("  {}\n", host.ticket);
        println!("Waiting for peer connection (press Ctrl+C to cancel)...");

        let mut engine = GameEngine::with_game_dir(&gd)?;
        if let Some(incoming) = host.endpoint.accept().await {
            let conn = incoming.await?;
            println!("\n>>> Peer connected via Iroh QUIC!");
            let (mut send_stream, recv_stream) = conn.accept_bi().await?;
            engine.net_connect(2)?;

            let mut reader = tokio::io::BufReader::new(recv_stream);
            let mut line = String::new();
            loop {
                line.clear();
                let bytes_read = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await?;
                if bytes_read == 0 {
                    println!("Peer disconnected.");
                    break;
                }
                let trimmed = line.trim();
                println!("[Peer CAD]: {}", trimmed);
                let resp = engine.net_message(2, trimmed)?;

                let reply = format!("{}\n", resp);
                tokio::io::AsyncWriteExt::write_all(&mut send_stream, reply.as_bytes()).await?;
                tokio::io::AsyncWriteExt::flush(&mut send_stream).await?;
            }
        }
        Ok(())
    })
}

fn run_p2p_join(ticket: &str) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let ticket_str = ticket.to_string();
    rt.block_on(async {
        println!("=== Connecting to CardCraft Host via Iroh P2P ===");
        let client = cardcraft::p2p::P2pClient::new().await?;
        let conn = client.connect_to_ticket(&ticket_str).await?;
        println!(">>> Connected successfully to P2P Host!");
        let (mut send_stream, recv_stream) = conn.open_bi().await?;

        // Send an initial handshake
        tokio::io::AsyncWriteExt::write_all(&mut send_stream, b"say Joined P2P session!\n").await?;
        tokio::io::AsyncWriteExt::flush(&mut send_stream).await?;

        let mut reader = tokio::io::BufReader::new(recv_stream);
        let mut line = String::new();
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await?;
        println!("Host response: {}", line.trim());
        Ok(())
    })
}

fn run_self_test() -> anyhow::Result<()> {
    println!("=== CardCraft Headless Self-Test ===");
    let mut engine = GameEngine::new()?;
    let mut screen = VirtualScreen::new(90, 24)?;
    let mut cmd_state = CommandState::new();

    // 1. Initial snapshot
    let snap = engine.get_snapshot()?;
    screen.render(&snap, &cmd_state)?;
    println!("Initial frame render success. Cards on board: {}", snap.meta.card_count);
    assert!(screen.contains("Villager"));
    assert!(screen.contains("BerryBush"));

    // 2. Stack Villager (1) onto BerryBush (2)
    let feedback = engine.execute_command("s 1 2")?;
    cmd_state.set_feedback(feedback);
    println!("Executed: s 1 2");

    // 3. Tick 2.0s
    engine.tick(2.0)?;
    let snap = engine.get_snapshot()?;
    screen.render(&snap, &cmd_state)?;
    assert!(screen.contains("Harvest Berry"));
    println!("2.0s Tick success: Recipe matching & progress verified.");

    // 4. Tick 3.0s (Completion)
    engine.tick(3.0)?;
    let snap = engine.get_snapshot()?;
    screen.render(&snap, &cmd_state)?;
    assert!(screen.contains("Produced: Berry") || snap.logs.iter().any(|l| l.contains("Produced")));
    println!("Recipe completed! Berry produced into new slot.");

    println!("\nRendered Virtual Frame:\n");
    println!("{}", screen.to_string());
    println!("All self-test assertions passed!");
    Ok(())
}

fn run_headless_demo(game_dir: &Path) -> anyhow::Result<()> {
    println!("Running CardCraft in Headless Mode (Game Dir: {:?})...", game_dir);
    let mut engine = GameEngine::with_game_dir(game_dir)?;
    let mut screen = VirtualScreen::new(80, 20)?;
    let cmd_state = CommandState::new();

    let snap = engine.get_snapshot()?;
    screen.render(&snap, &cmd_state)?;
    println!("{}", screen.to_string());
    Ok(())
}

fn run_interactive_tui(game_dir: &Path) -> anyhow::Result<()> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to initialize terminal")?;

    let res = run_event_loop(&mut terminal, game_dir);

    // Restore terminal safely even if an error occurred
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    if let Err(err) = res {
        eprintln!("Application error: {:?}", err);
    }

    Ok(())
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    game_dir: &Path,
) -> anyhow::Result<()> {
    let mut engine = GameEngine::with_game_dir(game_dir)?;
    let mut cmd_state = CommandState::new();
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        // Draw frame
        let snapshot = engine.get_snapshot()?;
        terminal.draw(|f| {
            render_ui(f, &snapshot, &cmd_state);
        })?;

        // Poll keyboard event with remaining tick time
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Windows Console emits both Press and Release events; filter out non-Press
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }

                match key.code {
                    KeyCode::Char(c) => {
                        cmd_state.insert_char(c);
                    }
                    KeyCode::Backspace => {
                        cmd_state.delete_backspace();
                    }
                    KeyCode::Delete => {
                        cmd_state.delete_char();
                    }
                    KeyCode::Left => {
                        cmd_state.move_cursor_left();
                    }
                    KeyCode::Right => {
                        cmd_state.move_cursor_right();
                    }
                    KeyCode::Home => {
                        cmd_state.move_cursor_home();
                    }
                    KeyCode::End => {
                        cmd_state.move_cursor_end();
                    }
                    KeyCode::Up => {
                        cmd_state.history_up();
                    }
                    KeyCode::Down => {
                        cmd_state.history_down();
                    }
                    KeyCode::Enter => {
                        let cmd = cmd_state.submit();
                        if cmd == "q" || cmd == "quit" || cmd == "exit" {
                            break;
                        }
                        match engine.execute_command(&cmd) {
                            Ok(feedback) => {
                                cmd_state.set_feedback(feedback);
                            }
                            Err(e) => {
                                cmd_state.set_feedback(format!("Error: {}", e));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Advance simulation tick
        if last_tick.elapsed() >= tick_rate {
            engine.tick(0.1)?;
            last_tick = Instant::now();
        }
    }

    Ok(())
}
