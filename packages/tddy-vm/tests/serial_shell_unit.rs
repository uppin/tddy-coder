//! Unit tests for the pure serial-console state machine (`tddy_vm::serial_shell`).
//!
//! `SerialShell::feed` is deliberately pure — it takes a chunk of console bytes and returns
//! the events that chunk produced — so the whole login/prompt/command protocol is testable
//! without booting a VM. The real-VM proof lives in `vm_boot_control_acceptance.rs`.

use std::process::Stdio;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_vm::serial_shell::{
    strip_ansi_codes, SerialConsole, SerialShell, SerialShellConfig, SerialShellEvent,
    SerialShellState,
};

/// A shell configured the way the acceptance tests drive a Debian guest.
fn a_serial_shell() -> SerialShell {
    SerialShell::new(SerialShellConfig::default())
}

/// A shell that has been through a getty's login conversation and is waiting for a command.
fn a_serial_shell_at_a_prompt() -> SerialShell {
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");
    shell.feed("tddy@tddy-host:~$ ");
    shell
}

/// A console wired to a process that never says anything, so only the driver's own rules
/// can decide what happens.
struct SilentGuest {
    /// Kept alive for as long as the console is: dropping it kills the process the
    /// console's pipes belong to.
    _process: tokio::process::Child,
    console: SerialConsole,
}

fn a_silent_guest() -> SilentGuest {
    let mut process = tokio::process::Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("a stand-in guest process must be spawnable");
    let stdin = process
        .stdin
        .take()
        .expect("the stand-in guest has a stdin");
    let stdout = process
        .stdout
        .take()
        .expect("the stand-in guest has a stdout");

    SilentGuest {
        _process: process,
        console: SerialConsole::new(stdin, stdout),
    }
}

#[test]
fn starts_in_the_prelude_state_before_any_output_arrives() {
    // Given a fresh shell
    let shell = a_serial_shell();

    // Then it is in the boot prelude
    assert_eq!(shell.state(), SerialShellState::Prelude);
}

#[test]
fn emits_boot_output_as_prelude_lines_before_the_first_prompt() {
    // Given a fresh shell
    let mut shell = a_serial_shell();

    // When kernel boot output arrives
    let events = shell.feed("[    0.000000] Booting Linux\n[    1.234567] systemd ready\n");

    // Then each line is reported as prelude output
    assert_eq!(
        events,
        vec![
            SerialShellEvent::PreludeLine("[    0.000000] Booting Linux".to_string()),
            SerialShellEvent::PreludeLine("[    1.234567] systemd ready".to_string()),
        ]
    );
}

#[test]
fn detects_a_login_prompt_that_arrives_without_a_trailing_newline() {
    // Given a shell that has seen boot output
    let mut shell = a_serial_shell();
    shell.feed("Debian GNU/Linux 12 tddy-host ttyAMA0\n");

    // When the login prompt arrives as a partial line, as a real getty emits it
    let events = shell.feed("tddy-host login: ");

    // Then the prompt is recognised despite the missing newline
    assert_eq!(events, vec![SerialShellEvent::Login]);
    assert_eq!(shell.state(), SerialShellState::AtLogin);
}

#[test]
fn detects_a_password_prompt_that_arrives_without_a_trailing_newline() {
    // Given a shell that has answered the login prompt
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");

    // When the password prompt arrives
    let events = shell.feed("Password: ");

    // Then it is recognised
    assert_eq!(events, vec![SerialShellEvent::Password]);
    assert_eq!(shell.state(), SerialShellState::AtPassword);
}

#[test]
fn reassembles_a_line_split_across_two_chunks() {
    // Given a shell mid-stream
    let mut shell = a_serial_shell();

    // When one logical line arrives in two reads
    let first = shell.feed("[    2.0] partial ");
    let second = shell.feed("line completed\n");

    // Then no event fires until the line is whole, and it arrives intact
    assert_eq!(first, vec![]);
    assert_eq!(
        second,
        vec![SerialShellEvent::PreludeLine(
            "[    2.0] partial line completed".to_string()
        )]
    );
}

#[test]
fn recognises_a_shell_prompt_wrapped_in_ansi_colour_codes() {
    // Given a shell that has logged in
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");

    // When a colourised bash prompt arrives
    let events = shell.feed("\x1b[01;32mtddy@tddy-host\x1b[00m:\x1b[01;34m~\x1b[00m$ ");

    // Then the escape codes do not prevent prompt detection
    assert_eq!(events, vec![SerialShellEvent::Prompt]);
    assert_eq!(shell.state(), SerialShellState::AtPrompt);
}

