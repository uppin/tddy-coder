//! One model call at a time per provider endpoint, with the waiting kept on this side of the
//! socket.
//!
//! A local provider is a single-slot resource: it runs one generation at a time and holds every
//! other request open until that one ends. Session 01a0e200 is what that costs when nothing models
//! it — two specialized agents were attached to one session and both resolved to the same local
//! Ollama; the first one's turn entered a runaway and held its only runner, and the second's turn,
//! issued fourteen minutes later, sat *inside Ollama's socket* for 38m21s without executing a
//! token. Ollama never evaluated it: `starting mlx runner subprocess model=gemma4:e4b-mlx` is
//! timestamped one second after the operator cancelled, and the request died `499 | 38m21s`
//! beside the runaway's `500 | 52m9s`.
//!
//! The waiting already existed. This module does not add any — it *moves* it, from the provider's
//! queue to ours, where it can be counted ([`ProviderQueue::waiting`]), reported
//! ([`ProviderQueue::position_of`]), bounded ([`ProviderQueue::with_wait_timeout`]) and cancelled
//! (dropping the [`ProviderQueue::admit`] future leaves the line).
//!
//! **Keyed by endpoint, not by model.** The contended resource is the server: two models on one
//! Ollama contend exactly as two calls to one model do, and two different endpoints do not contend
//! at all. Keying by model would serialise nothing that matters and keying every provider together
//! would invent the very delay this exists to remove.
//!
//! **Injected, never global.** A queue is carried by [`crate::subagent::SubagentConfig`] the way
//! codebase access is, so the conversations that are meant to contend share one and nothing else
//! does.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// How long a caller waits for a busy provider before it is refused.
///
/// Ten minutes covers roughly three maximal generations ahead of it:
/// [`crate::subagent::SUBAGENT_MAX_OUTPUT_TOKENS`] bounds one generation to 4096 tokens, which is
/// about three minutes at the 24 t/s session 01a0e200 was observed generating at. So a turn queued
/// behind a couple of genuinely long ones still runs, and one behind a provider that has stopped
/// making progress is told so instead of waiting out the 38 minutes the incident did.
///
/// It bounds **the wait, not the call**. A turn admitted at 9m59s may then spend as long inside
/// the provider as the provider takes: a subagent request has no wall-clock deadline yet
/// (docs/dev/todo/2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md), and this budget must
/// not be read as one.
pub const DEFAULT_PROVIDER_WAIT: Duration = Duration::from_secs(600);

/// Why a caller never reached the provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderQueueError {
    /// The caller stood in the line for its whole budget and nothing became free.
    ///
    /// `waited` is the budget that expired rather than a measured elapsed time: the caller is
    /// being told which deadline it hit, and a figure that drifts by a scheduler tick would make
    /// the same refusal unreportable twice.
    WaitedTooLong { provider: String, waited: Duration },
}

impl std::fmt::Display for ProviderQueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WaitedTooLong { provider, waited } => write!(
                f,
                "waited {waited:?} for a free slot on {provider} and it never came free: \
                 the endpoint runs one request at a time and an earlier turn still holds it"
            ),
        }
    }
}

impl std::error::Error for ProviderQueueError {}

/// Admission to one provider endpoint at a time, shared by every conversation that talks to it.
///
/// Cheap to clone — the line is behind an `Arc`, so a clone is the same queue and not a second
/// one. The wait budget is *not* shared: it is a property of the caller's patience rather than of
/// the endpoint, so [`Self::with_wait_timeout`] gives one caller a different deadline without
/// changing anybody else's.
#[derive(Clone)]
pub struct ProviderQueue {
    lines: Arc<Mutex<ProviderLines>>,
    wait_timeout: Duration,
}

