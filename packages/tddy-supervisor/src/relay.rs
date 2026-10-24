//! Relaying a spawned child's stdio over a unix socket the caller owns.
//!
//! The ACP driver talks to a coding agent over stdin/stdout, and on a supervised host the daemon
//! cannot start that agent as another OS user itself. [`SpawnRelayed`](crate::request::SpawnRelayedRequest)
//! starts it here instead and this module carries its bytes: the caller listens on a socket *path*
//! (a path crosses a process boundary, a file descriptor does not), the supervisor connects to it,
//! and the child's stdout goes out over that connection while whatever comes in goes to its stdin.
//!
//! The supervisor is root and connects to a path the caller named, so the connection is the
//! dangerous part, and two checks stand between a compromised caller and an arbitrary socket:
//!
//! 1. before connecting, the path must be a socket **owned by the caller's uid** (`lstat`, so a
//!    symlink is not followed to somebody else's socket);
//! 2. after connecting, the listener's `SO_PEERCRED` uid must be the caller's uid again, which
//!    closes the window between the check and the connect.
//!
//! Either failure is [`SupervisorError::Denied`], opaque like every other refusal.
//!
//! The child's exit status travels over `SessionStatus`, and a stop over `StopSession`, keyed by
//! the pid [`SpawnRelayed`](crate::request::SpawnRelayedRequest) returns. A caller that drops the
//! connection ends the child too: end of input from the caller is a stop request, so a daemon that
//! goes away does not leave an agent running as somebody else's user.

use std::os::fd::OwnedFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::pipe;
use tokio::net::UnixStream;

use crate::error::SupervisorError;
use crate::spawn_broker::fork_broker::ChildStdio;
use crate::supervisor::Supervisor;

/// Largest chunk of the child's stderr logged as one line.
const STDERR_CHUNK: usize = 16 * 1024;

/// Connect to the caller's relay socket, having checked that the socket is the caller's.
///
/// Both refusals are [`SupervisorError::Denied`]; what actually went wrong is logged.
pub async fn connect_to_callers_socket(
    path: &Path,
    caller_uid: u32,
) -> Result<UnixStream, SupervisorError> {
    crate::policy::plain_absolute_path(path)?;
    let metadata = std::fs::symlink_metadata(path).map_err(|e| {
        log::warn!(target: "tddy_supervisor::relay", "relay socket {} unusable: {e}", path.display());
        SupervisorError::Denied
    })?;
    if !metadata.file_type().is_socket() || metadata.uid() != caller_uid {
        log::warn!(
            target: "tddy_supervisor::relay",
            "relay socket {} is not a socket owned by uid {caller_uid}",
            path.display()
        );
        return Err(SupervisorError::Denied);
    }
    let stream = UnixStream::connect(path).await.map_err(|e| {
        log::warn!(target: "tddy_supervisor::relay", "cannot connect to {}: {e}", path.display());
        SupervisorError::Denied
    })?;
    match stream.peer_cred() {
        Ok(peer) if peer.uid() == caller_uid => Ok(stream),
        other => {
            log::warn!(
                target: "tddy_supervisor::relay",
                "the listener at {} is not uid {caller_uid}: {other:?}",
                path.display()
            );
            Err(SupervisorError::Denied)
        }
    }
}

/// Carry bytes between `stream` and the child's stdio until the child's output ends.
///
/// `stderr` is logged, never relayed: the protocol rides stdout. End of input from the caller stops
/// the session through `supervisor`, with `grace` before `SIGKILL` — the same grace `StopSession`
/// gives it, since how long a process gets to exit is a property of the host, not of who asked.
pub fn relay(
    supervisor: Arc<Supervisor>,
    pid: u32,
    stream: UnixStream,
    stdio: ChildStdio,
    grace: Duration,
) {
    let stopper = Arc::clone(&supervisor);
    if pump(pid, stream, stdio, move || stop(&stopper, pid, grace)).is_err() {
        log::error!(target: "tddy_supervisor::relay", "session {pid}: its pipes cannot be driven");
        stop(&supervisor, pid, grace);
    }
}

/// The byte-moving half of [`relay`], with what to do when the caller goes away passed in.
///
/// `Err` when a pipe cannot be driven asynchronously; nothing was started then.
pub fn pump(
    pid: u32,
    stream: UnixStream,
    stdio: ChildStdio,
    on_caller_gone: impl FnOnce() + Send + 'static,
) -> std::io::Result<()> {
    let ChildStdio {
        stdin,
        stdout,
        stderr,
    } = stdio;
    let (mut stdin, mut stdout, stderr) = (
        pipe_sender(stdin)?,
        pipe_receiver(stdout)?,
        pipe_receiver(stderr)?,
    );
    let (mut from_caller, mut to_caller) = stream.into_split();

    tokio::spawn(async move {
        // Ends at the caller's end of input, or at an error either side — which includes a child
        // that stopped reading its stdin. Either way there is nobody left to serve.
        let _ = tokio::io::copy(&mut from_caller, &mut stdin).await;
        drop(stdin);
        on_caller_gone();
    });
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut stdout, &mut to_caller).await;
        let _ = to_caller.shutdown().await;
    });
    tokio::spawn(log_stderr(pid, stderr));
    Ok(())
}

/// Log the child's stderr, chunk by chunk, until it ends.
async fn log_stderr(pid: u32, mut stderr: pipe::Receiver) {
    let mut buffer = vec![0u8; STDERR_CHUNK];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(n) => log::debug!(
                target: "tddy_supervisor::relay",
                "session {pid} stderr: {}",
                String::from_utf8_lossy(&buffer[..n]).trim_end()
            ),
        }
    }
}

