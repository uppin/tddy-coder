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
//!
//! # Why the identity list is read here rather than through `ssh-agent-lib`'s client
//!
//! The crate's client decodes each identity into an `ssh_key::public::KeyData`, which discards the
//! blob the agent actually sent. Both facts this module reports are properties of that blob: the
//! fingerprint OpenSSH prints is the SHA-256 of the bytes as received, and the key type is the
//! string they open with. Recovering them by re-encoding a decoded key would report our rendering
//! of the key rather than the agent's, and a single identity the crate cannot decode would turn a
//! populated agent into a probe failure. Reading the answer's fields directly costs a few dozen
//! lines and keeps every identity opaque. `ssh-agent-lib` earns its place in `#hosts-screen 6/8`,
//! where an ADD_IDENTITY request has to be *built* — the direction where a typed encoder is the
//! part worth borrowing.

use std::path::PathBuf;
use std::time::Duration;

use crate::host_tooling::ProbeOutcome;

/// How long the agent may take to answer before the probe gives up.
///
/// The Hosts screen probes every host it lists, so an unresponsive socket must never hold the RPC
/// open.
pub const AGENT_TIMEOUT: Duration = Duration::from_secs(3);

/// Ask the agent for the identities it holds (`SSH_AGENTC_REQUEST_IDENTITIES`).
const REQUEST_IDENTITIES: u8 = 11;

/// The agent's list of identities (`SSH_AGENT_IDENTITIES_ANSWER`).
const IDENTITIES_ANSWER: u8 = 12;

/// The agent declined the request (`SSH_AGENT_FAILURE`).
const AGENT_FAILURE: u8 = 5;

/// The largest answer this probe will read.
///
/// An agent's identity list is a few hundred bytes per key. The bound exists so a socket that is
/// not an agent — or one answering nonsense — cannot make the daemon allocate whatever length it
/// claims.
const MAX_ANSWER_BYTES: usize = 256 * 1024;

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
    /// An agent answered with `keys` — which may legitimately be none of them.
    #[must_use]
    pub fn holding(keys: Vec<AgentKey>) -> Self {
        Self {
            outcome: ProbeOutcome::Ok,
            reachable: true,
            keys,
        }
    }

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

    /// This platform has no Unix-socket agent to talk to at all — a property of the platform, not a
    /// finding about the host's keys.
    #[must_use]
    pub fn unsupported() -> Self {
        Self {
            outcome: ProbeOutcome::Unsupported,
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
///
/// OpenSSH hashes the blob exactly as it was received and prints the digest base64-encoded without
/// padding, so this hashes the same bytes rather than a key decoded and re-encoded — a round trip
/// would report our rendering of the key instead of the agent's. The blob's leading key type is
/// still read, so a byte string that is not a public key blob is refused rather than fingerprinted.
pub fn fingerprint_of(public_key_blob: &[u8]) -> Result<String, String> {
    use base64::Engine;
    use sha2::Digest;

    key_type_of(public_key_blob)?;
    let digest = sha2::Sha256::digest(public_key_blob);
    Ok(format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    ))
}

/// The key type a public key blob opens with, e.g. `ssh-ed25519`.
///
/// Read from the blob rather than mapped through a table of the types we know, so a host holding a
/// key this daemon has never heard of is still listed under the name its own agent uses.
pub fn key_type_of(public_key_blob: &[u8]) -> Result<String, String> {
    let name = Fields::over(public_key_blob).string()?;
    if name.is_empty() {
        return Err("a public key blob with no key type".to_string());
    }
    String::from_utf8(name.to_vec())
        .map_err(|_| "a public key blob whose key type is not text".to_string())
}

/// A reader over the fields of an agent message, in RFC 4251's encoding.
struct Fields<'a> {
    rest: &'a [u8],
}

impl<'a> Fields<'a> {
    fn over(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn u32(&mut self) -> Result<u32, String> {
        let (head, tail) = self
            .rest
            .split_at_checked(4)
            .ok_or_else(|| "the agent's answer ended inside a number".to_string())?;
        self.rest = tail;
        Ok(u32::from_be_bytes(
            head.try_into().expect("split_at_checked gave four bytes"),
        ))
    }

    /// The next `string`: a four-byte length and that many bytes, which are not assumed to be text.
    fn string(&mut self) -> Result<&'a [u8], String> {
        let len = self.u32()? as usize;
        let (head, tail) = self.rest.split_at_checked(len).ok_or_else(|| {
            format!(
                "the agent's answer claims a {len}-byte field but carries {} more bytes",
                self.rest.len()
            )
        })?;
        self.rest = tail;
        Ok(head)
    }
}