#[test]
fn reports_command_output_lines_once_the_shell_is_at_a_prompt() {
    // Given a shell sitting at a command prompt
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");
    shell.feed("tddy@tddy-host:~$ ");

    // When a command's output arrives
    let events = shell.feed("hello-from-host-9p\n");

    // Then it is classified as command output, not boot prelude
    assert_eq!(
        events,
        vec![SerialShellEvent::CommandOutputLine(
            "hello-from-host-9p".to_string()
        )]
    );
}

#[test]
fn strips_ansi_colour_cursor_and_control_sequences_from_a_line() {
    // Given a line carrying colour, a cursor move, and a stray control byte
    let raw = "\x1b[0;32m  OK  \x1b[0m Started \x1b[1;39mssh.service\x1b[0m\x1b[2K\x07";

    // When it is cleaned
    let cleaned = strip_ansi_codes(raw);

    // Then only the human-readable text survives
    assert_eq!(cleaned, "  OK   Started ssh.service");
}

#[test]
fn reports_the_exit_code_marker_a_finished_command_prints() {
    // Given a shell at a prompt that has been asked to run a command
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");
    shell.feed("tddy@tddy-host:~$ ");
    let marker = shell.begin_command("id -un");

    // When the guest echoes the output and then the exit-code marker
    shell.feed("tddy\n");
    let events = shell.feed(&format!("{marker}42\n"));

    // Then completion is reported with the real exit code, not inferred from the prompt
    assert_eq!(events, vec![SerialShellEvent::CommandFinished(42)]);
}

#[test]
fn does_not_treat_a_prompt_shaped_line_inside_command_output_as_completion() {
    // Given a running command whose output happens to contain a prompt-shaped line
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");
    shell.feed("tddy@tddy-host:~$ ");
    shell.begin_command("cat motd.txt");

    // When that line arrives
    let events = shell.feed("root@somewhere:/# \n");

    // Then it is reported as output, and the command is still running
    assert_eq!(
        events,
        vec![SerialShellEvent::CommandOutputLine(
            "root@somewhere:/# ".to_string()
        )]
    );
    assert_eq!(shell.state(), SerialShellState::ExecutingCommand);
}

#[test]
fn ignores_a_login_prompt_pattern_once_the_shell_is_executing_a_command() {
    // Given a running command that cats a file mentioning a login prompt
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");
    shell.feed("tddy@tddy-host:~$ ");
    shell.begin_command("cat /etc/issue");

    // When the output contains something that looks like a login prompt
    let events = shell.feed("debian login: \n");

    // Then the state machine does not fall back to the login state
    assert_eq!(
        events,
        vec![SerialShellEvent::CommandOutputLine(
            "debian login: ".to_string()
        )]
    );
    assert_eq!(shell.state(), SerialShellState::ExecutingCommand);
}

#[test]
fn does_not_mistake_a_boot_banner_of_hashes_for_a_shell_prompt() {
    // Given a fresh shell watching boot output
    let mut shell = a_serial_shell();

    // When cloud-init frames its SSH host-key banner in a row of hashes, as it always does
    let events =
        shell.feed("<14>Aug  2 10:44:31 cloud-init: ####################################\n");

    // Then it is boot output, not a shell prompt — believing otherwise makes the driver
    // type commands at a getty that is still waiting for a username
    assert_eq!(
        events,
        vec![SerialShellEvent::PreludeLine(
            "<14>Aug  2 10:44:31 cloud-init: ####################################".to_string()
        )]
    );
    assert_eq!(shell.state(), SerialShellState::Prelude);
}

#[test]
fn recognises_a_shell_prompt_once_the_login_conversation_has_started() {
    // Given a shell that has answered a getty's prompts
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");

    // When a line ending like a prompt arrives after that point
    let events = shell.feed("tddy@tddy-host:~$ ");

    // Then it is believed, because a login prompt has established that a getty is talking
    assert_eq!(events, vec![SerialShellEvent::Prompt]);
    assert_eq!(shell.state(), SerialShellState::AtPrompt);
}

