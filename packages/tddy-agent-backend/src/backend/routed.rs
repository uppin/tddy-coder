//! Running a headless backend on a provider route (`tddy-coder --provider-route-path`).
//!
//! Kept beside the backends rather than inside them: each backend applies the route where it builds
//! its child process, and this module is the one entry point `tddy-coder` calls.

use crate::provider_route::HarnessRoute;

use super::{AnyBackend, ClaudeAcpBackend, ClaudeCodeBackend, CodexAcpBackend, CodexBackend};

impl AnyBackend {
    /// Run this backend on `route`. Refused, with the reason, for a backend that cannot be routed —
    /// Cursor and the stub; never silently ignored.
    pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, String> {
        let _ = route;
        // TODO(tool-route): implement
        todo!()
    }
}

impl ClaudeCodeBackend {
    /// `claude -p` on a provider account: the route's environment on every child.
    #[must_use]
    pub fn with_provider_route(self, route: HarnessRoute) -> Self {
        let _ = route;
        // TODO(tool-route): implement
        todo!()
    }
}

impl ClaudeAcpBackend {
    /// `claude-agent-acp` on a provider account: the route's environment on the agent child.
    #[must_use]
    pub fn with_provider_route(self, route: HarnessRoute) -> Self {
        let _ = route;
        // TODO(tool-route): implement
        todo!()
    }
}

impl CodexBackend {
    /// `codex exec` on a provider account: the route's `-c model_provider…` arguments and its key.
    #[must_use]
    pub fn with_provider_route(self, route: HarnessRoute) -> Self {
        let _ = route;
        // TODO(tool-route): implement
        todo!()
    }
}

impl CodexAcpBackend {
    /// `codex-acp` on a provider account: the route's overrides and key on the agent child.
    #[must_use]
    pub fn with_provider_route(self, route: HarnessRoute) -> Self {
        let _ = route;
        // TODO(tool-route): implement
        todo!()
    }
}
