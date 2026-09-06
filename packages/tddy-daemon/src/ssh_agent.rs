//! Talking to a host user's ssh-agent.
//!
//! Nothing in tddy has ever spoken to an agent — before this module `Cargo.lock` contained no SSH
//! crate at all. We speak the **agent wire protocol** rather than shelling out to `ssh-add -l`,
//! because the four outcomes an operator needs distinguished are only unambiguous on the wire:
//!
//! | Outcome | `ssh-add -l` | protocol |
//! |---|---|---|
//! | agent holding keys | exit 0, parse the text | `IDENTITIES_ANSWER` with entries |
//! | agent, no keys | exit 1 + a message | `IDENTITIES_ANSWER`, empty |
//! | no agent reachable | exit 2 + a message | connect fails |
//! | `ssh-add` missing | spawn fails | n/a — no binary involved |
//!
//! `ssh-add`'s output is human-readable text, not an API, and the middle two are distinguished only
//! by an exit code and a message string. Getting that wrong tells an operator their agent is empty
//! when it is absent, or vice versa.
//!
//! It also pays forward: `#hosts-screen 6/8` adds a key by decrypting the private key **in process**
//! and handing the agent an ADD_IDENTITY, so it never needs a TTY or `SSH_ASKPASS`.

use std::path::PathBuf;
use std::time::Duration;

use crate::host_tooling::ProbeOutcome;

/// How long the agent may take to answer before the probe gives up.
///
/// The Hosts screen probes every host it lists, so an unresponsive socket must never hold the RPC
/// open.
pub const AGENT_TIMEOUT: Duration = Duration::from_secs(3);

/// One identity the agent is holding.
///
/// Deliberately **no originating path**: the agent does not know it. The comment is free text set at
/// key-generation time and must never be presented as a file location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentKey {
    /// e.g. `ssh-ed25519`.
    pub key_type: String,
    /// `SHA256:…`, the form `ssh-add -l` prints.
    pub fingerprint: String,
    pub comment: String,
}

/// What the agent probe found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub outcome: ProbeOutcome,
    /// An agent answered. `false` with `Ok` means none is reachable — which is a different fact from
    /// an agent that answered with no keys (`true` and an empty `keys`).
    pub reachable: bool,
    pub keys: Vec<AgentKey>,
}

impl AgentStatus {
    /// No agent could be reached — a successful probe with a negative finding.
    #[must_use]
    pub fn unreachable() -> Self {
        Self {
            outcome: ProbeOutcome::Ok,
            reachable: false,
            keys: Vec::new(),
        }
    }

    /// The probe itself could not run or its answer was not understood.
    #[must_use]
    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            outcome: ProbeOutcome::Failed(reason.into()),
            reachable: false,
            keys: Vec::new(),
        }
    }
}

/// Where a given OS user's agent socket lives.
///
/// A seam, because this is the hard part and the part a test must be able to control.
/// `SSH_AUTH_SOCK` is per-user and per-login-session, and
/// [`crate::spawner::run_capture_as_user`] *constructs* a child environment rather than inheriting a
/// login session's — so the daemon does not simply have the value to hand.
///
/// ⚠ Under a supervisor the daemon may not be able to see it at all: `resolve_env`
/// (`packages/tddy-supervisor/src/policy.rs`) is an **allowlist**, and its own test fixture uses
/// `SSH_AUTH_SOCK` as the example of a *denied* key. When that is the case the honest report is
/// "no agent reachable", not a fabricated empty key list.
pub trait AgentSocketResolver: Send + Sync {
    /// The agent socket for `os_user`, or `None` when none can be located.
    fn socket_for(&self, os_user: &str) -> Option<PathBuf>;
}

/// Reads a host user's agent over the wire protocol.
pub trait SshAgentProbe: Send + Sync {
    /// Enumerate the identities `os_user`'s agent holds, bounded by [`AGENT_TIMEOUT`].
    fn identities(&self, os_user: &str) -> AgentStatus;
}

/// Derive the `SHA256:` fingerprint of a public key blob, as `ssh-add -l` prints it.
///
/// Split out from the socket conversation so it can be pinned against a fingerprint computed
/// **outside this codebase** — otherwise the test proves only that we agree with ourselves.
pub fn fingerprint_of(_public_key_blob: &[u8]) -> Result<String, String> {
    // TODO(agent-keys): implement
    unimplemented!("agent-keys: fingerprint_of")
}

/// The live probe: connects to the resolved socket and performs `REQUEST_IDENTITIES`.
pub struct WireProtocolAgentProbe<R: AgentSocketResolver> {
    #[allow(dead_code)] // consumed by `identities` once implemented (#hosts-screen 5/8 green)
    resolver: R,
}

impl<R: AgentSocketResolver> WireProtocolAgentProbe<R> {
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }
}

