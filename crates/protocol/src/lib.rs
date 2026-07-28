use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMessage {
    pub version: String,
    #[serde(rename = "type")]
    pub kind: MessageType,
    pub message_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    pub sent_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Hello,
    Heartbeat,
    Catalog,
    Job,
    Chunk,
    Completed,
    Failed,
}

impl RelayMessage {
    pub fn new(kind: MessageType, payload: impl Serialize) -> serde_json::Result<Self> {
        Ok(Self {
            version: "1.0".into(),
            kind,
            message_id: Uuid::new_v4(),
            correlation_id: None,
            sent_at: Utc::now(),
            payload: serde_json::to_value(payload)?,
        })
    }

    pub fn correlated(
        kind: MessageType,
        correlation_id: Uuid,
        payload: impl Serialize,
    ) -> serde_json::Result<Self> {
        let mut message = Self::new(kind, payload)?;
        message.correlation_id = Some(correlation_id);
        Ok(message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub display_name: String,
    pub engine_model_id: String,
    pub description: String,
    pub architecture: String,
    pub context_length: u32,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub parameters: Vec<String>,
    pub license_id: String,
    pub commercial_hosting_confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    pub input_price: String,
    pub output_price: String,
    pub mode: ProviderMode,
    pub ready: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderMode {
    Static,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPayload {
    pub job_id: Uuid,
    pub request: ChatRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Value,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Heartbeat {
    pub engine: String,
    pub engine_url: String,
    pub accepting_jobs: bool,
    pub active_jobs: u32,
    pub earned_this_month: String,
    pub earnings_cap: String,
    pub client_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletedPayload {
    pub job_id: Uuid,
    pub content: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailedPayload {
    pub job_id: Uuid,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_wire_names() {
        let message = RelayMessage::new(MessageType::Heartbeat, serde_json::json!({"ok": true}))
            .expect("message");
        let value = serde_json::to_value(message).expect("json");
        assert_eq!(value["type"], "heartbeat");
        assert_eq!(value["version"], "1.0");
    }
}