impl ProviderQueue {
    pub fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(ProviderLines::default())),
            wait_timeout: DEFAULT_PROVIDER_WAIT,
        }
    }

    /// Wait at most `budget` for a slot, instead of [`DEFAULT_PROVIDER_WAIT`].
    #[must_use]
    pub fn with_wait_timeout(mut self, budget: Duration) -> Self {
        self.wait_timeout = budget;
        self
    }

    /// Take `provider`'s only slot, waiting behind whoever holds it for at most this queue's
    /// budget. The returned [`ProviderSlot`] releases it on drop.
    ///
    /// `waiter` is how this caller is named in [`Self::position_of`]; it identifies the caller and
    /// nothing is keyed on it, so two callers sharing a name only make that report ambiguous.
    ///
    /// **Cancel-safe.** A caller that drops this future — a `tokio::time::timeout` around it, a
    /// cancelled turn — leaves the line as it goes. Without that every abandoned wait would stay
    /// counted, and the next caller would be told it is behind callers that are gone.
    pub async fn admit(
        &self,
        provider: &str,
        waiter: &str,
    ) -> Result<ProviderSlot, ProviderQueueError> {
        let (place, slot) = LinePlace::join(&self.lines, provider, waiter);
        let ahead = place.ahead_of_it();
        if ahead > 0 {
            log::info!(
                target: "tddy_discovery::subagent_runtime",
                "'{waiter}' is waiting for {provider}: {ahead} call(s) ahead of it, up to {:?}",
                self.wait_timeout,
            );
        }
        let permit = match tokio::time::timeout(self.wait_timeout, slot.acquire_owned()).await {
            // The semaphore is never closed — nothing drops the line it lives in — so an acquire
            // fails only for a reason this process could not continue under anyway.
            Ok(permit) => permit.expect("a provider's slot is never closed"),
            Err(_) => {
                log::warn!(
                    target: "tddy_discovery::subagent_runtime",
                    "'{waiter}' gave up waiting for {provider} after {:?}",
                    self.wait_timeout,
                );
                return Err(ProviderQueueError::WaitedTooLong {
                    provider: provider.to_string(),
                    waited: self.wait_timeout,
                });
            }
        };
        place.now_holding();
        Ok(ProviderSlot {
            // Declared before the permit so it is dropped first: the holder leaves the line before
            // the slot is handed on, and no observer ever sees two callers holding one endpoint.
            place,
            _permit: permit,
        })
    }

    /// Whether `provider` is busy right now — `1` while a caller holds its slot, `0` otherwise.
    /// A provider nobody has asked for is idle.
    pub fn in_flight(&self, provider: &str) -> usize {
        self.count_in(provider, |waiter| waiter.holds)
    }

    /// How many callers are queued behind the one holding `provider`, the holder excluded.
    pub fn waiting(&self, provider: &str) -> usize {
        self.count_in(provider, |waiter| !waiter.holds)
    }

    /// Where `waiter` stands in its provider's line: `0` is holding the slot, `1` is next.
    ///
    /// `None` is a caller this queue has never seen, or one that has left. Not `Some(0)`: "at the
    /// front" reads as *about to run*, which is exactly how a 38-minute wait was mistaken for work
    /// in progress.
    pub fn position_of(&self, waiter: &str) -> Option<usize> {
        let lines = self.lines.lock().expect("the provider queue lock is sound");
        lines
            .lines
            .values()
            .find_map(|line| line.order.iter().position(|queued| queued.waiter == waiter))
    }

    fn count_in(&self, provider: &str, counted: impl Fn(&QueuedCaller) -> bool) -> usize {
        let lines = self.lines.lock().expect("the provider queue lock is sound");
        lines
            .lines
            .get(provider)
            .map(|line| line.order.iter().filter(|queued| counted(queued)).count())
            .unwrap_or(0)
    }
}

impl Default for ProviderQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// A held provider slot. The provider is busy for as long as this is alive and free the moment it
/// is dropped — including when the turn holding it fails or is cancelled, which is the case that
/// starves everyone behind it if a release is written by hand instead.
pub struct ProviderSlot {
    place: LinePlace,
    _permit: OwnedSemaphorePermit,
}

/// Names the endpoint it holds and nothing else — that is the whole of what a held slot is, and
/// it is what a caller unwrapping the wrong `Result` needs to read.
impl std::fmt::Debug for ProviderSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderSlot")
            .field("provider", &self.place.provider)
            .finish()
    }
}

/// Every endpoint this queue has seen, and the tickets that tell two callers of the same name
/// apart.
#[derive(Default)]
struct ProviderLines {
    lines: HashMap<String, ProviderLine>,
    next_ticket: u64,
}

