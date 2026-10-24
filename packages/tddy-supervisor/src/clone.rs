//! `CloneRepository`: one `git clone --progress` as an allowlisted OS user, streamed.
//!
//! The clone is an ordinary supervised session (same user and environment policy, same process
//! group, stoppable with `StopSession(pid)`), with two differences. Its stderr is a pipe the
//! supervisor reads instead of `/dev/null`, because git's progress and its explanation of a failure
//! are written there; and it clones into `<destination>.partial-<random>`, renamed into place only
//! when git succeeds and removed otherwise, so a destination either does not exist or holds a
//! complete clone.
//!
//! Event order on the stream is fixed: `Started{pid}`, then a `Progress{line}` per line git wrote
//! (split on `\r` as well as `\n`, because git redraws a stage in place), then `Exited`, sent only
//! after the filesystem is in its final state. A clone the caller stops listening to is stopped.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::Stream;
use tddy_rpc::Status;
use tokio::sync::mpsc;

use crate::error::SupervisorError;
use crate::proto::supervisor::clone_event::Event;
use crate::proto::supervisor::{CloneEvent, CloneExited, CloneProgressLine, CloneStarted};
use crate::supervisor::Supervisor;

/// How many of git's last non-progress stderr lines `Exited` carries.
const STDERR_TAIL_LINES: usize = 8;

/// How long a clone's stderr is read for after git has exited. Git's descendants (ssh, a credential
/// helper) can hold the pipe open past it; their output is not worth holding the stream for.
const STDERR_DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);

/// Events buffered between the clone and a slow reader before git's own output is held back.
const EVENT_BUFFER: usize = 64;

/// A clone's events as the generated server's stream type.
pub(crate) type CloneEvents =
    std::pin::Pin<Box<dyn Stream<Item = Result<CloneEvent, Status>> + Send>>;

/// A clone that has been forked: everything the task that reports on it owns.
pub(crate) struct RunningClone {
    pub supervisor: Arc<Supervisor>,
    pub pid: u32,
    pub stderr: std::process::ChildStderr,
    /// Where git is cloning to.
    pub partial: PathBuf,
    /// Where a successful clone is moved to.
    pub destination: PathBuf,
    /// Grace `StopSession` gives git before `SIGKILL`.
    pub stop_grace: Duration,
}

/// The first executable named `git` on the supervisor's own `PATH`.
///
/// The supervisor's `PATH` is the operator's (the unit's environment), never the caller's, and the
/// result still has to pass the tool allowlist: `CloneRepository` runs git and nothing else, but it
/// does not widen what may run as the session user.
pub(crate) fn git_on_supervisor_path() -> Result<PathBuf, SupervisorError> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .filter(|entry| entry.is_absolute())
        .map(|entry| entry.join("git"))
        .find(|candidate| is_executable_file(candidate))
        .ok_or_else(|| SupervisorError::OperationFailed {
            message: "no `git` on the supervisor's PATH".to_string(),
        })
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// `<destination>.partial-<random>`, beside the destination so the final rename stays on one
/// filesystem.
pub(crate) fn partial_sibling(destination: &Path) -> Result<PathBuf, SupervisorError> {
    let name = destination.file_name().ok_or(SupervisorError::Denied)?;
    let mut partial = name.to_os_string();
    partial.push(format!(
        ".partial-{:016x}{:016x}",
        random_u64(),
        random_u64()
    ));
    Ok(destination.with_file_name(partial))
}

