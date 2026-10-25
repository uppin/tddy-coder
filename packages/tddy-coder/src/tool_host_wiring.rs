//! What a `tddy-coder` tool session binds on its own toolcall listener on behalf of the daemon.
//!
//! The daemon hands a tool session `--host-session-socket <path>` — its OS user's socket — and the
//! session already knows its own `--session-id`. Together they are what the relays need: the socket
//! to reach the daemon, the id to name this session in every request. A handler exists exactly when
//! both are present, and **the prompt flag is derived from that same fact**: the recipes advertise the
//! GitHub PR tools only when a call to them can succeed.
//!
//! Without the socket there is no handler, and the listener refuses a token request as having no
//! credential handler. Nothing is read from the environment.

use std::sync::Arc;

use tddy_core::toolcall::{ConversationSpawnHandler, GithubCredentialHandler, ListenerHandlers};

use crate::conversation_spawn_relay::{
    DaemonRelayConversationSpawnHandler, DaemonRelayGithubCredential, HostSessionClient,
};

/// The relays this process binds, or none.
#[derive(Default)]
pub struct ToolHostHandlers {
    conversation_spawn: Option<Arc<dyn ConversationSpawnHandler>>,
    github_credential: Option<Arc<dyn GithubCredentialHandler>>,
    /// The session's project uses Local gh: its tools read the token from `gh` themselves.
    local_gh: bool,
}

impl ToolHostHandlers {
    /// Relays to `host_session_socket`, naming `session_id`. With either missing, none.
    pub fn from_flags(host_session_socket: Option<&str>, session_id: Option<&str>) -> Self {
        let Some(socket) = host_session_socket else {
            return Self::default();
        };
        let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) else {
            log::warn!(
                "tddy-coder: --host-session-socket was given but this session has no id to name \
                 itself with; the daemon's host cannot be asked for anything"
            );
            return Self::default();
        };
        let host = HostSessionClient::new(socket, session_id);
        Self {
            conversation_spawn: Some(Arc::new(DaemonRelayConversationSpawnHandler::new(
                Arc::clone(&host),
            ))),
            github_credential: Some(Arc::new(DaemonRelayGithubCredential::new(host))),
            local_gh: false,
        }
    }

    /// This session's GitHub source, as its `TDDY_GITHUB_SOURCE` names it (builder). A Local gh
    /// session's tools read the token from the host's own `gh`, so they can authenticate without a
    /// credential handler; a value that is not a source changes nothing.
    pub fn with_github_source(self, github_source: Option<&str>) -> Self {
        let Some(value) = github_source else {
            return self;
        };
        match tddy_core::toolcall::parse_github_source(value) {
            Ok(_) => Self {
                local_gh: true,
                ..self
            },
            Err(reason) => {
                log::warn!("tddy-coder: the GitHub PR tools are not offered: {reason}");
                self
            }
        }
    }

    /// Whether the agent's PR tools can authenticate: true exactly when a credential handler is
    /// bound. This is the value the workflow's context is seeded with.
    pub fn github_pr_tools_available(&self) -> bool {
        self.github_credential.is_some() || self.local_gh
    }

    /// The handlers as the toolcall listener binds them.
    pub fn into_listener_handlers(self) -> ListenerHandlers {
        ListenerHandlers {
            conversation_spawn: self.conversation_spawn,
            github_credential: self.github_credential,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_a_socket_and_a_session_id_the_pr_tools_are_available() {
        // Given the daemon handed this session its socket
        let handlers = ToolHostHandlers::from_flags(Some("/run/user/host.sock"), Some("s-1"));

        // Then a credential handler is bound, and the flag says so
        assert!(handlers.github_pr_tools_available());
        assert!(handlers
            .into_listener_handlers()
            .github_credential
            .is_some());
    }

    #[test]
    fn without_a_socket_there_is_no_handler_and_the_pr_tools_are_not_available() {
        // Given no --host-session-socket
        let handlers = ToolHostHandlers::from_flags(None, Some("s-1"));

        // Then nothing is bound, and the flag agrees
        assert!(!handlers.github_pr_tools_available());
        let bound = handlers.into_listener_handlers();
        assert!(bound.github_credential.is_none() && bound.conversation_spawn.is_none());
    }

    #[test]
    fn a_local_gh_session_is_offered_the_github_pr_tools_without_a_host_socket() {
        // Given a session told its project uses Local gh, and handed no host socket
        let handlers = ToolHostHandlers::from_flags(None, Some("s-1"))
            .with_github_source(Some("local-gh:github.com"));

        // Then its PR tools can authenticate, through gh
        assert!(handlers.github_pr_tools_available());
    }

    #[test]
    fn a_malformed_github_source_offers_no_github_pr_tools() {
        // Given a session with no host socket and a GitHub source nothing in tddy writes
        let handlers =
            ToolHostHandlers::from_flags(None, Some("s-1")).with_github_source(Some("github.com"));

        // Then its PR tools are not offered: nothing could answer them
        assert!(!handlers.github_pr_tools_available());
    }

    #[test]
    fn a_socket_without_a_session_id_binds_nothing() {
        // Given a socket but no id to name this session with
        for id in [None, Some(""), Some("  ")] {
            let handlers = ToolHostHandlers::from_flags(Some("/run/user/host.sock"), id);

            // Then no request could be addressed, so nothing is bound
            assert!(!handlers.github_pr_tools_available(), "id {id:?}");
        }
    }
}
