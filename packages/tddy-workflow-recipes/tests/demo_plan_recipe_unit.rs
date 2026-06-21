//! Unit + acceptance tests for the extended `DemoPlan` recipe model.
//!
//! Covers:
//! - `DemoMode` serde (port_forward / screen_share)
//! - `PortMap` serde
//! - New `DemoPlan` fields with `#[serde(default)]` back-compat
//! - `write_demo_plan_file` + `read_demo_plan_file` round-trip
//! - `parse_demo_response` populates `share_url`
//! - `DemoMode` decided from app shape (plan step contract)

use tddy_workflow_recipes::parser::{parse_demo_response, DemoMode, DemoPlan, DemoStep, PortMap};
use tddy_workflow_recipes::writer::{read_demo_plan_file, write_demo_plan_file};

fn minimal_demo_plan() -> DemoPlan {
    DemoPlan {
        demo_type: "port_forward".to_string(),
        setup_instructions: "Build the app with cargo build".to_string(),
        steps: vec![DemoStep {
            description: "Start the web server".to_string(),
            command_or_action: "cargo run".to_string(),
            expected_result: "Server listening on :8080".to_string(),
        }],
        verification: "curl http://localhost:8080/health returns 200".to_string(),
        mode: Some(DemoMode::PortForward),
        hostfwd: vec![PortMap {
            host_port: 8080,
            guest_port: 80,
        }],
        deploy_steps: vec!["apt install -y myapp".to_string()],
        verify_command: Some("curl -f http://localhost:80/health".to_string()),
        build_target: Some("my-os:qcow2".to_string()),
    }
}

fn temp_session_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("tddy-demo-recipe-{label}-"))
        .tempdir()
        .expect("temp session dir")
}

// ── DemoMode serde ───────────────────────────────────────────────────────────

#[test]
fn demo_mode_port_forward_serializes_to_snake_case() {
    let json = serde_json::to_string(&DemoMode::PortForward).expect("serialize DemoMode");
    assert_eq!(
        json, "\"port_forward\"",
        "DemoMode::PortForward must serialize as \"port_forward\", got: {json}"
    );
}

#[test]
fn demo_mode_screen_share_serializes_to_snake_case() {
    let json = serde_json::to_string(&DemoMode::ScreenShare).expect("serialize DemoMode");
    assert_eq!(
        json, "\"screen_share\"",
        "DemoMode::ScreenShare must serialize as \"screen_share\", got: {json}"
    );
}

#[test]
fn demo_mode_port_forward_deserializes_from_snake_case() {
    let mode: DemoMode = serde_json::from_str("\"port_forward\"").expect("deserialize DemoMode");
    assert_eq!(mode, DemoMode::PortForward);
}

#[test]
fn demo_mode_screen_share_deserializes_from_snake_case() {
    let mode: DemoMode = serde_json::from_str("\"screen_share\"").expect("deserialize DemoMode");
    assert_eq!(mode, DemoMode::ScreenShare);
}

// ── PortMap serde ────────────────────────────────────────────────────────────

#[test]
fn port_map_serializes_with_host_and_guest_port() {
    let pm = PortMap {
        host_port: 8080,
        guest_port: 80,
    };
    let json = serde_json::to_string(&pm).expect("serialize PortMap");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["host_port"], 8080);
    assert_eq!(v["guest_port"], 80);
}

// ── back-compat: existing demo-plan.md without new fields ───────────────────

/// An existing `DemoPlan` JSON without `mode`, `hostfwd`, `deploy_steps`, `verify_command`,
/// `build_target` must still deserialize (those fields default to None/empty).
#[test]
fn demo_plan_back_compat_existing_demo_plan_parses_without_mode() {
    let legacy_json = r#"{
        "demo_type": "cli",
        "setup_instructions": "run cargo build",
        "steps": [{"description":"run","command_or_action":"./app","expected_result":"ok"}],
        "verification": "check exit code"
    }"#;
    let plan: DemoPlan =
        serde_json::from_str(legacy_json).expect("legacy DemoPlan must deserialize");
    assert_eq!(plan.demo_type, "cli");
    assert!(
        plan.mode.is_none(),
        "legacy plan must have mode=None, got: {:?}",
        plan.mode
    );
    assert!(
        plan.hostfwd.is_empty(),
        "legacy plan must have empty hostfwd"
    );
    assert!(
        plan.deploy_steps.is_empty(),
        "legacy plan must have empty deploy_steps"
    );
    assert!(
        plan.verify_command.is_none(),
        "legacy plan must have verify_command=None"
    );
    assert!(
        plan.build_target.is_none(),
        "legacy plan must have build_target=None"
    );
}

// ── write + read round-trip ─────────────────────────────────────────────────

