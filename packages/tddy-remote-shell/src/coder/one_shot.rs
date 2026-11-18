use std::time::Duration;

/// The line the one-shot wrapper prints to stderr before the command runs.
///
/// `coder ssh` exits 1 both when the workspace cannot be entered and when the command exits 1,
/// so the exit status alone cannot tell those apart. This marker can: present means the command
/// ran and its status is its own; absent with a non-zero exit means nothing ran.
pub(super) const START_MARKER: &str = "tddy-coder-shell: the command is running";

/// `bytes` without its leading start-marker line, or `None` when the marker is not there.
/// The line ends `\n`, or `\r\n` when a pty wrote it.
pub(super) fn strip_start_marker(bytes: &[u8]) -> Option<&[u8]> {
    let rest = bytes.strip_prefix(START_MARKER.as_bytes())?;
    if let Some(stripped) = rest.strip_prefix(b"\r\n") {
        Some(stripped)
    } else if let Some(stripped) = rest.strip_prefix(b"\n") {
        Some(stripped)
    } else if rest.is_empty() {
        Some(rest)
    } else {
        None
    }
}

/// How wide a base64 line of a one-shot's stdin is: far inside the 4096 bytes a tty's canonical
/// line holds.
const STDIN_LINE_WIDTH: usize = 76;

/// What a one-shot exits with when its stdin did not arrive whole, and its command never ran.
const STDIN_NOT_WHOLE_STATUS: i32 = 125;

/// `bytes` as the lines a one-shot's stdin crosses the workspace's pty in: base64, every line
/// ended by a newline. A pty in canonical mode hands its reader only complete lines and acts on
/// control characters (`^D`, `^C`, a carriage return); base64 holds none of them, and the newline
/// that ends the last line releases it.
pub(super) fn stdin_lines(bytes: &[u8]) -> Vec<u8> {
    use base64::Engine as _;

    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut lines = Vec::with_capacity(encoded.len() + encoded.len() / STDIN_LINE_WIDTH + 1);
    for line in encoded.as_bytes().chunks(STDIN_LINE_WIDTH) {
        lines.extend_from_slice(line);
        lines.push(b'\n');
    }
    lines
}

/// The command line a one-shot runs on the workspace: the start marker, then `command`.
///
/// `coder ssh` runs it in a pty. Its output processing is turned off first (`stty -opost`), so
/// the bytes the command writes arrive as written — not with every `\n` made `\r\n` (`onlcr`).
/// Closing the CLI's stdin never reaches the workspace as an EOF, so `command` never reads the
/// pty itself. With no stdin the shell's stdin is `/dev/null`. With stdin, `stdin` is
/// `(sent, length)`: exactly `sent` bytes of [`stdin_lines`] are read off
/// the pty — counted, so no EOF is needed — decoded into a file only its owner can read and
/// checked to hold `length` bytes; the shell's stdin is then that file, unlinked before `command`
/// runs. A stdin that did not arrive whole runs nothing and exits [`STDIN_NOT_WHOLE_STATUS`].
/// Echo is turned off before the marker is printed, so lines written once the marker has arrived
/// are not echoed into the command's output.
///
/// `command` itself is [`bounded`]: pager-free, and stopped on the workspace when `deadline` has
/// passed.
pub(super) fn one_shot_command(
    command: &str,
    stdin: Option<(usize, usize)>,
    deadline: Duration,
) -> String {
    let command = bounded(command, deadline);
    let marker = format!("printf '%s\\n' '{START_MARKER}' >&2");
    match stdin {
        None => format!("stty -opost 2>/dev/null; exec </dev/null; {marker}; {command}"),
        Some((sent, length)) => format!(
            "stty -opost -echo 2>/dev/null; {marker}; \
             tddy_stdin=$(mktemp) || exit {STDIN_NOT_WHOLE_STATUS}; \
             if head -c {sent} | base64 -d > \"$tddy_stdin\" \
             && [ \"$(( $(wc -c < \"$tddy_stdin\") ))\" -eq {length} ]; then \
             exec < \"$tddy_stdin\"; rm -f \"$tddy_stdin\"; \
             else rm -f \"$tddy_stdin\"; \
             echo \"tddy-coder-shell: the command's stdin did not arrive whole\" >&2; \
             exit {STDIN_NOT_WHOLE_STATUS}; fi; \
             {command}"
        ),
    }
}

/// How long after `TERM` the watchdog waits before `KILL`, for a command that ignored `TERM`.
const KILL_GRACE_SECONDS: u64 = 2;

/// `command` as the workspace runs it: with the variables that keep a pager and a git prompt from
/// waiting on a terminal nobody is at, as its own process group, under a watchdog that ends the
/// group once `deadline` has passed.
///
/// Dropping the local `coder` does not stop what it started: the workspace's command outlives the
/// connection. The watchdog runs on the workspace and needs nothing beyond POSIX `sh`: a job-control
/// shell gives the background command a process group of its own (`set -m`), and a `sleep`
/// followed by `kill -- -<pgid>` ends the group, children included. The watchdog starts when the
/// command does, so it fires inside the caller's deadline by however long the connection took.
/// The command's status is the wrapper's; a command ended by the watchdog is `143`. A job-control
/// shell reports each job's end on its own stderr (bash as `sh` does, with `[1]+ Done`), so that
/// is pointed at `/dev/null` and the command keeps the original stderr as fd 3. A job-control
/// shell reports each job's end on its own stderr (bash as `sh` does, with `[1]+ Done`), so that
/// is pointed at `/dev/null` and the command keeps the original stderr as fd 3.
fn bounded(command: &str, deadline: Duration) -> String {
    let seconds = (deadline.as_secs() + u64::from(deadline.subsec_nanos() > 0)).max(1);
    let quoted = crate::shell_single_quote(command);
    format!(
        "PAGER=cat GIT_PAGER=cat GIT_TERMINAL_PROMPT=0; export PAGER GIT_PAGER GIT_TERMINAL_PROMPT; \
         set -m; exec 3>&2 2>/dev/null; sh -c {quoted} 2>&3 3>&- & tddy_command=$!; \
         ( sleep {seconds}; kill -TERM -- -$tddy_command 2>/dev/null; \
         sleep {KILL_GRACE_SECONDS}; kill -KILL -- -$tddy_command 2>/dev/null ) \
         >/dev/null 2>&1 & tddy_watchdog=$!; \
         wait $tddy_command; tddy_status=$?; \
         kill -TERM -- -$tddy_watchdog 2>/dev/null; exit $tddy_status"
    )
}

/// Whether `bytes` hold the start marker's whole line, ended `\n` or, from a pty, `\r\n`.
fn holds_the_start_marker_line(bytes: &[u8]) -> bool {
    bytes.windows(START_MARKER.len() + 1).any(|window| {
        window.starts_with(START_MARKER.as_bytes())
            && matches!(window[START_MARKER.len()], b'\n' | b'\r')
    })
}

/// Everything `stream` carries until it ends, noting on `marker_arrived` the moment the start
/// marker's line is among it.
pub(super) async fn read_noting_the_marker(
    stream: &mut (impl tokio::io::AsyncRead + Unpin),
    marker_arrived: &tokio::sync::Notify,
) -> Vec<u8> {
    use tokio::io::AsyncReadExt as _;

    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut noted = false;
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return bytes,
            Ok(read) => {
                bytes.extend_from_slice(&chunk[..read]);
                if !noted && holds_the_start_marker_line(&bytes) {
                    noted = true;
                    marker_arrived.notify_one();
                }
            }
        }
    }
}
