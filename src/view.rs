use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetaView {
    pub moon: i32,
    pub day_progress: f64,
    pub day_seconds_left: f64,
    pub gold: i32,
    pub food_current: i32,
    pub food_required: i32,
    pub card_count: i32,
    pub card_limit: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StackView {
    pub id: i32,
    pub cards: Vec<String>,
    pub progress: Option<f64>,
    pub total_time: Option<f64>,
    pub recipe_name: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShopItem {
    pub id: String,
    pub name: String,
    pub cost: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameViewSnapshot {
    pub meta: MetaView,
    pub stacks: Vec<StackView>,
    pub shop: Vec<ShopItem>,
    pub logs: Vec<String>,
}
