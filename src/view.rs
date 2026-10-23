use serde::{Deserialize, Serialize};

/// 100% Scheme-driven Full-Screen Canvas Snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GameViewSnapshot {
    /// 2D character lines forming the full-screen terminal canvas
    #[serde(alias = "lines", alias = "screen", default)]
    pub grid: Vec<String>,

    /// Optional hardware cursor position [x, y] on the canvas
    #[serde(default)]
    pub cursor: Option<[u16; 2]>,
}
