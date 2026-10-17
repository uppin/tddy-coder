//! The prompt-and-wait behind the helper's request, and the loopback server that carries it.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_askpass::prompt_text::{classify, AskpassPrompt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::grants::{CloneGrant, CloneGrants};
use super::outcome::{AskpassOutcome, AskpassReply, Passphrase};
use crate::host_keypair::HostKeypair;
use crate::host_prompts::{HostPromptRegistry, PromptKind, PROMPT_TTL};
use crate::host_registry::now_unix_ms;

/// What requests are decided against; shared with the server task.
pub(super) struct Core {
    prompts: Arc<dyn HostPromptRegistry>,
    keypair: Arc<dyn HostKeypair>,
    grants: CloneGrants,
    answer_window_ms: AtomicU64,
}

/// The daemon's loopback askpass endpoint. Serving stops when it is dropped.
pub struct AskpassEndpoint {
    core: Arc<Core>,
    url: String,
    server: JoinHandle<()>,
}

impl AskpassEndpoint {
    /// Bind `127.0.0.1` on a free port and serve helper requests.
    pub async fn start(
        prompts: Arc<dyn HostPromptRegistry>,
        keypair: Arc<dyn HostKeypair>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
        let url = format!("http://{}", listener.local_addr()?);
        let core = Arc::new(Core {
            prompts,
            keypair,
            grants: CloneGrants::for_endpoint(url.clone()),
            answer_window_ms: AtomicU64::new(millis(PROMPT_TTL)),
        });
        let router = super::http::router(Arc::clone(&core));
        let server = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router).await {
                log::error!("askpass endpoint stopped serving: {e}");
            }
        });
        log::info!("askpass endpoint listening at {url}");
        Ok(Self { core, url, server })
    }

    /// Cap how long a request waits for an answer. Defaults to the prompt's own lifetime.
    #[must_use]
    pub fn answer_window(self, window: Duration) -> Self {
        self.core
            .answer_window_ms
            .store(millis(window), Ordering::Relaxed);
        self
    }

    /// Admit the helper of a clone `operator` started, until the grant is dropped.
    #[must_use]
    pub fn register_clone(&self, operator: &str) -> CloneGrant {
        self.core.grants.register(operator)
    }

    /// The endpoint's base URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Decide one helper request: raise a prompt for the clone's operator and wait for the answer.
    pub async fn ask(&self, token: &str, prompt_text: &str) -> AskpassOutcome {
        self.core.ask(token, prompt_text).await.outcome
    }

    /// [`Self::ask`], with the flag the helper turns into a hint for the operator.
    pub async fn ask_reporting_rejection(&self, token: &str, prompt_text: &str) -> AskpassReply {
        self.core.ask(token, prompt_text).await
    }
}

impl Drop for AskpassEndpoint {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Core {
    pub(super) async fn ask(&self, token: &str, prompt_text: &str) -> AskpassReply {
        let Some(operator) = self.grants.operator_for(token) else {
            log::warn!("askpass: request refused, no clone holds that token");
            return AskpassReply::of(AskpassOutcome::Refused(
                "no clone in progress holds this token".to_string(),
            ));
        };
        let AskpassPrompt::KeyPassphrase { key } = classify(prompt_text) else {
            log::warn!("askpass: request refused, not a key passphrase request");
            return AskpassReply::of(AskpassOutcome::Refused(
                "not a key passphrase request".to_string(),
            ));
        };

        let outcome = self.prompt_for_answer(&operator, &key).await;
        if !matches!(outcome, AskpassOutcome::Answer(_)) {
            return AskpassReply::of(outcome);
        }
        // ssh asks again for a key only when what it was given did not unlock it.
        let previous_answer_rejected = self.grants.was_answered_for(token, &key);
        self.grants.record_answer_for(token, &key);
        AskpassReply {
            outcome,
            previous_answer_rejected,
        }
    }

    async fn prompt_for_answer(&self, operator: &str, key: &str) -> AskpassOutcome {
        // Stamped with the GitHub user whose clone this is: nobody else is shown it, and nobody
        // else can spend its one answer.
        let prompt = self
            .prompts
            .issue(operator, PromptKind::SshKeyPassphrase, key, now_unix_ms());
        // Claimed immediately after issuing, because issuing puts the prompt on the feed and a
        // browser may answer at once.
        let Some(handoff) = self.prompts.awaited_answer(&prompt.prompt_id) else {
            log::warn!("askpass: prompt vanished before it could be awaited");
            return AskpassOutcome::Expired;
        };
        let until_expiry = prompt.expires_at_unix_ms.saturating_sub(now_unix_ms());
        let until_expiry = Duration::from_millis(u64::try_from(until_expiry).unwrap_or(0));
        let window = Duration::from_millis(self.answer_window_ms.load(Ordering::Relaxed));
        let ciphertext = match tokio::time::timeout(until_expiry.min(window), handoff).await {
            Ok(Ok(ciphertext)) => ciphertext,
            _ => {
                log::info!("askpass: passphrase prompt expired unanswered");
                return AskpassOutcome::Expired;
            }
        };

        // An RSA decrypt blocks, so it runs on the blocking pool rather than parking a worker.
        let keypair = Arc::clone(&self.keypair);
        let decrypted = tokio::task::spawn_blocking(move || keypair.decrypt(&ciphertext)).await;
        match decrypted {
            Ok(Ok(plaintext)) => {
                log::info!("askpass: passphrase answered");
                AskpassOutcome::Answer(Passphrase::new(plaintext))
            }
            Ok(Err(_)) | Err(_) => {
                log::warn!("askpass: the answer did not decrypt");
                AskpassOutcome::Refused("the answer could not be decrypted".to_string())
            }
        }
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
