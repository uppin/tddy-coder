//! Acceptance tests for `tddy-tools set-session-context` (PRD: session variables via tddy-tools).
//!
//! Red phase: subcommand and merge semantics are not implemented yet.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use serde_json::Value;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

/// Testing Plan (2): Valid JSON updates active workflow session context and persists; downstream
/// graph resolution can read merged keys.
#[test]
fn tddy_tools_new_command_sets_session_variables_and_persists_for_next_transition() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wf = dir.path().join(".workflow");
    std::fs::create_dir_all(&wf).expect("mkdir .workflow");
    let session_id = "acceptance-sess-tool-1";
    let initial = format!(
        r#"{{"id":"{session_id}","graph_id":"tdd_full_workflow","current_task_id":"green","status_message":null,"context":{{}}}}"#
    );
    std::fs::write(wf.join(format!("{session_id}.session.json")), initial).expect("write session");

    let payload = r#"{"run_optional_step_x":true}"#;
    let mut cmd = tddy_tools_bin();
    cmd.env("TDDY_SESSION_DIR", dir.path());
    cmd.env("TDDY_WORKFLOW_SESSION_ID", session_id);
    cmd.args(["set-session-context", "--data", payload]);
    cmd.assert().success();

    let path = wf.join(format!("{session_id}.session.json"));
    let json = std::fs::read_to_string(&path).expect("read session after tool");
    let v: Value = serde_json::from_str(&json).expect("session json");
    let run = v
        .pointer("/context/run_optional_step_x")
        .and_then(|x| x.as_bool());
    assert_eq!(
        run,
        Some(true),
        "merged session context must contain run_optional_step_x=true; got {v}"
    );
}

/// Testing Plan (4): Merging new keys must not drop existing context entries (resume / reload).
#[test]
fn resume_session_preserves_conditional_context_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wf = dir.path().join(".workflow");
    std::fs::create_dir_all(&wf).expect("mkdir .workflow");
    let session_id = "acceptance-sess-resume-1";
    let initial = format!(
        r#"{{"id":"{session_id}","graph_id":"tdd_full_workflow","current_task_id":"green","status_message":null,"context":{{"existing_branch_flag":true}}}}"#
    );
    std::fs::write(wf.join(format!("{session_id}.session.json")), initial).expect("write session");

    let mut cmd = tddy_tools_bin();
    cmd.env("TDDY_SESSION_DIR", dir.path());
    cmd.env("TDDY_WORKFLOW_SESSION_ID", session_id);
    cmd.args([
        "set-session-context",
        "--data",
        r#"{"run_optional_step_x":false}"#,
    ]);
    cmd.assert().success();

    let json =
        std::fs::read_to_string(wf.join(format!("{session_id}.session.json"))).expect("read");
    let v: Value = serde_json::from_str(&json).expect("session json");
    assert_eq!(
        v.pointer("/context/existing_branch_flag")
            .and_then(|x| x.as_bool()),
        Some(true),
        "pre-existing context keys must survive merge; got {v}"
    );
    assert_eq!(
        v.pointer("/context/run_optional_step_x")
            .and_then(|x| x.as_bool()),
        Some(false),
        "new key must be present after merge; got {v}"
    );
}

/// Testing Plan (5): Non-object JSON must fail with non-zero exit and a clear validation message.
#[test]
fn invalid_session_var_payload_rejected_with_clear_error() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["set-session-context", "--data", "[]"]);
    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("object")
            || combined.contains("Object")
            || combined.contains("JSON object")
            || combined.contains("expected"),
        "invalid payload must mention object/top-level shape; got: {combined}"
    );
}
