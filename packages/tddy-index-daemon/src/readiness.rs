//! When a root's server counts as ready, and so which [`IndexState`] an answer it gave is reported
//! in (`#reshape` extension-language-server, the readiness rule of 2026-10-20).
//!
//! rust-analyzer says so itself: `experimental/serverStatus` with `quiescent: true`, which it sends
//! only to a client that advertised `experimental.serverStatusNotification` at `initialize` — Rust's
//! launch does. That rule is unchanged. Another language's server has no such notification, and
//! tddy does not ask it for one: it counts as ready once `initialize` has completed, which every
//! [`LspClient`] has — a client exists only after its handshake.

use tddy_code_restructuring::backends::rust::ServerChatter;
use tddy_lsp::{LaunchSpec, LspClient};

use crate::graph::state_of_quiescence;
use crate::proto::code_index::IndexState;

/// How a server's readiness is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    /// From its latest `experimental/serverStatus`: ready once it says it is quiescent, loading
    /// before it has said anything (rust-analyzer).
    ServerStatus,
    /// Ready once `initialize` has completed (every server not asked for `experimental/serverStatus`).
    AfterInitialize,
}

impl Readiness {
    /// The rule for a server started from `launch`: [`Readiness::ServerStatus`] when its advertised
    /// capabilities set `experimental.serverStatusNotification` to `true`, and
    /// [`Readiness::AfterInitialize`] otherwise — an extension language's
    /// `tddy_zed_extensions::client_capabilities` does not.
    #[must_use]
    pub fn of_launch(launch: &LaunchSpec) -> Self {
        let _ = launch;
        // TODO(reshape-extension-language-server): implement
        todo!("Readiness::of_launch")
    }
}

/// Where the root's index stood when `client` answered, read by `readiness`.
///
/// [`Readiness::ServerStatus`]: `READY` when the server's latest `experimental/serverStatus` said it
/// is quiescent, `LOADING` otherwise — including a server that has said nothing yet.
/// [`Readiness::AfterInitialize`]: `READY`. Never `IDLE`: a server answered.
///
/// Read from the client's own record of the latest status rather than from the root's progress
/// watcher: the reader records a status before it routes the response that follows it, so the
/// state is the one the answer was given in — the watcher may not have folded it in yet. Read
/// through [`ServerChatter`], the one reading of that notification.
#[must_use]
pub fn index_state_of(client: &LspClient, readiness: Readiness) -> IndexState {
    match readiness {
        Readiness::ServerStatus => {
            let mut chatter = ServerChatter::default();
            if let Some(status) = client.server_status() {
                chatter.absorb(&status);
            }
            state_of_quiescence(chatter.quiescent())
        }
        // TODO(reshape-extension-language-server): implement
        Readiness::AfterInitialize => todo!("index_state_of after initialize"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_launch_asking_for_server_status_is_read_by_it_and_any_other_after_initialize() {
        // Given Rust's launch, which asks for `experimental/serverStatus`, and an extension
        // server's, which asks only for byte positions and rename
        let rust = LaunchSpec::new("rust-analyzer")
            .with_capabilities(tddy_code_restructuring::backends::rust::client_capabilities());
        let extension = LaunchSpec::new("/opt/fake-ls").with_capabilities(json!({
            "general": { "positionEncodings": ["utf-8"] },
            "textDocument": { "rename": { "prepareSupport": false } }
        }));

        // When each launch's readiness rule is read
        let rules = (
            Readiness::of_launch(&rust),
            Readiness::of_launch(&extension),
        );

        // Then Rust keeps its rule, and the other is ready once initialized
        assert_eq!(rules, (Readiness::ServerStatus, Readiness::AfterInitialize));
    }
}
