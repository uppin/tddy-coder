//! A short-lived cache in front of the tooling probe, with in-flight dedup.
//!
//! `GetHostTooling` shells out — `git config`, `gh auth status` (which can reach the network), the
//! ssh agent, a TCP connect per desktop protocol — and the Hosts screen asks every online host for
//! it on every visit. Without a cache that is a subprocess fan-out per page load per operator.
//!
//! Two guarantees, both keyed by [`ToolingKey`]:
//!
//! - **Within the TTL, a repeated question runs no probe.** The answer is served from the last run.
//! - **Concurrent questions share one run.** A caller that arrives while a probe for the same key is
//!   still running waits for that run's answer rather than starting another.
//!
//! "Now" comes from an injected [`Clock`], so expiry is tested by moving a hand-driven clock rather
//! than by sleeping.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where the cache reads "now" from — injected so expiry is testable without sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// The monotonic wall clock the daemon runs on.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// What one tooling answer is about.
///
/// The probe is **per OS user** (`git config --global` and `gh auth status` read that user's
/// `$HOME`), so the OS user is part of the key: one user's answer must never be served to another.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolingKey {
    /// The host the answer describes.
    pub daemon_instance_id: String,
    /// The OS user the probe ran as.
    pub os_user: String,
    /// The GitHub host `gh` was asked about; `None` asks about every host `gh` is logged in to.
    pub github_hostname: Option<String>,
}

impl ToolingKey {
    pub fn new(
        daemon_instance_id: impl Into<String>,
        os_user: impl Into<String>,
        github_hostname: Option<String>,
    ) -> Self {
        Self {
            daemon_instance_id: daemon_instance_id.into(),
            os_user: os_user.into(),
            github_hostname,
        }
    }
}

/// One key's slot: the last answer and when it was taken. The slot's lock is held for the length of
/// a probe, which is what makes concurrent callers for the key wait for that run instead of
/// starting their own.
type Slot<T> = Arc<tokio::sync::Mutex<Option<(T, Instant)>>>;

/// Probe results by [`ToolingKey`], kept for a TTL, with concurrent callers sharing one run.
///
/// Every answer is kept for the full TTL, including one that reports a failed reading: the probe
/// returns its failures as readings (a missing `gh`, an unreachable agent) rather than as an error,
/// and re-running it per request would reinstate the fan-out the cache exists to bound.
pub struct ToolingCache<T: Clone> {
    slots: Mutex<HashMap<ToolingKey, Slot<T>>>,
    ttl: Duration,
    clock: Arc<dyn Clock>,
}

