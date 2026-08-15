//! Serial-console (UART) shell driver for QEMU guests.
//!
//! Before a guest has networking or an SSH daemon, the only way in is the emulated serial
//! port. This module turns that byte stream into a small protocol:
//!
//! * [`SerialShell`] is the **pure** parsing core — [`SerialShell::feed`] takes a chunk of
//!   console bytes (a partial line, a line, or many lines) and returns the
//!   [`SerialShellEvent`]s that chunk produced. It performs no I/O, so the whole
//!   login/prompt/command protocol is unit-testable without booting a VM.
//! * [`SerialConsole`] is the async driver that owns the QEMU child's serial stdin/stdout,
//!   pumps bytes into a [`SerialShell`], and answers the prompts it reports.
//!
//! Command completion is *not* inferred from a prompt reappearing — the guest is asked to
//! print a per-command marker followed by `$?`, and only that marker line ends a command.
//! That is what makes output containing prompt- or login-shaped lines (a `cat` of `/etc/issue`,
//! say) harmless: once a command is running, nothing in its output can move the state machine.

use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::time::Instant;

use crate::vm::VmError;

/// The escape byte introducing every ANSI sequence.
const ESC: char = '\x1b';
/// The bell byte, one of the two terminators an OSC sequence may end with.
const BEL: char = '\x07';

// ── State machine ───────────────────────────────────────────────────────────────────

/// Where the console conversation currently stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialShellState {
    /// Boot output, before any prompt has been seen.
    Prelude,
    /// A login prompt is waiting for a username.
    AtLogin,
    /// A password prompt is waiting for a password.
    AtPassword,
    /// A shell prompt is waiting for a command.
    AtPrompt,
    /// A command is running; everything until its exit-code marker is its output.
    ExecutingCommand,
}

/// What a chunk of console bytes meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialShellEvent {
    /// A line of boot output, before the first prompt.
    PreludeLine(String),
    /// A login prompt was printed.
    Login,
    /// A password prompt was printed.
    Password,
    /// A shell prompt was printed; the guest is ready for a command.
    Prompt,
    /// A line of output belonging to the running command.
    CommandOutputLine(String),
    /// The running command printed its exit-code marker with this exit code.
    CommandFinished(i32),
}

/// The shape of a prompt, as the set of endings a line may have.
///
/// Endings rather than regexes because a getty or shell prompt is always identified by how
/// its line *ends* — `login:`, `Password:`, `$`, `#`, `>` — and matching on endings keeps
/// this crate free of a regex dependency. Matching ignores trailing whitespace and case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptPattern {
    endings: Vec<String>,
}

impl PromptPattern {
    /// A pattern matching any line ending in one of `endings` (case-insensitively).
    pub fn ending_in<I, S>(endings: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            endings: endings
                .into_iter()
                .map(|ending| ending.as_ref().to_lowercase())
                .collect(),
        }
    }

    /// Whether `line` (already stripped of ANSI codes) ends with one of this pattern's
    /// endings. A blank line never matches.
    pub fn matches(&self, line: &str) -> bool {
        let tail = line.trim_end();
        if tail.is_empty() {
            return false;
        }
        let tail = tail.to_lowercase();
        self.endings.iter().any(|ending| tail.ends_with(ending))
    }
}

/// Credentials the console answers a login/password prompt with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// How a [`SerialShell`] recognises the guest's prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialShellConfig {
    /// Recognises a getty login prompt, e.g. `tddy-host login: `.
    pub login_prompt: PromptPattern,
    /// Recognises a password prompt, e.g. `Password: `.
    pub password_prompt: PromptPattern,
    /// Recognises an interactive shell prompt, e.g. `tddy@tddy-host:~$ `.
    pub command_prompt: PromptPattern,
    /// When set, [`SerialConsole`] answers login and password prompts with these without
    /// being asked to.
    pub auto_login: Option<Credentials>,
}

impl Default for SerialShellConfig {
    fn default() -> Self {
        Self {
            login_prompt: PromptPattern::ending_in(["login:", "username:"]),
            password_prompt: PromptPattern::ending_in(["password:"]),
            command_prompt: PromptPattern::ending_in(["$", "#", ">"]),
            auto_login: None,
        }
    }
}

