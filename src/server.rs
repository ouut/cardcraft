use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::engine::GameEngine;

static NEXT_CLIENT_ID: AtomicIsize = AtomicIsize::new(10);

fn get_next_client_id() -> isize {
    NEXT_CLIENT_ID.fetch_add(1, Ordering::SeqCst)
}

pub enum EngineCommand {
    Connect { client_id: isize },
    Message {
        client_id: isize,
        text: String,
        reply: oneshot::Sender<(String, Option<String>)>,
    },
    GetSnapshot {
        reply: oneshot::Sender<Option<String>>,
    },
    Disconnect { client_id: isize },
    Tick { dt: f64 },
}

#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<EngineCommand>,
}

impl EngineHandle {
    pub async fn connect(&self, client_id: isize) {
        self.tx.send(EngineCommand::Connect { client_id }).await.ok();
    }

    pub async fn message(&self, client_id: isize, text: String) -> (String, Option<String>) {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(EngineCommand::Message { client_id, text, reply: reply_tx }).await.is_ok() {
            reply_rx.await.unwrap_or_else(|_| ("Engine Error".to_string(), None))
        } else {
            ("Engine Stopped".to_string(), None)
        }
    }

    pub async fn get_snapshot(&self) -> Option<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(EngineCommand::GetSnapshot { reply: reply_tx }).await.is_ok() {
            reply_rx.await.unwrap_or(None)
        } else {
            None
        }
    }

    pub async fn disconnect(&self, client_id: isize) {
        self.tx.send(EngineCommand::Disconnect { client_id }).await.ok();
    }

    pub async fn tick(&self, dt: f64) {
        self.tx.send(EngineCommand::Tick { dt }).await.ok();
    }
}

pub fn spawn_engine_actor(mut engine: GameEngine) -> (EngineHandle, tokio::task::JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<EngineCommand>(100);

    // Runs on the current local thread/task holding the non-Send GameEngine
    let join_handle = tokio::task::spawn_local(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                EngineCommand::Connect { client_id } => {
                    engine.net_connect(client_id).ok();
                }
                EngineCommand::Message { client_id, text, reply } => {
                    let resp = engine.net_message(client_id, &text).unwrap_or_else(|e| format!("Error: {:?}", e));
                    let snap_json = engine.get_snapshot().ok().and_then(|s| serde_json::to_string(&s).ok());
                    reply.send((resp, snap_json)).ok();
                }
                EngineCommand::GetSnapshot { reply } => {
                    let snap_json = engine.get_snapshot().ok().and_then(|s| serde_json::to_string(&s).ok());
                    reply.send(snap_json).ok();
                }
                EngineCommand::Disconnect { client_id } => {
                    engine.net_disconnect(client_id).ok();
                }
                EngineCommand::Tick { dt } => {
                    engine.tick(dt).ok();
                }
            }
        }
    });

    (EngineHandle { tx }, join_handle)
}

// =============================================================================
// 1. Raw TCP Line-based Server (nc / telnet friendly)
// =============================================================================
pub async fn run_tcp_server(handle: EngineHandle, port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| anyhow!("Failed to bind TCP server on {}: {:?}", addr, e))?;
    println!("[TCP Server] Listening on {} (connect with: nc 127.0.0.1 {})", addr, port);

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        let h = handle.clone();
        let cid = get_next_client_id();

        tokio::spawn(async move {
            println!("[TCP Server] New client #{} connected from {:?}", cid, peer_addr);
            h.connect(cid).await;

            let (reader, mut writer) = socket.into_split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                line.clear();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF / disconnected
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            let (resp, _) = h.message(cid, trimmed.to_string()).await;
                            let reply = format!("{}\n", resp);
                            if writer.write_all(reply.as_bytes()).await.is_err() {
                                break;
                            }
                            writer.flush().await.ok();
                        }
                    }
                    Err(_) => break,
                }
            }

            println!("[TCP Server] Client #{} disconnected.", cid);
            h.disconnect(cid).await;
        });
    }
}

// =============================================================================
// 2. Hybrid Web Server: Static Files + WebSocket on the SAME Port
// =============================================================================
pub async fn run_web_server(handle: EngineHandle, port: u16, web_dir: PathBuf) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| anyhow!("Failed to bind Web/WS server on {}: {:?}", addr, e))?;
    println!("[Web & WS Server] Listening on http://localhost:{}", port);
    println!("[Web Server] Serving static files from: {:?}", web_dir);

    // Background game loop ticker (100ms)
    let h_ticker = handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            interval.tick().await;
            h_ticker.tick(0.1).await;
        }
    });

    loop {
        let (socket, _peer_addr) = listener.accept().await?;
        let h = handle.clone();
        let web_dir_clone = web_dir.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let n = match socket.peek(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let header_str = String::from_utf8_lossy(&buf[..n]);

            if header_str.to_lowercase().contains("upgrade: websocket") {
                handle_websocket(socket, h).await;
            } else {
                handle_http_static_file(socket, &web_dir_clone).await;
            }
        });
    }
}

async fn handle_websocket(socket: TcpStream, handle: EngineHandle) {
    let ws_stream = match tokio_tungstenite::accept_async(socket).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[WS Server] Handshake error: {:?}", e);
            return;
        }
    };

    let cid = get_next_client_id();
    println!("[WS Server] Client #{} connected via WebSocket.", cid);
    handle.connect(cid).await;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send initial snapshot
    if let Some(snap_json) = handle.get_snapshot().await {
        ws_sender.send(Message::Text(snap_json.into())).await.ok();
    }

    while let Some(msg_res) = ws_receiver.next().await {
        match msg_res {
            Ok(Message::Text(txt)) => {
                let trimmed = txt.trim();
                let (resp, snap_json) = handle.message(cid, trimmed.to_string()).await;

                let payload = serde_json::json!({
                    "type": "response",
                    "feedback": resp,
                    "snapshot": snap_json.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                });
                ws_sender.send(Message::Text(payload.to_string().into())).await.ok();
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    println!("[WS Server] Client #{} disconnected.", cid);
    handle.disconnect(cid).await;
}

async fn handle_http_static_file(mut socket: TcpStream, web_dir: &Path) {
    let mut buf = [0u8; 1024];
    let n = match socket.read(&mut buf).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let req = String::from_utf8_lossy(&buf[..n]);
    let mut path = "/";
    if let Some(first_line) = req.lines().next() {
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() >= 2 {
            path = parts[1];
        }
    }

    let clean_path = path.split('?').next().unwrap_or("/");
    let file_rel = if clean_path == "/" || clean_path.is_empty() {
        "index.html"
    } else {
        clean_path.trim_start_matches('/')
    };

    let target_file = web_dir.join(file_rel);
    if target_file.is_file() {
        if let Ok(content) = tokio::fs::read(&target_file).await {
            let mime = if file_rel.ends_with(".html") {
                "text/html; charset=utf-8"
            } else if file_rel.ends_with(".css") {
                "text/css"
            } else if file_rel.ends_with(".js") {
                "application/javascript"
            } else if file_rel.ends_with(".json") {
                "application/json"
            } else {
                "application/octet-stream"
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                mime,
                content.len()
            );
            socket.write_all(response.as_bytes()).await.ok();
            socket.write_all(&content).await.ok();
            socket.flush().await.ok();
            return;
        }
    }

    let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found";
    socket.write_all(not_found.as_bytes()).await.ok();
    socket.flush().await.ok();
}
