//! A tool's `network:` section — which hosts it may reach, and whether it takes an OAuth callback.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md` (`#agent-tools 3/12`).
//!
//! ```yaml
//! network:
//!   egress: none            # the default: the tool needs no network
//!   # egress: any           # any host — what every jail had before this section existed
//!   # egress: { hosts: ["api.anthropic.com", "*.statsig.com:443", "127.0.0.1:8080"] }
//!   oauth_inbound: false    # loopback inbound for an OAuth callback server
//! ```
//!
//! A missing section is `egress: none`: a tool that needs the network says so. That is a declared
//! default, not a fallback. Lowering a tool set's sections into one jail policy is
//! `tddy-sandbox-recipes`' `lower_network`.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The port a pattern that names none is read with.
pub const DEFAULT_EGRESS_PORT: u16 = 443;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSection {
    #[serde(default)]
    pub egress: Egress,
    /// The tool runs a loopback OAuth callback server the host's browser must reach.
    #[serde(default)]
    pub oauth_inbound: bool,
}

/// Which hosts a tool may reach: `none`, `any`, or `{ hosts: [pattern, …] }`.
///
/// Serde is hand-written because serde_yaml's derived enum form is a YAML tag (`!hosts [...]`),
/// not the `{ hosts: [...] }` map a tool file is written with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Egress {
    /// No network at all.
    #[default]
    None,
    /// Any `host:port`.
    Any,
    /// Exactly these hosts.
    Hosts(Vec<HostPattern>),
}

impl Serialize for Egress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Egress::None => serializer.serialize_str("none"),
            Egress::Any => serializer.serialize_str("any"),
            Egress::Hosts(hosts) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("hosts", hosts)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Egress {
    /// `none` / `any` as a string, `{ hosts: [...] }` as a one-key map. Anything else is refused
    /// naming what was expected; a malformed pattern is refused with its [`HostPatternError`].
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // TODO(egress-allowlist): implement — a visitor taking `visit_str` and `visit_map`.
        let _ = deserializer;
        todo!("egress-allowlist: Egress::deserialize")
    }
}

/// One `host[:port]` or `*.domain[:port]` a tool declares. Always valid: a malformed pattern is
/// refused when the tool file is read, so nothing downstream re-checks one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HostPattern {
    /// Lowercase; a leading `*.` matches every subdomain of the rest, never the rest itself.
    host: String,
    port: u16,
}

/// Why a host pattern was refused. Names the pattern as written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("host pattern `{pattern}`: {reason}")]
pub struct HostPatternError {
    pub pattern: String,
    pub reason: String,
}

impl HostPattern {
    /// Read `host[:port]` or `*.domain[:port]`; the port defaults to [`DEFAULT_EGRESS_PORT`]. A
    /// scheme, a path, an empty label, a `*` anywhere but a leading `*.`, and a port that is not
    /// `1..=65535` are refused.
    pub fn parse(pattern: &str) -> Result<Self, HostPatternError> {
        // TODO(egress-allowlist): implement
        let _ = pattern;
        todo!("egress-allowlist: HostPattern::parse")
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for HostPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl TryFrom<String> for HostPattern {
    type Error = HostPatternError;

    fn try_from(pattern: String) -> Result<Self, Self::Error> {
        Self::parse(&pattern)
    }
}

impl From<HostPattern> for String {
    fn from(pattern: HostPattern) -> Self {
        pattern.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{HostPattern, HostPatternError};

    fn refusal_of(pattern: &str, reason: &str) -> Result<HostPattern, HostPatternError> {
        Err(HostPatternError {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
        })
    }

    #[test]
    fn a_host_without_a_port_reads_as_port_443() {
        // Given / When
        let pattern = HostPattern::parse("api.anthropic.com").unwrap();

        // Then
        assert_eq!((pattern.host(), pattern.port()), ("api.anthropic.com", 443));
    }

    #[test]
    fn a_wildcard_with_a_port_keeps_both_and_the_host_is_lowercased() {
        // Given / When
        let pattern = HostPattern::parse("*.StatSig.com:8443").unwrap();

        // Then
        assert_eq!((pattern.host(), pattern.port()), ("*.statsig.com", 8443));
    }

    #[test]
    fn a_pattern_with_a_scheme_is_refused() {
        assert_eq!(
            HostPattern::parse("https://api.anthropic.com"),
            refusal_of("https://api.anthropic.com", "a scheme is not a host")
        );
    }

    #[test]
    fn a_wildcard_that_is_not_a_leading_label_is_refused() {
        assert_eq!(
            HostPattern::parse("api.*.com"),
            refusal_of("api.*.com", "`*` is only allowed as a leading `*.`")
        );
    }

    #[test]
    fn a_port_outside_1_to_65535_is_refused() {
        assert_eq!(
            HostPattern::parse("api.anthropic.com:0"),
            refusal_of("api.anthropic.com:0", "port must be 1..=65535")
        );
    }

    #[test]
    fn an_empty_label_is_refused() {
        assert_eq!(
            HostPattern::parse("api..anthropic.com"),
            refusal_of("api..anthropic.com", "empty label")
        );
    }

    #[test]
    fn a_section_reads_none_any_and_a_host_map_as_a_tool_file_writes_them() {
        // Given
        let sections = [
            "egress: none\n",
            "egress: any\noauth_inbound: true\n",
            "egress: { hosts: [\"api.anthropic.com\", \"127.0.0.1:8080\"] }\n",
        ];

        // When
        let read: Vec<super::NetworkSection> = sections
            .iter()
            .map(|yaml| serde_yaml::from_str(yaml).expect("a valid section"))
            .collect();

        // Then
        assert_eq!(
            read,
            vec![
                super::NetworkSection {
                    egress: super::Egress::None,
                    oauth_inbound: false,
                },
                super::NetworkSection {
                    egress: super::Egress::Any,
                    oauth_inbound: true,
                },
                super::NetworkSection {
                    egress: super::Egress::Hosts(vec![
                        HostPattern::parse("api.anthropic.com:443").unwrap(),
                        HostPattern::parse("127.0.0.1:8080").unwrap(),
                    ]),
                    oauth_inbound: false,
                },
            ]
        );
    }
}
