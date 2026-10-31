//! Which hosts a jail may reach — the egress policy its shim and its host relay both enforce.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md` (`#agent-tools 3/12`).
//!
//! This crate knows nothing of tools: a policy arrives already lowered (`tddy-sandbox-recipes`'
//! `lower_network` takes the union of a tool set's declarations). What lives here is the policy's
//! shape, the one matching rule ([`EgressPolicy::admits`]) and the policy's spelling on the
//! runner's command line ([`EgressPolicy::to_runner_arg`] / [`EgressPolicy::from_runner_arg`]),
//! because the runner and the host relay must decide a host the same way.

use std::fmt;

/// One host a jail may reach: `host:port`, or `*.domain:port` for every subdomain of `domain`
/// (never `domain` itself). The host is lowercase; the port is always explicit here — the
/// `443` default is applied where a tool file is read, not at match time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPattern {
    pub host: String,
    pub port: u16,
}

impl HostPattern {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// Whether a `CONNECT host:port` falls under this pattern. Hosts compare case-insensitively;
    /// `*.example.com` matches `api.example.com` and `a.b.example.com`, never `example.com`; an
    /// IPv4 literal matches only itself.
    pub fn matches(&self, host: &str, port: u16) -> bool {
        // TODO(egress-allowlist): implement
        let _ = (host, port);
        todo!("egress-allowlist: HostPattern::matches")
    }
}

impl fmt::Display for HostPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// What a jail may reach through its egress shim and host relay.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EgressPolicy {
    /// Nothing: no shim is started and no proxy environment is set.
    None,
    /// Any `host:port`. The default because it is what every jail had before policies existed,
    /// and every recipe-built agent jail keeps it until its own `#agent-tools` node declares hosts.
    #[default]
    Any,
    /// Exactly these hosts.
    Hosts(Vec<HostPattern>),
}

/// A `--egress-policy` value the runner cannot read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressPolicyArgError {
    pub arg: String,
    pub reason: String,
}

impl fmt::Display for EgressPolicyArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "egress policy `{}`: {}", self.arg, self.reason)
    }
}

impl std::error::Error for EgressPolicyArgError {}

impl EgressPolicy {
    /// Whether this policy lets the jail reach `host:port`.
    pub fn admits(&self, host: &str, port: u16) -> bool {
        match self {
            EgressPolicy::None => false,
            EgressPolicy::Any => true,
            EgressPolicy::Hosts(patterns) => {
                // TODO(egress-allowlist): implement
                let _ = (patterns, host, port);
                todo!("egress-allowlist: EgressPolicy::admits")
            }
        }
    }

    /// The policy as the runner's `--egress-policy` value: `none`, `any`, or
    /// `hosts:<host:port>,<host:port>…`.
    pub fn to_runner_arg(&self) -> String {
        match self {
            EgressPolicy::None => "none".to_string(),
            EgressPolicy::Any => "any".to_string(),
            EgressPolicy::Hosts(patterns) => {
                // TODO(egress-allowlist): implement
                let _ = patterns;
                todo!("egress-allowlist: EgressPolicy::to_runner_arg")
            }
        }
    }

    /// Read back what [`Self::to_runner_arg`] wrote. Anything else is refused, never read as `any`.
    pub fn from_runner_arg(arg: &str) -> Result<Self, EgressPolicyArgError> {
        match arg {
            "none" => Ok(EgressPolicy::None),
            "any" => Ok(EgressPolicy::Any),
            other => {
                // TODO(egress-allowlist): implement
                let _ = other;
                todo!("egress-allowlist: EgressPolicy::from_runner_arg")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EgressPolicy, HostPattern};

    fn a_policy_of(patterns: &[(&str, u16)]) -> EgressPolicy {
        EgressPolicy::Hosts(
            patterns
                .iter()
                .map(|(host, port)| HostPattern::new(*host, *port))
                .collect(),
        )
    }

    #[test]
    fn an_exact_host_is_admitted_on_its_port_and_no_other_host_is() {
        // Given
        let policy = a_policy_of(&[("api.anthropic.com", 443)]);

        // When / Then
        assert!(policy.admits("api.anthropic.com", 443));
        assert!(!policy.admits("api.openai.com", 443));
    }

    #[test]
    fn a_host_is_admitted_only_on_the_port_it_was_declared_with() {
        // Given
        let policy = a_policy_of(&[("registry.example.com", 8443)]);

        // When / Then
        assert!(policy.admits("registry.example.com", 8443));
        assert!(!policy.admits("registry.example.com", 443));
    }

    #[test]
    fn a_host_declared_without_a_port_is_admitted_on_443_only() {
        // Given — the 443 default is applied when the tool file is read
        let policy = a_policy_of(&[("statsig.anthropic.com", 443)]);

        // When / Then
        assert!(policy.admits("statsig.anthropic.com", 443));
        assert!(!policy.admits("statsig.anthropic.com", 80));
    }

    #[test]
    fn a_wildcard_admits_subdomains_at_any_depth_and_not_the_apex() {
        // Given
        let policy = a_policy_of(&[("*.statsig.com", 443)]);

        // When / Then
        assert!(policy.admits("api.statsig.com", 443));
        assert!(policy.admits("a.b.statsig.com", 443));
        assert!(!policy.admits("statsig.com", 443));
        assert!(!policy.admits("evilstatsig.com", 443));
    }

    #[test]
    fn hosts_compare_case_insensitively() {
        // Given
        let policy = a_policy_of(&[("api.anthropic.com", 443), ("*.statsig.com", 443)]);

        // When / Then
        assert!(policy.admits("API.Anthropic.COM", 443));
        assert!(policy.admits("Featuregates.STATSIG.com", 443));
    }

    #[test]
    fn an_ipv4_literal_admits_only_itself() {
        // Given
        let policy = a_policy_of(&[("127.0.0.1", 8080)]);

        // When / Then
        assert!(policy.admits("127.0.0.1", 8080));
        assert!(!policy.admits("127.0.0.2", 8080));
        assert!(!policy.admits("localhost", 8080));
    }

    #[test]
    fn an_empty_host_list_admits_nothing() {
        // Given
        let policy = a_policy_of(&[]);

        // When / Then
        assert!(!policy.admits("api.anthropic.com", 443));
    }

    #[test]
    fn a_host_policy_round_trips_through_the_runner_arg() {
        // Given
        let policy = a_policy_of(&[("api.anthropic.com", 443), ("*.statsig.com", 8443)]);

        // When
        let arg = policy.to_runner_arg();

        // Then
        assert_eq!(arg, "hosts:api.anthropic.com:443,*.statsig.com:8443");
        assert_eq!(EgressPolicy::from_runner_arg(&arg), Ok(policy));
    }

    #[test]
    fn a_runner_arg_that_names_no_policy_is_refused() {
        // Given / When
        let refused = EgressPolicy::from_runner_arg("everything").unwrap_err();

        // Then
        assert_eq!(
            refused.to_string(),
            "egress policy `everything`: expected `none`, `any` or `hosts:<host:port>,…`"
        );
    }
}
