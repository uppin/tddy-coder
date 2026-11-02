//! The jail's egress policy at its two enforcement points: the in-jail CONNECT shim and the host
//! relay's `TunnelOpen`.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md` (`#agent-tools 3/12`).
//!
//! Both points hold an [`EgressGuard`] and ask it independently, so a jailed process that bypasses
//! the shim by speaking the relay protocol directly is still refused. A refusal names the session,
//! the host and the tool set whose declarations lacked it, so an operator can see which host a tool
//! needed; the shim answers it as `403`, the relay as a failed `TunnelOpenAck`.

use std::fmt;

use tddy_sandbox::EgressPolicy;

/// One jail's egress policy, with what a refusal must name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressGuard {
    policy: EgressPolicy,
    session_id: String,
    /// The ids of the tools whose `network:` sections the policy was lowered from.
    tool_set: Vec<String>,
}

/// A connection the guard refused. Its `Display` is the line logged for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressRefusal {
    pub session_id: String,
    pub host: String,
    /// The port as it was asked for. A relayed `TunnelOpen` can ask for one no `u16` holds.
    pub port: u32,
    pub tool_set: Vec<String>,
}

impl EgressGuard {
    pub fn new(policy: EgressPolicy, session_id: impl Into<String>, tool_set: Vec<String>) -> Self {
        Self {
            policy,
            session_id: session_id.into(),
            tool_set,
        }
    }

    /// [`EgressPolicy::Any`] — what every jail had before policies existed, and what a recipe-built
    /// agent jail keeps until its node declares hosts.
    pub fn unrestricted(session_id: impl Into<String>) -> Self {
        Self::new(EgressPolicy::Any, session_id, Vec::new())
    }

    pub fn policy(&self) -> &EgressPolicy {
        &self.policy
    }

    /// Admit `host:port`, or the refusal naming it. Each enforcement point logs the refusal where
    /// its session's operator reads: the shim into the session's sandbox log, the relay through the
    /// host's logger.
    pub fn check(&self, host: &str, port: u16) -> Result<(), EgressRefusal> {
        if self.policy.admits(host, port) {
            return Ok(());
        }
        Err(self.refusal(host, u32::from(port)))
    }

    /// [`Self::check`] for a port as a `TunnelOpen` carries it, a `u32`. A value no `u16` holds is
    /// no port at all, so no policy admits it, `any` included, and it is refused outright with the
    /// same refusal an undeclared port gets.
    pub fn check_wire_port(&self, host: &str, port: u32) -> Result<(), EgressRefusal> {
        match u16::try_from(port) {
            Ok(port) => self.check(host, port),
            Err(_) => Err(self.refusal(host, port)),
        }
    }

    fn refusal(&self, host: &str, port: u32) -> EgressRefusal {
        EgressRefusal {
            session_id: self.session_id.clone(),
            host: host.to_string(),
            port,
            tool_set: self.tool_set.clone(),
        }
    }
}

impl EgressRefusal {
    /// The shim's answer to a refused `CONNECT`: `403 Forbidden`, the refusal as its body.
    pub fn forbidden_response(&self) -> Vec<u8> {
        let body = self.to_string();
        format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }
}

impl fmt::Display for EgressRefusal {
    /// `egress to <host>:<port> refused for session <id>: not declared by tool set [<a>, <b>]`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "egress to {}:{} refused for session {}: not declared by tool set [{}]",
            self.host,
            self.port,
            self.session_id,
            self.tool_set.join(", ")
        )
    }
}
