//! Red-phase granular tests for `tddy_core::session_actions`.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use tddy_core::session_actions::{
    ensure_action_architecture, list_action_summaries, parse_action_manifest_yaml,
    parse_test_summary_from_process_output, resolve_allowlisted_path, run_manifest_command,
    validate_action_arguments_json, ActionManifest, TestSummary,
};

fn unique_temp_session_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "session_actions_red_{label}_{}_{}",
        std::process::id(),
        nanos
    ))
}

fn write_fixture_action(session: &std::path::Path, fname: &str, body: &str) {
    let dir = session.join("actions");
    fs::create_dir_all(&dir).expect("mkdir actions");
    fs::write(dir.join(fname), body).expect("write manifest");
}

/// Contract: summaries are emitted in **strictly ascending** lexicographic `id` order (stable UX).
#[test]
fn list_action_summaries_must_be_sorted_ascending_by_id() {
    let dir = unique_temp_session_dir("sort");
    let session = dir.as_path();
    fs::create_dir_all(session).expect("mkdir session root");
    write_fixture_action(
        session,
        "zeta.yaml",
        "version: 1\nid: zeta\nsummary: Z\narchitecture: native\ncommand: ['/bin/true']\n",
    );
    write_fixture_action(
        session,
        "alpha.yaml",
        "version: 1\nid: alpha\nsummary: A\narchitecture: native\ncommand: ['/bin/true']\n",
    );
    let list = list_action_summaries(session).expect("discovery");
    let ids: Vec<&str> = list.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["alpha", "zeta"],
        "expected ascending order; refactor list_action_summaries when turning Green"
    );
}

/// Unknown manifest keys must be rejected once serde `deny_unknown_fields` is enabled (manifest version contract).
#[test]
fn manifest_must_reject_unknown_top_level_yaml_keys() {
    let yaml = r#"
version: 1
id: probe
summary: S
architecture: native
command: []
extra_unknown_field_must_fail_parse: true
"#;
    assert!(
        parse_action_manifest_yaml(yaml).is_err(),
        "YAML with unknown keys must error after deny_unknown_fields; skeleton still accepts extras"
    );
}

#[test]
fn cargo_style_test_totals_must_parse_into_test_summary() {
    let stdout = concat!(
        "running 0 tests\n\n",
        "test result: ok. 12 passed; 3 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
    );
    let got = parse_test_summary_from_process_output(stdout).expect("parse test totals when Green");
    assert_eq!(
        got,
        TestSummary {
            passed: 12,
            failed: 3,
            skipped: 4,
        }
    );
}

#[test]
fn native_architecture_guard_must_allow_native_label() {
    ensure_action_architecture("native").expect("`native` should match runtime host architecture");
}

/// Safe relative binding under the session directory must resolve once path sandbox is implemented.
#[test]
fn resolve_allowlisted_path_must_accept_destination_inside_session_tree() {
    let dir = unique_temp_session_dir("paths");
    let session = dir.as_path();
    fs::create_dir_all(session.join("out")).expect("mkdir");
    let got = resolve_allowlisted_path(session, None, "out/artifact.txt", "output_binding");
    assert!(
        got.is_ok(),
        "paths inside session must resolve when sandbox is Green; got {got:?}"
    );
}

/// Full JSON Schema semantics require rejecting wrong primitive types for typed properties.
#[test]
fn validate_arguments_must_reject_integer_for_string_property() {
    let schema = json!({
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"],
        "additionalProperties": false
    });
    let args = json!({ "name": 42 });
    let err =
        validate_action_arguments_json(&Some(schema), &args).expect_err("type mismatch must fail");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("string") || msg.contains("type"),
        "expected type-aware validation message; got {err}"
    );
}

/// Invocation executor stub must eventually return a structured JSON record (stdout/stderr/exit_code).
#[test]
fn run_manifest_command_must_return_ok_with_invocation_record_when_green() {
    let session = unique_temp_session_dir("invoke");
    fs::create_dir_all(&session).expect("mkdir session");
    let manifest: ActionManifest = parse_action_manifest_yaml(
        r#"
version: 1
id: noop
summary: N
architecture: native
command: ["/bin/true"]
"#,
    )
    .expect("fixture manifest");
    let out = run_manifest_command(session.as_path(), None, &manifest, &json!({}));
    assert!(
        out.is_ok(),
        "Green phase runs process and returns JSON record; Red skeleton returns Err: {out:?}"
    );
}