fn stop(supervisor: &Arc<Supervisor>, pid: u32, grace: Duration) {
    if let Err(e) = supervisor.stop_session(pid, grace) {
        log::debug!(target: "tddy_supervisor::relay", "session {pid} stop: {e}");
    }
}

fn pipe_sender(stdin: std::process::ChildStdin) -> std::io::Result<pipe::Sender> {
    pipe::Sender::from_file(std::fs::File::from(OwnedFd::from(stdin)))
}

fn pipe_receiver<T: Into<OwnedFd>>(end: T) -> std::io::Result<pipe::Receiver> {
    pipe::Receiver::from_file(std::fs::File::from(end.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn this_uid() -> u32 {
        unsafe { libc::geteuid() }
    }

    fn a_socket_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[tokio::test]
    async fn connects_to_a_socket_the_caller_owns() {
        // Given a socket the caller is listening on
        let dir = a_socket_dir();
        let path = dir.path().join("relay.sock");
        let _listener = tokio::net::UnixListener::bind(&path).expect("bind");

        // When
        let stream = connect_to_callers_socket(&path, this_uid()).await;

        // Then
        assert!(stream.is_ok());
    }

    #[tokio::test]
    async fn refuses_a_socket_owned_by_somebody_else() {
        // Given a socket owned by this uid
        let dir = a_socket_dir();
        let path = dir.path().join("relay.sock");
        let _listener = tokio::net::UnixListener::bind(&path).expect("bind");

        // When a caller with another uid names it
        let refusal = connect_to_callers_socket(&path, this_uid() + 1).await;

        // Then
        assert_eq!(refusal.err(), Some(SupervisorError::Denied));
    }

    #[tokio::test]
    async fn refuses_a_path_that_is_not_a_socket() {
        // Given a plain file the caller owns
        let dir = a_socket_dir();
        let path = dir.path().join("plain-file");
        std::fs::write(&path, b"x").expect("write");

        // When
        let refusal = connect_to_callers_socket(&path, this_uid()).await;

        // Then
        assert_eq!(refusal.err(), Some(SupervisorError::Denied));
    }

    #[tokio::test]
    async fn refuses_a_symlink_to_a_socket_rather_than_following_it() {
        // Given a symlink to a socket the caller owns
        let dir = a_socket_dir();
        let path = dir.path().join("relay.sock");
        let _listener = tokio::net::UnixListener::bind(&path).expect("bind");
        let link = dir.path().join("link.sock");
        std::os::unix::fs::symlink(&path, &link).expect("symlink");

        // When the caller names the link
        let refusal = connect_to_callers_socket(&link, this_uid()).await;

        // Then
        assert_eq!(refusal.err(), Some(SupervisorError::Denied));
    }

    #[tokio::test]
    async fn refuses_a_relative_socket_path() {
        // Given / When
        let refusal = connect_to_callers_socket(Path::new("relay.sock"), this_uid()).await;

        // Then
        assert_eq!(refusal.err(), Some(SupervisorError::Denied));
    }

    #[tokio::test]
    async fn refuses_a_socket_path_with_parent_components() {
        // Given / When
        let refusal = connect_to_callers_socket(Path::new("/tmp/../relay.sock"), this_uid()).await;

        // Then
        assert_eq!(refusal.err(), Some(SupervisorError::Denied));
    }

    fn a_cat() -> (std::process::Child, ChildStdio) {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn cat");
        let stdio = ChildStdio {
            stdin: child.stdin.take().expect("stdin"),
            stdout: child.stdout.take().expect("stdout"),
            stderr: child.stderr.take().expect("stderr"),
        };
        (child, stdio)
    }

    #[tokio::test]
    async fn relays_bytes_both_ways_between_the_socket_and_the_child() {
        // Given a caller connected to a relay pumping a `cat`
        let dir = a_socket_dir();
        let path = dir.path().join("relay.sock");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind");
        let supervisor_side = connect_to_callers_socket(&path, this_uid())
            .await
            .expect("connect");
        let (mut caller, _) = listener.accept().await.expect("accept");
        let (mut child, stdio) = a_cat();
        pump(1, supervisor_side, stdio, || {}).expect("pump");

        // When the caller writes and reads back
        caller.write_all(b"hello agent\n").await.expect("write");
        let mut echoed = [0u8; 12];
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            caller.read_exact(&mut echoed),
        )
        .await
        .expect("the echo arrived")
        .expect("read");

        // Then the child's echo came back over the socket
        assert_eq!(&echoed, b"hello agent\n");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[tokio::test]
    async fn a_caller_that_hangs_up_is_reported_so_the_child_can_be_stopped() {
        // Given a caller connected to a relay that records when the caller goes away
        let dir = a_socket_dir();
        let path = dir.path().join("relay.sock");
        let listener = tokio::net::UnixListener::bind(&path).expect("bind");
        let supervisor_side = connect_to_callers_socket(&path, this_uid())
            .await
            .expect("connect");
        let (caller, _) = listener.accept().await.expect("accept");
        let (mut child, stdio) = a_cat();
        let gone = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&gone);
        pump(1, supervisor_side, stdio, move || {
            flag.store(true, Ordering::SeqCst)
        })
        .expect("pump");

        // When the caller hangs up
        drop(caller);

        // Then the relay reports it within the deadline
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !gone.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(gone.load(Ordering::SeqCst));
        let _ = child.kill();
        let _ = child.wait();
    }
}
