//! Unit tests for ActionSpec conversions.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_actions::{
    action_spec_from_session_manifest, build_action_fields_to_spec, ActionSpec, ChannelMode,
    OutputKind, SessionManifestFields,
};

#[test]
fn session_action_manifest_round_trips_through_action_spec() {
    // Given
    let fields = SessionManifestFields {
        version: 1,
        id: "run-tests".into(),
        summary: "Run cargo test".into(),
        architecture: "native".into(),
        command: vec!["cargo".into(), "test".into()],
        input_schema: Some(json!({ "type": "object" })),
        output_schema: Some(json!({ "type": "object" })),
        result_kind: Some("test_summary".into()),
        output_path_arg: Some("log_path".into()),
        working_dir: Some(PathBuf::from("/tmp/session")),
    };

    // When
    let spec = action_spec_from_session_manifest(fields.clone());

    // Then
    assert_eq!(spec.id, "run-tests");
    assert_eq!(spec.kind, "session-action");
    assert_eq!(spec.command, vec!["cargo", "test"]);
    assert_eq!(spec.channel_mode, ChannelMode::StdoutStderr);
    let session = spec.session.expect("session extras");
    assert_eq!(session.summary, "Run cargo test");
    assert_eq!(session.architecture, "native");
    assert_eq!(session.manifest_version, 1);
    assert_eq!(session.result_kind.as_deref(), Some("test_summary"));
    assert_eq!(session.output_path_arg.as_deref(), Some("log_path"));
    assert_eq!(session.input_schema, Some(json!({ "type": "object" })));
}

#[test]
fn build_action_fields_produce_build_action_spec() {
    // Given
    let repo = PathBuf::from("/repo");
    let fields = tddy_actions::convert::BuildActionFields {
        id: "compile".into(),
        command: vec!["cargo".into(), "build".into()],
        env: BTreeMap::new(),
        input_globs: vec![("crate".into(), vec!["src/**/*.rs".into()])],
        outputs: vec![("target/debug/app".into(), OutputKind::File)],
        working_dir: Some(PathBuf::from("crate")),
    };

    // When
    let spec = build_action_fields_to_spec(&repo, fields);

    // Then
    assert_eq!(spec.kind, "build-action");
    assert_eq!(spec.command, vec!["cargo", "build"]);
    assert_eq!(spec.working_dir, Some(PathBuf::from("/repo/crate")));
    assert_eq!(spec.outputs.len(), 1);
    assert!(spec.validate().is_ok());
}

#[test]
fn action_spec_rejects_empty_command_for_non_pipeline() {
    // Given
    let spec = ActionSpec {
        id: "bad".into(),
        kind: "test".into(),
        command: vec![],
        inputs: vec![],
        outputs: vec![],
        env: BTreeMap::new(),
        working_dir: None,
        channel_mode: ChannelMode::None,
        sandbox: None,
        session: None,
        pipeline: None,
    };

    // When / Then
    assert!(spec.validate().is_err());
}
