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
//! # The conversation is `ssh-agent-lib`'s blocking client
//!
//! `ssh_agent_lib::blocking::Client` speaks the protocol; this module supplies the transport and
//! all of the bounds. It is the same crate `#hosts-screen 6/8` needs for *building* an
//! ADD_IDENTITY request, so the encoding is borrowed once rather than hand-written in one
//! direction and borrowed in the other.
//!
//! That client is **blocking**: `request_identities` writes and then reads on the calling thread,
//! and takes no deadline of its own. So every bound lives on the transport it is handed —
//!
//! * a read timeout and a write timeout on the socket, which bound a single read or write; and
//! * `UntilDeadline`, which fails the exchange once it has run past [`AGENT_TIMEOUT`] in total, no
//!   matter how little any one read waited. Without it an agent dribbling a byte at a time would
//!   renew the socket timeout for as long as it liked.
//!
//! Both socket timeouts are armed **once, on the freshly connected socket**: macOS refuses
//! `setsockopt` on a socket whose peer has already answered and hung up, which is exactly where
//! re-arming between reads would land. That is why the whole-exchange bound is a deadline the
//! transport checks rather than a timeout it re-applies.
//!
//! [`SshAgentProbe::identities`] is therefore synchronous and self-bounding, and the thread the
//! blocking client runs on is the one `host_tooling::start_agent_probe` already starts for it
//! beside the `git` and `gh` probes. Nothing here starts a thread of its own, and nothing here
//! reaches into the async runtime's blocking pool — `HostToolingProbe::probe` is *itself* already
//! running inside a `spawn_blocking` task, so a nested one would have to block a pool thread on
//! another pool thread.
//!
//! # Which bytes each fact is taken from
//!
//! Both facts are recovered by **re-encoding** the identity's credential, but from two different
//! encodings of it, because `ssh-add -l` draws its line between them exactly there:
//!
//! * the **key type** from the credential as a whole, so a certificate is listed as a certificate
//!   rather than as the key inside it — the distinction OpenSSH prints as `(ED25519-CERT)`;
//! * the **fingerprint** from `PublicCredential::key_data()`, the key being certified, because that
//!   is the one OpenSSH hashes for both. It prints the same digest for a key and for a certificate
//!   over it; hashing the certificate's own bytes would produce a value an operator could not match
//!   against anything they can print, which would defeat showing a fingerprint at all.
//!
//! # Known limits
//!
//! * **One unreadable identity fails the whole list.** `Identity::decode_vec` decodes the answer
//!   all-or-nothing, so an agent holding one key this version of `ssh-key` cannot parse
//!   is reported as a probe failure rather than as the host's other keys. A failure, never an
//!   empty list, so it cannot be read as "this host has no keys loaded".
//! * **A comment that is not UTF-8 sinks the list with it**, for the same reason: the crate decodes
//!   a comment as a `String`, where reading the field by hand rendered it lossily and kept the key.
//!   One odd byte in one comment now hides every key on the host.
//! * **The client allocates the answer's announced length before reading it**, with no cap of its
//!   own — a socket that is not an agent can make the daemon reserve up to 4 GiB before the read
//!   fails. Reaching one means being allowed to open it: [`WellKnownAgentSockets`] only ever hands
//!   back a socket owned by the user being asked about.
//! * **Finding the socket is not bounded at all.** `getpwnam_r` (through `resolve_pty_os_user`) and
//!   `UnixStream::connect` both run before any deadline exists and neither takes one, so a wedged
//!   NSS backend parks the probe thread indefinitely. The bounds above cover the conversation, not
//!   getting to it; the Hosts screen polls, so a host whose directory service is down accumulates a
//!   parked thread per poll.

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
    /// e.g. `ssh-ed25519`, or `ssh-ed25519-cert-v01@openssh.com` for a certificate.
    pub key_type: String,
    /// `SHA256:…`, the form `ssh-add -l` prints — for a certificate, the fingerprint of the key it
    /// certifies, which is what `ssh-add -l` prints for one.
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
/// [`tddy_daemon_kernel::spawn_as_user::run_capture_as_user`] *constructs* a child environment rather than inheriting a
/// login session's — so the daemon does not simply have the value to hand.
///
/// ⚠ Under a supervisor the daemon usually cannot reach another user's socket — not because of the
/// spawn policy's env allowlist (`resolve_env` gates a *session the daemon asks to spawn*, and a
/// declared managed service is started with `EnvironmentBase::Inherited`), but because `./install`
/// runs the daemon as an unprivileged service account and `/run/user/<uid>` is `0700`. The honest
/// report there is the permission error, never a fabricated empty key list. See
/// `packages/tddy-daemon/docs/host-tooling-probe.md` § *Reaching the socket on a supervised host*.
pub trait AgentSocketResolver: Send + Sync {
    /// The agent socket for `os_user`, or `Ok(None)` when the lookup succeeded and named none.
    ///
    /// `Err` is for a lookup that could not be *made* — a passwd database that did not answer, say.
    /// It is a separate answer from `Ok(None)` because it is separate evidence: "I looked and this
    /// user has no agent" sends an operator to start one, while "I could not look" sends them to
    /// their directory service. Collapsing the two would report a failure as a negative finding.
    fn socket_for(&self, os_user: &str) -> Result<Option<PathBuf>, String>;
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
/// OpenSSH hashes a key's standard public-key encoding — for a certificate, the encoding of the key
/// it certifies rather than of the certificate — and prints the digest base64-encoded without
/// padding. This hashes whatever blob it is handed, leaving which of the two to the caller; the
/// blob's leading key type is still read, so a byte string that is not a public key blob is refused
/// rather than fingerprinted.
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
/// key this daemon has never heard of is still listed under the name its own agent uses — and so a
/// certificate is listed as a certificate rather than as the key it certifies.
pub fn key_type_of(public_key_blob: &[u8]) -> Result<String, String> {
    use ssh_agent_lib::ssh_encoding::Decode;

    let name = String::decode(&mut &public_key_blob[..])
        .map_err(|e| format!("a public key blob whose key type could not be read: {e}"))?;
    if name.is_empty() {
        return Err("a public key blob with no key type".to_string());
    }
    Ok(name)
}

/// The live probe: connects to the resolved socket and asks it for its identities.
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
        let socket = match self.resolver.socket_for(os_user) {
            Ok(Some(socket)) => socket,
            Ok(None) => return AgentStatus::unreachable(),
            // Nothing was established about this user's agent, so nothing is reported about it.
            Err(reason) => {
                return AgentStatus::failed(format!(
                    "could not look for {os_user}'s ssh-agent socket: {reason}"
                ))
            }
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
fn ask_for_identities(stream: std::os::unix::net::UnixStream) -> Result<Vec<AgentKey>, String> {
    // Both timeouts are armed here, on a socket that was just connected: macOS refuses to set one
    // on a socket whose peer has already answered and hung up.
    stream
        .set_write_timeout(Some(AGENT_TIMEOUT))
        .and_then(|()| stream.set_read_timeout(Some(AGENT_TIMEOUT)))
        .map_err(|e| format!("could not be given a deadline: {e}"))?;

    let mut client = ssh_agent_lib::blocking::Client::new(UntilDeadline::over(
        stream,
        AGENT_TIMEOUT,
        "the ssh-agent took longer than the whole exchange is allowed",
    ));
    match client.request_identities() {
        Ok(identities) => identities
            .iter()
            .map(key_held)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|reason| {
                format!("sent an identity list this daemon could not read: {reason}")
            }),
        Err(e) if timed_out_talking(&e) => Err(timed_out()),
        Err(e) => Err(format!("did not answer with an identity list: {e}")),
    }
}

/// One identity, split the way `ssh-add -l` splits it.
///
/// The two facts come from two re-encodings of the same credential — the whole of it for the type,
/// the key it certifies for the fingerprint. See the module documentation for why they differ.
#[cfg(unix)]
fn key_held(identity: &ssh_agent_lib::proto::Identity) -> Result<AgentKey, String> {
    let credential = re_encoded(&identity.credential)?;
    let public_key = re_encoded(identity.credential.key_data())?;
    Ok(AgentKey {
        key_type: key_type_of(&credential)?,
        fingerprint: fingerprint_of(&public_key)?,
        // Free text from another user's key file: rendered, never parsed.
        comment: identity.comment.clone(),
    })
}

/// The bytes of a decoded key or credential, in the encoding it arrived in.
#[cfg(unix)]
fn re_encoded(encodable: &impl ssh_agent_lib::ssh_encoding::Encode) -> Result<Vec<u8>, String> {
    let mut blob = Vec::new();
    encodable
        .encode(&mut blob)
        .map_err(|e| format!("an identity whose key could not be read back: {e}"))?;
    Ok(blob)
}

/// Whether a client error is this exchange running out of time, in either of the two shapes it
/// takes: the socket's own timeout, and [`UntilDeadline`]'s.
///
/// The client's I/O errors reach us wrapped in a protocol error — its request and response both go
/// through a function that returns `ProtoError` — while `AgentError::IO` is the shape its other
/// entry points produce. Matching only one of them would report a timeout as an unreadable answer.
#[cfg(unix)]
fn timed_out_talking(error: &ssh_agent_lib::error::AgentError) -> bool {
    use ssh_agent_lib::error::AgentError;
    use ssh_agent_lib::proto::ProtoError;

    let io = match error {
        AgentError::Proto(ProtoError::IO(io)) => io,
        AgentError::IO(io) => io,
        _ => return false,
    };
    matches!(
        io.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

#[cfg(unix)]
fn timed_out() -> String {
    format!("did not answer within {}s", AGENT_TIMEOUT.as_secs())
}

/// A stream that stops carrying an exchange once its deadline has passed.
///
/// The socket's own timeouts bound one read or write; this bounds the conversation, so an agent
/// answering a byte at a time cannot renew them indefinitely. The two together cap the exchange at
/// twice the deadline — one read may start just inside it and then take a full socket timeout of
/// its own — which is a bound the caller keeps whatever the agent does.
///
/// A checked deadline rather than a re-armed socket timeout, because macOS refuses `setsockopt` on
/// a socket whose peer has already answered and closed.
#[cfg(unix)]
struct UntilDeadline<S> {
    stream: S,
    deadline: std::time::Instant,
    overrun: &'static str,
}

#[cfg(unix)]
impl<S> UntilDeadline<S> {
    fn over(stream: S, within: Duration, overrun: &'static str) -> Self {
        Self {
            stream,
            deadline: std::time::Instant::now() + within,
            overrun,
        }
    }

    /// `Err` once there is no time left, in the shape a socket that timed out would have produced.
    fn while_there_is_time(&self) -> std::io::Result<()> {
        if std::time::Instant::now() >= self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                self.overrun,
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
impl<S: std::io::Read> std::io::Read for UntilDeadline<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.while_there_is_time()?;
        self.stream.read(buf)
    }
}

#[cfg(unix)]
impl<S: std::io::Write> std::io::Write for UntilDeadline<S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.while_there_is_time()?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.while_there_is_time()?;
        self.stream.flush()
    }
}

/// Where an OS user's agent socket is looked for in production.
///
/// There is no way to *ask* for another user's `SSH_AUTH_SOCK`: it is set in that user's login
/// session, and [`tddy_daemon_kernel::spawn_as_user::run_capture_as_user`] builds a child environment rather than
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
/// empty key list. See `packages/tddy-daemon/docs/host-tooling-probe.md` § *Reaching the socket on
/// a supervised host*.
#[cfg(unix)]
pub struct WellKnownAgentSockets;

#[cfg(unix)]
impl AgentSocketResolver for WellKnownAgentSockets {
    fn socket_for(&self, os_user: &str) -> Result<Option<PathBuf>, String> {
        // A passwd lookup that failed is not a user without an agent: `resolve_pty_os_user`
        // collapses a down directory service into the same `Err` as an unknown name, and neither
        // establishes anything about this user's keys.
        let uid = tddy_daemon_kernel::privilege_drop::resolve_pty_os_user(os_user)?.uid;
        Ok(our_own_agent_socket(uid).or_else(|| runtime_dir_agent_socket(uid)))
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

    /// The whole frame that asks for the identity list: a four-byte length and the single byte of
    /// body it announces.
    const REQUEST_IDENTITIES_FRAME: [u8; 5] = [0, 0, 0, 1, SSH_AGENTC_REQUEST_IDENTITIES];

    /// A published ed25519 public key blob and the fingerprint OpenSSH prints for it.
    ///
    /// The fingerprint is computed **outside this codebase** (`ssh-keygen -lf`), so the assertion
    /// pins our derivation against OpenSSH rather than against our own output.
    const ED25519_BLOB_B64: &str =
        "AAAAC3NzaC1lZDI1NTE5AAAAINMHWkNaFcgtIbUiRuGXYCbYWjHPPGWLKxHqBLzWyPhL";
    const ED25519_FINGERPRINT: &str = "SHA256:JfISx02kSjWJevGy/MjUdXCv76HaRM3YkYNvepTyHD8";

    /// An OpenSSH certificate blob (`ssh-keygen -s ca -I ada -n ada id.pub`) and the fingerprint
    /// OpenSSH prints for it — the certified key's, which is what both `ssh-keygen -lf` on the
    /// certificate and `ssh-add -l` show for one.
    ///
    /// Computed **outside this codebase** like the plain key's, so the certificate case is pinned
    /// against OpenSSH rather than against our own output.
    const CERTIFICATE_BLOB_B64: &str = concat!(
        "AAAAIHNzaC1lZDI1NTE5LWNlcnQtdjAxQG9wZW5zc2guY29tAAAAINU2GI2VIfoTnl0KiKFK8RjruWFJqfFBg2Gp",
        "PHHwOb6mAAAAIPUutylFq1Qe5pz9bjh2uDE+t2HF51q/0PV1ipnVGwNUAAAAAAAAAAAAAAABAAAAA2FkYQAAAAcA",
        "AAADYWRhAAAAAGqdyYgAAAAAbH2r5AAAAAAAAACCAAAAFXBlcm1pdC1YMTEtZm9yd2FyZGluZwAAAAAAAAAXcGVy",
        "bWl0LWFnZW50LWZvcndhcmRpbmcAAAAAAAAAFnBlcm1pdC1wb3J0LWZvcndhcmRpbmcAAAAAAAAACnBlcm1pdC1w",
        "dHkAAAAAAAAADnBlcm1pdC11c2VyLXJjAAAAAAAAAAAAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIPj2m1jzf/wa5cTq",
        "ZVJWS1tPUVeZ3MywC+Jnw6h7wnHIAAAAUwAAAAtzc2gtZWQyNTUxOQAAAEB626le+4mAdEn1E5SqCosDZb8wAWqV",
        "qVQS8KHroxfunIsBdG/8Ltq+cplwpcGzn3vPmAm8i7PsIcTbrWtm+GEF",
    );
    const CERTIFIED_KEY_FINGERPRINT: &str = "SHA256:Y32ihbja+Y/2QWSUtnb8UU2TakV8JX0fevvvjO5xk0Q";

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

    /// A fake ssh-agent on a real Unix socket, answering exactly one request.
    ///
    /// A real socket rather than a mocked client: the point of choosing the wire protocol over
    /// `ssh-add` was that the protocol answers unambiguously, and a mocked client would exercise
    /// none of the encoding that claim rests on.
    struct FakeAgent {
        path: PathBuf,
        requests: mpsc::Receiver<Vec<u8>>,
        _dir: tempfile::TempDir,
    }

    impl FakeAgent {
        /// Serve one request with `response`, then close.
        fn serving(response: Vec<u8>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("agent.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let (asked, requests) = mpsc::channel();
            std::thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut request = [0u8; REQUEST_IDENTITIES_FRAME.len()];
                    if stream.read_exact(&mut request).is_ok() {
                        let _ = asked.send(request.to_vec());
                        let _ = stream.write_all(&response);
                        let _ = stream.flush();
                    }
                }
            });
            Self {
                path,
                requests,
                _dir: dir,
            }
        }

