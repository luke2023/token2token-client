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
struct ManagedVllmStatus {
    installed: bool,
    running: bool,
    container: String,
    engine_url: Option<String>,
}

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
    let process_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stderr(bytes) => {
                    eprintln!("{}", String::from_utf8_lossy(&bytes));
                }
                CommandEvent::Terminated(_) => {
                    process_app
                        .state::<ProviderProcess>()
                        .0
                        .lock()
                        .unwrap()
                        .take();
                    break;
                }
                _ => {}
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

async fn managed_vllm_command(app: &tauri::AppHandle, args: Vec<String>) -> Result<String, String> {
    let output = app
        .shell()
        .sidecar("token2token")
        .map_err(|error| error.to_string())?
        .args(args)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            "managed vLLM command failed".into()
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[tauri::command]
async fn managed_vllm_status(
    app: tauri::AppHandle,
    port: u16,
) -> Result<ManagedVllmStatus, String> {
    let output = managed_vllm_command(
        &app,
        vec![
            "managed-vllm".into(),
            "status".into(),
            "--port".into(),
            port.to_string(),
        ],
    )
    .await?;
    serde_json::from_str(&output).map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_managed_vllm(
    app: tauri::AppHandle,
    model: String,
    port: u16,
    cpu: bool,
    max_model_len: u32,
) -> Result<ManagedVllmStatus, String> {
    let mut args = vec![
        "managed-vllm".into(),
        "start".into(),
        "--model".into(),
        model,
        "--port".into(),
        port.to_string(),
        "--max-model-len".into(),
        max_model_len.to_string(),
    ];
    if cpu {
        args.push("--cpu".into());
    }
    managed_vllm_command(&app, args).await?;
    managed_vllm_status(app, port).await
}

#[tauri::command]
async fn stop_managed_vllm(app: tauri::AppHandle, port: u16) -> Result<ManagedVllmStatus, String> {
    managed_vllm_command(&app, vec!["managed-vllm".into(), "stop".into()]).await?;
    managed_vllm_status(app, port).await
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
            stop_provider,
            managed_vllm_status,
            start_managed_vllm,
            stop_managed_vllm
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token2Token");
}
