use super::*;
use std::path::Path;

/// The model probe must hand `tddy-tools` the **resolved absolute** binary via the generic
/// `--binary-path`, exactly as the PTY spawn resolves it. Otherwise `tddy-tools` runs the
/// listing command through the bare binary name, and the impersonated child's PATH lookup
/// fails with "binary not found" — the reported `[failed_precondition] model probe failed`.
/// The flag is the caller's resolved path as plain data; no agent has a flag of its own.
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
            "--binary-path".to_string(),
            "/home/dev/.local/bin/agent".to_string(),
        ]
    );
}

/// The flag names no agent: a claude probe resolved to the same path carries it identically,
/// and no per-agent flag (`--cursor-cli-path` and kin) exists to leak into another probe.
#[test]
fn a_non_cursor_probe_omits_the_cursor_cli_path_flag() {
    // When — probing a claude agent resolved to an absolute path
    let args = list_models_probe_args("claude", Some(Path::new("/home/dev/.local/bin/agent")));

    // Then — the generic flag carries the path; no cursor-named flag appears
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "claude".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.local/bin/agent".to_string(),
        ]
    );
    assert!(!args.contains(&"--cursor-cli-path".to_string()), "{args:?}");
}

/// The OpenCode model probe hands `tddy-tools` the binary the daemon resolved for the harness
/// (the host's resolution of the harness spec's binary), so the impersonated child execs it without
/// a PATH lookup that lacks its install dir.
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
            "--binary-path".to_string(),
            "/home/dev/.opencode/bin/opencode".to_string(),
        ]
    );
}

/// Each probe carries its own resolved path under the one generic flag.
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
    assert!(cursor.contains(&"/bin/agent".to_string()), "{cursor:?}");
    assert!(
        opencode.contains(&"/bin/opencode".to_string()),
        "{opencode:?}"
    );
}

/// A bare-name resolution (`opencode` on PATH) is not forwarded: only an absolute
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

/// The OpenCode ACP agent's probe hands `tddy-tools` the binary the daemon resolved for the
/// `opencode-cli` harness, under the same flag: `tddy-tools` runs it as `opencode acp`.
#[test]
fn an_opencode_acp_probe_carries_the_resolved_opencode_binary_path() {
    // Given
    let resolved = Path::new("/home/dev/.opencode/bin/opencode");

    // When
    let args = list_models_probe_args("opencode", Some(resolved));

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.opencode/bin/opencode".to_string(),
        ]
    );
}
