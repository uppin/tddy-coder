//! Red-phase unit tests for the stdio-safe core (`tddy_core::stdio_safety`).
//!
//! `--stdio` mode dedicates a process's stdin/stdout to RPC framing (via `tddy-stdio`), which has
//! zero tolerance for stray bytes on the peer's stdout. This module is the reusable core that
//! guarantees that: it force-overrides any `LogOutput::Stdout` log destination, and provides a
//! `--daemon`-style stderr-to-file redirect that (unlike `--daemon`) must leave stdin/stdout live.
//!
//! See docs/ft/coder/1-WIP/PRD-2026-07-01-stdio-transport-for-grpc-binaries.md (Milestone 1).

use tddy_core::LogConfig;

fn log_config_with_default_output(output_yaml: &str) -> LogConfig {
    let yaml = format!(
        r#"
loggers:
  default:
    output: {output_yaml}
default:
  level: info
  logger: default
"#
    );
    serde_yaml::from_str(&yaml).expect("parse log config fixture")
}

#[test]
fn overrides_a_stdout_log_output_to_stderr_to_keep_fd1_clean_for_stdio_rpc() {
    // Given a log config that would otherwise write log lines to stdout
    let mut config = log_config_with_default_output("stdout");

    // When enforcing stdio-safe logging
    tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then the logger's output is forced to stderr
    let output = &config
        .loggers
        .get("default")
        .expect("default logger")
        .output;
    assert!(
        matches!(output, tddy_core::LogOutput::Stderr),
        "expected Stderr, got {output:?}"
    );
}

#[rstest::rstest]
#[case::stderr("stderr")]
#[case::mute("mute")]
#[case::buffer("buffer")]
fn leaves_non_stdout_log_outputs_unchanged(#[case] output_yaml: &str) {
    // Given a log config that already avoids stdout
    let mut config = log_config_with_default_output(output_yaml);
    let before = format!(
        "{:?}",
        config
            .loggers
            .get("default")
            .expect("default logger")
            .output
    );

    // When enforcing stdio-safe logging
    tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then the output is untouched
    let after = format!(
        "{:?}",
        config
            .loggers
            .get("default")
            .expect("default logger")
            .output
    );
    assert_eq!(before, after);
}

#[test]
fn leaves_a_file_log_output_unchanged() {
    // Given a log config that writes to a file
    let mut config: LogConfig = serde_yaml::from_str(
        r#"
loggers:
  default:
    output: { file: "logs/debug.log" }
default:
  level: info
  logger: default
"#,
    )
    .expect("parse log config fixture");

    // When enforcing stdio-safe logging
    tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then the file output is untouched
    let output = &config
        .loggers
        .get("default")
        .expect("default logger")
        .output;
    assert!(
        matches!(output, tddy_core::LogOutput::File(path) if path == std::path::Path::new("logs/debug.log")),
        "expected the file output to be untouched, got {output:?}"
    );
}

#[test]
fn reports_how_many_loggers_it_overrode() {
    // Given a config with two stdout loggers and one already-safe logger
    let mut config: LogConfig = serde_yaml::from_str(
        r#"
loggers:
  default:
    output: stdout
  webrtc_file:
    output: stdout
  workflow_file:
    output: stderr
default:
  level: info
  logger: default
"#,
    )
    .expect("parse log config fixture");

    // When enforcing stdio-safe logging
    let overridden = tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then exactly the two stdout loggers are counted
    assert_eq!(overridden, 2);
}