impl<T: Clone + Send + 'static> ToolingCache<T> {
    pub fn new(ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            ttl,
            clock,
        }
    }

    /// `key`'s slot, created if absent. Slots that nobody holds and whose answer has expired are
    /// dropped first, so the map is bounded by the keys asked about within one TTL rather than by
    /// every key ever asked about (the key carries a caller-supplied GitHub host).
    ///
    /// A slot is evicted only when the map holds the sole reference to it — callers clone slots
    /// under this same lock, so no one is waiting on or running a probe in it — and its lock is
    /// free and its answer is older than the TTL. A slot being probed is therefore never evicted.
    fn slot_for(&self, key: ToolingKey) -> Slot<T> {
        let now = self.clock.now();
        let mut slots = self
            .slots
            .lock()
            .expect("the tooling cache's slot map was poisoned");
        slots.retain(|_, slot| !self.is_expired_and_unheld(slot, now));
        Arc::clone(slots.entry(key).or_default())
    }

    fn is_expired_and_unheld(&self, slot: &Slot<T>, now: Instant) -> bool {
        if Arc::strong_count(slot) != 1 {
            return false;
        }
        let Ok(answer) = slot.try_lock() else {
            return false;
        };
        matches!(
            answer.as_ref(),
            Some((_, taken_at)) if now.saturating_duration_since(*taken_at) >= self.ttl
        )
    }

    /// The answer for `key`: a cached one younger than the TTL, the one a probe already running for
    /// `key` produces, or — only when neither exists — the one `probe` produces.
    pub async fn get_or_probe<F, Fut>(&self, key: ToolingKey, probe: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let slot = self.slot_for(key);
        // Waiting here is waiting for a probe already running for this key; once it releases the
        // slot, its answer is fresh and is served below.
        let mut answer = slot.lock().await;
        if let Some((cached, taken_at)) = answer.as_ref() {
            if self.clock.now().saturating_duration_since(*taken_at) < self.ttl {
                return cached.clone();
            }
        }
        let fresh = probe().await;
        *answer = Some((fresh.clone(), self.clock.now()));
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tokio::sync::oneshot;

    const A_TTL: Duration = Duration::from_secs(30);
    const A_MOMENT: Duration = Duration::from_secs(1);
    /// Long enough for any in-process answer; a deduplicated caller that never wakes fails here
    /// instead of hanging the suite.
    const PATIENCE: Duration = Duration::from_secs(5);

    const HOST_A: &str = "workstation-1";
    const HOST_B: &str = "server-2";
    const AN_OS_USER: &str = "dev";
    const ANOTHER_OS_USER: &str = "ops";
    const AN_ENTERPRISE_GITHUB: &str = "github.example.com";

    // -- Given helpers ---------------------------------------------------------------------------

    /// A clock that moves only when the test says so.
    struct AHandDrivenClock(Mutex<Instant>);

    impl AHandDrivenClock {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(Instant::now())))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    impl Clock for AHandDrivenClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    fn a_cache_on(clock: &Arc<AHandDrivenClock>) -> ToolingCache<String> {
        ToolingCache::new(A_TTL, Arc::clone(clock) as Arc<dyn Clock>)
    }

    fn the_tooling_of(host: &str) -> ToolingKey {
        ToolingKey::new(host, AN_OS_USER, None)
    }

    fn the_tooling_of_as(host: &str, os_user: &str) -> ToolingKey {
        ToolingKey::new(host, os_user, None)
    }

    fn the_tooling_of_about_github(host: &str, github_hostname: &str) -> ToolingKey {
        ToolingKey::new(host, AN_OS_USER, Some(github_hostname.to_string()))
    }

    /// How many keys the cache is holding a slot for.
    fn slots_held_by(cache: &ToolingCache<String>) -> usize {
        cache.slots.lock().unwrap().len()
    }

    type AnAnswer = Pin<Box<dyn Future<Output = String> + Send>>;

    /// A probe that counts every time it is started — not awaited — so a caller served from the
    /// cache or from someone else's run is visible as a run that never happened.
    #[derive(Clone, Default)]
    struct ACountingProbe {
        runs: Arc<AtomicUsize>,
    }

    impl ACountingProbe {
        fn answering(&self, answer: &str) -> impl FnOnce() -> AnAnswer {
            let runs = Arc::clone(&self.runs);
            let answer = answer.to_string();
            move || {
                runs.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { answer })
            }
        }

        /// Answers only once `release` fires — a probe still running when the next caller arrives.
        fn held_open_until(
            &self,
            release: oneshot::Receiver<()>,
            answer: &str,
        ) -> impl FnOnce() -> AnAnswer {
            let runs = Arc::clone(&self.runs);
            let answer = answer.to_string();
            move || {
                runs.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    release.await.expect("the test dropped the probe's release");
                    answer
                })
            }
        }

        fn runs(&self) -> usize {
            self.runs.load(Ordering::SeqCst)
        }
    }

    async fn within_patience<F: Future>(future: F) -> F::Output {
        tokio::time::timeout(PATIENCE, future)
            .await
            .expect("the cache never answered")
    }

    // -- Tests -----------------------------------------------------------------------------------

    #[tokio::test]
    async fn two_concurrent_requests_for_one_host_run_one_probe() {
        // Given a probe for host A that is still running when a second request for host A arrives
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        let (release, held) = oneshot::channel();

        // When both requests are made and the running probe then answers
        let (first, second, ()) = within_patience(async {
            tokio::join!(
                cache.get_or_probe(
                    the_tooling_of(HOST_A),
                    probe.held_open_until(held, "the first run's answer"),
                ),
                cache.get_or_probe(
                    the_tooling_of(HOST_A),
                    probe.answering("a second run's answer"),
                ),
                async {
                    tokio::task::yield_now().await;
                    release.send(()).expect("the probe stopped waiting");
                },
            )
        })
        .await;

        // Then both callers got the one run's answer, and the second never started its own
        assert_eq!(first, "the first run's answer");
        assert_eq!(second, "the first run's answer");
        assert_eq!(
            probe.runs(),
            1,
            "concurrent requests must share one probe run"
        );
    }

    #[tokio::test]
    async fn a_request_within_the_ttl_runs_no_probe() {
        // Given host A probed a moment less than the TTL ago
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("the cached answer"))
            .await;
        clock.advance(A_TTL - A_MOMENT);

        // When host A is asked about again
        let answer = cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("a fresh answer"))
            .await;

        // Then the cached answer is served and no second probe ran
        assert_eq!(answer, "the cached answer");
        assert_eq!(probe.runs(), 1, "a cached answer must not start a probe");
    }

    #[tokio::test]
    async fn a_request_after_the_ttl_probes_again() {
        // Given host A probed a moment more than the TTL ago
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("a stale answer"))
            .await;
        clock.advance(A_TTL + A_MOMENT);

        // When host A is asked about again
        let answer = cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("a fresh answer"))
            .await;

        // Then the host is probed again and the fresh answer is served
        assert_eq!(answer, "a fresh answer");
        assert_eq!(probe.runs(), 2, "an expired answer must be probed again");
    }

    #[tokio::test]
    async fn different_hosts_are_cached_separately() {
        // Given host A's answer is cached
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("host A's tooling"))
            .await;

        // When host B is asked about within the TTL
        let answer = cache
            .get_or_probe(the_tooling_of(HOST_B), probe.answering("host B's tooling"))
            .await;

        // Then host B is probed for its own answer rather than handed host A's
        assert_eq!(answer, "host B's tooling");
        assert_eq!(probe.runs(), 2, "each host must be probed for itself");
    }

    #[tokio::test]
    async fn different_os_users_on_one_host_are_cached_separately() {
        // Given host A's answer for one OS user is cached
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(
                the_tooling_of_as(HOST_A, AN_OS_USER),
                probe.answering("dev's git identity"),
            )
            .await;

        // When the same host is asked about for another OS user within the TTL
        let answer = cache
            .get_or_probe(
                the_tooling_of_as(HOST_A, ANOTHER_OS_USER),
                probe.answering("ops's git identity"),
            )
            .await;

        // Then that user gets their own probe — the probe reads each user's own `$HOME`
        assert_eq!(answer, "ops's git identity");
        assert_eq!(
            probe.runs(),
            2,
            "one user's tooling must never be served to another"
        );
    }

    #[tokio::test]
    async fn a_different_github_hostname_is_cached_separately() {
        // Given host A's answer about every GitHub host is cached
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("gh on any host"))
            .await;

        // When the same host is asked about for one named GitHub host within the TTL
        let answer = cache
            .get_or_probe(
                the_tooling_of_about_github(HOST_A, AN_ENTERPRISE_GITHUB),
                probe.answering("gh on the enterprise host"),
            )
            .await;

        // Then that question is probed for itself rather than handed the other answer
        assert_eq!(answer, "gh on the enterprise host");
        assert_eq!(
            probe.runs(),
            2,
            "each GitHub host must be probed for itself"
        );
    }

    #[tokio::test]
    async fn a_caller_dropped_mid_probe_stores_nothing() {
        // Given a probe for host A that is abandoned while it is still running
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        let (_release, held) = oneshot::channel();
        tokio::select! {
            biased;
            _ = cache.get_or_probe(
                the_tooling_of(HOST_A),
                probe.held_open_until(held, "an answer nobody waited for"),
            ) => unreachable!("the probe is held open"),
            // Ready at once, after the probe above has started and parked: the caller gives up.
            _ = std::future::ready(()) => {}
        }
        assert_eq!(probe.runs(), 1, "the abandoned probe did start");

        // When host A is asked about again
        let answer = within_patience(
            cache.get_or_probe(the_tooling_of(HOST_A), probe.answering("a fresh answer")),
        )
        .await;

        // Then nothing was stored by the abandoned run, so the new caller probed for itself
        assert_eq!(answer, "a fresh answer");
        assert_eq!(
            probe.runs(),
            2,
            "an abandoned probe must not poison the slot"
        );
    }

    #[tokio::test]
    async fn an_answer_reporting_a_failure_is_held_for_the_ttl() {
        // Given host A probed and the answer reports a failed reading
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(
                the_tooling_of(HOST_A),
                probe.answering("gh is not installed"),
            )
            .await;
        clock.advance(A_TTL - A_MOMENT);

        // When host A is asked about again within the TTL
        let answer = cache
            .get_or_probe(
                the_tooling_of(HOST_A),
                probe.answering("gh is installed now"),
            )
            .await;

        // Then the failed-looking answer is served and no second probe ran
        assert_eq!(answer, "gh is not installed");
        assert_eq!(
            probe.runs(),
            1,
            "a failed reading is still held for the TTL"
        );
    }

    #[tokio::test]
    async fn an_expired_slot_nobody_holds_is_evicted() {
        // Given a slot for each of two GitHub hosts, both answered
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        for github_hostname in ["one.example.com", "two.example.com"] {
            cache
                .get_or_probe(
                    the_tooling_of_about_github(HOST_A, github_hostname),
                    probe.answering("an answer"),
                )
                .await;
        }
        assert_eq!(slots_held_by(&cache), 2);
        clock.advance(A_TTL + A_MOMENT);

        // When another key is asked about after both have expired
        cache
            .get_or_probe(the_tooling_of(HOST_B), probe.answering("host B's tooling"))
            .await;

        // Then only the new key's slot remains
        assert_eq!(slots_held_by(&cache), 1, "expired, unheld slots must go");
    }

    #[tokio::test]
    async fn a_slot_still_within_the_ttl_is_kept() {
        // Given host A's answer is cached
        let clock = AHandDrivenClock::new();
        let cache = a_cache_on(&clock);
        let probe = ACountingProbe::default();
        cache
            .get_or_probe(the_tooling_of(HOST_A), probe.answering("host A's tooling"))
            .await;
        clock.advance(A_TTL - A_MOMENT);

        // When another key is asked about
        cache
            .get_or_probe(the_tooling_of(HOST_B), probe.answering("host B's tooling"))
            .await;

        // Then both slots are held
        assert_eq!(slots_held_by(&cache), 2);
    }
}
