use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use cardcraft::buffer::VirtualScreen;
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

    // 1. Parse game dir (-g / --game)
    let game_dir = if let Some(pos) = args.iter().position(|a| a == "-g" || a == "--game") {
        args.get(pos + 1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(".")
    };

    // 2. Parse web dir:
    // If explicitly provided via --serve / --web-dir, use it.
    // Otherwise check <game_dir>/web, fallback to ./web
    let web_dir = if let Some(pos) = args.iter().position(|a| a == "--serve" || a == "--web-dir") {
        args.get(pos + 1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("web"))
    } else {
        let game_web = game_dir.join("web");
        if game_web.is_dir() {
            game_web
        } else {
            PathBuf::from("web")
        }
    };

    // 3. Parse port overrides
    let ws_port = if let Some(pos) = args.iter().position(|a| a == "--ws" || a == "--ws-port") {
        args.get(pos + 1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(8080)
    } else {
        8080
    };

    let tcp_port = if let Some(pos) = args.iter().position(|a| a == "--tcp" || a == "--tcp-port") {
        args.get(pos + 1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(9000)
    } else {
        9000
    };

    // 4. Feature toggles (Opt-out)
    let no_net = args.iter().any(|a| a == "--no-net");
    let no_tui = args.iter().any(|a| a == "--no-tui" || a == "--headless");
    let no_ws = no_net || args.iter().any(|a| a == "--no-ws");
    let no_tcp = no_net || args.iter().any(|a| a == "--no-tcp");
    let no_p2p = no_net || args.iter().any(|a| a == "--no-p2p");

    // 5. P2P Join mode: join <TICKET>
    if let Some(pos) = args.iter().position(|arg| arg == "join" || arg == "--p2p-join" || arg == "--join") {
        if let Some(ticket) = args.get(pos + 1) {
            return run_join_mode(ticket, no_tui, no_ws, ws_port, no_tcp, tcp_port, web_dir);
        } else {
            eprintln!("Usage: cardcraft join <ticket> [--no-tui] [--ws PORT] [--tcp PORT]");
            return Ok(());
        }
    }

    // 6. Default Server Mode: starts Engine Actor + TUI + WS + TCP + P2P all concurrently
    run_default_server_mode(game_dir, no_tui, no_ws, ws_port, no_tcp, tcp_port, no_p2p, web_dir)
}

fn run_default_server_mode(
    game_dir: PathBuf,
    no_tui: bool,
    no_ws: bool,
    ws_port: u16,
    no_tcp: bool,
    tcp_port: u16,
    no_p2p: bool,
    web_dir: PathBuf,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let local = tokio::task::LocalSet::new();

    local.block_on(&rt, async {
        let engine = GameEngine::with_game_dir(&game_dir)?;
        let (handle, engine_task) = cardcraft::server::spawn_engine_actor(engine);

        // 1. Simulation ticker (100ms interval)
        let ticker_handle = handle.clone();
        let ticker_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                ticker_handle.tick(0.1).await;
            }
        });

        let mut tasks = Vec::new();

        // 2. Raw TCP Server (:9000 by default)
        if !no_tcp {
            let h = handle.clone();
            tasks.push(tokio::spawn(async move {
                cardcraft::server::run_tcp_server(h, tcp_port).await
            }));
            if no_tui {
                println!("[TCP Server] Listening on 127.0.0.1:{}", tcp_port);
            }
        }

        // 3. Web & WebSocket Server (:8080 by default)
        if !no_ws {
            let h = handle.clone();
            let wdir = web_dir.clone();
            tasks.push(tokio::spawn(async move {
                cardcraft::server::run_web_server(h, ws_port, wdir).await
            }));
            if no_tui {
                println!("[Web Server] Listening on http://localhost:{} (serving {:?})", ws_port, web_dir);
            }
        }

        // 4. Iroh P2P Node (auto NAT-punching)
        if !no_p2p {
            match cardcraft::p2p::P2pHost::bind().await {
                Ok(host) => {
                    let ticket = host.ticket.clone();
                    if no_tui {
                        println!("\n[P2P Ready] Connection Ticket for friends:\n  {}\n", ticket);
                    }
                    let p2p_handle = handle.clone();
                    tasks.push(tokio::spawn(async move {
                        cardcraft::p2p::run_p2p_host_listener(p2p_handle, host).await
                    }));
                }
                Err(e) => {
                    eprintln!("[P2P] Failed to bind endpoint: {:?}", e);
                }
            }
        }

        // 5. User Interface (Local TUI or Headless Wait)
        if !no_tui {
            run_interactive_tui(handle.clone()).await?;
        } else {
            println!("CardCraft Server running in headless mode. Press Ctrl+C to exit.");
            tokio::signal::ctrl_c().await.ok();
            println!("\nShutdown signal received. Exiting server...");
        }

        drop(handle);
        ticker_task.abort();
        for t in tasks {
            t.abort();
        }
        engine_task.await.ok();
        Ok(())
    })
}

