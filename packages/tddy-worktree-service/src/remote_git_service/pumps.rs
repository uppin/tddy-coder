use crate::remote_git_service::FrameSender;
use crate::remote_git_service::CHILD_TERMINATION_GRACE;
use crate::remote_git_service::LOG_TARGET;
use crate::remote_git_service::MAX_GIT_FRAME_BYTES;
use crate::remote_git_service::PUMP_STALL_TIMEOUT;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tddy_rpc::Status;
use tddy_service::proto::remote_git::GitServerFrame;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;

/// Which `GitServerFrame` field a pump writes into. The two streams never share a field: git
/// writes progress on stderr while the pack goes down stdout, and interleaving them corrupts it.
#[derive(Clone, Copy)]
pub(super) enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    fn frame(self, payload: Vec<u8>) -> GitServerFrame {
        match self {
            OutputStream::Stdout => GitServerFrame {
                stdout: payload,
                ..Default::default()
            },
            OutputStream::Stderr => GitServerFrame {
                stderr: payload,
                ..Default::default()
            },
        }
    }
}

/// What the output pumps are doing, shared with teardown.
///
/// A pump that has stopped can have stopped for two very different reasons: the pipe has no more
/// bytes to give yet (a worker git forked still holds it open), or the wire is slower than the
/// child and the pump is parked writing a frame. Only the first is a wedge worth killing the
/// process group over — mistaking the second for it would truncate a pack on a slow link.
#[derive(Default)]
pub(super) struct PumpProgress {
    /// How many pumps are parked writing a frame right now.
    writing: AtomicUsize,
    /// Frames handed over so far, so a slow-but-moving wire is never mistaken for a stall.
    frames_sent: AtomicU64,
}

impl PumpProgress {
    fn snapshot(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }

    fn is_moving(&self, since: u64) -> bool {
        self.snapshot() != since || self.writing.load(Ordering::Relaxed) > 0
    }
}

/// The output pumps of one child, with the progress they report.
pub(super) struct Pumps {
    pub(super) pumps: Vec<tokio::task::JoinHandle<()>>,
    pub(super) progress: Arc<PumpProgress>,
}

/// Relay one of the child's output pipes to the frame stream until it reaches EOF. Reads are
/// bounded by [`MAX_GIT_FRAME_BYTES`], so a frame is never large enough to be chunk-framed, and
/// each frame is *awaited* onto the bounded channel, so a wire slower than the child stops the pump
/// rather than accumulating the pack in the daemon's heap.
pub(super) async fn pump_output<R>(
    mut reader: R,
    frames: FrameSender,
    stream: OutputStream,
    progress: Arc<PumpProgress>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; MAX_GIT_FRAME_BYTES];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => {
                let frame = stream.frame(buf[..n].to_vec());
                if send_frame(&frames, Ok(frame), &progress).await.is_err() {
                    return;
                }
            }
            Err(e) => {
                let failure = Err(Status::internal(format!("read child output: {e}")));
                let _ = send_frame(&frames, failure, &progress).await;
                return;
            }
        }
    }
}

/// Hand one frame to the bounded channel, recording that the pump is parked on the wire while it
/// waits. `Err` means the consumer is gone.
async fn send_frame(
    frames: &FrameSender,
    frame: Result<GitServerFrame, Status>,
    progress: &PumpProgress,
) -> Result<(), ()> {
    progress.writing.fetch_add(1, Ordering::Relaxed);
    let sent = frames.send(frame).await;
    progress.writing.fetch_sub(1, Ordering::Relaxed);
    progress.frames_sent.fetch_add(1, Ordering::Relaxed);
    sent.map_err(|_| ())
}

/// Wait for the child, or terminate it when the connection ends, then close the stream with the
/// exit status.
///
/// The done frame is emitted only after both pumps have reached EOF, so output the child wrote
/// just before exiting cannot be lost to the teardown.
pub(super) async fn supervise_child(
    mut child: tokio::process::Child,
    pid: u32,
    connection_ended: oneshot::Receiver<()>,
    pumps: Pumps,
    frames: FrameSender,
) {
    let status = tokio::select! {
        status = child.wait() => status,
        _ = connection_ended => terminate_child(&mut child, pid).await,
    };
    drain_pumps(pumps, pid).await;
    match status {
        Ok(status) => {
            let exit_code = exit_code_of(&status);
            log::info!(target: LOG_TARGET, "git child {pid} exited with code {exit_code}");
            let _ = frames
                .send(Ok(GitServerFrame {
                    exit_code,
                    done: true,
                    ..Default::default()
                }))
                .await;
        }
        Err(e) => {
            log::warn!(target: LOG_TARGET, "git child {pid} could not be reaped: {e}");
            let _ = frames
                .send(Err(Status::internal(format!("wait for git child: {e}"))))
                .await;
        }
    }
}