/// The pure console state machine. Feed it bytes, read back events.
pub struct SerialShell {
    config: SerialShellConfig,
    state: SerialShellState,
    /// Bytes received since the last newline — a line that is still arriving.
    partial: String,
    /// Whether a shell prompt has ever been seen; distinguishes boot output from the
    /// output of whatever the shell is doing.
    has_seen_prompt: bool,
    /// The marker the currently running command will print before its exit code.
    exit_marker: Option<String>,
    /// Whether a login or password prompt has been seen. Until one has, a shell prompt is
    /// not believed — see [`SerialShell::recognise_prompt`].
    seen_login_conversation: bool,
}

impl SerialShell {
    pub fn new(config: SerialShellConfig) -> Self {
        Self {
            config,
            state: SerialShellState::Prelude,
            partial: String::new(),
            has_seen_prompt: false,
            exit_marker: None,
            seen_login_conversation: false,
        }
    }

    /// Where the conversation stands right now.
    pub fn state(&self) -> SerialShellState {
        self.state
    }

    /// The credentials to answer prompts with, if any were configured.
    pub(crate) fn credentials(&self) -> Option<&Credentials> {
        self.config.auto_login.as_ref()
    }

    /// Answer future login and password prompts with `credentials`.
    pub(crate) fn set_auto_login(&mut self, credentials: Credentials) {
        self.config.auto_login = Some(credentials);
    }

    /// Consume a chunk of console bytes and report what it meant.
    ///
    /// Pure: no I/O, no clock. A chunk may end mid-line — the tail is buffered and only
    /// reported once the rest of the line arrives — except that a prompt is recognised on
    /// an unterminated line, because a real getty prints `login: ` with no newline and then
    /// waits.
    pub fn feed(&mut self, chunk: &str) -> Vec<SerialShellEvent> {
        let mut events = Vec::new();
        self.partial.push_str(chunk);

        while let Some(newline) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=newline).collect();
            self.handle_line(line.trim_end_matches('\n'), &mut events);
        }

        if !self.partial.is_empty() {
            let partial = std::mem::take(&mut self.partial);
            if !self.handle_partial(&partial, &mut events) {
                self.partial = partial;
            }
        }

        events
    }

    /// Start a command and return the unique marker whose appearance — followed by the
    /// command's exit code — ends it.
    ///
    /// The caller is expected to send `<command>; echo <marker>$?` to the guest.
    pub fn begin_command(&mut self, command: &str) -> String {
        log::debug!("serial console: running {command:?}");
        let marker = next_exit_marker();
        self.exit_marker = Some(marker.clone());
        self.state = SerialShellState::ExecutingCommand;
        // Whatever is buffered is the prompt the command is being typed at, not output.
        self.partial.clear();
        marker
    }

    /// Handle one complete line (its newline already removed).
    fn handle_line(&mut self, raw: &str, events: &mut Vec<SerialShellEvent>) {
        let line = clean_line(raw);

        if self.state == SerialShellState::ExecutingCommand {
            self.handle_command_line(line, events);
            return;
        }

        if line.trim().is_empty() {
            return;
        }
        if self.recognise_prompt(&line, events) {
            return;
        }

        if self.has_seen_prompt {
            events.push(SerialShellEvent::CommandOutputLine(line));
        } else {
            events.push(SerialShellEvent::PreludeLine(line));
        }
    }

    /// Handle a line that arrived while a command was running. Nothing here may change
    /// state except the command's own exit-code marker: a `cat` of a file full of prompts
    /// is output, not a prompt.
    fn handle_command_line(&mut self, line: String, events: &mut Vec<SerialShellEvent>) {
        let Some(marker) = self.exit_marker.clone() else {
            events.push(SerialShellEvent::CommandOutputLine(line));
            return;
        };

        let Some((before_marker, exit_code)) = split_at_exit_marker(&line, &marker) else {
            events.push(SerialShellEvent::CommandOutputLine(line));
            return;
        };

        // Output that ended without a newline shares the marker's line.
        if !before_marker.is_empty() {
            events.push(SerialShellEvent::CommandOutputLine(
                before_marker.to_string(),
            ));
        }
        self.exit_marker = None;
        self.state = SerialShellState::AtPrompt;
        self.has_seen_prompt = true;
        events.push(SerialShellEvent::CommandFinished(exit_code));
    }

    /// Check a line that is still arriving for a prompt. Returns whether one was found, in
    /// which case the caller drops the buffered text — it has been reported already.
    fn handle_partial(&mut self, partial: &str, events: &mut Vec<SerialShellEvent>) -> bool {
        if self.state == SerialShellState::ExecutingCommand {
            return false;
        }
        let line = clean_line(partial);
        if line.trim().is_empty() {
            return false;
        }
        self.recognise_prompt(&line, events)
    }

    /// Match `line` against the configured prompts, transitioning and emitting on a hit.
    fn recognise_prompt(&mut self, line: &str, events: &mut Vec<SerialShellEvent>) -> bool {
        if self.config.login_prompt.matches(line) {
            self.state = SerialShellState::AtLogin;
            self.seen_login_conversation = true;
            events.push(SerialShellEvent::Login);
            return true;
        }
        if self.config.password_prompt.matches(line) {
            self.state = SerialShellState::AtPassword;
            self.seen_login_conversation = true;
            events.push(SerialShellEvent::Password);
            return true;
        }
        // A shell prompt is only believed once the login conversation has started. Boot
        // output routinely contains lines ending in the same characters a prompt does —
        // cloud-init frames its SSH host-key banner in rows of `#` — and mistaking one for
        // a prompt makes the driver type commands at a getty that is still waiting for a
        // username. Until a login or password prompt has been seen, such lines are just
        // boot output.
        if self.seen_login_conversation && self.config.command_prompt.matches(line) {
            self.state = SerialShellState::AtPrompt;
            self.has_seen_prompt = true;
            events.push(SerialShellEvent::Prompt);
            return true;
        }
        false
    }
}