/// `write_demo_plan_file` followed by `read_demo_plan_file` must reconstruct the same `DemoPlan`.
#[test]
fn demo_plan_recipe_roundtrips_through_demo_plan_md() {
    let dir = temp_session_dir("roundtrip");
    let original = minimal_demo_plan();

    write_demo_plan_file(dir.path(), &original).expect("write demo-plan.md");
    let restored = read_demo_plan_file(dir.path()).expect("read demo-plan.md");

    assert_eq!(
        restored.demo_type, original.demo_type,
        "demo_type must roundtrip"
    );
    assert_eq!(
        restored.verification, original.verification,
        "verification must roundtrip"
    );

    let restored_mode = restored.mode.as_ref().expect("mode must survive roundtrip");
    assert_eq!(
        restored_mode,
        original.mode.as_ref().unwrap(),
        "mode must roundtrip"
    );

    assert_eq!(
        restored.hostfwd.len(),
        original.hostfwd.len(),
        "hostfwd length must roundtrip"
    );
    assert_eq!(
        restored.hostfwd[0].host_port, original.hostfwd[0].host_port,
        "hostfwd host_port must roundtrip"
    );
    assert_eq!(
        restored.hostfwd[0].guest_port, original.hostfwd[0].guest_port,
        "hostfwd guest_port must roundtrip"
    );
    assert_eq!(
        restored.deploy_steps, original.deploy_steps,
        "deploy_steps must roundtrip"
    );
    assert_eq!(
        restored.verify_command, original.verify_command,
        "verify_command must roundtrip"
    );
    assert_eq!(
        restored.build_target, original.build_target,
        "build_target must roundtrip"
    );
}

/// `read_demo_plan_file` on a legacy file (written by old code without front-matter) must
/// return a plan without panicking; mode is None.
#[test]
fn read_demo_plan_file_on_legacy_file_returns_plan_without_mode() {
    let dir = temp_session_dir("legacy-read");
    let legacy_content = "# Demo Plan\n\n## Type\ncli\n\n## Setup\n\nrun cargo build\n\n## Verification\n\ncheck exit code\n";
    std::fs::write(dir.path().join("demo-plan.md"), legacy_content)
        .expect("write legacy demo-plan.md");

    let plan = read_demo_plan_file(dir.path()).expect("read legacy demo-plan.md");
    assert!(plan.mode.is_none(), "legacy file must produce mode=None");
    assert!(
        plan.hostfwd.is_empty(),
        "legacy file must produce empty hostfwd"
    );
}

// ── plan step decides demo mode ─────────────────────────────────────────────

/// When the plan agent produces a `DemoPlan` with `mode: port_forward` (web app),
/// the recipe must reflect `DemoMode::PortForward`.
#[test]
fn plan_step_decides_demo_mode_port_forward_for_web_app() {
    let plan_json = r#"{
        "demo_type": "web_app",
        "setup_instructions": "deploy web server",
        "steps": [{"description":"open browser","command_or_action":"open http://localhost:8080","expected_result":"dashboard appears"}],
        "verification": "http 200 on /health",
        "mode": "port_forward",
        "hostfwd": [{"host_port": 8080, "guest_port": 80}],
        "deploy_steps": ["systemctl start myapp"],
        "verify_command": "curl -f http://localhost:80/health",
        "build_target": "my-app:qcow2"
    }"#;
    let plan: DemoPlan = serde_json::from_str(plan_json).expect("parse plan-step DemoPlan");

    let mode = plan
        .mode
        .as_ref()
        .expect("mode must be set for web app demo");
    assert_eq!(
        mode,
        &DemoMode::PortForward,
        "web app must yield DemoMode::PortForward, got: {mode:?}"
    );
    assert_eq!(plan.hostfwd.len(), 1, "must have exactly one hostfwd");
    assert_eq!(plan.hostfwd[0].host_port, 8080);
    assert_eq!(plan.hostfwd[0].guest_port, 80);
}

/// When the plan agent produces a `DemoPlan` with `mode: screen_share` (GUI app),
/// the recipe must reflect `DemoMode::ScreenShare`.
#[test]
fn plan_step_decides_demo_mode_screen_share_for_gui_app() {
    let plan_json = r#"{
        "demo_type": "gui",
        "setup_instructions": "launch desktop app",
        "steps": [{"description":"click button","command_or_action":"N/A (screenshare)","expected_result":"dialog appears"}],
        "verification": "dialog text matches expected",
        "mode": "screen_share",
        "hostfwd": [],
        "deploy_steps": ["./install-app.sh"],
        "verify_command": null,
        "build_target": "desktop-app:qcow2"
    }"#;
    let plan: DemoPlan = serde_json::from_str(plan_json).expect("parse plan-step DemoPlan");

    let mode = plan.mode.as_ref().expect("mode must be set for GUI demo");
    assert_eq!(
        mode,
        &DemoMode::ScreenShare,
        "GUI app must yield DemoMode::ScreenShare, got: {mode:?}"
    );
    assert!(
        plan.hostfwd.is_empty(),
        "screen_share must have empty hostfwd"
    );
}

// ── parse_demo_response share_url ────────────────────────────────────────────

/// `parse_demo_response` must populate `share_url` when the agent includes it.
#[test]
fn demo_output_includes_share_url() {
    let json = r#"{"goal":"demo","summary":"Demo ran successfully.","demo_type":"port_forward","steps_completed":3,"verification":"HTTP 200 on /health","share_url":"http://localhost:8080"}"#;
    let output = parse_demo_response(json).expect("parse demo response with share_url");

    let url = output
        .share_url
        .as_deref()
        .expect("share_url must be present in DemoOutput");
    assert_eq!(
        url, "http://localhost:8080",
        "share_url must be http://localhost:8080, got: {url:?}"
    );
}

/// `parse_demo_response` must accept missing `share_url` (back-compat: older agent output).
#[test]
fn demo_output_share_url_absent_is_none() {
    let json = r#"{"goal":"demo","summary":"Demo ran.","demo_type":"cli","steps_completed":1,"verification":"ok"}"#;
    let output = parse_demo_response(json).expect("parse demo response without share_url");
    assert!(
        output.share_url.is_none(),
        "share_url must be None when absent, got: {:?}",
        output.share_url
    );
}
