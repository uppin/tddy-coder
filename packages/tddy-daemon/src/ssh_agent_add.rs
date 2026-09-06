//! Handing an unlocked identity to a host user's ssh-agent.
//!
//! Separate from [`crate::ssh_agent`], which reads what an agent holds and is owned by
//! `#hosts-screen 5/8`. This is the write side, and it exists as its own trait for one reason: an
//! add is the only step of the flow that touches the operator's real agent, so it is the step a
//! test must be able to replace. Everything before it — the prompt, the encryption, the decrypt,
//! the unlock — is exercised for real; only the agent itself is stood in for.
//!
//! The identity arrives **already unlocked**. The passphrase does not reach this module, and no
//! implementation of this trait ever sees one: the decrypt happens in process, in the handler,
//! and the plaintext is dropped there.
//!
//! Feature: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`

use ssh_key::PrivateKey;

/// Why an identity did not reach an agent.
///
/// Split rather than a bare string because the two call for different things from an operator: no
/// agent is something they fix on the host, a refusal is something they report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAddFailure {
    /// No agent answered for this OS user, so there was nothing to add the key to.
    Unreachable,
    /// An agent answered and the exchange failed. For an operator, not for parsing.
    Refused(String),
}

/// Adds an unlocked private key to a host user's ssh-agent.
pub trait SshAgentKeyAdder: Send + Sync {
    /// Hand `identity` to `os_user`'s agent.
    ///
    /// Takes the key by reference: the caller owns the unlocked key and drops it as soon as this
    /// returns, and an implementation that wanted to retain one would have to say so.
    fn add_identity(&self, os_user: &str, identity: &PrivateKey) -> Result<(), AgentAddFailure>;
}

/// Adds a key over the ssh-agent wire protocol, the same conversation [`crate::ssh_agent`] reads
/// identities with.
///
/// Speaking the protocol rather than shelling out to `ssh-add` is what lets the passphrase stay in
/// this process: `ssh-add` would want a TTY or `SSH_ASKPASS`, and the daemon deliberately has
/// neither.
pub struct WireProtocolAgentKeyAdder;

impl SshAgentKeyAdder for WireProtocolAgentKeyAdder {
    fn add_identity(&self, _os_user: &str, _identity: &PrivateKey) -> Result<(), AgentAddFailure> {
        unimplemented!("agent-add-key: add_identity")
    }
}