/// Split a command-output line at its exit-code marker into the output preceding the
/// marker and the exit code following it.
///
/// A line carrying the marker without a numeric exit code is *not* a completion: it is the
/// guest's terminal echoing the command line (`… ; echo <marker>$?`) back at us. Such a
/// line is reported as ordinary output. A corrupted exit code therefore leaves the command
/// outstanding until the caller's timeout says so, rather than being reported as a success
/// that never happened.
fn split_at_exit_marker<'a>(line: &'a str, marker: &str) -> Option<(&'a str, i32)> {
    let marker_at = line.find(marker)?;
    let (before_marker, from_marker) = line.split_at(marker_at);
    let exit_code = from_marker[marker.len()..].trim().parse::<i32>().ok()?;
    Some((before_marker, exit_code))
}

/// A marker unique to one command, so a stale marker from an earlier command — still in
/// flight on a slow console — cannot end the wrong one.
fn next_exit_marker() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("__TDDY_EXIT_{}_{sequence}__", std::process::id())
}

/// Strip the escape sequences a terminal would have consumed, and the trailing carriage
/// return of a CRLF line ending, leaving the text a human would have seen.
///
/// `pub(crate)` for [`crate::cloud_init::boot_log_line`], which owes a bake's durable boot
/// log the same treatment for the same reason: text a tool can grep, not a terminal
/// recording only a terminal can replay.
pub(crate) fn clean_line(raw: &str) -> String {
    // A serial console carries carriage returns as framing, not content: a guest terminal
    // emits `\r\r\n` line endings and prefixes a redrawn line with a bare `\r`. Splitting on
    // `\n` therefore leaves CRs on either end of the text, none of which belong to it.
    strip_ansi_codes(raw).trim_matches('\r').to_string()
}

/// Remove ANSI escape sequences and stray control bytes from console text.
///
/// Handles CSI sequences (`ESC [ … final`, per ECMA-48, so `ESC[0;32m` and `ESC[3~` alike),
/// a CSI truncated by the end of the chunk, OSC sequences (`ESC ] … BEL` or `ESC ] … ESC \`),
/// the `ESC =` / `ESC >` keypad-mode sequences, and bare control characters. Newline and
/// carriage return survive — line splitting depends on them.
pub fn strip_ansi_codes(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut cleaned = String::with_capacity(s.len());
    let mut at = 0;

    while at < chars.len() {
        let current = chars[at];
        if current != ESC {
            // Newline and carriage return are structure, not noise; other C0 bytes go.
            if current == '\n' || current == '\r' || (current as u32) >= 0x20 {
                cleaned.push(current);
            }
            at += 1;
            continue;
        }

        match chars.get(at + 1) {
            Some('[') => at = skip_csi(&chars, at + 2),
            Some(']') => at = skip_osc(&chars, at + 2),
            Some('=') | Some('>') => at += 2,
            // A lone or unrecognised escape: drop the ESC, keep what follows.
            _ => at += 1,
        }
    }

    cleaned
}

