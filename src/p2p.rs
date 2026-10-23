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
        // Send initial newline to activate the QUIC bi-stream on the host immediately
        let _ = send_stream.write_all(b"\n").await;
        let _ = send_stream.flush().await;

        let mut last_snapshot: Option<String> = None;
        let mut line = String::new();

        loop {
            tokio::select! {
                res = reader.read_line(&mut line) => {
                    match res {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                let msg = serde_json::from_str::<String>(trimmed)
                                    .unwrap_or_else(|_| trimmed.to_string());
                                if msg.contains("\"grid\"") {
                                    last_snapshot = Some(msg.clone());
                                }
                                hub_client.send_all(&msg);
                            }
                            line.clear();
                        }
                    }
                }
                cmd_opt = rx.recv() => {
                    match cmd_opt {
                        Some(EngineCommand::Connect { client_id }) => {
                            if let Some(ref snap) = last_snapshot {
                                hub_client.send_to(client_id, snap);
                            }
                        }
                        Some(EngineCommand::Message { client_id: _, text, reply }) => {
                            let mut line_to_send = text;
                            line_to_send.push('\n');
                            if send_stream.write_all(line_to_send.as_bytes()).await.is_err()
                                || send_stream.flush().await.is_err()
                            {
                                reply.send("P2P Connection Lost".to_string()).ok();
                                break;
                            }
                            reply.send("OK".to_string()).ok();
                        }
                        Some(EngineCommand::GetSnapshot { reply }) => {
                            reply.send(last_snapshot.clone()).ok();
                        }
                        Some(EngineCommand::Disconnect { .. }) | Some(EngineCommand::Tick { .. }) => {}
                        None => break,
                    }
                }
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
            let (mut send_stream, recv_stream) = match conn.accept_bi().await {
                Ok(streams) => streams,
                Err(e) => {
                    eprintln!("[P2P Host] accept_bi failed: {:?}", e);
                    return;
                }
            };

            let client_id = get_next_client_id();
            let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            h.hub().register(client_id, out_tx);
            h.connect(client_id).await;
            println!("[P2P Host] Peer #{} connected via Iroh QUIC.", client_id);

            let mut reader = BufReader::new(recv_stream);
            let mut line = String::new();

            loop {
                tokio::select! {
                    Some(out_msg) = out_rx.recv() => {
                        let mut out = serde_json::to_string(&out_msg).unwrap_or_default();
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
                                if !trimmed.is_empty() {
                                    h.message(client_id, trimmed.to_string()).await;
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