#[test]
fn keeps_the_command_running_when_the_terminal_echoes_the_marker_back_unresolved() {
    // Given a running command whose command line the guest's terminal echoes back
    let mut shell = a_serial_shell_at_a_prompt();
    let marker = shell.begin_command("id -un");

    // When that echo arrives — the marker followed by a literal `$?`, not by a number
    let events = shell.feed(&format!("id -un; echo {marker}$?\n"));

    // Then it is output, not a result: a corrupted exit code must never be reported as a
    // completion the caller would read as success
    assert_eq!(
        events,
        vec![SerialShellEvent::CommandOutputLine(format!(
            "id -un; echo {marker}$?"
        ))]
    );
    assert_eq!(shell.state(), SerialShellState::ExecutingCommand);
}

#[test]
fn strips_the_carriage_returns_a_guest_terminal_frames_its_lines_with() {
    // Given a shell at a command prompt
    let mut shell = a_serial_shell_at_a_prompt();
    shell.begin_command("cat /mnt/host/greeting.txt");

    // When output arrives with the CR framing a real UART carries: a redraw prefix and a
    // `\r\r\n` line ending
    let events = shell.feed("\rhello-from-host-9p\r\r\n");

    // Then the carriage returns are framing, not content
    assert_eq!(
        events,
        vec![SerialShellEvent::CommandOutputLine(
            "hello-from-host-9p".to_string()
        )]
    );
}

#[test]
fn drops_an_osc_window_title_sequence_terminated_by_a_bell() {
    // Given a prompt preceded by the OSC sequence a bash `PS1` sets the window title with
    let raw = "\x1b]0;tddy@tddy-host: ~\x07tddy@tddy-host:~$ ";

    // When it is cleaned
    let cleaned = strip_ansi_codes(raw);

    // Then only the prompt a human would have seen survives
    assert_eq!(cleaned, "tddy@tddy-host:~$ ");
}

#[test]
fn drops_an_osc_sequence_terminated_by_a_string_terminator() {
    // Given the same title sequence closed with `ESC \` instead of a bell
    let raw = "\x1b]0;tddy@tddy-host: ~\x1b\\tddy@tddy-host:~$ ";

    // When it is cleaned
    let cleaned = strip_ansi_codes(raw);

    // Then the alternative terminator ends the sequence just as the bell does
    assert_eq!(cleaned, "tddy@tddy-host:~$ ");
}

#[test]
fn drops_the_keypad_mode_switches_a_full_screen_program_brackets_its_output_with() {
    // Given output bracketed by the `ESC =` / `ESC >` keypad-mode switches
    let raw = "\x1b=Select a boot entry\x1b>";

    // When it is cleaned
    let cleaned = strip_ansi_codes(raw);

    // Then the two-byte sequences go without swallowing the text between them
    assert_eq!(cleaned, "Select a boot entry");
}

#[test]
fn drops_a_csi_sequence_the_chunk_ended_in_the_middle_of() {
    // Given a read that landed inside an escape sequence, as any fixed-size read may
    let raw = "Started ssh.service\x1b[01;3";

    // When it is cleaned
    let cleaned = strip_ansi_codes(raw);

    // Then the unfinished sequence contributes nothing, rather than leaking `[01;3`
    assert_eq!(cleaned, "Started ssh.service");
}

#[tokio::test]
async fn refuses_to_run_a_command_before_the_guest_reaches_a_shell_prompt() {
    // Given a console whose guest is still in its boot prelude
    let mut guest = a_silent_guest();

    // When a command is attempted anyway
    let error = guest
        .console
        .run_command("id -un", Duration::from_secs(1))
        .await
        .expect_err("a command must not be accepted before a shell prompt");

    // Then it is refused outright, instead of being typed at whatever is listening
    assert_eq!(
        error.to_string(),
        "Verify command failed: serial console is not at a shell prompt (state: Prelude); \
         cannot run \"id -un\""
    );
}

#[test]
fn issues_a_distinct_exit_code_marker_for_each_command() {
    // Given a shell at a prompt
    let mut shell = a_serial_shell();
    shell.feed("tddy-host login: ");
    shell.feed("Password: ");
    shell.feed("tddy@tddy-host:~$ ");

    // When two commands are begun in turn
    let first = shell.begin_command("true");
    shell.feed(&format!("{first}0\n"));
    let second = shell.begin_command("false");

    // Then their markers differ, so a stale marker cannot end the wrong command
    assert_ne!(first, second);
}