/// Skip a CSI sequence's parameter, intermediate, and final bytes, starting just after the
/// `ESC [`. A sequence cut off by the end of the chunk simply ends there.
fn skip_csi(chars: &[char], mut at: usize) -> usize {
    while at < chars.len() && matches!(chars[at], '\u{30}'..='\u{3f}' | '\u{20}'..='\u{2f}') {
        at += 1;
    }
    if at < chars.len() && matches!(chars[at], '\u{40}'..='\u{7e}') {
        at += 1;
    }
    at
}

/// Skip an OSC sequence, starting just after the `ESC ]`, through its BEL or `ESC \`
/// terminator. An unterminated OSC swallows the rest of the chunk, as a terminal would.
fn skip_osc(chars: &[char], mut at: usize) -> usize {
    while at < chars.len() {
        match chars[at] {
            BEL => return at + 1,
            ESC if chars.get(at + 1) == Some(&'\\') => return at + 2,
            _ => at += 1,
        }
    }
    at
}

// ── Async driver ────────────────────────────────────────────────────────────────────

/// What running a command over the serial console produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout_lines: Vec<String>,
    pub exit_code: i32,
}

/// Drives a guest's serial console: owns the QEMU child's serial stdin/stdout, pumps
/// received bytes through a [`SerialShell`], and writes the answers its events call for.
///
/// Every wait is bounded by the caller's timeout, so a guest that stops talking produces a
/// descriptive error rather than a hang.
pub struct SerialConsole {
    stdin: ChildStdin,
    stdout: ChildStdout,
    shell: SerialShell,
    /// Bytes read but not yet decodable — a multi-byte character split across two reads.
    undecoded: Vec<u8>,
    /// The tail of everything the console has produced, kept so a timeout or an unexpected
    /// close can say what the guest was actually showing. A console that stalls is otherwise
    /// indistinguishable from one that never received the command at all.
    transcript: String,
    /// Where the emulator's own stderr is being written, if the caller registered it via
    /// [`SerialConsole::with_stderr_log`].
    stderr_log_path: Option<PathBuf>,
}

/// How much console output [`SerialConsole`] keeps for diagnostics.
const TRANSCRIPT_TAIL_BYTES: usize = 4096;

/// How much of the console [`SerialConsole`] takes off the pipe in one read. Independent of
/// [`TRANSCRIPT_TAIL_BYTES`] — a read is a chunk of the conversation, not a diagnostic
/// budget — and only bounded by what a UART can plausibly deliver between two polls.
const CONSOLE_READ_BYTES: usize = 4096;

