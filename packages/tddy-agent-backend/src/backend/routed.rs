//! Running a headless backend on a provider route (`tddy-coder --provider-route-path`).
//!
//! Kept beside the backends rather than inside them: each backend applies the route where it builds
//! its child process, and this module is the one entry point `tddy-coder` calls. A route reaches a
//! child as its environment (plain and secret), its arguments, and the files it names. The files
//! are for harnesses that read a configuration file named by a variable: OpenCode's ACP agent reads
//! the file `OPENCODE_CONFIG` names. The other routed harnesses take everything from the environment
//! and the arguments, and write no file.

use crate::provider_route::{Harness, HarnessRoute, RouteRefusal};

use super::{
    AnyBackend, ClaudeAcpBackend, ClaudeCodeBackend, CodexAcpBackend, CodexBackend,
    OpenCodeAcpBackend,
};

impl AnyBackend {
    /// Run this backend on `route`. Refused, with the reason, for a backend that cannot be routed —
    /// Cursor and the stub; never silently ignored.
    pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, String> {
        match self {
            AnyBackend::Claude(b) => Ok(AnyBackend::Claude(b.with_provider_route(route))),
            AnyBackend::ClaudeAcp(b) => Ok(AnyBackend::ClaudeAcp(b.with_provider_route(route))),
            AnyBackend::Codex(b) => Ok(AnyBackend::Codex(b.with_provider_route(route))),
            AnyBackend::CodexAcp(b) => Ok(AnyBackend::CodexAcp(b.with_provider_route(route))),
            AnyBackend::OpenCode(b) => Ok(AnyBackend::OpenCode(b.with_provider_route(route))),
            AnyBackend::Cursor(_) => Err(RouteRefusal::HarnessCannotBeRouted {
                harness: Harness::Cursor,
            }
            .to_string()),
            AnyBackend::Stub(_) => Err(
                "the stub backend cannot run on a provider account: it makes no provider call"
                    .to_string(),
            ),
        }
    }
}

impl ClaudeCodeBackend {
    /// `claude -p` on a provider account: the route's environment on every child, and its arguments.
    #[must_use]
    pub fn with_provider_route(mut self, route: HarnessRoute) -> Self {
        self.provider_route = Some(route);
        self
    }
}

impl ClaudeAcpBackend {
    /// `claude-agent-acp` on a provider account: the route's environment on the agent child. The
    /// agent takes its model from that environment, so the route's arguments are not passed.
    #[must_use]
    pub fn with_provider_route(mut self, route: HarnessRoute) -> Self {
        self.set_provider_route(route);
        self
    }
}

impl CodexBackend {
    /// `codex exec` on a provider account: the route's `-c model_provider…` arguments and its key.
    #[must_use]
    pub fn with_provider_route(mut self, route: HarnessRoute) -> Self {
        self.provider_route = Some(route);
        self
    }
}

impl CodexAcpBackend {
    /// `codex-acp` on a provider account: the route's overrides and key on the agent child.
    #[must_use]
    pub fn with_provider_route(mut self, route: HarnessRoute) -> Self {
        self.set_provider_route(route);
        self
    }
}

impl OpenCodeAcpBackend {
    /// `opencode acp` on a provider account: the route's key and its generated configuration on the
    /// agent child, the configuration reached through `OPENCODE_CONFIG`. The route's `--model` is
    /// not passed: the generated configuration already selects the model.
    #[must_use]
    pub fn with_provider_route(mut self, route: HarnessRoute) -> Self {
        self.set_provider_route(route);
        self
    }
}

/// Every environment pair `route` gives a child: its plain pairs, then its secret pairs. Both reach
/// the child's environment and neither reaches its argv.
pub(super) fn route_child_env(route: &HarnessRoute) -> impl Iterator<Item = (&str, &str)> + '_ {
    route
        .env
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .chain(
            route
                .secret_env
                .iter()
                .map(|(name, secret)| (name.as_str(), secret.expose())),
        )
}

/// Splice `route`'s arguments onto `args`, the argv a backend built for its request. The route
/// names the model it runs (`model_flag <model>`), so that flag replaces the request's own: the
/// child is given one model, the route's.
pub(super) fn splice_route_args(args: &mut Vec<String>, route: &HarnessRoute, model_flag: &str) {
    if route.args.iter().any(|arg| arg == model_flag) {
        if let Some(at) = args.iter().position(|arg| arg == model_flag) {
            args.drain(at..(at + 2).min(args.len()));
        }
    }
    args.extend(route.args.iter().cloned());
}

/// The route's arguments as `codex-acp` takes them. It accepts `-c key=value` overrides, as `codex`
/// does, but has no `-m`: its model is the `model` override, so `-m <model>` becomes
/// `-c model="<model>"`. Any other argument passes through unchanged.
pub(super) fn codex_acp_route_args(route: &HarnessRoute) -> Vec<String> {
    let mut out = Vec::with_capacity(route.args.len());
    let mut args = route.args.iter();
    while let Some(arg) = args.next() {
        if arg == "-m" {
            match args.next() {
                Some(model) => {
                    out.push("-c".to_string());
                    out.push(format!(
                        "model={}",
                        serde_json::to_string(model).expect("a string always serializes as JSON")
                    ));
                }
                None => out.push(arg.clone()),
            }
        } else {
            out.push(arg.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_model_flag_reaches_codex_acp_as_its_model_override() {
        // Given a route's arguments, which name the model with `-m` as `codex` does
        let route = HarnessRoute {
            args: vec![
                "-c".to_string(),
                "model_provider=\"tddy-route\"".to_string(),
                "-m".to_string(),
                "muse-wonder-1.0".to_string(),
            ],
            ..HarnessRoute::default()
        };

        // When
        let args = codex_acp_route_args(&route);

        // Then codex-acp, which has no `-m`, is given the same model as a `-c` override
        assert_eq!(
            args,
            vec![
                "-c".to_string(),
                "model_provider=\"tddy-route\"".to_string(),
                "-c".to_string(),
                "model=\"muse-wonder-1.0\"".to_string(),
            ]
        );
    }
}
