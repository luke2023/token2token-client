use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use token2token_protocol::{CatalogModel, ChatRequest, ProviderMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Ollama,
    OpenAiCompatible,
}

impl EngineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAiCompatible => "openai-compatible",
        }
    }
}

impl std::str::FromStr for EngineKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" | "lmstudio" | "llama.cpp" | "vllm" | "openai-compatible" => {
                Ok(Self::OpenAiCompatible)
            }
            _ => bail!("unsupported engine: {value}"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredModel {
    pub id: String,
    pub display_name: String,
    pub architecture: String,
    pub context_length: u32,
    pub quantization: Option<String>,
}

impl DiscoveredModel {
    pub fn into_catalog(
        self,
        input_price: String,
        output_price: String,
        license_id: String,
        commercial_hosting_confirmed: bool,
        mode: ProviderMode,
    ) -> CatalogModel {
        CatalogModel {
            id: sanitize_slug(&self.id),
            display_name: self.display_name,
            engine_model_id: self.id,
            description: "Automatically discovered by Token2Token Client.".into(),
            architecture: self.architecture,
            context_length: self.context_length,
            input_modalities: vec!["text".into()],
            output_modalities: vec!["text".into()],
            parameters: vec!["temperature".into(), "max_tokens".into(), "top_p".into()],
            license_id,
            commercial_hosting_confirmed,
            quantization: self.quantization,
            input_price,
            output_price,
            mode,
            ready: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub content: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[async_trait]
pub trait Engine: Send + Sync {
    fn name(&self) -> &'static str;
    async fn discover(&self) -> Result<Vec<DiscoveredModel>>;
    async fn chat(&self, request: &ChatRequest) -> Result<InferenceResult>;
}

pub fn build_engine(
    kind: EngineKind,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Box<dyn Engine>> {
    let base_url = Url::parse(base_url).context("invalid engine URL")?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;
    Ok(match kind {
        EngineKind::Ollama => Box::new(OllamaEngine { client, base_url }),
        EngineKind::OpenAiCompatible => Box::new(OpenAiEngine {
            client,
            base_url,
            api_key: api_key.filter(|value| !value.is_empty()).map(str::to_owned),
        }),
    })
}

struct OllamaEngine {
    client: Client,
    base_url: Url,
}

#[derive(Deserialize)]
struct OllamaTags {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    details: OllamaDetails,
}

#[derive(Default, Deserialize)]
struct OllamaDetails {
    #[serde(default)]
    family: String,
    #[serde(default)]
    parameter_size: String,
    quantization_level: Option<String>,
}

#[async_trait]
impl Engine for OllamaEngine {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn discover(&self) -> Result<Vec<DiscoveredModel>> {
        let response = self
            .client
            .get(self.base_url.join("/api/tags")?)
            .send()
            .await?
            .error_for_status()?
            .json::<OllamaTags>()
            .await?;
        Ok(response
            .models
            .into_iter()
            .map(|model| DiscoveredModel {
                display_name: model.name.clone(),
                id: model.name,
                architecture: if model.details.family.is_empty() {
                    "unknown".into()
                } else {
                    model.details.family
                },
                context_length: 4096,
                quantization: model.details.quantization_level.or_else(|| {
                    (!model.details.parameter_size.is_empty())
                        .then_some(model.details.parameter_size)
                }),
            })
            .collect())
    }

    async fn chat(&self, request: &ChatRequest) -> Result<InferenceResult> {
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|message| {
                json!({
                    "role": message.role,
                    "content": content_as_text(&message.content)
                })
            })
            .collect();
        let response = self
            .client
            .post(self.base_url.join("/api/chat")?)
            .json(&json!({
                "model": request.model.split('/').next_back().unwrap_or(&request.model),
                "messages": messages,
                "stream": false,
                "options": {
                    "temperature": request.temperature,
                    "num_predict": request.max_tokens
                }
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(InferenceResult {
            content: response
                .pointer("/message/content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            prompt_tokens: response["prompt_eval_count"].as_u64().unwrap_or(0),
            completion_tokens: response["eval_count"].as_u64().unwrap_or(0),
        })
    }
}

struct OpenAiEngine {
    client: Client,
    base_url: Url,
    api_key: Option<String>,
}

#[async_trait]
impl Engine for OpenAiEngine {
    fn name(&self) -> &'static str {
        "openai-compatible"
    }

    async fn discover(&self) -> Result<Vec<DiscoveredModel>> {
        let mut request = self.client.get(self.base_url.join("/v1/models")?);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let models = response["data"]
            .as_array()
            .context("models response has no data array")?;
        Ok(models
            .iter()
            .filter_map(|model| model["id"].as_str())
            .map(|id| DiscoveredModel {
                id: id.into(),
                display_name: id.into(),
                architecture: "unknown".into(),
                context_length: 4096,
                quantization: None,
            })
            .collect())
    }

    async fn chat(&self, request: &ChatRequest) -> Result<InferenceResult> {
        let mut outbound = self
            .client
            .post(self.base_url.join("/v1/chat/completions")?);
        if let Some(api_key) = &self.api_key {
            outbound = outbound.bearer_auth(api_key);
        }
        let response = outbound
            .json(&json!({
                "model": request.model.split('/').next_back().unwrap_or(&request.model),
                "messages": request.messages,
                "stream": false,
                "max_tokens": request.max_tokens,
                "temperature": request.temperature
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(InferenceResult {
            content: response
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            prompt_tokens: response
                .pointer("/usage/prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            completion_tokens: response
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
    }
}

fn content_as_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.into();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_slug(id: &str) -> String {
    id.to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_become_safe_slugs() {
        assert_eq!(sanitize_slug("Qwen2.5:7B Instruct"), "qwen2.5-7b-instruct");
    }
}
