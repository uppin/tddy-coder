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
//! Feature: `docs/ft/web/hosts-screen-add-key.md`

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
///
/// The socket is found by [`crate::ssh_agent::WellKnownAgentSockets`] — the same resolution the
/// read side uses, called and not reimplemented, so an operator can never be shown a key list from
/// one agent and have a key added to another. Its limits are that resolver's: on a supervised host
/// the daemon's service account may resolve a socket it is not allowed to open, which is reported
/// as the refusal it is rather than as a missing agent.
pub struct WireProtocolAgentKeyAdder;

/// How long the agent may take to accept a key.
///
/// The same bound the read side gives an agent, for the same reason: an unresponsive socket must
/// never hold an RPC open. An add is a longer message than a request for identities but the same
/// single round trip, so it is not given longer.
#[cfg(unix)]
const ADD_TIMEOUT: std::time::Duration = crate::ssh_agent::AGENT_TIMEOUT;

#[cfg(unix)]
impl SshAgentKeyAdder for WireProtocolAgentKeyAdder {
    fn add_identity(&self, os_user: &str, identity: &PrivateKey) -> Result<(), AgentAddFailure> {
        use crate::ssh_agent::AgentSocketResolver;

        let socket = match crate::ssh_agent::WellKnownAgentSockets.socket_for(os_user) {
            Ok(Some(socket)) => socket,
            // Nowhere to send it — the same negative finding the probe reports as "no agent".
            Ok(None) => return Err(AgentAddFailure::Unreachable),
            // A lookup that could not be made establishes nothing about this user's agent, so it is
            // never reported as one being absent.
            Err(reason) => {
                return Err(AgentAddFailure::Refused(format!(
                    "could not look for {os_user}'s ssh-agent socket: {reason}"
                )))
            }
        };
        let stream = match std::os::unix::net::UnixStream::connect(&socket) {
            Ok(stream) => stream,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Err(AgentAddFailure::Unreachable)
            }
            // Anything else — a socket this daemon is not permitted to open, above all — says
            // nothing about whether an agent is running there, so it is not called absent.
            Err(e) => {
                return Err(AgentAddFailure::Refused(format!(
                    "could not open the ssh-agent socket {}: {e}",
                    socket.display()
                )))
            }
        };
        hand_over(stream, identity).map_err(AgentAddFailure::Refused)
    }
}

/// Send one ADD_IDENTITY down a connected socket and read what the agent made of it.
///
/// Bounded the way the read side bounds its exchange, and for the reasons documented there: both
/// socket timeouts are armed once on the freshly connected socket, because macOS refuses
/// `setsockopt` on a socket whose peer has already answered and hung up.
///
/// The private key reaches the agent and nothing else: no error raised here formats the credential,
/// and the passphrase that unlocked it never came to this module at all.
#[cfg(unix)]
fn hand_over(stream: std::os::unix::net::UnixStream, identity: &PrivateKey) -> Result<(), String> {
    use ssh_agent_lib::proto::{AddIdentity, PrivateCredential};

    stream
        .set_write_timeout(Some(ADD_TIMEOUT))
        .and_then(|()| stream.set_read_timeout(Some(ADD_TIMEOUT)))
        .map_err(|e| format!("the ssh-agent socket could not be given a deadline: {e}"))?;

    let mut client = ssh_agent_lib::blocking::Client::new(stream);
    client
        .add_identity(AddIdentity {
            credential: PrivateCredential::Key {
                privkey: identity.key_data().clone(),
                // What `ssh-add -l` will show beside the key. It is the comment the key file
                // carries, so the agent lists it under the name the operator already knows it by.
                comment: identity.comment().to_string(),
            },
        })
        // Says which exchange failed, never what was sent: an `AgentError` renders the transport
        // failure or the unexpected response, and neither is built from the credential.
        .map_err(|e| format!("the exchange failed: {e}"))
}

/// A platform with no Unix-domain socket has no agent to hand a key to. Reported as nothing being
/// reachable, which is what it is, rather than as an internal failure an operator would try to fix.
#[cfg(not(unix))]
impl SshAgentKeyAdder for WireProtocolAgentKeyAdder {
    fn add_identity(&self, _os_user: &str, _identity: &PrivateKey) -> Result<(), AgentAddFailure> {
        Err(AgentAddFailure::Unreachable)
    }
}
