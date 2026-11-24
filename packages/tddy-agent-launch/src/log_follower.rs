//! Following a growing file through the remote-shell port: the one byte-offset reader behind both a
//! target session's hook log ([`crate::remote_hook_follower`]) and an agent's own transcript
//! ([`crate::transcript_follower`]), on a target or — through `LocalShell` — on this host.
//!
//! The follower runs a `tail -c +<offset + 1>` of the file through [`RemoteShell::exec_streamed`]
//! and hands every whole line to a [`LineSink`], in file order. Its offset counts the bytes of the
//! lines it has **delivered**, never the bytes it has read: a torn last line is read again whole
//! when a stream is reopened. That offset is kept in a [`FollowOffset`], which a follower that is to
//! survive its own replacement (a resume, a re-wire, a restarted daemon) persists, so the next one
//! reads on from the last delivered byte rather than from the file's start or its end.
//!
//! A pty (`coder ssh` always runs its command in one) would turn every `\n` the file holds into
//! `\r\n` on the stream (`onlcr`), and a follower counting stream bytes would then resume past what
//! it delivered. The command turns the pty's output processing off first (`stty -opost`), as the
//! Coder transport's one-shot commands do, so the stream is the file's bytes — and, should a pty
//! keep its processing anyway, a `\r\n` on the stream is counted as the file's one `\n`. Every file
//! followed is JSON lines, whose own lines never end in a raw `\r`. `tail`'s own
//! complaints are discarded too, because a transport that merges stderr into stdout would otherwise
//! put them on the stream as if the file held them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_remote_shell::RemoteShell;
use tokio::io::AsyncReadExt as _;

/// Quote `arg` as one single-quoted POSIX shell word (`'\''` for an embedded quote).
pub(crate) fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// How long a follower waits before reopening a stream that ended or could not be opened.
const REOPEN_BACKOFF: Duration = Duration::from_millis(200);

/// The command that streams `path` from the byte `offset` on, and everything appended after it,
/// until `alive_while` is removed:
///
/// ```text
/// stty -opost 2>/dev/null; tail -c +<offset + 1> -f '<path>' 2>/dev/null & t=$!; (while [ -e '<alive_while>' ]; do sleep 1; done; kill "$t") >/dev/null 2>&1 & wait "$t"
/// ```
///
/// `tail -c +N` starts at the Nth byte, counted from one. `-F` instead of `-f` (`waits_for_file`)
/// keeps waiting for a file that does not exist yet — an agent's transcript appears with its first
/// prompt. Ending the follower's client does not end a remote command — ssh without a terminal
/// sends it no hang-up, and `tail -f` on a file nobody writes to never learns its reader is gone —
/// so a watch of its own ends the `tail` within a second of `alive_while`'s removal. The watch runs
/// in a background subshell, not in the shell that waits on `tail`, so it outlives that shell being
/// killed with its client. Paths are single-quoted for a POSIX shell.
#[must_use]
pub fn tail_command(path: &str, offset: u64, alive_while: &str, waits_for_file: bool) -> String {
    let file = shell_quote(path);
    let alive = shell_quote(alive_while);
    let follow = if waits_for_file { "-F" } else { "-f" };
    format!(
        "stty -opost 2>/dev/null; tail -c +{} {follow} {file} 2>/dev/null & t=$!; (while [ -e {alive} ]; do sleep 1; done; kill \"$t\") >/dev/null 2>&1 & wait \"$t\"",
        offset + 1
    )
}

/// How far a follower has delivered a file: kept in memory, or persisted to a file of this host so
/// a follower that replaces it reads on from the same byte.
///
/// Clones share the offset: the one in memory, and the file.
#[derive(Debug, Clone, Default)]
pub struct FollowOffset {
    in_memory: Arc<AtomicU64>,
    persisted: Option<PathBuf>,
}

impl FollowOffset {
    /// An offset that starts at the file's first byte and lives as long as its follower.
    #[must_use]
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// An offset persisted at `path` on this host: read when a follower starts, written after each
    /// delivered line. A missing or unreadable file is the file's first byte.
    #[must_use]
    pub fn persisted_at(path: PathBuf) -> Self {
        Self {
            in_memory: Arc::default(),
            persisted: Some(path),
        }
    }