/// Turn an `IDENTITIES_ANSWER` body — everything after the message type — into typed identities.
///
/// The count is not trusted as a capacity: an answer claiming four billion identities has to run
/// out of bytes on its first one, not allocate for them.
fn identities_in(answer: &[u8]) -> Result<Vec<AgentKey>, String> {
    let mut fields = Fields::over(answer);
    let count = fields.u32()?;
    let mut keys = Vec::new();
    for _ in 0..count {
        let blob = fields.string()?;
        let comment = fields.string()?;
        keys.push(AgentKey {
            key_type: key_type_of(blob)?,
            fingerprint: fingerprint_of(blob)?,
            // Free text from another user's key file: rendered, never parsed, and lossy rather than
            // rejected — one key with an odd byte in its comment must not hide the whole list.
            comment: String::from_utf8_lossy(comment).into_owned(),
        });
    }
    Ok(keys)
}

/// The live probe: connects to the resolved socket and performs `REQUEST_IDENTITIES`.
#[cfg(unix)]
pub struct WireProtocolAgentProbe<R: AgentSocketResolver> {
    resolver: R,
}

#[cfg(unix)]
impl<R: AgentSocketResolver> WireProtocolAgentProbe<R> {
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }
}

#[cfg(unix)]
impl<R: AgentSocketResolver> SshAgentProbe for WireProtocolAgentProbe<R> {
    fn identities(&self, os_user: &str) -> AgentStatus {
        let Some(socket) = self.resolver.socket_for(os_user) else {
            return AgentStatus::unreachable();
        };
        match std::os::unix::net::UnixStream::connect(&socket) {
            Ok(stream) => match ask_for_identities(stream) {
                Ok(keys) => AgentStatus::holding(keys),
                Err(reason) => {
                    AgentStatus::failed(format!("the ssh-agent at {} {reason}", socket.display()))
                }
            },
            // Nothing listening — the negative finding this probe exists to report.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                AgentStatus::unreachable()
            }
            // Any other refusal — a socket this daemon is not allowed to open, above all — says
            // nothing about whether an agent is running there. Reporting "no agent" for it would
            // send an operator to start one that is already running.
            Err(e) => AgentStatus::failed(format!(
                "could not open the ssh-agent socket {} belonging to {os_user}: {e}",
                socket.display()
            )),
        }
    }
}

