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
pub(super) fn one_shot_command(command: &str, stdin: Option<(usize, usize)>) -> String {
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
