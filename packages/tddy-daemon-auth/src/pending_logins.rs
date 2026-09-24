//! How long a sign-in's GitHub token waits in memory for its credential vault, and the sweep that
//! drops it when nobody asks.
//!
//! A sign-in over a closed vault holds its token unsealed in `SessionVaults` until the passphrase
//! opens the vault, and that waiting token is what permits a first passphrase or a reset. Both
//! expire after `github.pending_login_ttl_seconds` (`tddy_daemon_kernel::pending_login_ttl`).
//! `SessionVaults` expires a token whenever it looks at the pending set; the sweep here covers the
//! token nobody looks at again, so it does not sit in memory until the next sign-in or unlock.
//!
//! Kept out of `auth.rs` and `runtime.rs`, both over their size budget: each calls one function.

use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::pending_login_ttl::PendingLoginTtl;

use crate::AUTH_LOG_TARGET;

/// The longest the sweep waits between looks, whatever the lifetime: an expired token outlives its
/// lifetime by at most this much, even with nobody signing in.
const LONGEST_SWEEP_PERIOD: Duration = Duration::from_secs(60);

/// The vaults over `dir`, holding a pending sign-in's token for `lifetime` — and said so, once, at
/// startup: an `info` naming the lifetime, and a `warn` when tokens never expire.
pub fn credential_vaults_in(dir: &Path, lifetime: PendingLoginTtl) -> SessionVaults {
    match lifetime.lifetime() {
        Some(_) => log::info!(
            target: AUTH_LOG_TARGET,
            "a sign-in's GitHub token waits in memory for its credential vault for {lifetime} \
             (github.pending_login_ttl_seconds) before it is dropped"
        ),
        None => log::warn!(
            target: AUTH_LOG_TARGET,
            "github.pending_login_ttl_seconds is 0: a sign-in's pending GitHub token is held in \
             memory until the vault is unlocked, its lineage logs out, or the daemon restarts — \
             and it permits choosing the vault's passphrase as long"
        ),
    }
    SessionVaults::new(dir).with_pending_lifetime(lifetime.lifetime())
}

/// How often the sweep looks: once per `lifetime`, and at least once a minute.
#[must_use]
pub fn sweep_period(lifetime: Duration) -> Duration {
    lifetime.min(LONGEST_SWEEP_PERIOD)
}

/// Start the sweep over `vaults` on the current tokio runtime — `None`, and no task at all, when
/// pending tokens never expire.
///
/// The task holds the vaults weakly and ends once the daemon drops them, so it keeps nothing
/// alive. Dropping the returned handle detaches it; it does not stop it.
pub fn spawn_pending_login_sweep(
    vaults: &Arc<SessionVaults>,
) -> Option<tokio::task::JoinHandle<()>> {
    let period = sweep_period(vaults.pending_lifetime()?);
    // Scheduled from now, not from whenever the task is first polled, so the first look is one
    // period after the sweep was asked for.
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    Some(tokio::spawn(sweep(Arc::downgrade(vaults), ticks)))
}

async fn sweep(vaults: Weak<SessionVaults>, mut ticks: tokio::time::Interval) {
    loop {
        ticks.tick().await;
        let Some(vaults) = vaults.upgrade() else {
            return;
        };
        vaults.expire_pending();
    }
}
