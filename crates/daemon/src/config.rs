use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use token2token_protocol::ProviderMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub relay_url: String,
    pub enrollment_token: String,
    pub engine: String,
    pub engine_url: String,
    pub engine_api_key: String,
    pub input_price: String,
    pub output_price: String,
    pub monthly_earnings_cap: String,
    pub commercial_hosting_confirmed: bool,
    pub model_license: String,
    pub mode: ProviderMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            relay_url: "wss://api.tokens2tokens.com/v1/nodes/connect".into(),
            enrollment_token: String::new(),
            engine: "ollama".into(),
            engine_url: "http://127.0.0.1:11434".into(),
            engine_api_key: String::new(),
            input_price: "0.20".into(),
            output_price: "0.80".into(),
            monthly_earnings_cap: "50000".into(),
            commercial_hosting_confirmed: false,
            model_license: "unknown".into(),
            mode: ProviderMode::Static,
        }
    }
}

impl Config {
    pub async fn load(path: &Path) -> Result<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("could not read {}", path.display()))?;
        serde_json::from_str(&content).context("invalid client config")
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, serde_json::to_vec_pretty(self)?).await?;
        secure_permissions(path).await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageState {
    pub month: String,
    pub earned: f64,
}

impl UsageState {
    pub async fn load(config_path: &Path) -> Result<Self> {
        let month = chrono::Utc::now().format("%Y-%m").to_string();
        let path = state_path(config_path);
        let existing = tokio::fs::read_to_string(&path)
            .await
            .ok()
            .and_then(|value| serde_json::from_str::<Self>(&value).ok());
        Ok(match existing {
            Some(value) if value.month == month => value,
            _ => Self { month, earned: 0.0 },
        })
    }

    pub async fn save(&self, config_path: &Path) -> Result<()> {
        let path = state_path(config_path);
        tokio::fs::write(&path, serde_json::to_vec_pretty(self)?).await?;
        secure_permissions(&path).await
    }
}

fn state_path(config_path: &Path) -> PathBuf {
    config_path.with_extension("state.json")
}

#[cfg(unix)]
async fn secure_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn secure_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

pub fn default_path() -> Result<PathBuf> {
    let directory = dirs::config_dir().context("operating system has no config directory")?;
    Ok(directory.join("token2token").join("client.json"))
}
