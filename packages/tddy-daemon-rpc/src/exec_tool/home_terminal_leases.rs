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
use std::sync::Mutex;

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
        // TODO(terminal-keys): implement — insert into `self.tokens`.
        let _ = (&self.tokens, key, control_token);
    }

    /// The token `key` was issued by its last claim, or `None` when it never claimed.
    #[must_use]
    pub fn token_of(&self, key: &HomeTerminalClaimKey) -> Option<String> {
        // TODO(terminal-keys): implement — look `key` up in `self.tokens`.
        let _ = (&self.tokens, key);
        None
    }
}