impl SerialConsole {
    /// Drive `stdin`/`stdout` with the default prompt patterns.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self::with_config(stdin, stdout, SerialShellConfig::default())
    }

    /// Drive `stdin`/`stdout` with guest-specific prompt patterns or credentials.
    pub fn with_config(stdin: ChildStdin, stdout: ChildStdout, config: SerialShellConfig) -> Self {
        Self {
            stdin,
            stdout,
            shell: SerialShell::new(config),
            undecoded: Vec::new(),
            transcript: String::new(),
            stderr_log_path: None,
        }
    }

    /// Where the console conversation stands right now.
    pub fn state(&self) -> SerialShellState {
        self.shell.state()
    }

    /// Read the boot output until the guest's getty asks for a username.
    pub async fn wait_for_login_prompt(&mut self, timeout: Duration) -> Result<(), VmError> {
        let deadline = Instant::now() + timeout;
        while self.shell.state() != SerialShellState::AtLogin {
            self.pump(deadline, "a login prompt", VmError::BootFailed)
                .await?;
        }
        Ok(())
    }

    /// Log in as `username`, then wait for the shell prompt that follows.
    pub async fn login(
        &mut self,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<(), VmError> {
        let deadline = Instant::now() + timeout;
        self.shell.set_auto_login(Credentials {
            username: username.to_string(),
            password: password.to_string(),
        });

        // A prompt printed before this call was made has already been reported, so answer
        // where the conversation currently stands; later prompts are answered by `pump`.
        match self.shell.state() {
            SerialShellState::AtLogin => self.write_line(username).await?,
            SerialShellState::AtPassword => self.write_line(password).await?,
            _ => {}
        }

        while self.shell.state() != SerialShellState::AtPrompt {
            self.pump(
                deadline,
                "a shell prompt after logging in",
                VmError::BootFailed,
            )
            .await?;
        }
        Ok(())
    }

    /// Read and discard whatever the guest writes, until `timeout` elapses or the console
    /// closes.
    ///
    /// The console is a pipe, and a test that works over SSH stops reading it the moment the
    /// guest boots. Once the pipe's buffer fills — about 64 KiB, and a Debian boot with
    /// cloud-init produces more than that — the guest blocks writing to `ttyS0`. A guest
    /// blocked there cannot finish shutting down: `system_powerdown` is accepted, systemd
    /// begins writing its stop messages, and the sequence stalls with QEMU still running and
    /// still holding its forwarded ports. Draining while waiting for it to go keeps it
    /// moving.
    pub async fn drain_for(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            // Any error is a reason to stop draining, not a failure: the deadline passing and
            // the console closing because QEMU exited are both expected endings here.
            if self
                .pump(deadline, "the guest to finish writing", VmError::BootFailed)
                .await
                .is_err()
            {
                return;
            }
        }
    }

    /// Run `command` in the guest's shell and collect its output and exit code.
    ///
    /// The command is sent with a trailing `echo <marker>$?`, and only that marker's line
    /// ends the command — so output that looks like a prompt cannot end it early.
    pub async fn run_command(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, VmError> {
        if self.shell.state() != SerialShellState::AtPrompt {
            return Err(VmError::VerifyFailed(format!(
                "serial console is not at a shell prompt (state: {:?}); cannot run {command:?}",
                self.shell.state()
            )));
        }

        let deadline = Instant::now() + timeout;
        let marker = self.shell.begin_command(command);
        self.write_line(&format!("{command}; echo {marker}$?"))
            .await?;

        let waiting_for = format!("command {command:?} to finish");
        let mut stdout_lines = Vec::new();
        loop {
            let events = self
                .pump(deadline, &waiting_for, VmError::VerifyFailed)
                .await?;
            for event in events {
                match event {
                    // A line carrying the marker is the guest's terminal echoing the
                    // command line back, not output the command produced.
                    SerialShellEvent::CommandOutputLine(line) if !line.contains(&marker) => {
                        stdout_lines.push(line)
                    }
                    SerialShellEvent::CommandFinished(exit_code) => {
                        return Ok(CommandOutput {
                            stdout_lines,
                            exit_code,
                        })
                    }
                    _ => {}
                }
            }
        }
    }

    /// Append `text` to the diagnostic transcript, keeping only the most recent
    /// [`TRANSCRIPT_TAIL_BYTES`] so a long boot cannot grow it without bound.
    fn record_transcript(&mut self, text: &str) {
        self.transcript.push_str(text);
        if self.transcript.len() <= TRANSCRIPT_TAIL_BYTES {
            return;
        }
        // Drop exactly the overflow, rounded forward to the next character boundary so the
        // tail stays valid UTF-8. The string's own length is always a boundary, so this
        // advances at most three bytes.
        let mut cut = self.transcript.len() - TRANSCRIPT_TAIL_BYTES;
        while !self.transcript.is_char_boundary(cut) {
            cut += 1;
        }
        self.transcript.drain(..cut);
    }

    /// The recorded console tail, formatted for inclusion in an error message.
    ///
    /// A console that produced nothing usually means the emulator never got as far as
    /// running a guest, so that case reports the emulator's own stderr instead — the only
    /// place a bad argv, a busy port, or an unreadable image is explained.
    fn transcript_tail(&self) -> String {
        if self.transcript.trim().is_empty() {
            return format!(
                " (the console produced no output at all{})",
                self.emulator_stderr_tail()
            );
        }
        format!(
            "; the console last showed:\n---\n{}\n---",
            strip_ansi_codes(&self.transcript)
        )
    }

    /// Whatever the emulator wrote to stderr, when a stderr log was registered.
    ///
    /// The log is opened with `O_NOFOLLOW`: its contents reach the caller of the RPC that
    /// booted this VM, so a symlink standing where the log belongs must be an error rather
    /// than a way to have this process read out a file on the reader's behalf.
    fn emulator_stderr_tail(&self) -> String {
        use std::io::Read;

        let Some(path) = &self.stderr_log_path else {
            return String::new();
        };
        let stderr = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .and_then(|mut log| {
                let mut contents = String::new();
                log.read_to_string(&mut contents)?;
                Ok(contents)
            });
        match stderr {
            Ok(stderr) if !stderr.trim().is_empty() => {
                format!("; the emulator reported: {}", stderr.trim())
            }
            _ => String::new(),
        }
    }

    /// Register where the emulator's stderr is being written, so failures that produce no
    /// console output can still explain themselves.
    pub fn with_stderr_log(mut self, path: impl Into<PathBuf>) -> Self {
        self.stderr_log_path = Some(path.into());
        self
    }

    /// Read one chunk from the guest, feed it to the state machine, and answer any prompt
    /// it reported. Bounded by `deadline`; `waiting_for` names what the wait was for, and
    /// `fail` classifies the failure for the caller's context.
    async fn pump(
        &mut self,
        deadline: Instant,
        waiting_for: &str,
        fail: fn(String) -> VmError,
    ) -> Result<Vec<SerialShellEvent>, VmError> {
        let mut buffer = [0u8; CONSOLE_READ_BYTES];
        let read = tokio::time::timeout_at(deadline, self.stdout.read(&mut buffer))
            .await
            .map_err(|_| {
                fail(format!(
                    "timed out waiting for {waiting_for} on the serial console{}",
                    self.transcript_tail()
                ))
            })?
            .map_err(|e| {
                fail(format!(
                    "serial console read failed waiting for {waiting_for}: {e}{}",
                    self.transcript_tail()
                ))
            })?;
        if read == 0 {
            return Err(fail(format!(
                "serial console closed while waiting for {waiting_for}{}",
                self.transcript_tail()
            )));
        }

        self.undecoded.extend_from_slice(&buffer[..read]);
        let text = decode_utf8_prefix(&mut self.undecoded);
        self.record_transcript(&text);
        let events = self.shell.feed(&text);
        self.answer_prompts(&events).await?;
        Ok(events)
    }

    /// Send the configured credentials in response to login and password prompts.
    async fn answer_prompts(&mut self, events: &[SerialShellEvent]) -> Result<(), VmError> {
        let Some(credentials) = self.shell.credentials().cloned() else {
            return Ok(());
        };
        for event in events {
            match event {
                SerialShellEvent::Login => self.write_line(&credentials.username).await?,
                SerialShellEvent::Password => self.write_line(&credentials.password).await?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Type `text` into the guest, followed by Enter.
    async fn write_line(&mut self, text: &str) -> Result<(), VmError> {
        self.stdin
            .write_all(format!("{text}\n").as_bytes())
            .await
            .map_err(|e| VmError::VerifyFailed(format!("serial console write failed: {e}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| VmError::VerifyFailed(format!("serial console flush failed: {e}")))
    }
}

/// Decode as much of `bytes` as forms whole characters, leaving a trailing multi-byte
/// character that is still arriving in the buffer for the next read. Bytes that are not
/// valid UTF-8 at all — line noise on a UART — become the replacement character rather
/// than derailing the decode.
fn decode_utf8_prefix(bytes: &mut Vec<u8>) -> String {
    let mut decoded = String::new();
    loop {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                decoded.push_str(text);
                bytes.clear();
                return decoded;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if let Ok(text) = std::str::from_utf8(&bytes[..valid_up_to]) {
                    decoded.push_str(text);
                }
                match error.error_len() {
                    Some(invalid_len) => {
                        decoded.push(char::REPLACEMENT_CHARACTER);
                        bytes.drain(..valid_up_to + invalid_len);
                    }
                    // Incomplete tail: keep it for the next read.
                    None => {
                        bytes.drain(..valid_up_to);
                        return decoded;
                    }
                }
            }
        }
    }
}
