use super::*;
use std::path::Path;

/// The cursor model probe must hand `tddy-tools` the **resolved absolute** cursor binary via
/// `--cursor-cli-path`, exactly as the PTY spawn does. Otherwise `tddy-tools` builds a
/// `CursorBackend` with the bare name `agent`, and the impersonated child's PATH lookup fails
/// with "binary not found: agent" — the reported `[failed_precondition] model probe failed`.
#[test]
fn cursor_probe_args_carry_the_resolved_cursor_binary_path() {
    // Given — the daemon has resolved the real cursor binary to an absolute path
    let resolved = Path::new("/home/dev/.local/bin/agent");

    // When — building the args for the `tddy-tools list-models` cursor probe
    let args = list_models_probe_args("cursor", Some(resolved));

    // Then — the resolved absolute path is passed through to tddy-tools
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "cursor".to_string(),
            "--cursor-cli-path".to_string(),
            "/home/dev/.local/bin/agent".to_string(),
        ]
    );
}

/// A non-cursor agent's probe carries no cursor override — `--cursor-cli-path` is cursor-only.
#[test]
fn a_non_cursor_probe_omits_the_cursor_cli_path_flag() {
    // When — probing a claude agent, even if a cursor path happens to be resolvable
    let args = list_models_probe_args("claude", Some(Path::new("/home/dev/.local/bin/agent")));

    // Then — only the agent is passed; no cursor override leaks in
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "claude".to_string(),
        ]
    );
}

/// The OpenCode model probe hands `tddy-tools` the binary the daemon resolved for the harness
/// (`cli_harness_binary`: the environment, then `cli_harnesses.opencode-cli.binary_path`, then
/// `opencode`), so the impersonated child execs it without a PATH lookup that lacks its install dir.
#[test]
fn an_opencode_cli_probe_carries_the_resolved_opencode_binary_path() {
    // Given
    let resolved = Path::new("/home/dev/.opencode/bin/opencode");

    // When
    let args = list_models_probe_args("opencode-cli", Some(resolved));

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode-cli".to_string(),
            "--opencode-cli-path".to_string(),
            "/home/dev/.opencode/bin/opencode".to_string(),
        ]
    );
}

/// Each binary override is its own agent's: Cursor's path never reaches the OpenCode probe.
#[test]
fn a_cursor_probe_carries_no_opencode_flag_and_an_opencode_probe_no_cursor_flag() {
    // When
    let cursor = list_models_probe_args("cursor", Some(Path::new("/bin/agent")));
    let opencode = list_models_probe_args("opencode-cli", Some(Path::new("/bin/opencode")));

    // Then
    assert!(
        !cursor.contains(&"--opencode-cli-path".to_string()),
        "{cursor:?}"
    );
    assert!(
        !opencode.contains(&"--cursor-cli-path".to_string()),
        "{opencode:?}"
    );
}

/// A bare-name resolution (`opencode` on PATH) is not forwarded, as for Cursor: only an absolute
/// path pins the child to a fully-qualified binary.
#[test]
fn an_opencode_cli_probe_without_a_resolved_path_omits_the_flag() {
    // When
    let args = list_models_probe_args("opencode-cli", None);

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode-cli".to_string(),
        ]
    );
}