/// Ask a connected agent for its identities, giving up at [`AGENT_TIMEOUT`].
///
/// Every failure reads as the tail of "the ssh-agent at `<path>` …", so an operator is told which
/// socket misbehaved and how.
#[cfg(unix)]
fn ask_for_identities(mut stream: std::os::unix::net::UnixStream) -> Result<Vec<AgentKey>, String> {
    use std::io::Write;

    // Both timeouts are armed here, on a socket that was just connected: macOS refuses to set one
    // on a socket whose peer has already answered and hung up, which is exactly where re-arming the
    // read timeout between reads would land.
    stream
        .set_write_timeout(Some(AGENT_TIMEOUT))
        .and_then(|()| stream.set_read_timeout(Some(AGENT_TIMEOUT)))
        .map_err(|e| format!("could not be given a deadline: {e}"))?;
    // A second, whole-exchange bound. The socket timeout bounds one read, and an agent answering a
    // byte at a time would renew it for as long as it liked — the Hosts screen probes every host it
    // lists, so the conversation needs a bound of its own.
    let deadline = std::time::Instant::now() + AGENT_TIMEOUT;
    stream
        .write_all(&[0, 0, 0, 1, REQUEST_IDENTITIES])
        .map_err(|e| format!("could not be asked for its identities: {e}"))?;

    let mut length = [0u8; 4];
    read_exactly(&mut stream, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 {
        return Err("answered with an empty message".to_string());
    }
    if length > MAX_ANSWER_BYTES {
        return Err(format!(
            "announced a {length}-byte answer, past the {MAX_ANSWER_BYTES} bytes an identity list \
             is allowed to take"
        ));
    }
    let mut body = vec![0u8; length];
    read_exactly(&mut stream, &mut body, deadline)?;

    match body[0] {
        IDENTITIES_ANSWER => identities_in(&body[1..]).map_err(|reason| {
            format!("sent an identity list this daemon could not read: {reason}")
        }),
        AGENT_FAILURE => Err("refused to list its identities".to_string()),
        other => Err(format!(
            "answered with message type {other} rather than an identity list"
        )),
    }
}

/// Fill `buf` from `stream`, or give up once `deadline` has passed.
///
/// `Read::read_exact` is not used because it leaves the buffer in an unspecified state when a read
/// times out, and this has to tell a short answer apart from a slow one. The socket's own timeout
/// bounds each read and `deadline` bounds the exchange, so the two together cap it at twice
/// [`AGENT_TIMEOUT`] — a bound the caller keeps whatever the agent does.
#[cfg(unix)]
fn read_exactly(
    stream: &mut std::os::unix::net::UnixStream,
    buf: &mut [u8],
    deadline: std::time::Instant,
) -> Result<(), String> {
    use std::io::Read;

    let mut filled = 0;
    while filled < buf.len() {
        if std::time::Instant::now() >= deadline {
            return Err(timed_out());
        }
        match stream.read(&mut buf[filled..]) {
            Ok(0) => return Err("closed the connection before answering".to_string()),
            Ok(read) => filled += read,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            // A socket read that hits its timeout surfaces as either of these, platform depending.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(timed_out())
            }
            Err(e) => return Err(format!("could not be read: {e}")),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn timed_out() -> String {
    format!("did not answer within {}s", AGENT_TIMEOUT.as_secs())
}

/// Where an OS user's agent socket is looked for in production.
///
/// There is no way to *ask* for another user's `SSH_AUTH_SOCK`: it is set in that user's login
/// session, and [`crate::spawner::run_capture_as_user`] builds a child environment rather than
/// inheriting one. So the socket is located on the filesystem instead, at the two places a
/// user-session agent puts it, and only ever one that belongs to the user being asked about:
///
/// 1. **The daemon's own `SSH_AUTH_SOCK`**, and only when the user asked about is the daemon's own
///    user. It names one user's agent — ours — so it is an answer about nobody else.
/// 2. **The per-user runtime directory**, `/run/user/<uid>/…`, where a systemd user unit,
///    `gpg-agent` and `gnome-keyring` each publish a socket at a fixed name.
///
/// A shell-launched `ssh-agent` (`/tmp/ssh-XXXXXX/agent.<pid>`) is deliberately **not** hunted for.
/// Its directory name is random, a user with several login sessions has several of them, and
/// picking one would report one session's keys as the host's. "No agent reachable" is the honest
/// answer where the socket cannot be named.
///
/// ⚠ Locating the socket is not the same as being allowed to open it. A user-session agent's socket
/// is only reachable by that user and by root, and the daemon installed by `./install` runs as an
/// unprivileged service account — so on a supervised host this resolves sockets it cannot connect
/// to, and the probe reports "could not check" with the permission error rather than a fabricated
/// empty key list. See `docs/dev/1-WIP/2026-09-06-agent-keys.md`.
#[cfg(unix)]
pub struct WellKnownAgentSockets;

#[cfg(unix)]
impl AgentSocketResolver for WellKnownAgentSockets {
    fn socket_for(&self, os_user: &str) -> Option<PathBuf> {
        let uid = match crate::pty_runtime::resolve_pty_os_user(os_user) {
            Ok(user) => user.uid,
            Err(reason) => {
                log::debug!("ssh-agent: no passwd entry to resolve a socket for: {reason}");
                return None;
            }
        };
        our_own_agent_socket(uid).or_else(|| runtime_dir_agent_socket(uid))
    }
}

/// The agent socket this daemon's own session was handed, when `uid` is the daemon's own user.
#[cfg(unix)]
fn our_own_agent_socket(uid: u32) -> Option<PathBuf> {
    // SAFETY: `getuid` reads the calling process's own real uid and cannot fail.
    if uid != unsafe { libc::getuid() } {
        return None;
    }
    let socket = PathBuf::from(std::env::var_os("SSH_AUTH_SOCK")?);
    // A stale value — an agent that has since exited — falls through to the runtime directory
    // rather than deciding the answer.
    socket.exists().then_some(socket)
}

/// The socket a user-session agent publishes at a fixed name under `/run/user/<uid>`.
#[cfg(unix)]
fn runtime_dir_agent_socket(uid: u32) -> Option<PathBuf> {
    use std::os::unix::fs::FileTypeExt;

    let runtime_dir = PathBuf::from(format!("/run/user/{uid}"));
    let candidates = [
        // A systemd user unit, `ssh-agent.service`.
        runtime_dir.join("ssh-agent.socket"),
        // `gpg-agent` with `enable-ssh-support`.
        runtime_dir.join("gnupg/S.gpg-agent.ssh"),
        // `gnome-keyring-daemon`.
        runtime_dir.join("keyring/ssh"),
    ];
    let mut unreadable = None;
    for candidate in candidates {
        match std::fs::metadata(&candidate) {
            Ok(found) if found.file_type().is_socket() => return Some(candidate),
            Ok(_) => {}
            // Not allowed to look. The path is handed back anyway, so connecting to it reports the
            // permission error — which is the truth — where skipping it would report "no agent", a
            // negative finding nothing here established.
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                unreadable.get_or_insert(candidate);
            }
            Err(_) => {}
        }
    }
    unreadable
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
    const ED25519_FINGERPRINT: &str = "SHA256:JfISx02kSjWJevGy/MjUdXCv76HaRM3YkYNvepTyHD8";

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