impl<R: AgentSocketResolver> SshAgentProbe for WireProtocolAgentProbe<R> {
    fn identities(&self, _os_user: &str) -> AgentStatus {
        // TODO(agent-keys): implement
        unimplemented!("agent-keys: identities")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::mpsc;

    /// SSH agent protocol constants (RFC 4251 framing, draft-miller-ssh-agent).
    const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
    const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;

    /// A published ed25519 public key blob and the fingerprint OpenSSH prints for it.
    ///
    /// The fingerprint is computed **outside this codebase** (`ssh-keygen -lf`), so the assertion
    /// pins our derivation against OpenSSH rather than against our own output.
    const ED25519_BLOB_B64: &str =
        "AAAAC3NzaC1lZDI1NTE5AAAAINMHWkNaFcgtIbUiRuGXYCbYWjHPPGWLKxHqBLzWyPhL";
    const ED25519_FINGERPRINT: &str = "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg";

    fn ssh_string(bytes: &[u8]) -> Vec<u8> {
        let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(bytes);
        out
    }

    /// Frame an agent message: a 4-byte length, a type byte, then the payload.
    fn agent_frame(msg_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut body = vec![msg_type];
        body.extend_from_slice(payload);
        let mut out = ((body.len()) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    /// An `IDENTITIES_ANSWER` carrying `keys` as (blob, comment) pairs.
    fn identities_answer(keys: &[(Vec<u8>, &str)]) -> Vec<u8> {
        let mut payload = (keys.len() as u32).to_be_bytes().to_vec();
        for (blob, comment) in keys {
            payload.extend_from_slice(&ssh_string(blob));
            payload.extend_from_slice(&ssh_string(comment.as_bytes()));
        }
        agent_frame(SSH_AGENT_IDENTITIES_ANSWER, &payload)
    }

    /// A fake ssh-agent on a real Unix socket.
    ///
    /// A real socket rather than a mocked trait: the point of choosing the wire protocol over
    /// `ssh-add` was that the protocol answers unambiguously, and a mocked client would exercise
    /// none of the encoding that claim rests on.
    struct FakeAgent {
        path: PathBuf,
        _dir: tempfile::TempDir,
    }

    impl FakeAgent {
        /// Serve exactly one `REQUEST_IDENTITIES` with `response`, then close.
        fn serving(response: Vec<u8>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("agent.sock");
            let listener = UnixListener::bind(&path).unwrap();
            std::thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut header = [0u8; 5];
                    if stream.read_exact(&mut header).is_ok()
                        && header[4] == SSH_AGENTC_REQUEST_IDENTITIES
                    {
                        let _ = stream.write_all(&response);
                        let _ = stream.flush();
                    }
                }
            });
            Self { path, _dir: dir }
        }

        /// Accept the connection and then never answer — the hang case.
        fn silent() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("agent.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let (tx, rx) = mpsc::channel::<UnixStream>();
            std::thread::spawn(move || {
                if let Ok((stream, _)) = listener.accept() {
                    // Hold the connection open, unanswered, until the test drops the receiver.
                    let _ = tx.send(stream);
                    let _ = rx.recv();
                }
            });
            Self { path, _dir: dir }
        }
    }

    /// A resolver that points every user at one socket path.
    struct FixedSocket(Option<PathBuf>);

    impl AgentSocketResolver for FixedSocket {
        fn socket_for(&self, _os_user: &str) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    fn probe_against(socket: Option<PathBuf>) -> AgentStatus {
        WireProtocolAgentProbe::new(FixedSocket(socket)).identities("someone")
    }

    fn a_blob() -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(ED25519_BLOB_B64)
            .expect("the fixture blob is valid base64")
    }

    #[test]
    fn lists_the_identities_a_reachable_agent_holds() {
        let agent = FakeAgent::serving(identities_answer(&[(a_blob(), "ada@workstation")]));

        let status = probe_against(Some(agent.path.clone()));

        assert_eq!(status.outcome, ProbeOutcome::Ok);
        assert!(status.reachable);
        assert_eq!(status.keys.len(), 1);
        assert_eq!(status.keys[0].comment, "ada@workstation");
        assert_eq!(status.keys[0].key_type, "ssh-ed25519");
    }

    /// An agent holding nothing is a different problem from no agent at all: one needs a key added,
    /// the other needs an agent started.
    #[test]
    fn reports_an_agent_holding_no_keys_distinctly_from_no_agent() {
        let agent = FakeAgent::serving(identities_answer(&[]));

        let empty = probe_against(Some(agent.path.clone()));
        let absent = probe_against(None);

        assert!(empty.reachable, "the agent answered, so it is reachable");
        assert!(empty.keys.is_empty());
        assert!(!absent.reachable, "no socket means no agent");
        assert_eq!(
            absent.outcome,
            ProbeOutcome::Ok,
            "finding no agent is a successful probe with a negative finding, not a failure"
        );
    }

    #[test]
    fn reports_that_no_agent_is_reachable_when_the_socket_is_absent() {
        let dir = tempfile::tempdir().unwrap();

        let status = probe_against(Some(dir.path().join("nothing-here.sock")));

        assert_eq!(status.outcome, ProbeOutcome::Ok);
        assert!(!status.reachable);
    }

    /// A socket that accepts and never answers must time out into a reported failure rather than
    /// holding the RPC open — the Hosts screen probes every host it lists.
    #[test]
    fn reports_a_probe_failure_when_the_agent_does_not_answer_in_time() {
        let agent = FakeAgent::silent();

        let started = std::time::Instant::now();
        let status = probe_against(Some(agent.path.clone()));

        assert!(
            matches!(status.outcome, ProbeOutcome::Failed(_)),
            "a silent agent is a probe failure, got {:?}",
            status.outcome
        );
        assert!(
            started.elapsed() < AGENT_TIMEOUT * 3,
            "the probe must give up near its own timeout, not hang"
        );
    }

    /// Pinned against OpenSSH's own output, not ours.
    #[test]
    fn derives_the_sha256_fingerprint_ssh_add_displays() {
        let fingerprint = fingerprint_of(&a_blob()).expect("a valid ed25519 blob");

        assert_eq!(fingerprint, ED25519_FINGERPRINT);
    }
}
