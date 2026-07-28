use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_shell::{
    ShellExt,
    process::{CommandChild, CommandEvent},
};
use token2token_connectors::{EngineKind, build_engine};
use token2token_protocol::ProviderMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct DesktopConfig {
    relay_url: String,
    enrollment_token: String,
    engine: String,
    engine_url: String,
    engine_api_key: String,
    input_price: String,
    output_price: String,
    monthly_earnings_cap: String,
    commercial_hosting_confirmed: bool,
    model_license: String,
    mode: ProviderMode,
}

impl Default for DesktopConfig {
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

struct ProviderProcess(Mutex<Option<CommandChild>>);

fn config_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("client.json"))
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn load_config(app: tauri::AppHandle) -> Result<DesktopConfig, String> {
    let path = config_path(&app)?;
    match tokio::fs::read_to_string(path).await {
        Ok(content) => serde_json::from_str(&content).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DesktopConfig::default()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: DesktopConfig) -> Result<(), String> {
    let path = config_path(&app)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    tokio::fs::write(path, serde_json::to_vec_pretty(&config).unwrap())
        .await
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(config_path(&app)?, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn discover_models(
    config: DesktopConfig,
) -> Result<Vec<token2token_connectors::DiscoveredModel>, String> {
    let kind = config
        .engine
        .parse::<EngineKind>()
        .map_err(|error| error.to_string())?;
    build_engine(kind, &config.engine_url, Some(&config.engine_api_key))
        .map_err(|error| error.to_string())?
        .discover()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_provider(
    app: tauri::AppHandle,
    process: tauri::State<'_, ProviderProcess>,
) -> Result<(), String> {
    if process.0.lock().unwrap().is_some() {
        return Ok(());
    }
    let config = config_path(&app)?;
    let sidecar = app
        .shell()
        .sidecar("token2token")
        .map_err(|error| error.to_string())?
        .args(["--config", &config.to_string_lossy(), "run"]);
    let (mut events, child) = sidecar.spawn().map_err(|error| error.to_string())?;
    *process.0.lock().unwrap() = Some(child);
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let CommandEvent::Stderr(bytes) = event {
                eprintln!("{}", String::from_utf8_lossy(&bytes));
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn stop_provider(process: tauri::State<'_, ProviderProcess>) -> Result<(), String> {
    if let Some(child) = process.0.lock().unwrap().take() {
        child.kill().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(ProviderProcess(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            discover_models,
            start_provider,
            stop_provider
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token2Token");
}
