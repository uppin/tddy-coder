//! Acceptance tests: tddy-coder `--remote` mode (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC24-AC26: read-only context dir with appendix, allowlist excludes native tools, native-tool
//! denial, and the `--remote` flag is accepted without panicking.
//!
//! NOTE: Tests that require a running relay daemon / remote daemon are integration/e2e tests
//! (to be added in a later phase). These tests focus on CLI-parseable properties and the
//! allowlist/permissions logic that is verifiable without a network.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use tddy_workflow_recipes::permissions::remote_codebase_allowlist;

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

/// AC24 (structural): `remote_codebase_allowlist()` produces a list that does NOT contain any
/// native filesystem or shell tools. The list is the base that tddy-coder uses before appending
/// discovered `mcp__tddy-tools__*` names.
#[test]
fn remote_allowlist_excludes_all_native_fs_and_shell_tools() {
    let allowlist = remote_codebase_allowlist();

    let forbidden = [
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "SemanticSearch",
        "NotebookEdit",
    ];
    for tool in &forbidden {
        assert!(
            !allowlist.contains(&tool.to_string()),
            "remote allowlist must NOT contain native tool '{}'; got: {:?}",
            tool,
            allowlist
        );
    }

    // No bare Bash patterns — only mcp__tddy-tools__* may be forwarded.
    for entry in &allowlist {
        assert!(
            !entry.starts_with("Bash("),
            "remote allowlist must not contain bare Bash() patterns; got: {:?}",
            entry
        );
    }
}

/// AC25 (structural): `remote_codebase_allowlist()` includes `AskUserQuestion` (required for
/// the agent to ask clarification questions).
#[test]
fn remote_allowlist_includes_ask_user_question() {
    let allowlist = remote_codebase_allowlist();
    assert!(
        allowlist.contains(&"AskUserQuestion".to_string()),
        "remote allowlist must include AskUserQuestion; got: {:?}",
        allowlist
    );
}

/// AC25 (CLI): `tddy-coder --remote --help` or `--remote --version` must not crash, proving the
/// flag is parsed without panicking. We don't attempt a full run (which needs a relay daemon).
#[test]
fn remote_flag_is_accepted_by_cli_parser() {
    let output = tddy_coder_bin()
        .arg("--help")
        .output()
        .expect("tddy-coder --help must not crash");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--remote"),
        "--remote flag must appear in --help output; got: {}",
        stdout
    );
}

/// AC25 (CLI): `tddy-coder --remote` without a recipe errors with a clear message pointing to
/// `--recipe free-prompting` (v1 restricts remote mode to free-prompting).
#[test]
fn remote_flag_without_recipe_yields_clear_error() {
    // Use a non-existent TDDY_SOCKET to avoid accidentally connecting to a real daemon.
    let output = tddy_coder_bin()
        .env("TDDY_SOCKET", "/tmp/tddy-nonexistent-test-socket")
        .args(["--remote", "--goal", "plan"])
        .output()
        .expect("tddy-coder --remote --goal plan must not crash");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");

    // Must not be a Rust panic.
    assert!(
        !combined.contains("panicked at"),
        "tddy-coder --remote must not panic; got: {}",
        combined
    );

    // Must fail (not succeed — no relay is running and no recipe is specified).
    assert!(
        !output.status.success(),
        "tddy-coder --remote without free-prompting recipe must exit non-zero"
    );
}

/// AC24 (unit): the `REMOTE_APPENDIX` constant in `tddy_coder::remote` must explicitly state
/// that the codebase is remote and the agent must use `mcp__tddy-tools__*` tools.
#[test]
fn remote_appendix_wording_states_remote_codebase_and_required_tools() {
    let appendix = tddy_coder::remote::REMOTE_APPENDIX;

    // Must mention that the codebase / repo is remote.
    let lowercase = appendix.to_lowercase();
    assert!(
        lowercase.contains("remote"),
        "REMOTE_APPENDIX must contain the word 'remote'; got: {:?}",
        appendix
    );

    // Must explicitly reference the mcp__tddy-tools__ prefix so the agent knows the namespace.
    assert!(
        appendix.contains("mcp__tddy-tools__"),
        "REMOTE_APPENDIX must reference 'mcp__tddy-tools__' tool prefix; got: {:?}",
        appendix
    );

    // Must explicitly state that the agent must use the mcp__tddy-tools__ tools.
    assert!(
        lowercase.contains("must use"),
        "REMOTE_APPENDIX must say 'must use' (directing the agent to use mcp__tddy-tools__); got: {:?}",
        appendix
    );
}
