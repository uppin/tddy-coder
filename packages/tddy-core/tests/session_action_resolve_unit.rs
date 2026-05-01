//! Acceptance tests (PRD Testing Plan): session action pipeline **pure** resolution —
//! env merge precedence, canonical invocation envelope (no mapper), glob resolution, channel manifest.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde_json::json;
use tddy_core::session_action_pipeline::{
    build_extended_channel_manifest, build_invocation_envelope_direct, merge_session_action_env,
    resolve_output_globs_sorted,
};

fn temp_base(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tddy_session_action_resolve_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir fixture base");
    dir
}

/// PRD: invocation `env_override` merges with action defaults; override wins on key conflicts (deterministic).
#[test]
fn session_action_default_env_merge_respects_override_precedence() {
    let mut defaults = HashMap::new();
    defaults.insert("A".into(), "from_default".into());
    defaults.insert("B".into(), "only_default".into());
    let mut overrides = HashMap::new();
    overrides.insert("A".into(), "from_override".into());
    overrides.insert("C".into(), "only_override".into());

    let merged = merge_session_action_env(&defaults, &overrides);
    assert_eq!(
        merged.get("A").map(String::as_str),
        Some("from_override"),
        "override must win for disputed key A; got {:?}",
        merged.get("A")
    );
    assert_eq!(merged.get("B").map(String::as_str), Some("only_default"));
    assert_eq!(merged.get("C").map(String::as_str), Some("only_override"));
}

/// PRD (granular): when the action defines no default env entries, merged env must still include override keys.
#[test]
fn session_action_merge_env_applies_overrides_when_defaults_empty() {
    let defaults = HashMap::new();
    let mut overrides = HashMap::new();
    overrides.insert("ONLY_OVERRIDE".into(), "1".into());
    let merged = merge_session_action_env(&defaults, &overrides);
    assert_eq!(
        merged.get("ONLY_OVERRIDE").map(String::as_str),
        Some("1"),
        "override must apply when defaults are empty; got {:?}",
        merged
    );
}

/// PRD: without an input mapper, the canonical serialized invocation is exactly `args` + `env` (no extra keys).
#[test]
fn session_action_invocation_without_mapper_uses_args_env_envelope_directly() {
    let args = vec!["/bin/sh".into(), "-c".into(), "echo hi".into()];
    let mut env = HashMap::new();
    env.insert("PATH".into(), "/usr/bin".into());

    let v = build_invocation_envelope_direct(&args, &env);
    assert_eq!(
        v,
        json!({
            "args": args,
            "env": env,
        }),
        "envelope must be exactly {{\"args\",\"env\"}}; got {v}"
    );
}

/// PRD: declared output globs resolve against fixture layout; failures are explicit when rules are violated.
#[test]
fn session_action_outputs_glob_resolution_matches_fixture_layout() {
    let base = temp_base("globs");
    let out = base.join("out");
    fs::create_dir_all(&out).expect("mkdir out");
    fs::write(out.join("x.log"), b"x").expect("write x.log");
    fs::write(out.join("y.log"), b"y").expect("write y.log");
    fs::write(out.join("read.me"), b"z").expect("write read.me");

    let patterns = vec!["out/*.log".to_string()];
    let got = resolve_output_globs_sorted(&base, &patterns).expect("resolution returns Result");
    let mut expected = vec![out.join("x.log"), out.join("y.log")];
    expected.sort();
    assert_eq!(
        got, expected,
        "sorted glob results must match *.log files only; got {:?}",
        got
    );
}

/// PRD: channel manifest includes at least `stdout`, `stderr`, and example `logs` for mapper/transform.
#[test]
fn session_action_mapper_receives_extended_channel_manifest_including_logs_dir() {
    let session = temp_base("channels");
    let logs = session.join("logs");
    fs::create_dir_all(&logs).expect("mkdir logs");
    fs::write(logs.join("app.log"), b"tick\n").expect("write app.log");

    let manifest =
        build_extended_channel_manifest(&session, None, None).expect("channel manifest Ok");

    for key in ["stdout", "stderr", "logs"] {
        assert!(
            manifest.contains_key(key),
            "channel manifest must include `{key}` for mapper/transform hooks; keys: {:?}",
            manifest.keys().collect::<Vec<_>>()
        );
    }
    let logs_path = manifest.get("logs").expect("logs channel");
    assert!(
        logs_path.is_dir() || logs_path.exists(),
        "logs channel must point at the fixture logs directory; got {}",
        logs_path.display()
    );
}
