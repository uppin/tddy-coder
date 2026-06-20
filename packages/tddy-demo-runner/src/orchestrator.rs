//! `DemoOrchestrator` — drives the agent-side demo cycle (deploy → verify → forward → notify).
//!
//! The VM is **not** booted here. Boot is UI-triggered (daemon RPC `StartDemoVm`) and the caller
//! passes an already-running `RunningVm` handle to `run()`.
//!
//! Sequence:
//! 1. Deploy the app via SSH (`deploy_steps`) to the provided `RunningVm`
//! 2. Verify the app is running (`verify_command`)
//! 3. Open/confirm the port-forward and build the `share_url`
//! 4. Post the link to all configured Telegram chats
//! 5. Return `DemoResult` (share_url, steps_completed, verification text)

use crate::vm::{DemoVm, DemoVmError, RunningVm};
use std::sync::Arc;
use tddy_workflow_recipes::parser::{DemoMode, DemoPlan};

/// Result of a completed demo.
#[derive(Debug, Clone)]
pub struct DemoResult {
    pub share_url: String,
    pub steps_completed: u32,
    pub verification: String,
}

/// Error from an orchestration run.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("VM error: {0}")]
    Vm(#[from] DemoVmError),
    #[error("Telegram error: {0}")]
    Telegram(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Telegram sender abstraction (mirrors `tddy-daemon`'s `TelegramSender` trait but without
/// pulling in that entire package as a dep; the daemon wires a `TelegramNotifier` impl).
#[async_trait::async_trait]
pub trait TelegramNotifier: Send + Sync {
    /// Post a plain-text message to the given chat.
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), String>;
}

/// Orchestrates the demo step: boot VM → deploy → verify → forward → notify.
pub struct DemoOrchestrator {
    vm: Arc<dyn DemoVm>,
    telegram: Option<Arc<dyn TelegramNotifier>>,
    /// Chat IDs to notify when the link is ready.
    chat_ids: Vec<i64>,
}

impl DemoOrchestrator {
    pub fn new(
        vm: Arc<dyn DemoVm>,
        telegram: Option<Arc<dyn TelegramNotifier>>,
        chat_ids: Vec<i64>,
    ) -> Self {
        Self {
            vm,
            telegram,
            chat_ids,
        }
    }

    /// Deploy, verify, and forward for the given already-running VM.
    ///
    /// The VM must already be booted (triggered by the UI `StartDemoVm` action). This method
    /// never calls `boot()` — it operates entirely on the provided `vm` handle.
    ///
    /// # Errors
    /// Returns `OrchestratorError::Config` if the recipe has no `mode` set or is missing
    /// required fields. Returns `OrchestratorError::Vm` if deploy/verify/forward fails.
    pub async fn run(
        &self,
        recipe: &DemoPlan,
        vm: RunningVm,
    ) -> Result<DemoResult, OrchestratorError> {
        match &recipe.mode {
            Some(DemoMode::PortForward) => {}
            Some(DemoMode::ScreenShare) => {
                return Err(OrchestratorError::Config(
                    "ScreenShare mode is not yet supported".into(),
                ));
            }
            None => {
                return Err(OrchestratorError::Config("recipe has no mode set".into()));
            }
        }

        let port_map = recipe
            .hostfwd
            .first()
            .ok_or_else(|| OrchestratorError::Config("no hostfwd ports configured".into()))?;

        self.vm.deploy(&vm, &recipe.deploy_steps).await?;

        let verify_result = self
            .vm
            .verify(&vm, recipe.verify_command.as_deref().unwrap_or_default())
            .await?;
        if !verify_result.success {
            return Err(OrchestratorError::Vm(DemoVmError::VerifyFailed(
                verify_result.output,
            )));
        }

        let handle = self.vm.forward(&vm, port_map).await?;

        self.notify_telegram(&handle.share_url).await?;

        Ok(DemoResult {
            share_url: handle.share_url,
            steps_completed: recipe.deploy_steps.len() as u32,
            verification: verify_result.output,
        })
    }

    /// Post the share URL to all configured Telegram chats.
    async fn notify_telegram(&self, share_url: &str) -> Result<(), OrchestratorError> {
        if let Some(notifier) = &self.telegram {
            let message = format!("Demo is ready: {share_url}");
            for chat_id in &self.chat_ids {
                notifier
                    .send(*chat_id, &message)
                    .await
                    .map_err(OrchestratorError::Telegram)?;
            }
        }
        Ok(())
    }
}