    /// The byte the next follower starts at: the last delivered, persisted or not.
    pub fn load(&self) -> u64 {
        let Some(path) = &self.persisted else {
            return self.in_memory.load(Ordering::SeqCst);
        };
        match std::fs::read_to_string(path) {
            Ok(text) => match text.trim().parse() {
                Ok(offset) => offset,
                Err(_) => {
                    log::warn!(
                        "follow offset {}: {text:?} is not a byte offset; reading from the start",
                        path.display()
                    );
                    0
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => {
                log::warn!(
                    "follow offset {}: {error}; reading from the start",
                    path.display()
                );
                0
            }
        }
    }

    /// Record that every byte before `offset` has been delivered. A persisted offset that cannot be
    /// written is logged: the follower keeps delivering, and only its replacement is affected.
    pub fn save(&self, offset: u64) {
        self.in_memory.store(offset, Ordering::SeqCst);
        let Some(path) = &self.persisted else {
            return;
        };
        if let Err(error) = write_atomically(path, offset.to_string().as_bytes()) {
            log::warn!("follow offset {}: {error}", path.display());
        }
    }

    /// Forget the offset: the file it counted is gone, and a file created in its place is read from
    /// its first byte.
    pub fn forget(&self) {
        self.in_memory.store(0, Ordering::SeqCst);
        if let Some(path) = &self.persisted {
            if let Err(error) = std::fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("follow offset {}: {error}", path.display());
                }
            }
        }
    }
}

fn write_atomically(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("offset.tmp");
    std::fs::write(&temporary, contents)?;
    std::fs::rename(&temporary, path)
}

/// Where a followed file's lines go.
#[async_trait]
pub trait LineSink: Send + Sync {
    /// One whole line of the file, without its `\n` (nor a `\r` before it). `end_offset` is the
    /// byte just past its newline: the follower's offset once the line is delivered. Called once
    /// per line, in file order; the next is not delivered until this one returns.
    async fn line(&self, line: &[u8], end_offset: u64);
}

/// What a follower follows, and how its log lines name it.
#[derive(Debug, Clone)]
pub struct FollowedFile {
    /// The file's path on the shell's machine.
    pub path: String,
    /// The path whose removal ends the remote `tail` (see [`tail_command`]).
    pub alive_while: String,
    /// Whether the file may not exist yet when the follower starts (`tail -F`).
    pub waits_for_file: bool,
    /// What the file is, for the follower's log lines: "hook log", "transcript".
    pub label: &'static str,
}

/// The reader of one followed file, running until it is stopped.
pub struct FileFollower {
    task: Option<tokio::task::JoinHandle<()>>,
}

impl FileFollower {
    /// Start following `file` on `shell`'s machine from `offset`, delivering every whole line to
    /// `sink`. Must be called inside a tokio runtime.
    ///
    /// A stream that ends, or cannot be opened, is opened again after a back-off, from the byte the
    /// follower had delivered. Its start, each break and each reopen are logged with that byte.
    pub fn start(
        shell: Arc<dyn RemoteShell>,
        file: FollowedFile,
        offset: FollowOffset,
        sink: Arc<dyn LineSink>,
    ) -> Self {
        Self {
            task: Some(tokio::spawn(follow(shell, file, offset, sink))),
        }
    }

    /// Stop following at once, without waiting for the reader to be gone: a follower being replaced
    /// by another of the same file, which must not deliver beside it.
    pub fn abort(&self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }

