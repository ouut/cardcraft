use std::io::stdout;
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

    if args.iter().any(|arg| arg == "--test") {
        return run_self_test();
    }

    if args.iter().any(|arg| arg == "--headless") {
        return run_headless_demo();
    }

    if args.iter().any(|arg| arg == "--p2p-host" || arg == "host") {
        return run_p2p_host();
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
    run_interactive_tui()
}

fn print_help() {
    println!(r#"CardCraft - A CAD-Driven Stacking Card Game in Steel Scheme & Rust

USAGE:
    cardcraft [OPTIONS]
    cardcraft <COMMAND> [ARGS]

OPTIONS:
    -h, --help                  Print this help message
    -v, --version               Print version information
    --headless                  Run once in headless virtual buffer mode and output ASCII frame to stdout
    --test                      Run internal headless self-test loop with state assertions

P2P MULTIPLAYER:
    host, --p2p-host            Start as an Iroh P2P Host (generates a NAT-traversing direct ticket)
    join, --p2p-join <TICKET>   Join an Iroh P2P Host session using its ticket

DEFAULT:
    Running without arguments starts the full interactive terminal TUI game.
"#);
}

fn run_p2p_host() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        println!("=== CardCraft Iroh P2P Host Starting ===");
        let host = cardcraft::p2p::P2pHost::bind().await?;
        println!("\n[P2P Host Ready!]");
        println!("Endpoint ID: {}", host.addr.id);
        println!("\nShare this ticket with your friend to connect without any central server or public IP:\n");
        println!("  {}\n", host.ticket);
        println!("Waiting for peer connection (press Ctrl+C to cancel)...");

        let mut engine = GameEngine::new()?;
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

fn run_headless_demo() -> anyhow::Result<()> {
    println!("Running CardCraft in Headless Mode...");
    let mut engine = GameEngine::new()?;
    let mut screen = VirtualScreen::new(80, 20)?;
    let cmd_state = CommandState::new();

    let snap = engine.get_snapshot()?;
    screen.render(&snap, &cmd_state)?;
    println!("{}", screen.to_string());
    Ok(())
}

fn run_interactive_tui() -> anyhow::Result<()> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to initialize terminal")?;

    let res = run_event_loop(&mut terminal);

    // Restore terminal safely even if an error occurred
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    if let Err(err) = res {
        eprintln!("Application error: {:?}", err);
    }

    Ok(())
}

fn run_event_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
    let mut engine = GameEngine::new()?;
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
                        if !cmd.is_empty() {
                            let feedback = match engine.execute_command(&cmd) {
                                Ok(fb) => fb,
                                Err(e) => format!("Error: {}", e),
                            };
                            cmd_state.set_feedback(feedback);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Tick game engine if duration has elapsed
        if last_tick.elapsed() >= tick_rate {
            let dt = last_tick.elapsed().as_secs_f64();
            engine.tick(dt)?;
            last_tick = Instant::now();
        }
    }

    Ok(())
}
