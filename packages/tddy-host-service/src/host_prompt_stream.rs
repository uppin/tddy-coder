//! The `StreamHostPrompts` pump: the host's questions, on the wire.
//!
//! Separate from `connection_service.rs` for the same reason `livekit_rooms_stream.rs` is — the
//! handler there is the auth check and the channel, and the loop that outlives it belongs where it
//! can be read on its own. `docs/dev/TODO.md` records that the connection
//! service is already 19,600 lines; this node adds to it as little as it can.
//!
//! ⚠ **The teardown is the point of this module.** A prompt feed is silent almost all the time — a
//! host raises a question only when an operator starts an add-key flow — so a send failure, the
//! usual way a handler learns its subscriber left, is *never attempted*. Per
//! `packages/tddy-codegen/docs/server-streaming.md` such a pump must watch `tx.closed()` directly,
//! or it parks forever on a prompt that will never come: one leaked task per browser tab that ever
//! opened the Hosts screen. Pinned by
//! `packages/tddy-daemon/tests/stream_host_prompts_rpc.rs::stops_the_prompt_pump_once_the_subscriber_is_gone`.
//!
//! Feature: `docs/ft/web/hosts-screen-add-key.md`

use crate::host_keypair::HostKeypair;
use crate::host_prompts::{HostPromptRegistry, PendingPrompt, PromptKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tddy_rpc::Status;
use tddy_service::proto::host::{HostPromptEvent, HostPromptKind};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::UnboundedSender;

/// Keeps [`HostServiceImpl::pending_prompt_pump_count`] honest for the life of one pump.
///
/// A guard rather than a decrement at the end of the loop: every way out of the pump — a closed
/// subscriber, a failed send, a panic — has to be counted, and a decrement written at one exit is a
/// decrement missing from the others.
///
/// [`HostServiceImpl::pending_prompt_pump_count`]: crate::service::HostServiceImpl::pending_prompt_pump_count
pub struct PumpCount(Arc<AtomicUsize>);

impl PumpCount {
    /// Count one pump as running until the returned guard is dropped.
    #[must_use]
    pub fn running(pumps: Arc<AtomicUsize>) -> Self {
        pumps.fetch_add(1, Ordering::SeqCst);
        Self(pumps)
    }
}

impl Drop for PumpCount {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Forward `subscriber`'s prompts to them until they go away.
///
/// Sends whatever is already outstanding first — an operator who opens the Hosts screen while a
/// prompt is waiting must see it, not wait for the next one — then follows the registry's feed.
///
/// `subscriber` is the **GitHub user** this subscription belongs to, and a prompt raised by anybody
/// else never reaches it. The registry's feed is host-wide, so the filter is here as well as in
/// [`HostPromptRegistry::pending`]: a prompt names a private-key path on this host, and the
/// operator who typed it is the only one it is for.
pub async fn pump_host_prompts(
    prompts: Arc<dyn HostPromptRegistry>,
    keypair: Arc<dyn HostKeypair>,
    daemon_instance_id: String,
    subscriber: String,
    tx: UnboundedSender<Result<HostPromptEvent, Status>>,
) {
    // Subscribed before the outstanding ones are read, so a prompt issued between the two is
    // delivered by the feed rather than falling into the gap.
    let mut issued = prompts.subscribe();

    for prompt in prompts.pending(&subscriber, crate::host_registry::now_unix_ms()) {
        if !forward(&prompt, &keypair, &daemon_instance_id, &tx) {
            return;
        }
    }

    loop {
        let prompt = tokio::select! {
            // An idle host sends nothing, so the send that would report a departed subscriber is
            // never reached. Without this arm the pump outlives its stream for the life of the
            // daemon — see the module docs.
            _ = tx.closed() => return,
            issued = issued.recv() => match issued {
                Ok(prompt) => prompt,
                // The registry outlives every subscription in practice; if it ever does not, this
                // host can raise no further prompts and the subscription has nothing left to carry.
                Err(RecvError::Closed) => return,
                // The buffer is sized for far more than the one-at-a-time prompts an operator
                // raises, so this means something went badly wrong upstream — and the operator is
                // left waiting on a dialog that never opens, which is worth a line in the log.
                Err(RecvError::Lagged(missed)) => {
                    log::warn!(
                        target: "tddy_daemon::host_prompt_stream",
                        "a host prompt subscriber fell {missed} prompts behind; those prompts will \
                         not reach it and will expire unanswered"
                    );
                    continue;
                }
            },
        };
        // Everything the registry publishes arrives here, including other operators' prompts; this
        // subscription carries only its own.
        if prompt.issued_for != subscriber {
            continue;
        }
        if !forward(&prompt, &keypair, &daemon_instance_id, &tx) {
            return;
        }
    }
}

/// Send one prompt, reporting whether the subscription is still worth pumping.
fn forward(
    prompt: &PendingPrompt,
    keypair: &Arc<dyn HostKeypair>,
    daemon_instance_id: &str,
    tx: &UnboundedSender<Result<HostPromptEvent, Status>>,
) -> bool {
    // Read per prompt rather than once at subscribe: the key is cached behind the keypair, and a
    // subscription opened before this host had ever generated one would otherwise carry a stale
    // (or absent) key for as long as the browser stayed connected.
    let published = match keypair.published() {
        Ok(published) => published,
        Err(reason) => {
            // Dropping the prompt rather than the subscription: without a public key the answer
            // could only travel in the clear, which is the one thing this node exists to prevent.
            log::error!(
                target: "tddy_daemon::host_prompt_stream",
                "this host cannot publish a key for prompt {}, so it cannot be answered: {reason}",
                prompt.prompt_id
            );
            return true;
        }
    };

    // Always a frame, never a status: every failure this pump can hit is either the subscriber
    // leaving or a prompt it cannot publish a key for, and neither ends the subscription.
    tx.send(Ok(HostPromptEvent {
        prompt_id: prompt.prompt_id.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        kind: wire_kind(prompt.kind) as i32,
        subject: prompt.subject.clone(),
        host_public_key: published.spki_der,
        host_public_key_fingerprint: published.fingerprint,
        expires_at_unix_ms: prompt.expires_at_unix_ms,
    }))
    .is_ok()
}

fn wire_kind(kind: PromptKind) -> HostPromptKind {
    match kind {
        PromptKind::SshKeyPassphrase => HostPromptKind::SshKeyPassphrase,
        PromptKind::DesktopPassword => HostPromptKind::DesktopPassword,
    }
}