    /// Stop following: the remote command is ended and nothing more is delivered. Returns once the
    /// reader is gone.
    pub async fn stop(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

/// Follow `file` until aborted: open its stream from the delivered offset, deliver its lines, and
/// when the stream ends open another from the byte delivered, after a back-off.
async fn follow(
    shell: Arc<dyn RemoteShell>,
    file: FollowedFile,
    offset: FollowOffset,
    sink: Arc<dyn LineSink>,
) {
    let target = shell.address();
    let mut delivered = offset.load();
    log::info!(
        "following the {} {} on {target} from byte {delivered}",
        file.label,
        file.path
    );
    let mut opened_before = false;
    loop {
        if opened_before {
            log::info!(
                "reopening the {} {} on {target} from byte {delivered}",
                file.label,
                file.path
            );
        }
        opened_before = true;
        let command = tail_command(
            &file.path,
            delivered,
            &file.alive_while,
            file.waits_for_file,
        );
        match shell.exec_streamed(&command).await {
            Ok(mut streamed) => {
                let ended = read_stream(&mut streamed, &mut delivered, &offset, &sink).await;
                log::info!(
                    "the {} {} on {target} stopped streaming at byte {delivered}: {ended}",
                    file.label,
                    file.path
                );
            }
            Err(error) => log::warn!(
                "the {} {} on {target} could not be followed from byte {delivered}: {error}",
                file.label,
                file.path
            ),
        }
        tokio::time::sleep(REOPEN_BACKOFF).await;
    }
}

/// Read one opened stream to its end, delivering every whole line it carries and advancing
/// `delivered` past each. A trailing partial line is dropped with the stream: the next stream
/// starts at its first byte. Returns why the stream ended, for the log.
async fn read_stream(
    streamed: &mut tddy_remote_shell::StreamedExec,
    delivered: &mut u64,
    offset: &FollowOffset,
    sink: &Arc<dyn LineSink>,
) -> String {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match streamed.stdout.read(&mut chunk).await {
            Ok(0) => return "end of stream".to_string(),
            Err(error) => return error.to_string(),
            Ok(read) => {
                pending.extend_from_slice(&chunk[..read]);
                while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                    let raw: Vec<u8> = pending.drain(..=end).collect();
                    let mut line = &raw[..raw.len() - 1];
                    if line.last() == Some(&b'\r') {
                        line = &line[..line.len() - 1];
                    }
                    // The line and its `\n`: a `\r` before it is a pty's, not the file's.
                    *delivered += line.len() as u64 + 1;
                    sink.line(line, *delivered).await;
                    offset.save(*delivered);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_file_that_may_not_exist_yet_is_tailed_with_retry_until_its_watch_is_removed() {
        // Given a transcript that appears with the agent's first prompt, kept alive by the
        // session's hook log
        let transcript = "/home/dev/.cursor/projects/p/agent-transcripts/c/c.jsonl";
        let alive = "/home/dev/repo/.tddy/hooks.log";

        // When
        let command = tail_command(transcript, 120, alive, true);

        // Then
        assert_eq!(
            command,
            "stty -opost 2>/dev/null; \
             tail -c +121 -F '/home/dev/.cursor/projects/p/agent-transcripts/c/c.jsonl' 2>/dev/null & t=$!; \
             (while [ -e '/home/dev/repo/.tddy/hooks.log' ]; do sleep 1; done; kill \"$t\") >/dev/null 2>&1 & wait \"$t\""
        );
    }

    #[test]
    fn a_persisted_offset_is_read_back_by_the_next_follower() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followers").join("transcript.offset");
        FollowOffset::persisted_at(path.clone()).save(4711);

        // When
        let next = FollowOffset::persisted_at(path).load();

        // Then
        assert_eq!(next, 4711);
    }

    #[test]
    fn a_forgotten_offset_starts_the_next_follower_at_the_first_byte() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks.offset");
        let offset = FollowOffset::persisted_at(path.clone());
        offset.save(99);

        // When
        offset.forget();

        // Then
        assert_eq!(
            (offset.load(), FollowOffset::persisted_at(path).load()),
            (0, 0)
        );
    }

    #[test]
    fn an_unparsable_persisted_offset_reads_from_the_start() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks.offset");
        std::fs::write(&path, "not a number").unwrap();

        // When
        let offset = FollowOffset::persisted_at(path).load();

        // Then
        assert_eq!(offset, 0);
    }
}