        /// The whole frame the probe sent, length prefix included.
        ///
        /// Asserted in full because the request is no longer written here: it is `ssh-agent-lib`
        /// that encodes it, and an assertion on the type byte alone would let its framing drift.
        fn assert_was_asked_for_identities(&self) -> &Self {
            let request = self
                .requests
                .recv_timeout(AGENT_TIMEOUT)
                .expect("the agent was never asked for its identities");
            assert_eq!(
                request, REQUEST_IDENTITIES_FRAME,
                "the probe sent something other than a request for the identity list"
            );
            self
        }
    }

    /// An agent that accepts the connection and then never answers — the hang case.
    ///
    /// The accepted connection is handed *out* of the accepting thread and held here, because a
    /// dropped `UnixStream` closes the socket: the probe would then see a peer that hung up, which
    /// is a different failure and one that returns immediately.
    struct SilentAgent {
        path: PathBuf,
        _accepted: mpsc::Receiver<UnixStream>,
        _dir: tempfile::TempDir,
    }

    impl SilentAgent {
        fn listening() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("agent.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let (accepted, held_open) = mpsc::channel();
            std::thread::spawn(move || {
                if let Ok((stream, _)) = listener.accept() {
                    let _ = accepted.send(stream);
                }
            });
            Self {
                path,
                _accepted: held_open,
                _dir: dir,
            }
        }
    }

