use anyhow::{anyhow, Result};
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr};

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
