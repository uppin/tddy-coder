//! Acceptance tests: Session **Actions** CLI (`list-actions`, `invoke-action`) per PRD Testing Plan.
//!
//! Red phase: subcommands and action manifest plumbing are not implemented yet. Each test asserts
//! the contract from the PRD (JSON shapes, exit codes, security) so production work can turn them green.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use tddy_core::changeset::Changeset;
use tddy_core::write_changeset;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

/// Minimal action manifest shape (YAML) expected by `list-actions` / `invoke-action` loaders.
/// Field names align with the Session Actions PRD; serde types will match this in implementation.
fn write_sample_action(session: &Path, filename: &str, body: &str) {
    let dir = session.join("actions");
    fs::create_dir_all(&dir).expect("mkdir actions");
    fs::write(dir.join(filename), body).expect("write action yaml");
}

/// `list-actions` stdout must be JSON: `{ "actions": [ { "id", "summary", "has_input_schema", "has_output_schema" }, ... ] }`
/// with stable sorting by `id`.
#[test]
fn list_actions_discovers_session_yaml_manifests() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = dir.path();
    write_sample_action(
        session,
        "alpha.yaml",
        r#"
version: 1
id: alpha
summary: First fixture action
architecture: native
command: ["/bin/true"]
input_schema:
  type: object
  additionalProperties: false
"#,
    );
    write_sample_action(
        session,
        "beta.yaml",
        r#"
version: 1
id: beta
summary: Second fixture action
architecture: native
command: ["/bin/true"]
output_schema:
  type: object
  properties:
    tests_run:
      type: integer
"#,
    );

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "list-actions",
        "--session-dir",
        session.to_str().expect("utf8"),
    ]);
    let out = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    let v: Value = serde_json::from_str(stdout.trim()).expect("list-actions stdout must be JSON");
    let actions = v
        .get("actions")
        .and_then(|a| a.as_array())
        .expect("response must contain \"actions\" array");
    assert_eq!(actions.len(), 2, "expected two manifests; got {v}");
    let id0 = actions[0]
        .get("id")
        .and_then(|x| x.as_str())
        .expect("action id");
    let id1 = actions[1]
        .get("id")
        .and_then(|x| x.as_str())
        .expect("action id");
    assert!(
        id0 <= id1,
        "actions must be sorted by id for stable output; got {id0}, {id1}"
    );
    let alpha = actions
        .iter()
        .find(|a| a.get("id").and_then(|i| i.as_str()) == Some("alpha"))
        .expect("alpha action");
    assert_eq!(
        alpha.get("summary").and_then(|s| s.as_str()),
        Some("First fixture action")
    );
    assert_eq!(
        alpha.get("has_input_schema").and_then(|b| b.as_bool()),
        Some(true)
    );
    assert_eq!(
        alpha.get("has_output_schema").and_then(|b| b.as_bool()),
        Some(false)
    );
    let beta = actions
        .iter()
        .find(|a| a.get("id").and_then(|i| i.as_str()) == Some("beta"))
        .expect("beta action");
    assert_eq!(
        beta.get("has_output_schema").and_then(|b| b.as_bool()),
        Some(true)
    );
}

/// Invalid JSON arguments must fail validation (non-zero exit), surface a schema-related error,
/// and must not run the action command (no marker file).
#[test]
fn invoke_action_validates_json_args_before_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = dir.path();
    let marker = session.join("must-not-be-created");
    let sh = format!(
        r#"#!/bin/sh
touch "{}"
exit 0
"#,
        marker.display()
    );
    let stub = session.join("bad-args-stub.sh");
    fs::write(&stub, &sh).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).unwrap();
    }

    write_sample_action(
        session,
        "needs-name.yaml",
        &format!(
            r#"
version: 1
id: needs-name
summary: Requires string name
architecture: native
command: ["{}"]
input_schema:
  type: object
  required: [name]
  properties:
    name:
      type: string
  additionalProperties: false
"#,
            stub.display()
        ),
    );

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "invoke-action",
        "--session-dir",
        session.to_str().expect("utf8"),
        "--action",
        "needs-name",
        "--data",
        "{}",
    ]);
    let assert = cmd.assert();
    assert!(
        !assert.get_output().status.success(),
        "invalid args must yield non-zero exit"
    );
    assert_eq!(
        assert.get_output().status.code(),
        Some(3),
        "invalid args should yield exit code 3 (validation) once invoke-action distinguishes codes"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&assert.get_output().stdout),
        String::from_utf8_lossy(&assert.get_output().stderr)
    );
    assert!(
        combined.to_lowercase().contains("schema")
            || combined.to_lowercase().contains("validat"),
        "expected schema/validation error in output; got: {combined}"
    );
    assert!(
        !marker.exists(),
        "command must not run when validation fails (marker file missing); stub was executed"
    );
}

#[test]
fn invoke_action_returns_exit_code_and_stdout_stderr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = dir.path();
    let stub_body = r#"#!/bin/sh
