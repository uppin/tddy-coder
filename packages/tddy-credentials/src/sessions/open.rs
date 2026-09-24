//! The vaults open right now — each one's data key in memory — and when each was last used.
//!
//! Session tokens are stateless, so the daemon learns of no session ending but a logout. A lineage
//! that simply stops coming back (a browser closed for good, a refresh token that lapsed) removes
//! nothing, and its vault would stay open until the daemon exits. So an open vault is dropped once
//! nothing has **used** it for its idle lifetime: a login sealing into it, an unlock, a refresh
//! reopening or rotating through it, or a credential read. A lookup that only asks *whether* it is
//! open — a status poll — is not a use, or a dashboard left on a screen would hold it open forever.
//!
//! A handle past its idle lifetime is dropped whenever it is looked up, and by
//! [`OpenVaults::evict_idle`], which the daemon's sweep calls for the one nobody looks up. Dropping
//! the last `Arc` drops the `SessionVault`, and its data key wipes itself as it goes. The file is
//! untouched: the next refresh presenting an unlock key reopens it, exactly as after a restart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::held;
use super::pending::{Clock, LOG_TARGET};
use crate::vault::SessionVault;

/// One open vault, and when it was last used.
struct OpenVault {
    vault: Arc<SessionVault>,
    used: Instant,
}

/// Every subject's open vault, each kept while it is used.
pub(super) struct OpenVaults {
    handles: Mutex<HashMap<String, OpenVault>>,
    /// How long an open vault may go unused — `None` for as long as the daemon runs.
    pub(super) idle_lifetime: Option<Duration>,
    pub(super) clock: Clock,
}

impl Default for OpenVaults {
    fn default() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
            idle_lifetime: None,
            clock: Arc::new(Instant::now),
        }
    }
}

impl OpenVaults {
    /// `subject`'s open vault, without counting the look as a use.
    pub(super) fn peek(&self, subject: &str) -> Option<Arc<SessionVault>> {
        self.lookup(subject, false)
    }

    /// `subject`'s open vault, counting the look as a use — atomically, so a handle found here
    /// cannot be evicted before its caller has used it.
    pub(super) fn use_open(&self, subject: &str) -> Option<Arc<SessionVault>> {
        self.lookup(subject, true)
    }

    /// Keep `vault` open for `subject`, used as of now, in place of any handle held before.
    pub(super) fn insert(&self, subject: &str, vault: SessionVault) {
        let used = (self.clock)();
        held(&self.handles).insert(
            subject.to_string(),
            OpenVault {
                vault: Arc::new(vault),
                used,
            },
        );
    }

    /// Drop `subject`'s handle.
    pub(super) fn remove(&self, subject: &str) {
        held(&self.handles).remove(subject);
    }

    /// Drop `subject`'s handle if it is still `vault` — not one a concurrent unlock put there since.
    pub(super) fn close(&self, subject: &str, vault: &Arc<SessionVault>) {
        let mut handles = held(&self.handles);
        if handles
            .get(subject)
            .is_some_and(|open| Arc::ptr_eq(&open.vault, vault))
        {
            handles.remove(subject);
        }
    }

    /// Drop every open vault unused for its idle lifetime, logging each, and say how many went.
    pub(super) fn evict_idle(&self) -> usize {
        let mut handles = held(&self.handles);
        let before = handles.len();
        let now = (self.clock)();
        handles.retain(|subject, open| self.still_in_use(subject, open, now));
        before - handles.len()
    }

    fn lookup(&self, subject: &str, is_a_use: bool) -> Option<Arc<SessionVault>> {
        let mut handles = held(&self.handles);
        let now = (self.clock)();
        let open = handles.get_mut(subject)?;
        if !self.still_in_use(subject, open, now) {
            handles.remove(subject);
            return None;
        }
        if is_a_use {
            open.used = now;
        }
        Some(Arc::clone(&open.vault))
    }

    /// Whether `open` has been used within its idle lifetime; logs the eviction when it has not.
    fn still_in_use(&self, subject: &str, open: &OpenVault, now: Instant) -> bool {
        let Some(lifetime) = self.idle_lifetime else {
            return true;
        };
        let idle = now.saturating_duration_since(open.used);
        if idle < lifetime {
            return true;
        }
        log::info!(
            target: LOG_TARGET,
            "closed the credential vault of '{subject}': nothing used it for {} s, its idle \
             lifetime is {} s; its data key is dropped, and the next session refresh presenting an \
             unlock key, or its passphrase, reopens it",
            idle.as_secs(),
            lifetime.as_secs()
        );
        false
    }
}
