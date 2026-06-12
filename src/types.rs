use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,
    pub created_at: u64,
}

impl MemoryEntry {
    pub fn new(content: &str, source: &str) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: format!("mem_{}", timestamp),
            content: content.to_string(),
            source: source.to_string(),
            embedding: None,
            created_at: timestamp,
        }
    }
}

fn default_source() -> String {
    "manual".to_string()
}