/// A random value from the standard library's per-instance hasher keys — enough to make a
/// directory name unguessable without a dependency for it.
fn random_u64() -> u64 {
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// A stream that is already over after `Exited`: the clone that was not needed.
pub(crate) fn already_present() -> CloneEvents {
    events_of(vec![Ok(exited(Some(0), String::new()))])
}

fn events_of(events: Vec<Result<CloneEvent, Status>>) -> CloneEvents {
    Box::pin(futures_util::stream::iter(events))
}

fn event(event: Event) -> CloneEvent {
    CloneEvent { event: Some(event) }
}

fn exited(code: Option<i32>, stderr_tail: String) -> CloneEvent {
    event(Event::Exited(CloneExited {
        code,
        // The session table records an exit code, not which signal ended a process: `code: None`
        // is "ended by a signal".
        signal: None,
        stderr_tail,
    }))
}

/// Report on a running clone until it has ended: the returned stream is the whole of its events.
pub(crate) fn report_on(clone: RunningClone) -> CloneEvents {
    let (events, receiver) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(drive(clone, events));
    Box::pin(futures_util::stream::unfold(
        receiver,
        |mut receiver| async { receiver.recv().await.map(|item| (item, receiver)) },
    ))
}

async fn drive(clone: RunningClone, events: mpsc::Sender<Result<CloneEvent, Status>>) {
    let RunningClone {
        supervisor,
        pid,
        stderr,
        partial,
        destination,
        stop_grace,
    } = clone;

    // Set when the caller stopped listening: nobody is left to report to, so git is stopped and
    // the partial directory removed.
    let mut abandoned = events
        .send(Ok(event(Event::Started(CloneStarted { pid }))))
        .await
        .is_err();
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let (lines, mut line_receiver) = mpsc::channel::<String>(EVENT_BUFFER);
    read_lines_in_the_background(stderr, lines, Arc::clone(&tail));
    let stop = |supervisor: &Arc<Supervisor>| {
        if let Err(error) = supervisor.stop_session(pid, stop_grace) {
            log::warn!(target: "tddy_supervisor::clone", "could not stop clone {pid}: {error}");
        }
    };
    if abandoned {
        stop(&supervisor);
    }

    let mut exit = Box::pin(supervisor.wait_for_session_exit(pid));
    let mut status = None;
    // Until git has exited: every line, reported as it arrives.
    while status.is_none() && !abandoned {
        tokio::select! {
            line = line_receiver.recv() => match line {
                Some(line) => {
                    let progress = event(Event::Progress(CloneProgressLine { line }));
                    if events.send(Ok(progress)).await.is_err() {
                        abandoned = true;
                        stop(&supervisor);
                    }
                }
                // Stderr closed: git is going, or gone. Only its exit is left to wait for.
                None => status = Some((&mut exit).await),
            },
            exited = &mut exit => status = Some(exited),
        }
    }
    let status = match status {
        Some(status) => status,
        None => exit.await,
    };
    // After git has exited: what it wrote last, but not for longer than the drain allows.
    if !abandoned {
        let _ = tokio::time::timeout(STDERR_DRAIN_AFTER_EXIT, async {
            while let Some(line) = line_receiver.recv().await {
                let progress = event(Event::Progress(CloneProgressLine { line }));
                if events.send(Ok(progress)).await.is_err() {
                    break;
                }
            }
        })
        .await;
    }

    let last = match status {
        Ok(status) => {
            let moved = if status.exit_code == Some(0) && !abandoned {
                move_into_place(&partial, &destination).await
            } else {
                remove_partial(&partial).await;
                Ok(())
            };
            moved.map(|()| exited(status.exit_code, joined(&tail)))
        }
        Err(error) => {
            remove_partial(&partial).await;
            Err(Status::internal(error.to_string()))
        }
    };
    let _ = events.send(last).await;
}

fn joined(tail: &Mutex<VecDeque<String>>) -> String {
    tail.lock()
        .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

/// A clone git finished is moved to where it was asked for. A destination that appeared while git
/// ran is not replaced: the finished clone is discarded and the caller told.
async fn move_into_place(partial: &Path, destination: &Path) -> Result<(), Status> {
    let (partial, destination) = (partial.to_path_buf(), destination.to_path_buf());
    let moved = tokio::task::spawn_blocking(move || {
        if destination.exists() {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(format!(
                "{} appeared while it was being cloned",
                destination.display()
            ));
        }
        std::fs::rename(&partial, &destination).map_err(|error| {
            let _ = std::fs::remove_dir_all(&partial);
            format!(
                "moving {} to {}: {error}",
                partial.display(),
                destination.display()
            )
        })
    })
    .await;
    match moved {
        Ok(result) => result.map_err(|message| Status::internal(format!("git clone: {message}"))),
        Err(error) => Err(Status::internal(format!("git clone: {error}"))),
    }
}

async fn remove_partial(partial: &Path) {
    let partial = partial.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || match std::fs::remove_dir_all(&partial) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => log::warn!(
            target: "tddy_supervisor::clone",
            "could not remove {}: {error}",
            partial.display()
        ),
        _ => {}
    })
    .await;
}

