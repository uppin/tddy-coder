//! The transport: ask the daemon over HTTP for a room to join, join it, open
//! `RemoteGitService/Serve` against the daemon's participant, and pump local stdio through it
//! until the daemon reports the child's exit status.

use std::io::{Read, Write};
use std::time::Duration;

use prost::Message as _;
use tddy_livekit::client_connect::{connect_client, ConnectError};
use tddy_service::proto::remote_git::{GitClientFrame, GitOpen, GitServerFrame};

use crate::credentials::{Credentials, DaemonToken};
use crate::daemon_rpc::{DaemonRpc, DaemonRpcError};
use crate::ssh_argv::GitRequest;

/// ssh's transport-failure exit code. Git reports a remote as unreachable when its transport
/// command exits with this.
pub const TRANSPORT_FAILURE_EXIT_CODE: i32 = 255;

/// Largest stdin payload carried in one `GitClientFrame`. Kept well under
/// `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` (60 000) so a git frame is never chunk-framed —
/// a lost chunk wedges the call with no error, and a pushed pack is exactly the workload that would
/// produce oversized frames.
const MAX_STDIN_FRAME_BYTES: usize = 32 * 1024;

/// How long the daemon may say nothing at all before its side is declared gone.
///
/// Nothing below the transport notices a daemon that leaves the room mid-clone: the stream simply
/// stops producing frames, and git waits on a pipe that will never close. The budget is generous
/// because silence is legitimate while `pack-objects` counts a large repository — but it is finite,
/// which a hung `git clone` is not.
const SERVER_SILENCE_DEADLINE: Duration = Duration::from_secs(600);

/// Run one git operation to completion and return the **remote child's** exit code, which the
/// caller uses as its own so git sees the true remote status.
pub async fn run(request: GitRequest, credentials: Credentials) -> Result<i32, RelayError> {
    let daemon = DaemonRpc::new(&credentials.daemon_url, credentials.connect_timeout)?;

    // One access token serves both legs: it authenticates the mint, and it is what `GitOpen`
    // carries. Its 5-minute life is ample for a request that is about to be made twice in a row.
    let session_token = match &credentials.token {
        DaemonToken::Access(token) => token.clone(),
        DaemonToken::Refresh(token) => daemon.refresh_session(token).await?,
    };
    let admission = daemon.mint_room_admission(&session_token).await?;

    // The daemon's RPC-serving participant. `daemon_instance_id` is the whole address.
    let daemon_identity = format!("daemon-{}", request.daemon_instance_id);
    let connected = connect_client(
        &admission.url,
        &admission.token,
        &daemon_identity,
        credentials.connect_timeout,
    )
    .await
    .map_err(|e| match e {
        ConnectError::Room(reason) => RelayError::Room {
            url: admission.url.clone(),
            room: admission.room.clone(),
            reason,
        },
        ConnectError::ParticipantAbsent { identity, .. } => {
            RelayError::DaemonUnreachable { identity }
        }
    })?;
    // Holds the room open for the whole operation; dropping it would leave the room mid-pack.
    let _room = connected.room;
    let client = connected.client;

    let (mut outbound, mut inbound) = client
        .start_bidi_stream("remote_git.RemoteGitService", "Serve")
        .map_err(|e| RelayError::Stream(format!("open the git stream: {e}")))?;

    send_frame(
        &mut outbound,
        GitClientFrame {
            open: Some(GitOpen {
                session_token,
                project_ref: request.project_ref.clone(),
                verb: request.verb.wire_name().to_string(),
            }),
            ..Default::default()
        },
    )
    .await?;

    let mut stdin_chunks = spawn_stdin_reader();
    let mut stdin_open = true;
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();

    loop {
        tokio::select! {
            chunk = stdin_chunks.recv(), if stdin_open => match chunk {
                Some(bytes) => {
                    send_frame(&mut outbound, GitClientFrame { stdin: bytes, ..Default::default() }).await?;
                }
                None => {
                    // The stream itself stays open — the daemon is still streaming the pack down it.
                    // Only the child's stdin is closed, which is what git waits for to finish.
                    stdin_open = false;
                    send_frame(&mut outbound, GitClientFrame { stdin_eof: true, ..Default::default() }).await?;
                }
            },
            // `Receiver::recv` is cancel-safe, so re-arming the deadline on every loop iteration
            // loses no frame; the effect is a deadline on *silence*, reset by any traffic in
            // either direction.
            frame = tokio::time::timeout(SERVER_SILENCE_DEADLINE, inbound.recv()) => match frame {
                Ok(Some(Ok(bytes))) => {
                    let frame = GitServerFrame::decode(&bytes[..])
                        .map_err(|e| RelayError::Decode(e.to_string()))?;
                    if !frame.stdout.is_empty() {
                        write_out(&mut stdout, &frame.stdout, "stdout")?;
                    }
                    if !frame.stderr.is_empty() {
                        write_out(&mut stderr, &frame.stderr, "stderr")?;
                    }
                    if frame.done {
                        return Ok(frame.exit_code);
                    }
                }
                Ok(Some(Err(status))) => {
                    return Err(RelayError::Rejected {
                        code: status.code.as_str().to_string(),
                        message: status.message,
                    })
                }
                Ok(None) => return Err(RelayError::StreamEndedWithoutExitStatus),
                Err(_) => return Err(RelayError::ServerWentSilent {
                    identity: daemon_identity.clone(),
                    after: SERVER_SILENCE_DEADLINE,
                }),
            },
        }
    }
}

