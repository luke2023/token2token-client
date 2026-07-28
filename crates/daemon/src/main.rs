mod config;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use config::{Config, UsageState, default_path};
use futures_util::{SinkExt, StreamExt};
use std::{path::PathBuf, sync::Arc, time::Duration};
use token2token_connectors::{Engine, EngineKind, build_engine};
use token2token_protocol::{
    Catalog, CompletedPayload, FailedPayload, Heartbeat, JobPayload, MessageType, RelayMessage,
};
use tokio::sync::Semaphore;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use url::Url;

#[derive(Parser)]
#[command(
    name = "token2token",
    version,
    about = "Share GPU. Earn Indigo. Run any model."
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        token: String,
        #[arg(long, default_value = "wss://api.tokens2tokens.com/v1/nodes/connect")]
        relay: String,
        #[arg(long, default_value = "ollama")]
        engine: String,
        #[arg(long, default_value = "http://127.0.0.1:11434")]
        engine_url: String,
        #[arg(long)]
        accept_commercial_terms: bool,
    },
    Discover,
    Run,
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let cli = Cli::parse();
    let path = cli.config.map(Ok).unwrap_or_else(default_path)?;
    match cli.command {
        Command::Init {
            token,
            relay,
            engine,
            engine_url,
            accept_commercial_terms,
        } => {
            let config = Config {
                relay_url: relay,
                enrollment_token: token,
                engine,
                engine_url,
                commercial_hosting_confirmed: accept_commercial_terms,
                ..Config::default()
            };
            config.save(&path).await?;
            println!("Saved {}", path.display());
        }
        Command::Discover => {
            let config = Config::load(&path).await?;
            let engine = engine(&config)?;
            for model in engine.discover().await? {
                println!(
                    "{}\t{}\t{}",
                    model.id, model.architecture, model.context_length
                );
            }
        }
        Command::Run => run(Config::load(&path).await?, path).await?,
        Command::Config => println!("{}", path.display()),
    }
    Ok(())
}

fn engine(config: &Config) -> Result<Box<dyn Engine>> {
    build_engine(
        config.engine.parse::<EngineKind>()?,
        &config.engine_url,
        Some(&config.engine_api_key),
    )
}

async fn run(config: Config, config_path: PathBuf) -> Result<()> {
    if config.enrollment_token.is_empty() {
        bail!("enrollment_token is required; run `token2token init` first");
    }
    if !config.commercial_hosting_confirmed {
        bail!("commercial hosting terms must be confirmed before publishing models");
    }
    let engine: Arc<dyn Engine> = engine(&config)?.into();
    let mut backoff = 1;
    loop {
        match relay_session(&config, &config_path, engine.clone()).await {
            Ok(()) => {
                info!("provider stopped");
                return Ok(());
            }
            Err(error) => error!(%error, "relay session failed"),
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn relay_session(
    config: &Config,
    config_path: &std::path::Path,
    engine: Arc<dyn Engine>,
) -> Result<()> {
    let relay = Url::parse(&config.relay_url)?;
    let mut request = relay.as_str().into_client_request()?;
    request.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", config.enrollment_token))?,
    );
    let (socket, _) = connect_async(request)
        .await
        .context("could not connect to relay")?;
    info!(url = %config.relay_url, "connected to Token2Token");
    let (mut sender, mut receiver) = socket.split();
    let hello = receiver
        .next()
        .await
        .context("relay closed before enrollment")??;
    let hello: RelayMessage = serde_json::from_str(hello.to_text()?)?;
    if hello.kind != MessageType::Hello {
        bail!("relay did not confirm node enrollment");
    }

    let models = engine
        .discover()
        .await?
        .into_iter()
        .map(|model| {
            model.into_catalog(
                config.input_price.clone(),
                config.output_price.clone(),
                config.model_license.clone(),
                config.commercial_hosting_confirmed,
                config.mode,
            )
        })
        .collect();
    sender
        .send(Message::Text(
            serde_json::to_string(&RelayMessage::new(
                MessageType::Catalog,
                Catalog { models },
            )?)?
            .into(),
        ))
        .await?;

    let semaphore = Arc::new(Semaphore::new(1));
    let mut usage = UsageState::load(config_path).await?;
    let earnings_cap = config
        .monthly_earnings_cap
        .parse::<f64>()
        .unwrap_or(f64::MAX);
    let input_price = config.input_price.parse::<f64>().unwrap_or(0.0);
    let output_price = config.output_price.parse::<f64>().unwrap_or(0.0);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let message = RelayMessage::new(MessageType::Heartbeat, Heartbeat {
                    engine: engine.name().into(),
                    engine_url: config.engine_url.clone(),
                    accepting_jobs: semaphore.available_permits() > 0 && usage.earned < earnings_cap,
                    active_jobs: (1 - semaphore.available_permits()) as u32,
                    earned_this_month: format!("{:.9}", usage.earned),
                    earnings_cap: config.monthly_earnings_cap.clone(),
                    client_version: env!("CARGO_PKG_VERSION").into(),
                })?;
                sender.send(Message::Text(serde_json::to_string(&message)?.into())).await?;
            }
            incoming = receiver.next() => {
                let Some(incoming) = incoming else { break };
                let incoming = incoming?;
                if !incoming.is_text() { continue }
                let message: RelayMessage = serde_json::from_str(incoming.to_text()?)?;
                if message.kind != MessageType::Job { continue }
                let correlation_id = message.correlation_id.context("job has no correlation id")?;
                let job: JobPayload = serde_json::from_value(message.payload)?;
                if usage.earned >= earnings_cap {
                    let failed = RelayMessage::correlated(
                        MessageType::Failed,
                        correlation_id,
                        FailedPayload { job_id: job.job_id, message: "monthly earnings cap reached".into() },
                    )?;
                    sender.send(Message::Text(serde_json::to_string(&failed)?.into())).await?;
                    continue;
                }
                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        let failed = RelayMessage::correlated(
                            MessageType::Failed,
                            correlation_id,
                            FailedPayload { job_id: job.job_id, message: "node is busy".into() },
                        )?;
                        sender.send(Message::Text(serde_json::to_string(&failed)?.into())).await?;
                        continue;
                    }
                };
                let result = engine.chat(&job.request).await;
                drop(permit);
                let response = match result {
                    Ok(result) => {
                        usage.earned += ((result.prompt_tokens as f64 * input_price
                            + result.completion_tokens as f64 * output_price)
                            / 1_000_000.0)
                            * 0.95;
                        usage.save(config_path).await?;
                        RelayMessage::correlated(
                            MessageType::Completed,
                            correlation_id,
                            CompletedPayload {
                            job_id: job.job_id,
                            content: result.content,
                            prompt_tokens: result.prompt_tokens,
                            completion_tokens: result.completion_tokens,
                            },
                        )?
                    }
                    Err(error) => RelayMessage::correlated(
                        MessageType::Failed,
                        correlation_id,
                        FailedPayload { job_id: job.job_id, message: error.to_string() },
                    )?,
                };
                sender.send(Message::Text(serde_json::to_string(&response)?.into())).await?;
            }
            _ = tokio::signal::ctrl_c() => {
                sender.send(Message::Close(None)).await?;
                return Ok(());
            }
        }
    }
    Err(anyhow::anyhow!("relay closed the connection"))
}
