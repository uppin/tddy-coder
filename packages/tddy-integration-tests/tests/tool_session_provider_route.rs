//! Headless backends on a provider route, against stub binaries that record their argv and env.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tddy_core::backend::{AnyBackend, CodingBackend, InvokeRequest};
use tddy_core::provider_route::{HarnessRoute, RouteSecret};
use tddy_core::{
    ClaudeAcpBackend, ClaudeCodeBackend, CodexAcpBackend, CodexBackend, CursorBackend,
};

/// A stub `name` in `dir` that writes its argv to `argv.txt` and its environment to `env.txt`,
/// then prints `stdout` and exits 0.
fn a_recording_stub(dir: &Path, name: &str, stdout: &str) -> PathBuf {
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{dir}/argv.txt\"\nenv > \"{dir}/env.txt\"\nprintf '%s\\n' '{stdout}'\nexit 0\n",
        dir = dir.display(),
    );
    let path = dir.join(name);
    fs::write(&path, script).expect("stub written");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

fn recorded(dir: &Path, file: &str) -> String {
    fs::read_to_string(dir.join(file)).unwrap_or_default()
}

fn a_route(env: &[(&str, &str)], secret: (&str, &str), args: &[&str]) -> HarnessRoute {
    HarnessRoute {
        env: env
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        secret_env: vec![(secret.0.to_string(), RouteSecret::new(secret.1))],
        files: Vec::new(),
        args: args.iter().map(|a| (*a).to_string()).collect(),
    }
}

fn a_request(dir: &Path) -> InvokeRequest {
    InvokeRequest {
        prompt: "hello".to_string(),
        working_dir: Some(dir.to_path_buf()),
        ..InvokeRequest::default()
    }
}

fn run(backend: impl CodingBackend, request: InvokeRequest) {
    let _ = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(backend.invoke(request));
}

#[test]
fn codex_on_meta_runs_exec_with_a_responses_provider_and_the_key_in_env() {
    // Given
    let dir = tempfile::tempdir().expect("dir");
    let codex = a_recording_stub(
        dir.path(),
        "codex",
        r#"{"type":"item.completed","item":{"text":"ok"}}"#,
    );
    let route = a_route(
        &[],
        ("TDDY_ROUTE_API_KEY", "a-meta-key"),
        &[
            "-c",
            "model_provider=\"tddy-route\"",
            "-m",
            "muse-wonder-1.0",
        ],
    );

    // When
    run(
        CodexBackend::with_path(codex).with_provider_route(route),
        a_request(dir.path()),
    );

    // Then
    let argv = recorded(dir.path(), "argv.txt");
    assert!(argv.contains("model_provider=\"tddy-route\""), "{argv}");
    assert!(argv.contains("muse-wonder-1.0"), "{argv}");
    assert!(!argv.contains("a-meta-key"), "the key is never an argument");
    assert!(recorded(dir.path(), "env.txt").contains("TDDY_ROUTE_API_KEY=a-meta-key"));
}

#[test]
fn headless_claude_on_fireworks_gets_the_anthropic_route_env() {
    // Given
    let dir = tempfile::tempdir().expect("dir");
    let claude = a_recording_stub(dir.path(), "claude", r#"{"type":"result","result":"ok"}"#);
    let route = a_route(
        &[("ANTHROPIC_BASE_URL", "https://api.fireworks.ai/inference")],
        ("ANTHROPIC_AUTH_TOKEN", "a-fireworks-key"),
        &[],
    );

    // When
    run(
        ClaudeCodeBackend::with_path(claude).with_provider_route(route),
        a_request(dir.path()),
    );

    // Then
    let env = recorded(dir.path(), "env.txt");
    assert!(
        env.contains("ANTHROPIC_BASE_URL=https://api.fireworks.ai/inference"),
        "{env}"
    );
    assert!(env.contains("ANTHROPIC_AUTH_TOKEN=a-fireworks-key"));
}

#[test]
fn acp_agents_spawn_with_the_route_applied() {
    for codex in [false, true] {
        // Given an ACP "agent" that records its environment and exits before the handshake
        let dir = tempfile::tempdir().expect("dir");
        let agent = a_recording_stub(dir.path(), "acp-agent", "");
        let route = a_route(&[("ROUTED", "yes")], ("ROUTE_KEY", "a-key"), &[]);

        // When
        if codex {
            run(
                CodexAcpBackend::with_agent_path(agent).with_provider_route(route),
                a_request(dir.path()),
            );
        } else {
            run(
                ClaudeAcpBackend::with_agent_path(agent).with_provider_route(route),
                a_request(dir.path()),
            );
        }

        // Then
        let env = recorded(dir.path(), "env.txt");
        assert!(env.contains("ROUTED=yes"), "codex={codex}: {env}");
        assert!(env.contains("ROUTE_KEY=a-key"), "codex={codex}");
    }
}

#[test]
fn cursor_and_the_stub_cannot_be_routed() {
    // Given
    let route = a_route(&[], ("K", "v"), &[]);

    // Then
    let cursor = AnyBackend::Cursor(CursorBackend::new()).with_provider_route(route.clone());
    assert!(cursor.err().unwrap_or_default().contains("Cursor"));
    assert!(AnyBackend::Stub(tddy_core::StubBackend::new())
        .with_provider_route(route)
        .is_err());
}
