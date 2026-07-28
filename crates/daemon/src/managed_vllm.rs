use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::process::{Command, Output};
use std::time::Duration;

pub const CONTAINER_NAME: &str = "token2token-managed-vllm";
const GPU_IMAGE: &str = "vllm/vllm-openai:v0.26.0";
const CPU_IMAGE: &str = "vllm/vllm-openai-cpu:latest-x86_64";

#[derive(Debug, Clone)]
pub struct StartOptions {
    pub model: String,
    pub port: u16,
    pub cpu: bool,
    pub max_model_len: u32,
    pub image: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeStatus {
    pub installed: bool,
    pub running: bool,
    pub container: &'static str,
    pub engine_url: Option<String>,
}

pub async fn start(options: StartOptions) -> Result<()> {
    ensure_supported_device(&options)?;
    ensure_docker()?;
    if container_exists()? {
        bail!("managed vLLM container already exists; run `token2token managed-vllm stop` first");
    }
    let image = options
        .image
        .as_deref()
        .unwrap_or(if options.cpu { CPU_IMAGE } else { GPU_IMAGE });
    let args = docker_run_args(&options, image);
    command("docker", &args).context("could not start managed vLLM")?;
    wait_until_ready(options.port).await?;
    println!("Managed vLLM is ready at http://127.0.0.1:{}", options.port);
    Ok(())
}

fn ensure_supported_device(options: &StartOptions) -> Result<()> {
    if options.model.trim().is_empty() {
        bail!("managed vLLM model cannot be empty");
    }
    if options.port < 1024 {
        bail!("managed vLLM port must be between 1024 and 65535");
    }
    if !(128..=1_000_000).contains(&options.max_model_len) {
        bail!("managed vLLM max model length must be between 128 and 1000000");
    }
    if options.cpu && !cfg!(target_arch = "x86_64") {
        bail!("managed vLLM CPU currently requires an x86_64 host");
    }
    if !options.cpu && cfg!(target_os = "macos") {
        bail!(
            "managed vLLM GPU is unavailable on macOS; connect Ollama or LM Studio, or run vLLM on an NVIDIA Linux/Windows host"
        );
    }
    Ok(())
}

pub fn stop() -> Result<()> {
    ensure_docker()?;
    if container_exists()? {
        command("docker", &["rm", "-f", CONTAINER_NAME])?;
    }
    println!("Managed vLLM is stopped");
    Ok(())
}

pub fn status(port: u16) -> Result<()> {
    ensure_docker()?;
    let installed = container_exists()?;
    let running = if installed {
        let output = raw_command(
            "docker",
            &["inspect", "-f", "{{.State.Running}}", CONTAINER_NAME],
        )?;
        String::from_utf8_lossy(&output.stdout).trim() == "true"
    } else {
        false
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&RuntimeStatus {
            installed,
            running,
            container: CONTAINER_NAME,
            engine_url: running.then(|| format!("http://127.0.0.1:{port}")),
        })?
    );
    Ok(())
}

fn docker_run_args(options: &StartOptions, image: &str) -> Vec<String> {
    let mut args = vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        CONTAINER_NAME.into(),
        "--restart".into(),
        "unless-stopped".into(),
        "-p".into(),
        format!("127.0.0.1:{}:8000", options.port),
        "-v".into(),
        "token2token-hf-cache:/root/.cache/huggingface".into(),
        "-v".into(),
        "token2token-vllm-cache:/root/.cache/vllm".into(),
    ];
    if options.cpu {
        args.extend([
            "-e".into(),
            "VLLM_CPU_KVCACHE_SPACE=4".into(),
            "-e".into(),
            "VLLM_CPU_NUM_OF_RESERVED_CPU=1".into(),
        ]);
    } else {
        args.extend(["--gpus".into(), "all".into(), "--ipc".into(), "host".into()]);
    }
    args.extend([
        image.into(),
        options.model.clone(),
        "--served-model-name".into(),
        model_alias(&options.model),
        "--max-model-len".into(),
        options.max_model_len.to_string(),
    ]);
    if options.cpu {
        args.extend(["--dtype".into(), "float32".into()]);
    }
    args
}

fn model_alias(model: &str) -> String {
    model
        .split('/')
        .next_back()
        .unwrap_or(model)
        .to_ascii_lowercase()
}

async fn wait_until_ready(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{port}/health");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    for _ in 0..180 {
        if client
            .get(&url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }
        if !container_running()? {
            bail!("managed vLLM exited during startup; inspect `docker logs {CONTAINER_NAME}`");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    bail!("managed vLLM did not become healthy within 180 seconds")
}

fn ensure_docker() -> Result<()> {
    command("docker", &["version", "--format", "{{.Server.Version}}"])
        .context("Docker is required for the managed vLLM runtime")?;
    Ok(())
}

fn container_exists() -> Result<bool> {
    let output = raw_command("docker", &["inspect", "-f", "{{.Id}}", CONTAINER_NAME])?;
    Ok(output.status.success())
}

fn container_running() -> Result<bool> {
    let output = raw_command(
        "docker",
        &["inspect", "-f", "{{.State.Running}}", CONTAINER_NAME],
    )?;
    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true")
}

fn command(program: &str, args: &[impl AsRef<std::ffi::OsStr>]) -> Result<Output> {
    let output = raw_command(program, args)?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

fn raw_command(program: &str, args: &[impl AsRef<std::ffi::OsStr>]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("could not execute {program}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_runtime_is_local_only_and_uses_persistent_caches() {
        let args = docker_run_args(
            &StartOptions {
                model: "Qwen/Qwen2.5-0.5B-Instruct".into(),
                port: 18000,
                cpu: true,
                max_model_len: 1024,
                image: None,
            },
            CPU_IMAGE,
        );
        assert!(args.contains(&"127.0.0.1:18000:8000".to_string()));
        assert!(args.contains(&"token2token-hf-cache:/root/.cache/huggingface".to_string()));
        assert!(!args.contains(&"--gpus".to_string()));
        assert_eq!(
            model_alias("Qwen/Qwen2.5-0.5B-Instruct"),
            "qwen2.5-0.5b-instruct"
        );
    }

    #[test]
    fn gpu_runtime_requests_all_gpus() {
        let args = docker_run_args(
            &StartOptions {
                model: "org/model".into(),
                port: 18000,
                cpu: false,
                max_model_len: 4096,
                image: Some("custom/image".into()),
            },
            "custom/image",
        );
        assert!(args.windows(2).any(|pair| pair == ["--gpus", "all"]));
        assert!(args.contains(&"custom/image".to_string()));
    }

    #[test]
    fn the_current_host_has_an_explicit_supported_device_result() {
        let options = StartOptions {
            model: "org/model".into(),
            port: 18000,
            cpu: cfg!(target_os = "macos"),
            max_model_len: 1024,
            image: None,
        };
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert!(ensure_supported_device(&options).is_err());
        } else {
            assert!(ensure_supported_device(&options).is_ok());
        }
    }

    #[test]
    fn invalid_runtime_configuration_is_rejected_before_docker() {
        let options = StartOptions {
            model: " ".into(),
            port: 80,
            cpu: true,
            max_model_len: 1,
            image: None,
        };
        assert!(ensure_supported_device(&options).is_err());
    }
}
