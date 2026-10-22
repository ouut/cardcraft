use serde::{Deserialize, Serialize};

/// Dynamic metric item displayed in the UI Header (e.g. MOON, HP, Gold, Food, Level, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricItem {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// Generic slot or stack on the board
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StackView {
    pub id: i32,
    #[serde(alias = "items", default)]
    pub cards: Vec<String>,
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub total_time: Option<f64>,
    #[serde(alias = "task", alias = "recipe", default)]
    pub recipe_name: Option<String>,
    #[serde(default = "default_idle_status")]
    pub status: String,
}

fn default_idle_status() -> String {
    "Idle".to_string()
}

/// Generic panel, store item, or action shortcut
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PanelItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cost: Option<i32>,
    #[serde(default)]
    pub desc: Option<String>,
}

/// Type alias for backward compatibility
pub type ShopItem = PanelItem;

/// Legacy Stacklands metadata (all fields optional with sensible defaults)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetaView {
    #[serde(default)]
    pub moon: i32,
    #[serde(default)]
    pub day_progress: f64,
    #[serde(default)]
    pub day_seconds_left: f64,
    #[serde(default)]
    pub gold: i32,
    #[serde(default)]
    pub food_current: i32,
    #[serde(default)]
    pub food_required: i32,
    #[serde(default)]
    pub card_count: i32,
    #[serde(default)]
    pub card_limit: i32,
    #[serde(default = "default_running_status")]
    pub status: String,
}

fn default_running_status() -> String {
    "RUNNING".to_string()
}

/// Completely generic, decoupled Game View Snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GameViewSnapshot {
    /// Custom game title for the top window banner
    #[serde(default)]
    pub title: Option<String>,

    /// Dynamic key-value metric badges displayed in the header bar
    #[serde(default)]
    pub metrics: Vec<MetricItem>,

    /// Legacy metadata support for Stacklands (optional, with defaults)
    #[serde(default)]
    pub meta: MetaView,

    /// Board slots/stacks
    #[serde(alias = "slots", alias = "board", default)]
    pub stacks: Vec<StackView>,

    /// Generic panels/actions/shortcuts (aliased to "shop" for backward compatibility)
    #[serde(alias = "shop", default)]
    pub panels: Vec<PanelItem>,

    /// Custom panel title (e.g. "Store", "Actions", "Shortcuts")
    #[serde(default)]
    pub panel_title: Option<String>,

    /// System & chat event logs
    #[serde(default)]
    pub logs: Vec<String>,
}

impl GameViewSnapshot {
    /// Backward-compatibility accessor for snapshot.shop
    pub fn shop(&self) -> &[PanelItem] {
        &self.panels
    }
}
