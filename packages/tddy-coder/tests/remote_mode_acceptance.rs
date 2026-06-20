//! Acceptance tests: tddy-coder `--remote` mode (PRD: docs/ft/daemon/remote-codebase-mode.md).

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use tddy_workflow_recipes::permissions::remote_codebase_allowlist;

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

/// AC24 (structural): `remote_codebase_allowlist()` must not contain native filesystem or shell tools.
#[test]
fn remote_allowlist_excludes_all_native_fs_and_shell_tools() {
    // When
    let allowlist = remote_codebase_allowlist();

    // Then
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
    for entry in &allowlist {
        assert!(
            !entry.starts_with("Bash("),
            "remote allowlist must not contain bare Bash() patterns; got: {:?}",
            entry
        );
    }
}

/// AC25 (structural): `remote_codebase_allowlist()` includes `AskUserQuestion`.
#[test]
fn remote_allowlist_includes_ask_user_question() {
    // When
    let allowlist = remote_codebase_allowlist();

    // Then
    assert!(
        allowlist.contains(&"AskUserQuestion".to_string()),
        "remote allowlist must include AskUserQuestion; got: {:?}",
        allowlist
    );
}

/// AC25 (CLI): `tddy-coder --help` must include `--remote` in its output.
#[test]
fn remote_flag_is_accepted_by_cli_parser() {
    // When
    let output = tddy_coder_bin()
        .arg("--help")
        .output()
        .expect("tddy-coder --help must not crash");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then
    assert!(
        stdout.contains("--remote"),
        "--remote flag must appear in --help output; got: {}",
        stdout
    );
}

/// AC25 (CLI): `tddy-coder --remote` without a recipe exits non-zero with a clear error.
#[test]
fn remote_flag_without_recipe_yields_clear_error() {
    // Given
    let nonexistent_socket = "/tmp/tddy-nonexistent-test-socket";

    // When
    let output = tddy_coder_bin()
        .env("TDDY_SOCKET", nonexistent_socket)
        .args(["--remote", "--goal", "plan"])
        .output()
        .expect("tddy-coder --remote --goal plan must not crash");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");

    // Then
    assert!(
        !combined.contains("panicked at"),
        "tddy-coder --remote must not panic; got: {}",
        combined
    );
    assert!(
        !output.status.success(),
        "tddy-coder --remote without free-prompting recipe must exit non-zero"
    );
}

/// AC24 (unit): `REMOTE_APPENDIX` must state the codebase is remote and reference `mcp__tddy-tools__`.
#[test]
fn remote_appendix_wording_states_remote_codebase_and_required_tools() {
    // When
    let appendix = tddy_coder::remote::REMOTE_APPENDIX;
    let lowercase = appendix.to_lowercase();

    // Then
    assert!(
        lowercase.contains("remote"),
        "REMOTE_APPENDIX must contain the word 'remote'; got: {:?}",
        appendix
    );
    assert!(
        appendix.contains("mcp__tddy-tools__"),
        "REMOTE_APPENDIX must reference 'mcp__tddy-tools__' tool prefix; got: {:?}",
        appendix
    );
    assert!(
        lowercase.contains("must use"),
        "REMOTE_APPENDIX must say 'must use'; got: {:?}",
        appendix
    );
}
