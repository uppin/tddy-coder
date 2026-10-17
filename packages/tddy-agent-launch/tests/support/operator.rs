//! The human at the other end of a host prompt, as a task.
//!
//! Answers exactly the way the browser does: encrypts under the host's published public key and
//! submits to the registry on the operator's own identity. Everything between the clone and this
//! answer is the real daemon code.

use std::sync::{Arc, Mutex};

use tddy_host_service::host_keypair::HostKeypair;
use tddy_host_service::host_prompts::{HostPromptRegistry, PendingPrompt};
use tddy_host_service::host_registry::now_unix_ms;
use tddy_host_service::test_util::encrypt_for_host;

pub struct ScriptedOperator {
    raised: Arc<Mutex<Vec<PendingPrompt>>>,
    task: tokio::task::JoinHandle<()>,
}

/// An operator signed in as `github_user`, watching `prompts` from now on.
///
/// Subscribed before this returns, so a prompt raised the instant a clone starts cannot be missed.
pub fn an_operator_answering(
    answer: &'static str,
    github_user: &'static str,
    prompts: Arc<dyn HostPromptRegistry>,
    keypair: Arc<dyn HostKeypair>,
) -> ScriptedOperator {
    let raised = Arc::new(Mutex::new(Vec::new()));
    let mut feed = prompts.subscribe();
    let seen = Arc::clone(&raised);
    let task = tokio::spawn(async move {
        while let Ok(prompt) = feed.recv().await {
            if prompt.issued_for != github_user {
                continue;
            }
            seen.lock().expect("the raised list").push(prompt.clone());
            let spki = keypair
                .published()
                .expect("the host publishes a key")
                .spki_der;
            // Refused answers (an expired or already-answered prompt) are the registry's to report;
            // the clone sees them as its own failure, which is what the test asserts on.
            let _ = prompts.answer(
                &prompt.prompt_id,
                github_user,
                encrypt_for_host(&spki, answer.as_bytes()),
                now_unix_ms(),
            );
        }
    });
    ScriptedOperator { raised, task }
}

impl ScriptedOperator {
    /// Every prompt raised for this operator so far.
    pub fn prompts_raised(&self) -> Vec<PendingPrompt> {
        self.raised.lock().expect("the raised list").clone()
    }
}

impl Drop for ScriptedOperator {
    fn drop(&mut self) {
        self.task.abort();
    }
}