fn run_join_mode(
    ticket: &str,
    no_tui: bool,
    no_ws: bool,
    ws_port: u16,
    no_tcp: bool,
    tcp_port: u16,
    web_dir: PathBuf,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let ticket_str = ticket.to_string();

    rt.block_on(async {
        if no_tui {
            println!("=== Connecting to CardCraft Host via Iroh P2P ===");
        }
        let client = cardcraft::p2p::P2pClient::new().await?;
        let conn = client.connect_to_ticket(&ticket_str).await?;
        if no_tui {
            println!(">>> Connected successfully to P2P Host!");
        }
        let (send_stream, recv_stream) = conn.open_bi().await?;

        let (handle, client_task) = cardcraft::p2p::spawn_p2p_client_actor(send_stream, recv_stream);

        let mut tasks = Vec::new();

        // 1. Local TCP bridge
        if !no_tcp {
            let h = handle.clone();
            tasks.push(tokio::spawn(async move {
                cardcraft::server::run_tcp_server(h, tcp_port).await
            }));
            if no_tui {
                println!("[P2P TCP Bridge] Listening on 127.0.0.1:{}", tcp_port);
            }
        }

        // 2. Local Web bridge
        if !no_ws {
            let h = handle.clone();
            let wdir = web_dir.clone();
            tasks.push(tokio::spawn(async move {
                cardcraft::server::run_web_server(h, ws_port, wdir).await
            }));
            if no_tui {
                println!("[P2P Web Bridge] Open in browser: http://localhost:{}", ws_port);
            }
        }

        // 3. User Interface
        if !no_tui {
            run_interactive_tui(handle.clone()).await?;
        } else {
            println!("P2P Client bridge running. Press Ctrl+C to exit.");
            tokio::signal::ctrl_c().await.ok();
            println!("\nShutdown signal received. Exiting P2P client...");
        }

        drop(handle);
        for t in tasks {
            t.abort();
        }
        client_task.await.ok();
        Ok(())
    })
}

async fn run_interactive_tui(handle: cardcraft::server::EngineHandle) -> anyhow::Result<()> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to initialize terminal")?;

    let res = run_event_loop(&mut terminal, handle).await;

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    if let Err(err) = res {
        eprintln!("Application error: {:?}", err);
    }

    Ok(())
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    handle: cardcraft::server::EngineHandle,
) -> anyhow::Result<()> {
    handle.connect(0).await;
    let tick_interval = Duration::from_millis(50);

    loop {
        // Draw frame from EngineHandle
        if let Some(snapshot) = handle.get_snapshot_view().await {
            terminal.draw(|f| {
                render_ui(f, &snapshot);
            })?;
        }

        // Poll keyboard and hardware events non-blockingly
        if event::poll(tick_interval)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                        break;
                    }

                    let key_msg = match key.code {
                        KeyCode::Char(' ') => "key:space".to_string(),
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                format!("key:ctrl+{}", c)
                            } else {
                                format!("key:{}", c)
                            }
                        }
                        KeyCode::Up => "key:up".to_string(),
                        KeyCode::Down => "key:down".to_string(),
                        KeyCode::Left => "key:left".to_string(),
                        KeyCode::Right => "key:right".to_string(),
                        KeyCode::Enter => "key:enter".to_string(),
                        KeyCode::Backspace => "key:backspace".to_string(),
                        KeyCode::Delete => "key:delete".to_string(),
                        KeyCode::Esc => "key:esc".to_string(),
                        KeyCode::Tab => "key:tab".to_string(),
                        KeyCode::Home => "key:home".to_string(),
                        KeyCode::End => "key:end".to_string(),
                        KeyCode::PageUp => "key:pageup".to_string(),
                        KeyCode::PageDown => "key:pagedown".to_string(),
                        KeyCode::F(n) => format!("key:f{}", n),
                        _ => continue,
                    };
                    handle.message(0, key_msg).await;
                }
                Event::Mouse(mouse) => {
                    let kind_str = match mouse.kind {
                        crossterm::event::MouseEventKind::Down(btn) => format!("down:{:?}", btn).to_lowercase(),
                        crossterm::event::MouseEventKind::Up(btn) => format!("up:{:?}", btn).to_lowercase(),
                        crossterm::event::MouseEventKind::Drag(btn) => format!("drag:{:?}", btn).to_lowercase(),
                        crossterm::event::MouseEventKind::Moved => "move".to_string(),
                        crossterm::event::MouseEventKind::ScrollDown => "scrolldown".to_string(),
                        crossterm::event::MouseEventKind::ScrollUp => "scrollup".to_string(),
                        _ => "other".to_string(),
                    };
                    let mouse_msg = format!("mouse:{}:{}:{}", kind_str, mouse.column, mouse.row);
                    handle.message(0, mouse_msg).await;
                }
                Event::Resize(cols, rows) => {
                    handle.message(0, format!("resize:{}:{}", cols, rows)).await;
                }
                _ => {}
            }
        }
    }

    handle.disconnect(0).await;
    Ok(())
}

