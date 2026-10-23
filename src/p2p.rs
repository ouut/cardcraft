use anyhow::{anyhow, Result};
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::server::{get_next_client_id, EngineCommand, EngineHandle};

pub const ALPN: &[u8] = b"cardcraft/p2p/v1";

pub fn addr_to_ticket(addr: &EndpointAddr) -> String {
    let json = serde_json::to_string(addr).unwrap_or_default();
    let hex: String = json.as_bytes().iter().map(|b| format!("{:02x}", b)).collect();
    format!("cardcraft://{}", hex)
}

pub fn ticket_to_addr(ticket: &str) -> Result<EndpointAddr> {
    let clean = ticket.trim().trim_start_matches("cardcraft://");
    if clean.len() % 2 != 0 {
        return Err(anyhow!("Invalid ticket format: odd length"));
    }
    let mut bytes = Vec::new();
    for i in (0..clean.len()).step_by(2) {
        let byte = u8::from_str_radix(&clean[i..i + 2], 16)
            .map_err(|e| anyhow!("Failed to parse ticket hex: {:?}", e))?;
        bytes.push(byte);
    }
    let addr: EndpointAddr = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow!("Failed to deserialize EndpointAddr: {:?}", e))?;
    Ok(addr)
}

pub struct P2pHost {
    pub endpoint: Endpoint,
    pub addr: EndpointAddr,
    pub ticket: String,
}

impl P2pHost {
    pub async fn bind() -> Result<Self> {
        let endpoint = Endpoint::builder(presets::N0)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await?;
        let addr = endpoint.addr();
        let ticket = addr_to_ticket(&addr);
        Ok(Self { endpoint, addr, ticket })
    }
}

pub struct P2pClient {
    pub endpoint: Endpoint,
}

impl P2pClient {
    pub async fn new() -> Result<Self> {
        let endpoint = Endpoint::bind(presets::N0).await?;
        Ok(Self { endpoint })
    }

    pub async fn connect_to_ticket(&self, ticket: &str) -> Result<iroh::endpoint::Connection> {
        let addr = ticket_to_addr(ticket)?;
        let conn = self.endpoint.connect(addr, ALPN).await?;
        Ok(conn)
    }
}

pub fn spawn_p2p_client_actor(
    mut send_stream: iroh::endpoint::SendStream,
    recv_stream: iroh::endpoint::RecvStream,
) -> (EngineHandle, tokio::task::JoinHandle<()>) {
    let hub = crate::server::ClientHub::new();
    let hub_client = hub.clone();
    let (tx, mut rx) = mpsc::channel::<EngineCommand>(100);
    let mut reader = BufReader::new(recv_stream);

    let join_handle = tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                EngineCommand::Message { client_id: _, text, reply } => {
                    let mut line_to_send = text;
                    line_to_send.push('\n');
                    if send_stream.write_all(line_to_send.as_bytes()).await.is_err()
                        || send_stream.flush().await.is_err()
                    {
                        reply.send(("P2P Connection Lost".to_string(), None)).ok();
                        break;
                    }

                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(n) if n > 0 => {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                    if val.get("type").and_then(|v| v.as_str()) == Some("event") {
                                        if let Some(msg) = val.get("message").and_then(|v| v.as_str()) {
                                            hub_client.broadcast(msg);
                                        }
                                        continue;
                                    }
                                    let feedback = val.get("feedback")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let snap_str = val.get("snapshot")
                                        .filter(|v| !v.is_null())
                                        .map(|v| v.to_string());
                                    reply.send((feedback, snap_str)).ok();
                                    break;
                                } else {
                                    reply.send((line.trim().to_string(), None)).ok();
                                    break;
                                }
                            }
                            _ => {
                                reply.send(("P2P Connection Closed".to_string(), None)).ok();
                                break;
                            }
                        }
                    }
                }
                EngineCommand::GetSnapshot { reply } => {
                    if send_stream.write_all(b"SNAPSHOT\n").await.is_err()
                        || send_stream.flush().await.is_err()
                    {
                        reply.send(None).ok();
                        break;
                    }

                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(n) if n > 0 => {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                    if val.get("type").and_then(|v| v.as_str()) == Some("event") {
                                        if let Some(msg) = val.get("message").and_then(|v| v.as_str()) {
                                            hub_client.broadcast(msg);
                                        }
                                        continue;
                                    }
                                    let snap_str = val.get("snapshot")
                                        .filter(|v| !v.is_null())
                                        .map(|v| v.to_string());
                                    reply.send(snap_str).ok();
                                    break;
                                } else {
                                    reply.send(None).ok();
                                    break;
                                }
                            }
                            _ => {
                                reply.send(None).ok();
                                break;
                            }
                        }
                    }
                }
                EngineCommand::Connect { .. } => {}
                EngineCommand::Disconnect { .. } => {}
                EngineCommand::Tick { .. } => {}
            }
        }
    });

    (EngineHandle::new(tx, hub), join_handle)
}

pub async fn run_p2p_host_listener(
    handle: EngineHandle,
    host: P2pHost,
) -> Result<()> {
    while let Some(incoming) = host.endpoint.accept().await {
        let conn = match incoming.await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[P2P Host] Incoming peer handshake failed: {:?}", e);
                continue;
            }
        };

        let h = handle.clone();
        tokio::spawn(async move {
            let client_id = get_next_client_id();
            let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            h.hub().register(client_id, out_tx);
            h.connect(client_id).await;
            println!("[P2P Host] Peer #{} connected via Iroh QUIC.", client_id);

            let (mut send_stream, recv_stream) = match conn.accept_bi().await {
                Ok(streams) => streams,
                Err(e) => {
                    eprintln!("[P2P Host] accept_bi failed: {:?}", e);
                    h.hub().unregister(client_id);
                    h.disconnect(client_id).await;
                    return;
                }
            };

            let mut reader = BufReader::new(recv_stream);
            let mut line = String::new();

            loop {
                tokio::select! {
                    Some(out_msg) = out_rx.recv() => {
                        let payload = serde_json::json!({
                            "type": "event",
                            "message": out_msg
                        });
                        let mut out = serde_json::to_string(&payload).unwrap_or_default();
                        out.push('\n');
                        if send_stream.write_all(out.as_bytes()).await.is_err()
                            || send_stream.flush().await.is_err()
                        {
                            break;
                        }
                    }
                    res = reader.read_line(&mut line) => {
                        match res {
                            Ok(0) => break,
                            Ok(_) => {
                                let trimmed = line.trim();
                                let (feedback, snap_json) = if trimmed == "SNAPSHOT" {
                                    let s = h.get_snapshot().await;
                                    (String::new(), s)
                                } else {
                                    h.message(client_id, trimmed.to_string()).await
                                };

                                let payload = serde_json::json!({
                                    "type": "response",
                                    "feedback": feedback,
                                    "snapshot": snap_json.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                                });

                                let mut out = serde_json::to_string(&payload).unwrap_or_default();
                                out.push('\n');
                                if send_stream.write_all(out.as_bytes()).await.is_err()
                                    || send_stream.flush().await.is_err()
                                {
                                    break;
                                }
                                line.clear();
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            println!("[P2P Host] Peer #{} disconnected.", client_id);
            h.hub().unregister(client_id);
            h.disconnect(client_id).await;
        });
    }
    Ok(())
}

