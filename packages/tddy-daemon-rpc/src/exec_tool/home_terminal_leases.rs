//! The control tokens a project-home session holds on this host's terminals (`#home-inbox` 5/8).
//!
//! `ClaimSessionTerminal` takes a session's terminal control lease and is issued a token; the token
//! is kept here, on the host that owns the terminal, and is **never returned to the agent**, so the
//! agent cannot replay it through another route. `SendSessionKeys` looks it up, then verifies it
//! against the lease itself: a person who stole control since leaves this entry stale, and the
//! verification is what refuses.
//!
//! An entry is keyed by the **confirmed caller** (its session id and the daemon instance that owns
//! that session) and the target session, so two homes never share a token — not on one host, and not
//! when they are on two different hosts reaching the same target.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Whose claim an entry is: the calling home session, as its owner confirms it, and the session
/// whose terminal it claimed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HomeTerminalClaimKey {
    /// The daemon instance that owns the calling home session.
    pub caller_daemon_instance_id: String,
    pub caller_session_id: String,
    /// The session on this host whose terminal was claimed.
    pub target_session_id: String,
}

/// `(confirmed caller, target session) → control token`, held on this host.
#[derive(Default)]
pub struct HomeTerminalLeases {
    tokens: Mutex<HashMap<HomeTerminalClaimKey, String>>,
}

impl HomeTerminalLeases {
    /// Remember `control_token` as `key`'s, replacing an earlier one.
    pub fn remember(&self, key: HomeTerminalClaimKey, control_token: String) {
        self.tokens().insert(key, control_token);
    }

    /// The token `key` was issued by its last claim, or `None` when it never claimed.
    #[must_use]
    pub fn token_of(&self, key: &HomeTerminalClaimKey) -> Option<String> {
        self.tokens().get(key).cloned()
    }

    /// Forget `key`'s token, returning it: its home released the claim (`#orchestra` 7/9).
    pub fn forget(&self, key: &HomeTerminalClaimKey) -> Option<String> {
        // TODO(orchestrator-delivery): implement
        let _ = key;
        unimplemented!("HomeTerminalLeases::forget")
    }

    /// Forget every token held on `target_session_id`'s terminal: the session ended.
    pub fn forget_target(&self, target_session_id: &str) {
        // TODO(orchestrator-delivery): implement
        let _ = target_session_id;
        unimplemented!("HomeTerminalLeases::forget_target")
    }

    /// The map, locked. Each method is one insert or one lookup, so a panic on another thread
    /// while it held the lock cannot leave the map half-updated: a poisoned lock is read through
    /// rather than turned into a panic of the daemon's own.
    fn tokens(&self) -> MutexGuard<'_, HashMap<HomeTerminalClaimKey, String>> {
        self.tokens.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;

    fn a_claim(caller: &str, target: &str) -> HomeTerminalClaimKey {
        HomeTerminalClaimKey {
            caller_daemon_instance_id: "host-a".to_string(),
            caller_session_id: caller.to_string(),
            target_session_id: target.to_string(),
        }
    }

    #[test]
    fn forgetting_a_claim_returns_its_token_and_leaves_none() {
        let leases = HomeTerminalLeases::default();
        leases.remember(a_claim("home", "child"), "token-1".to_string());

        let forgotten = leases.forget(&a_claim("home", "child"));

        assert_eq!(forgotten.as_deref(), Some("token-1"));
        assert_eq!(leases.token_of(&a_claim("home", "child")), None);
    }

    #[test]
    fn an_ended_session_takes_every_claim_on_it_with_it() {
        let leases = HomeTerminalLeases::default();
        leases.remember(a_claim("home", "child"), "token-1".to_string());
        leases.remember(a_claim("other-home", "child"), "token-2".to_string());
        leases.remember(a_claim("home", "sibling"), "token-3".to_string());

        leases.forget_target("child");

        assert_eq!(
            (
                leases.token_of(&a_claim("home", "child")),
                leases.token_of(&a_claim("other-home", "child")),
                leases.token_of(&a_claim("home", "sibling")).as_deref(),
            ),
            (None, None, Some("token-3"))
        );
    }
}