/// One endpoint's line: who is in it, in arrival order, and the single slot they are queued for.
///
/// A line is never removed once created, even when it empties. Its `Semaphore` is what makes the
/// endpoint single-slot, and callers hold clones of it while they wait: dropping it and minting a
/// fresh one for the next caller would hand out a second permit for the same endpoint. The cost is
/// one small entry per distinct endpoint for the life of the queue.
struct ProviderLine {
    slot: Arc<Semaphore>,
    order: Vec<QueuedCaller>,
}

struct QueuedCaller {
    ticket: u64,
    waiter: String,
    /// Whether this one is the caller currently holding the endpoint, rather than queued for it.
    holds: bool,
}

/// A caller's standing in a line, which it gives up when this is dropped.
///
/// One type for both halves of the wait — queued and holding — because leaving the line is the
/// same act either way, and a release written separately for each is a release that can be
/// forgotten on one of them.
struct LinePlace {
    lines: Arc<Mutex<ProviderLines>>,
    provider: String,
    ticket: u64,
}

impl LinePlace {
    /// Join the back of `provider`'s line, and hand back the slot to wait on.
    fn join(
        lines: &Arc<Mutex<ProviderLines>>,
        provider: &str,
        waiter: &str,
    ) -> (Self, Arc<Semaphore>) {
        let mut guard = lines.lock().expect("the provider queue lock is sound");
        let ticket = guard.next_ticket;
        guard.next_ticket += 1;
        let line = guard
            .lines
            .entry(provider.to_string())
            .or_insert_with(|| ProviderLine {
                slot: Arc::new(Semaphore::new(1)),
                order: Vec::new(),
            });
        line.order.push(QueuedCaller {
            ticket,
            waiter: waiter.to_string(),
            holds: false,
        });
        let slot = Arc::clone(&line.slot);
        drop(guard);
        (
            Self {
                lines: Arc::clone(lines),
                provider: provider.to_string(),
                ticket,
            },
            slot,
        )
    }

    /// How many callers took this line before this one and are still in it.
    fn ahead_of_it(&self) -> usize {
        self.with_line(|line, ticket| {
            line.order
                .iter()
                .filter(|queued| queued.ticket < ticket)
                .count()
        })
        .unwrap_or(0)
    }

    /// Record that this caller now holds the endpoint rather than waiting for it.
    fn now_holding(&self) {
        self.with_line(|line, ticket| {
            if let Some(queued) = line.order.iter_mut().find(|queued| queued.ticket == ticket) {
                queued.holds = true;
            }
        });
    }

    fn with_line<T>(&self, act: impl FnOnce(&mut ProviderLine, u64) -> T) -> Option<T> {
        let mut guard = self.lines.lock().expect("the provider queue lock is sound");
        let line = guard.lines.get_mut(&self.provider)?;
        Some(act(line, self.ticket))
    }
}

impl Drop for LinePlace {
    fn drop(&mut self) {
        self.with_line(|line, ticket| line.order.retain(|queued| queued.ticket != ticket));
    }
}

/// One conversation's standing arrangement with a provider: the queue it waits in, the endpoint it
/// waits for, and the name it waits under.
///
/// The three travel together because a slot is meaningless without all three, and a turn loop that
/// carried them separately would be free to queue on one endpoint and call another.
pub struct ProviderAdmission {
    queue: ProviderQueue,
    provider: String,
    waiter: String,
}

impl ProviderAdmission {
    pub fn new(
        queue: ProviderQueue,
        provider: impl Into<String>,
        waiter: impl Into<String>,
    ) -> Self {
        Self {
            queue,
            provider: provider.into(),
            waiter: waiter.into(),
        }
    }

    /// Take the endpoint's slot for **one** model call.
    ///
    /// One call, not one turn: a turn's loop makes several, plus a synthesis turn, and a slot held
    /// across all of them would keep every sibling agent off the endpoint for the whole loop —
    /// a milder version of the failure this gate exists to prevent.
    pub async fn hold(&self) -> Result<ProviderSlot, ProviderQueueError> {
        self.queue.admit(&self.provider, &self.waiter).await
    }
}