    /// A resolver that points every user at one socket path.
    struct FixedSocket(Option<PathBuf>);

    impl AgentSocketResolver for FixedSocket {
        fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
            Ok(self.0.clone())
        }
    }

    /// A resolver whose lookup could not be made at all — a passwd database that did not answer.
    struct UnanswerableLookup;

    impl AgentSocketResolver for UnanswerableLookup {
        fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
            Err("the passwd database did not answer".to_string())
        }
    }

    fn probe_against(socket: Option<PathBuf>) -> AgentStatus {
        WireProtocolAgentProbe::new(FixedSocket(socket)).identities("someone")
    }

    fn blob_from(base64_blob: &str) -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(base64_blob)
            .expect("the fixture blob is valid base64")
    }

    fn a_blob() -> Vec<u8> {
        blob_from(ED25519_BLOB_B64)
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
        assert_eq!(status.keys[0].fingerprint, ED25519_FINGERPRINT);
        agent.assert_was_asked_for_identities();
    }

    /// The two axes `ssh-add -l` uses for a certificate: the certified key's fingerprint, and the
    /// certificate's own type beside it.
    #[test]
    fn reports_a_certificate_under_the_certified_keys_fingerprint_and_its_own_key_type() {
        let certificate = blob_from(CERTIFICATE_BLOB_B64);
        let agent = FakeAgent::serving(identities_answer(&[(certificate, "ada@workstation")]));

        let status = probe_against(Some(agent.path.clone()));

        assert_eq!(status.keys.len(), 1, "got {status:?}");
        assert_eq!(
            status.keys[0].fingerprint, CERTIFIED_KEY_FINGERPRINT,
            "an operator lines this row up against their own `ssh-add -l`, which prints the \
             certified key's fingerprint for a certificate — the certificate's own digest matches \
             nothing they can print"
        );
        assert_eq!(
            status.keys[0].key_type, "ssh-ed25519-cert-v01@openssh.com",
            "a certificate listed under the type of the key it certifies is indistinguishable \
             from that key, which is the one thing `ssh-add -l` tells them apart by"
        );
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

    /// A lookup that could not be made establishes nothing about this user's keys. Reporting it as
    /// "no agent" would send an operator to start one that may well be running.
    #[test]
    fn reports_a_probe_failure_when_the_user_cannot_be_looked_up() {
        let probe = WireProtocolAgentProbe::new(UnanswerableLookup);

        let status = probe.identities("ada");

        assert_eq!(
            status.outcome,
            ProbeOutcome::Failed(
                "could not look for ada's ssh-agent socket: the passwd database did not answer"
                    .to_string()
            )
        );
        assert!(!status.reachable);
    }

    /// A socket that accepts and never answers must time out into a reported failure rather than
    /// holding the RPC open — the Hosts screen probes every host it lists.
    #[test]
    fn reports_a_probe_failure_when_the_agent_does_not_answer_in_time() {
        let agent = SilentAgent::listening();

        let started = std::time::Instant::now();
        let status = probe_against(Some(agent.path.clone()));

        assert!(
            matches!(status.outcome, ProbeOutcome::Failed(_)),
            "a silent agent is a probe failure, got {:?}",
            status.outcome
        );
        assert!(
            started.elapsed() >= AGENT_TIMEOUT,
            "the probe gave up in {:?}, so it never waited on the agent at all — the timeout is \
             what this proves",
            started.elapsed()
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
