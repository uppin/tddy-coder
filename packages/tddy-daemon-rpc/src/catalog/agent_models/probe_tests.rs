use super::*;
use std::path::Path;

/// The `<tddy home>/tools` a probe is handed, where it does not matter to the test.
const A_TOOLS_DIR: &str = "/var/lib/tddy/tools";

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
    let args = list_models_probe_args("cursor", Some(resolved), Path::new(A_TOOLS_DIR));

    // Then — the resolved absolute path is passed through to tddy-tools
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "cursor".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.local/bin/agent".to_string(),
            "--tools-dir".to_string(),
            "/var/lib/tddy/tools".to_string(),
        ]
    );
}

/// The flag names no agent: a claude probe resolved to the same path carries it identically,
/// and no per-agent flag (`--cursor-cli-path` and kin) exists to leak into another probe.
#[test]
fn a_non_cursor_probe_omits_the_cursor_cli_path_flag() {
    // When — probing a claude agent resolved to an absolute path
    let args = list_models_probe_args(
        "claude",
        Some(Path::new("/home/dev/.local/bin/agent")),
        Path::new(A_TOOLS_DIR),
    );

    // Then — the generic flag carries the path; no cursor-named flag appears
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "claude".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.local/bin/agent".to_string(),
            "--tools-dir".to_string(),
            "/var/lib/tddy/tools".to_string(),
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
    let args = list_models_probe_args("opencode-cli", Some(resolved), Path::new(A_TOOLS_DIR));

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode-cli".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.opencode/bin/opencode".to_string(),
            "--tools-dir".to_string(),
            "/var/lib/tddy/tools".to_string(),
        ]
    );
}

/// Each probe carries its own resolved path under the one generic flag.
#[test]
fn a_cursor_probe_carries_no_opencode_flag_and_an_opencode_probe_no_cursor_flag() {
    // When
    let cursor = list_models_probe_args(
        "cursor",
        Some(Path::new("/bin/agent")),
        Path::new(A_TOOLS_DIR),
    );
    let opencode = list_models_probe_args(
        "opencode-cli",
        Some(Path::new("/bin/opencode")),
        Path::new(A_TOOLS_DIR),
    );

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
    let args = list_models_probe_args("opencode-cli", None, Path::new(A_TOOLS_DIR));

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode-cli".to_string(),
            "--tools-dir".to_string(),
            "/var/lib/tddy/tools".to_string(),
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
    let args = list_models_probe_args("opencode", Some(resolved), Path::new(A_TOOLS_DIR));

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.opencode/bin/opencode".to_string(),
            "--tools-dir".to_string(),
            "/var/lib/tddy/tools".to_string(),
        ]
    );
}

/// The probe lists from the host's registry, not the built-in specs: it hands `tddy-tools` the
/// host's `<tddy home>/tools`, whose operator overrides `list-models --tools-dir` applies.
#[test]
fn the_catalog_probe_lists_the_hosts_override_not_the_builtins() {
    // Given a host keeping its tool overrides in its tddy home
    let tools_dir = Path::new("/home/dev/.tddy/tools");

    // When building the args for a curated agent's probe, which needs no binary
    let args = list_models_probe_args("claude-cli", None, tools_dir);

    // Then the host's tools directory is handed over
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "claude-cli".to_string(),
            "--tools-dir".to_string(),
            "/home/dev/.tddy/tools".to_string(),
        ]
    );
}

/// A probe that also carries a resolved binary hands over both, the tools directory last.
#[test]
fn a_listed_agents_probe_carries_its_binary_and_the_hosts_tools_dir() {
    // When
    let args = list_models_probe_args(
        "opencode-cli",
        Some(Path::new("/home/dev/.opencode/bin/opencode")),
        Path::new("/home/dev/.tddy/tools"),
    );

    // Then
    assert_eq!(
        args,
        vec![
            "list-models".to_string(),
            "--agent".to_string(),
            "opencode-cli".to_string(),
            "--binary-path".to_string(),
            "/home/dev/.opencode/bin/opencode".to_string(),
            "--tools-dir".to_string(),
            "/home/dev/.tddy/tools".to_string(),
        ]
    );
}