/// Read `stderr` on a thread of its own, since a read of a pipe cannot be abandoned: each line goes
/// to `lines`, and each one that is not a progress redraw is kept in `tail`.
fn read_lines_in_the_background(
    stderr: std::process::ChildStderr,
    lines: mpsc::Sender<String>,
    tail: Arc<Mutex<VecDeque<String>>>,
) {
    let spawned = std::thread::Builder::new()
        .name("tddy-supervisor-clone-stderr".to_string())
        .spawn(move || {
            for_each_line(stderr, |line| {
                if !is_progress_redraw(line) {
                    if let Ok(mut tail) = tail.lock() {
                        tail.push_back(line.to_string());
                        if tail.len() > STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                    }
                }
                // A closed receiver means the report ended: nothing left to read for.
                lines.blocking_send(line.to_string()).is_ok()
            });
        });
    if let Err(error) = spawned {
        log::error!(target: "tddy_supervisor::clone", "could not read a clone's stderr: {error}");
    }
}

/// Git's progress lines carry a percentage (`Receiving objects:  42% (21/50)`); an error never
/// does, and a long failing clone must not have its explanation pushed out of the tail by them.
fn is_progress_redraw(line: &str) -> bool {
    line.contains('%')
}

/// Call `on_line` with each non-empty, trimmed line of `reader`, where git ends a line with `\r`
/// while a stage redraws and `\n` when it ends. Stops at end-of-file, a read error, or when
/// `on_line` returns `false`.
fn for_each_line(mut reader: impl Read, mut on_line: impl FnMut(&str) -> bool) {
    let mut chunk = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    let mut emit = |line: &mut Vec<u8>| -> bool {
        let keep_going = {
            let text = String::from_utf8_lossy(line);
            let text = text.trim();
            text.is_empty() || on_line(text)
        };
        line.clear();
        keep_going
    };
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &chunk[..n] {
                    if byte == b'\r' || byte == b'\n' {
                        if !emit(&mut line) {
                            return;
                        }
                    } else {
                        line.push(byte);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    emit(&mut line);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for_each_line(bytes, |line| {
            lines.push(line.to_string());
            true
        });
        lines
    }

    #[test]
    fn splits_a_redrawn_stage_on_carriage_returns() {
        let lines =
            lines_of(b"Receiving objects:  10% (1/10)\rReceiving objects: 100% (10/10), done.\n");

        assert_eq!(
            lines,
            [
                "Receiving objects:  10% (1/10)",
                "Receiving objects: 100% (10/10), done."
            ]
        );
    }

    #[test]
    fn drops_blank_lines_and_keeps_a_final_line_without_a_terminator() {
        let lines = lines_of(b"\n\r\nfatal: repository 'x' not found");

        assert_eq!(lines, ["fatal: repository 'x' not found"]);
    }

    #[test]
    fn stops_reading_when_the_consumer_is_gone() {
        let mut seen = 0;

        for_each_line(&b"a\nb\nc\n"[..], |_| {
            seen += 1;
            false
        });

        assert_eq!(seen, 1);
    }

    #[test]
    fn names_a_partial_directory_beside_the_destination() {
        let partial = partial_sibling(Path::new("/srv/repos/alice/project")).unwrap();

        assert_eq!(partial.parent(), Some(Path::new("/srv/repos/alice")));
        assert!(partial
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("project.partial-"));
    }

    #[test]
    fn names_a_different_partial_directory_each_time() {
        let destination = Path::new("/srv/repos/alice/project");

        assert_ne!(
            partial_sibling(destination).unwrap(),
            partial_sibling(destination).unwrap()
        );
    }

    #[test]
    fn treats_a_percentage_line_as_progress_and_an_error_as_not() {
        assert!(is_progress_redraw("Receiving objects:  42% (21/50)"));
        assert!(!is_progress_redraw("fatal: repository 'x' does not exist"));
    }
}