fn run_self_test() -> anyhow::Result<()> {
    println!("=== CardCraft Headless Self-Test ===");
    let mut engine = GameEngine::new()?;
    let mut screen = VirtualScreen::new(90, 24)?;

    // 1. Initial snapshot
    let snap = engine.get_snapshot()?;
    screen.render(&snap)?;
    println!("Initial frame render success.");
    assert!(screen.contains("Villager"));
    assert!(screen.contains("BerryBush"));

    // 2. Stack Villager (1) onto BerryBush (2)
    let feedback = engine.execute_command("s 1 2")?;
    println!("Executed: s 1 2 -> {}", feedback);

    // 3. Tick 2.0s
    engine.tick(2.0)?;
    let snap = engine.get_snapshot()?;
    screen.render(&snap)?;
    assert!(screen.contains("Harvest Berry"));
    println!("2.0s Tick success: Recipe matching & progress verified.");

    // 4. Tick 3.0s (Completion)
    engine.tick(3.0)?;
    let snap = engine.get_snapshot()?;
    screen.render(&snap)?;
    assert!(screen.contains("Produced: Berry"));
    println!("Recipe completed! Berry produced into new slot.");

    println!("\nRendered Virtual Frame:\n");
    println!("{}", screen.to_string());
    println!("All self-test assertions passed!");
    Ok(())
}

fn print_help() {
    println!(r#"CardCraft - A CAD-Driven Stacking Card Game in Steel Scheme & Rust

USAGE:
    cardcraft [OPTIONS] [COMMAND]

GAME & DIRECTORY:
    -g, --game <DIR>            Specify game directory (default: .)
                                Loads <DIR>/core.scm, saves jailed to <DIR>/saves/, web from <DIR>/web
    --serve, --web-dir <DIR>    Override directory for static web files (default: <game>/web or ./web)

DEFAULT BEHAVIOR:
    Running `cardcraft` without arguments automatically starts ALL interfaces concurrently:
      * Interactive Local Terminal (TUI)
      * Web/WebSocket Server on http://localhost:8080
      * Raw TCP Socket Server on 127.0.0.1:9000
      * Iroh P2P Host Node (generating an encrypted NAT-punching ticket)

JOINING A GAME:
    join <TICKET>               Connect to an active P2P host session using its ticket.
                                Bridges to local TUI and Web interface automatically.

PORT OVERRIDES:
    --ws [PORT]                 Set Web/WebSocket port (default: 8080)
    --tcp [PORT]                Set raw TCP server port (default: 9000)

OPT-OUT FLAGS:
    --headless, --no-tui        Run without local terminal TUI (background server mode)
    --no-net                    Offline single-player mode (disable all network ports & P2P)
    --no-ws                     Disable Web/WebSocket server
    --no-tcp                    Disable TCP server
    --no-p2p                    Disable P2P node

DEVELOPMENT & DEBUG:
    --test                      Run internal headless self-test loop with state assertions
    -h, --help                  Print this help message
    -v, --version               Print version information
"#);
}