#[cfg(unix)]
#[test]
fn redirects_a_target_file_descriptor_to_a_log_file_and_closes_its_original_destination() {
    // Given a pipe whose write end stands in for a real fd (e.g. STDERR_FILENO in production —
    // a pipe lets the test observe both sides without touching the test process's own stderr)
    let mut fds = [0i32; 2];
    let pipe_ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
    assert_eq!(pipe_ret, 0, "failed to create test pipe");
    let [read_fd, write_fd] = fds;

    let log_dir = tempfile::tempdir().expect("tempdir");
    let log_path = log_dir.path().join("redirected.log");

    // When redirecting the pipe's write end to a log file
    tddy_core::stdio_safety::redirect_fd_to_file(write_fd, &log_path)
        .expect("redirect_fd_to_file should succeed");

    // Then the pipe's original write end is gone — the read end observes EOF immediately,
    // proving the target descriptor was fully replaced (dup2), not merely copied alongside it
    let mut buf = [0u8; 1];
    let n = unsafe { libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
    assert_eq!(
        n, 0,
        "expected EOF on the pipe's read end after redirection"
    );

    // And writes through the redirected descriptor land in the log file, not the old pipe
    let message = b"hello from the redirected descriptor\n";
    let written = unsafe {
        libc::write(
            write_fd,
            message.as_ptr() as *const libc::c_void,
            message.len(),
        )
    };
    assert_eq!(written as usize, message.len());

    unsafe {
        libc::close(read_fd);
        libc::close(write_fd);
    }

    let contents = std::fs::read(&log_path).expect("read redirected log file");
    assert_eq!(contents, message);
}

#[cfg(unix)]
#[test]
fn returns_an_error_when_the_log_file_path_is_unwritable() {
    // Given a target path inside a directory that does not exist
    let missing_dir_path =
        std::path::Path::new("/nonexistent-tddy-stdio-safety-test-dir/redirected.log");

    // When / Then — the target descriptor is never touched if the file can't be created
    let result =
        tddy_core::stdio_safety::redirect_fd_to_file(libc::STDOUT_FILENO, missing_dir_path);
    assert!(
        result.is_err(),
        "expected an error for an unwritable log path"
    );
}

#[test]
fn overrides_a_stdout_destination_hidden_inside_a_fan_out() {
    // Given a logger that fans out to a file and, among other destinations, stdout
    let mut config = log_config_with_default_output(r#"[stdout, { file: "logs/debug.log" }]"#);

    // When enforcing stdio-safe logging
    tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then only the stdout member moved to stderr; the file member is untouched
    let output = &config
        .loggers
        .get("default")
        .expect("default logger")
        .output;
    assert!(
        matches!(
            output,
            tddy_core::LogOutput::Many(destinations)
                if matches!(destinations.as_slice(), [
                    tddy_core::LogOutput::Stderr,
                    tddy_core::LogOutput::File(path),
                ] if path == std::path::Path::new("logs/debug.log"))
        ),
        "expected [stderr, file], got {output:?}"
    );
}

#[test]
fn collapses_a_fan_out_whose_destinations_are_identical_after_the_override() {
    // Given a logger that fans out to both stdout and stderr
    let mut config = log_config_with_default_output("[stdout, stderr]");

    // When enforcing stdio-safe logging
    tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then it is a single stderr destination, so no line is written to stderr twice
    let output = &config
        .loggers
        .get("default")
        .expect("default logger")
        .output;
    assert!(
        matches!(output, tddy_core::LogOutput::Stderr),
        "expected Stderr, got {output:?}"
    );
}

#[test]
fn counts_a_fan_out_logger_once_however_it_was_overridden() {
    // Given one fan-out logger with a stdout member and one already-safe logger
    let mut config: LogConfig = serde_yaml::from_str(
        r#"
loggers:
  default:
    output: [stdout, { file: "logs/debug.log" }]
  workflow_file:
    output: { file: "logs/workflow.log" }
default:
  level: info
  logger: default
"#,
    )
    .expect("parse log config fixture");

    // When enforcing stdio-safe logging
    let overridden = tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then the fan-out logger counts once
    assert_eq!(overridden, 1);
}

#[test]
fn leaves_a_fan_out_without_stdout_unchanged() {
    // Given a logger that fans out only to safe destinations
    let mut config = log_config_with_default_output(r#"[stderr, { file: "logs/debug.log" }]"#);
    let before = format!(
        "{:?}",
        config
            .loggers
            .get("default")
            .expect("default logger")
            .output
    );

    // When enforcing stdio-safe logging
    let overridden = tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut config);

    // Then nothing is touched and nothing is counted
    let after = format!(
        "{:?}",
        config
            .loggers
            .get("default")
            .expect("default logger")
            .output
    );
    assert_eq!(after, before);
    assert_eq!(overridden, 0);
}