/// Wait for a streamed remote command, or stop waiting when the connection ends, then close the
/// stream with its exit status.
///
/// On connection end the exit future is dropped, which stops the remote command (the ssh transport
/// kills its client), and the stream closes with a `done` frame carrying `-1`: no exit status was
/// ever reported for it.
pub(super) async fn supervise_streamed(
    exit: tddy_remote_shell::StreamedExit,
    connection_ended: oneshot::Receiver<()>,
    pumps: Pumps,
    frames: FrameSender,
) {
    let outcome = tokio::select! {
        status = exit.wait() => Some(status),
        _ = connection_ended => None,
    };
    drain_pumps(pumps, 0).await;
    match outcome {
        Some(Ok(exit_code)) => {
            log::info!(target: LOG_TARGET, "remote git command exited with code {exit_code}");
            let _ = frames
                .send(Ok(GitServerFrame {
                    exit_code,
                    done: true,
                    ..Default::default()
                }))
                .await;
        }
        Some(Err(error)) => {
            log::warn!(target: LOG_TARGET, "remote git command could not be reaped: {error}");
            let _ = frames
                .send(Err(Status::internal(format!(
                    "wait for remote git command: {error}"
                ))))
                .await;
        }
        None => {
            let _ = frames
                .send(Ok(GitServerFrame {
                    exit_code: -1,
                    done: true,
                    ..Default::default()
                }))
                .await;
        }
    }
}

/// Wait for both output pumps to finish, so nothing the child wrote just before exiting is lost to
/// the teardown — but never wait forever.
///
/// A process the child forked can inherit its stdout and outlive it (`git-upload-pack` forks
/// `pack-objects`), and then the pipe never reaches EOF. Waiting on it indefinitely would leave the
/// client on a `done` frame that never arrives with no error to explain it, which is precisely the
/// silent wedge this service is built to avoid. Once the pumps have gone [`PUMP_STALL_TIMEOUT`]
/// without delivering a frame or being parked writing one, the process group is signalled so the
/// pipe closes and the stream can be terminated.
async fn drain_pumps(pumps: Pumps, pid: u32) {
    let Pumps { pumps, progress } = pumps;
    let aborts: Vec<_> = pumps.iter().map(|pump| pump.abort_handle()).collect();
    let mut joined = futures_util::future::join_all(pumps);
    loop {
        let before = progress.snapshot();
        if tokio::time::timeout(PUMP_STALL_TIMEOUT, &mut joined)
            .await
            .is_ok()
        {
            return;
        }
        if !progress.is_moving(before) {
            break;
        }
    }
    log::warn!(
        target: LOG_TARGET,
        "git child {pid} is gone but its output pipe is still held — a process it forked inherited \
         it; killing the process group so the stream can be closed"
    );
    // A streamed remote command has no local process group to signal (pid 0).
    if pid != 0 {
        signal_process_group(pid, libc::SIGKILL);
    }
    if tokio::time::timeout(CHILD_TERMINATION_GRACE, &mut joined)
        .await
        .is_err()
    {
        // The done frame has to be the last thing on this stream, so no pump may outlive this
        // point and write after it.
        for abort in aborts {
            abort.abort();
        }
    }
}

/// Signal the child's whole process group — SIGTERM, then SIGKILL once the grace period is up —
/// and reap it. A grandchild left behind would hold the repository's object database open.
async fn terminate_child(
    child: &mut tokio::process::Child,
    pid: u32,
) -> std::io::Result<ExitStatus> {
    signal_process_group(pid, libc::SIGTERM);
    if let Ok(status) = tokio::time::timeout(CHILD_TERMINATION_GRACE, child.wait()).await {
        return status;
    }
    log::warn!(
        target: "tddy_daemon::remote_git_service",
        "git child {pid} ignored SIGTERM for {}s; killing its process group",
        CHILD_TERMINATION_GRACE.as_secs()
    );
    signal_process_group(pid, libc::SIGKILL);
    child.wait().await
}

fn signal_process_group(pid: u32, signal: libc::c_int) {
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

/// The status git should see. A child killed by a signal has no exit code of its own, so it is
/// reported the way a shell reports one.
fn exit_code_of(status: &ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}