async fn send_frame(
    outbound: &mut tddy_livekit::BidiStreamSender<'_>,
    frame: GitClientFrame,
) -> Result<(), RelayError> {
    outbound
        .send(frame.encode_to_vec(), false)
        .await
        .map_err(|e| RelayError::Send(e.to_string()))
}

/// Relay one of the daemon's output streams onto ours. A write failure here is the local end
/// breaking (git closed the pipe, or the disk filled), and it is reported rather than dropped:
/// silently discarding pack bytes is how a clone ends up quietly incomplete.
fn write_out(sink: &mut impl Write, bytes: &[u8], which: &'static str) -> Result<(), RelayError> {
    sink.write_all(bytes)
        .and_then(|()| sink.flush())
        .map_err(|e| RelayError::LocalIo {
            stream: which,
            reason: e.to_string(),
        })
}

/// Read git's stdin on a thread — it is a blocking pipe with no async handle — and hand it over in
/// chunks small enough that a frame is never chunk-framed. The channel closing signals EOF.
fn spawn_stdin_reader() -> tokio::sync::mpsc::Receiver<Vec<u8>> {
    let (chunks, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(4);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = vec![0u8; MAX_STDIN_FRAME_BYTES];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => {
                    if chunks.blocking_send(buf[..n].to_vec()).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    log::warn!(target: "tddy_remote_git_repo::relay", "read local stdin: {e}");
                    return;
                }
            }
        }
    });
    rx
}

/// A failure on the transport or authentication leg — everything that is not the remote git
/// command itself failing.
///
/// The variants are split by *what a user would have to fix*: an expired token, an offline daemon,
/// a full local disk and a corrupt frame have nothing in common but the exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
    /// A call to the daemon's HTTP surface failed — the token exchange or the room-token mint.
    Daemon(DaemonRpcError),
    /// The LiveKit room itself could not be joined with the token the daemon minted.
    Room {
        url: String,
        room: String,
        reason: String,
    },
    /// The daemon's participant never appeared within the connect timeout.
    DaemonUnreachable { identity: String },
    /// The daemon went quiet mid-operation and never sent its final frame.
    ServerWentSilent { identity: String, after: Duration },
    /// The git stream could not be opened.
    Stream(String),
    /// A frame could not be put on the wire.
    Send(String),
    /// A frame arrived that is not a `GitServerFrame`.
    Decode(String),
    /// The remote's output could not be written locally — a closed pipe, a full disk.
    LocalIo {
        stream: &'static str,
        reason: String,
    },
    /// The daemon refused the open frame (auth, unknown project, rejected verb).
    Rejected { code: String, message: String },
    /// The stream ended without a final `done` frame, so no exit status was ever reported.
    StreamEndedWithoutExitStatus,
}

impl From<DaemonRpcError> for RelayError {
    fn from(e: DaemonRpcError) -> Self {
        RelayError::Daemon(e)
    }
}

impl RelayError {
    pub fn exit_code(&self) -> i32 {
        TRANSPORT_FAILURE_EXIT_CODE
    }
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::Daemon(e) => write!(f, "{e}"),
            RelayError::Room { url, room, reason } => {
                write!(f, "could not join room \"{room}\" at {url}: {reason}")
            }
            RelayError::DaemonUnreachable { identity } => write!(
                f,
                "daemon \"{identity}\" is not in the room; it may be offline or named by a \
                 different instance id"
            ),
            RelayError::ServerWentSilent { identity, after } => write!(
                f,
                "daemon \"{identity}\" sent nothing for {}s and never reported the git command's \
                 exit status",
                after.as_secs()
            ),
            RelayError::Stream(reason) => write!(f, "{reason}"),
            RelayError::Send(reason) => write!(f, "could not send to the daemon: {reason}"),
            RelayError::Decode(reason) => {
                write!(
                    f,
                    "the daemon sent a frame that could not be read: {reason}"
                )
            }
            RelayError::LocalIo { stream, reason } => {
                write!(f, "could not write the remote's {stream} locally: {reason}")
            }
            // The daemon's own message is the whole diagnostic a user gets — it names the project
            // that could not be resolved, the verb that was refused, or the token that was
            // rejected — so it is surfaced verbatim rather than summarised.
            RelayError::Rejected { code, message } => {
                write!(f, "the daemon refused the request ({code}): {message}")
            }
            RelayError::StreamEndedWithoutExitStatus => write!(
                f,
                "the daemon closed the stream without reporting the git command's exit status"
            ),
        }
    }
}
