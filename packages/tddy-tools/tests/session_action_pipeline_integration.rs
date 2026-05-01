//! Acceptance tests (PRD Testing Plan): session action pipeline **integration** — input mapper
//! envelope round-trip, mapper failure surfaces as structured error, output transform + JSON Schema,
//! stdout/stderr capture default and override.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::json;
use tddy_core::session_action_pipeline::{
    run_input_mapper_for_envelope, run_output_transform_and_validate, run_primary_action_with_capture_paths,
    SessionActionPipelineError,
};

fn session_root(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tddy_session_action_pipe_{}_{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir session root");
    dir
}

fn sample_channels(session: &Path) -> HashMap<String, PathBuf> {
    let cap = session.join("capture");
    let _ = fs::create_dir_all(&cap);
    let mut m = HashMap::new();
    m.insert("stdout".into(), cap.join("stdout.raw.txt"));
    m.insert("stderr".into(), cap.join("stderr.raw.txt"));
    m.insert("logs".into(), session.join("logs"));
    m
}

/// PRD: input mapper emits a single JSON document with validated `args`/`env` before primary spawn.
#[test]
fn session_action_input_mapper_emits_valid_args_env_json() {
    let session = session_root("mapper_ok");
    let mapper = session.join("mapper.sh");
    fs::write(
        &mapper,
        r#"#!/bin/sh
# Consumes stdin (schema-valid action input); emits canonical invocation envelope on stdout.
cat >/dev/null
printf '%s\n' '{"args":["/bin/sh","-c","echo mapper_ok"],"env":{"MAPPER":"1"}}'
"#,
    )
    .expect("write mapper");
    fs::set_permissions(&mapper, fs::Permissions::from_mode(0o755)).expect("chmod mapper");

    let channels = sample_channels(&session);
    let input = json!({"tool": "probe", "n": 1});
    let mapper_cmd = vec![
        mapper.to_string_lossy().into_owned(),
    ];
    let got = run_input_mapper_for_envelope(&mapper_cmd, &input, &channels);
    assert!(
        got.is_ok(),
        "mapper stage must run helper, parse stdout JSON, validate envelope: {:?}",
        got
    );
    let (args, env) = got.unwrap();
    assert_eq!(
        args,
        vec!["/bin/sh".to_string(), "-c".to_string(), "echo mapper_ok".to_string()]
    );
    assert_eq!(env.get("MAPPER").map(String::as_str), Some("1"));
}

/// PRD: mapper failure (non-zero exit) surfaces as a structured workflow/tool error, not a generic panic.
#[test]
fn session_action_input_mapper_failure_surfaces_structured_error() {
    let session = session_root("mapper_fail");
    let channels = sample_channels(&session);
    let input = json!({});
    let err = run_input_mapper_for_envelope(
        &["/bin/false".into()],
        &input,
        &channels,
    )
    .unwrap_err();
    assert!(
        matches!(err, SessionActionPipelineError::InputMapperFailed { .. }),
        "expected InputMapperFailed for non-zero mapper exit; got {err:?}"
    );
}

/// PRD: output transform stdout must be JSON that validates against the action's declared output schema.
#[test]
fn session_action_output_transform_validates_against_output_schema() {
    let session = session_root("transform");
    let channels = sample_channels(&session);

    let output_schema = json!({
        "type": "object",
        "required": ["status"],
        "properties": { "status": { "type": "string" } },
        "additionalProperties": false
    });

    // Valid transform: print one JSON object on stdout; invalid cases must be rejected when Green.
    let transform_cmd = vec![
        "/bin/sh".into(),
        "-c".into(),
        "echo '{\"status\":\"ok\"}'".into(),
    ];

    let value = run_output_transform_and_validate(&transform_cmd, &channels, &output_schema)
        .expect("transform must succeed and validate against output schema");
    assert_eq!(value, json!({"status": "ok"}));

    let bad_transform = vec!["/bin/sh".into(), "-c".into(), "echo '{\"status\": 7}'".into()];
    let bad = run_output_transform_and_validate(&bad_transform, &channels, &output_schema)
        .expect_err("wrong JSON type must fail schema validation");
    assert!(
        matches!(bad, SessionActionPipelineError::TransformOutputSchema(_)),
        "expected TransformOutputSchema; got {bad:?}"
    );
}

/// PRD: invocation may redirect stdout/stderr; defaults under session work; channel id → path mapping is usable.
#[test]
fn session_action_stdout_stderr_paths_default_and_override_round_trip() {
    let session = session_root("capture_paths");
    let custom_out = session.join("nested").join("stdout.txt");
    fs::create_dir_all(custom_out.parent().expect("parent")).expect("mkdir nested");

    run_primary_action_with_capture_paths(
        &session,
        Path::new("/bin/sh"),
        &["-c".into(), "echo -n hello".into()],
        &HashMap::new(),
        Some(custom_out.as_path()),
        None,
    )
    .expect("primary with custom stdout path");

    assert_eq!(
        fs::read_to_string(&custom_out).expect("read custom stdout"),
        "hello",
        "custom stdout capture path must contain process stdout bytes"
    );

    let def_stderr = session.join("capture").join("stderr.default.txt");
    fs::create_dir_all(def_stderr.parent().expect("parent")).expect("mkdir capture");
    run_primary_action_with_capture_paths(
        &session,
        Path::new("/bin/sh"),
        &[
            "-c".into(),
            "echo -n err1 >&2; echo -n err2 >&2".into(),
        ],
        &HashMap::new(),
        None,
        Some(def_stderr.as_path()),
    )
    .expect("primary with default stdout and explicit stderr path");

    assert_eq!(
        fs::read_to_string(&def_stderr).expect("read stderr capture"),
        "err1err2",
        "stderr bytes must land in the resolved stderr path"
    );
}
