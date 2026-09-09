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