printf '%s' 'HELLO_OUT'
printf '%s' 'HELLO_ERR' 1>&2
exit 42
"#;
    let stub = session.join("stub-42.sh");
    fs::write(&stub, stub_body).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).unwrap();
    }

    write_sample_action(
        session,
        "echo-stub.yaml",
        &format!(
            r#"
version: 1
id: echo-stub
summary: Stub with stdout/stderr and exit 42
architecture: native
command: ["{}"]
input_schema:
  type: object
  additionalProperties: false
"#,
            stub.display()
        ),
    );

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "invoke-action",
        "--session-dir",
        session.to_str().expect("utf8"),
        "--action",
        "echo-stub",
        "--data",
        "{}",
    ]);
    let out = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    let v: Value =
        serde_json::from_str(stdout.trim()).expect("invoke-action stdout must be JSON");
    assert_eq!(v.get("exit_code").and_then(|c| c.as_i64()), Some(42));
    assert_eq!(
        v.get("stdout").and_then(|s| s.as_str()),
        Some("HELLO_OUT")
    );
    assert_eq!(
        v.get("stderr").and_then(|s| s.as_str()),
        Some("HELLO_ERR")
    );
}

/// Action with `result_kind: test_summary` (or equivalent) parses stub output like `cargo test` totals.
#[test]
fn invoke_action_test_summary_includes_pass_fail_skip_totals() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = dir.path();
    let stub_body = r#"#!/bin/sh
cat <<'EOS'
running 0 tests

test result: ok. 12 passed; 3 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
EOS
exit 1
"#;
    let stub = session.join("fake-cargo-test.sh");
    fs::write(&stub, stub_body).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).unwrap();
    }

    write_sample_action(
        session,
        "run-tests.yaml",
        &format!(
            r#"
version: 1
id: run-tests
summary: Parses cargo-style test summary
architecture: native
command: ["{}"]
result_kind: test_summary
input_schema:
  type: object
  additionalProperties: false
"#,
            stub.display()
        ),
    );

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "invoke-action",
        "--session-dir",
        session.to_str().expect("utf8"),
        "--action",
        "run-tests",
        "--data",
        "{}",
    ]);
    let out = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    let v: Value =
        serde_json::from_str(stdout.trim()).expect("invoke-action stdout must be JSON");
    let summary = v
        .get("summary")
        .expect("structured record must include summary for test_summary actions");
    assert_eq!(summary.get("passed").and_then(|x| x.as_u64()), Some(12));
    assert_eq!(summary.get("failed").and_then(|x| x.as_u64()), Some(3));
    assert_eq!(summary.get("skipped").and_then(|x| x.as_u64()), Some(4));
}

/// Path arguments or bindings outside the session tree / declared repo must fail closed (no command run).
#[test]
fn invoke_action_rejects_disallowed_path_patterns() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = dir.path();
    let repo = tempfile::tempdir().expect("repo temp");
    let mut cs = Changeset::default();
    cs.repo_path = Some(repo.path().to_string_lossy().to_string());
    write_changeset(session, &cs).expect("seed changeset");

    // Absolute path outside the session tree; resolver must reject before running the command.
    let breakout = PathBuf::from("/tmp/tddy-actions-breakout-marker");
    let stub = session.join("noop.sh");
    fs::write(&stub, "#!/bin/sh\nexit 0\n").expect("stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).unwrap();
    }

    write_sample_action(
        session,
        "unsafe-bind.yaml",
        &format!(
            r#"
version: 1
id: unsafe-bind
summary: Attempts disallowed output path via args
architecture: native
command: ["{}"]
output_path_arg: dest
input_schema:
  type: object
  required: [dest]
  properties:
    dest:
      type: string
  additionalProperties: false
"#,
            stub.display()
        ),
    );

    let payload = format!(
        r#"{{"dest":"{}"}}"#,
        breakout.to_string_lossy().replace('\\', "\\\\")
    );

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "invoke-action",
        "--session-dir",
        session.to_str().expect("utf8"),
        "--action",
        "unsafe-bind",
        "--data",
        &payload,
    ]);
    let assert = cmd.assert();
    assert!(
        !assert.get_output().status.success(),
        "path traversal / escape must be rejected with non-zero exit"
    );
    assert_eq!(
        assert.get_output().status.code(),
        Some(3),
        "path binding violations should yield exit code 3 once classified"
    );
    let msg = format!(
        "{}{}",
        String::from_utf8_lossy(&assert.get_output().stdout),
        String::from_utf8_lossy(&assert.get_output().stderr)
    );
    assert!(
        msg.to_lowercase().contains("path")
            || msg.contains("travers")
            || msg.to_lowercase().contains("invalid"),
        "error must indicate path rejection; got: {msg}"
    );
    assert!(
        !breakout.exists(),
        "marker path must not be created; invocation must fail closed"
    );
}
