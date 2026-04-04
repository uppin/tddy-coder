//! gRPC client: subscribe to child `tddy-coder` [`PresenterObserver`] and drive [`TelegramSessionWatcher`].

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tonic::transport::Endpoint;

use tddy_service::gen::presenter_observer_client::PresenterObserverClient;
use tddy_service::gen::ObserveRequest;

use crate::config::DaemonConfig;
use crate::telegram_notifier::{TelegramSender, TelegramSessionWatcher};

const OBSERVER_CONNECT_MAX_ATTEMPTS: u32 = 90;
const OBSERVER_RETRY_DELAY_MS: u64 = 100;

/// Shared handles for Telegram + watcher; used to spawn per-session observer tasks from ConnectionService.
pub struct TelegramDaemonHooks {
    pub config: DaemonConfig,
    pub sender: Arc<dyn TelegramSender + Send + Sync>,
    pub watcher: Arc<Mutex<TelegramSessionWatcher>>,
}

impl TelegramDaemonHooks {
    /// Spawn a background task that connects to `127.0.0.1:{grpc_port}` and processes Presenter events.
    pub fn spawn_presenter_observer_task(&self, session_id: &str, grpc_port: u16) {
        let config = self.config.clone();
        let sender = self.sender.clone();
        let watcher = self.watcher.clone();
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            match run_presenter_observer_loop(config, sender, watcher, session_id, grpc_port).await
            {
                Ok(()) => {}
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::telegram",
                        "presenter observer task ended with error: {e}"
                    );
                }
            }
        });
    }
}

async fn connect_observer_endpoint(grpc_port: u16) -> anyhow::Result<tonic::transport::Channel> {
    let uri = format!("http://127.0.0.1:{}", grpc_port);
    let mut last_err = None::<String>;
    for attempt in 0..OBSERVER_CONNECT_MAX_ATTEMPTS {
        match Endpoint::from_shared(uri.clone())?.connect().await {
            Ok(ch) => return Ok(ch),
            Err(e) => {
                last_err = Some(e.to_string());
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "presenter observer connect attempt {} to {} failed: {}",
                    attempt + 1,
                    uri,
                    last_err.as_deref().unwrap_or("")
                );
                tokio::time::sleep(Duration::from_millis(OBSERVER_RETRY_DELAY_MS)).await;
            }
        }
    }
    anyhow::bail!(
        "gRPC PresenterObserver connect failed after {} attempts (last_err={})",
        OBSERVER_CONNECT_MAX_ATTEMPTS,
        last_err.unwrap_or_default()
    )
}

async fn run_presenter_observer_loop(
    config: DaemonConfig,
    sender: Arc<dyn TelegramSender + Send + Sync>,
    watcher: Arc<Mutex<TelegramSessionWatcher>>,
    session_id: String,
    grpc_port: u16,
) -> anyhow::Result<()> {
    let channel = connect_observer_endpoint(grpc_port).await?;
    let mut client = PresenterObserverClient::new(channel);
    let mut stream = client.observe_events(ObserveRequest {}).await?.into_inner();

    while let Some(result) = stream.message().await? {
        let mut guard = watcher.lock().await;
        guard
            .on_server_message(&config, sender.as_ref(), &session_id, &result)
            .await?;
    }
    Ok(())
}
