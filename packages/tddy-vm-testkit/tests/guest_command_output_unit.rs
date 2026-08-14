//! Unit tests for what one command run in a guest reports back.
//!
//! The distinction being pinned here is the one a CLI assertion lives or dies by: `ssh`
//! carries the guest's `stdout` and its `stderr` back as two separate streams, and a test
//! asking "what did the tool print?" must not be answered with sshd's banner, a `sudo`
//! warning, or the tool's own diagnostics mixed into the same string.

use pretty_assertions::assert_eq;
use tddy_vm::vm::VerifyResult;
use tddy_vm_testkit::guest::GuestCommandOutput;

/// An SSH round trip that produced `stdout`, `stderr` and `exit_code`.
fn an_ssh_result(stdout: &str, stderr: &str, exit_code: i32) -> VerifyResult {
    VerifyResult {
        success: exit_code == 0,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_code,
    }
}

#[test]
fn reports_the_commands_answer_without_the_diagnostics_it_wrote_alongside_it() {
    // Given a command that answered on stdout while ssh warned on stderr
    let ssh = an_ssh_result(
        "provisioned\n",
        "Warning: Permanently added '[127.0.0.1]:2271' (ED25519) to the list of known hosts.\n",
        0,
    );

    // When it is reported as the outcome of one guest command
    let output = GuestCommandOutput::from_ssh("cat /var/lib/tddy-bake-marker", ssh);

    // Then the two streams stay apart, so an assertion on the answer is an assertion on the
    // answer
    assert_eq!(output.stdout(), "provisioned\n");
    assert_eq!(
        output.stderr(),
        "Warning: Permanently added '[127.0.0.1]:2271' (ED25519) to the list of known hosts.\n"
    );
}

#[test]
fn reports_the_status_a_failing_command_exited_with() {
    // Given a command that failed
    let ssh = an_ssh_result("", "cat: /var/lib/tddy-bake-marker: No such file\n", 1);

    // When it is reported as the outcome of one guest command
    let output = GuestCommandOutput::from_ssh("cat /var/lib/tddy-bake-marker", ssh);

    // Then the exit code is the guest's own, not a success placeholder
    assert_eq!(output.exit_code(), 1);
}

#[test]
fn matches_a_one_line_answer_against_the_text_a_reader_would_have_seen() {
    // Given a command whose answer is one line, as a shell delivers it
    let output = GuestCommandOutput::from_ssh("cat /var/lib/tddy-bake-marker", {
        an_ssh_result("provisioned\n", "", 0)
    });

    // When the answer is asserted
    // Then the trailing newline is not something the test has to spell out
    output.assert_succeeded().assert_stdout_line("provisioned");
}

#[test]
#[should_panic(expected = "`cat /var/lib/tddy-bake-marker` printed \"unprovisioned\\n\"")]
fn refuses_an_answer_that_is_not_the_expected_one() {
    // Given a command that answered with something else
    let output = GuestCommandOutput::from_ssh("cat /var/lib/tddy-bake-marker", {
        an_ssh_result("unprovisioned\n", "", 0)
    });

    // When the expected answer is asserted
    // Then it fails, naming the command and what it actually printed
    output.assert_stdout_line("provisioned");
}

#[test]
#[should_panic(expected = "No such file")]
fn reports_what_a_failed_command_wrote_to_stderr_when_it_was_required_to_succeed() {
    // Given a command that failed with a diagnostic on stderr only
    let output = GuestCommandOutput::from_ssh(
        "cat /var/lib/tddy-bake-marker",
        an_ssh_result("", "cat: /var/lib/tddy-bake-marker: No such file\n", 1),
    );

    // When it is required to have succeeded
    // Then the failure carries the diagnostic — the whole reason stderr is captured
    output.assert_succeeded();
}
