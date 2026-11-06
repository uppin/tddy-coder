//! Headless backends on a provider route, against stub binaries that record their argv and env.
//! The `claude`, `codex` and `cursor` turns are the spec-driven `SpecHeadlessBackend` (`#agent-tools 15/19`).
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tddy_core::backend::{CodingBackend, InvokeRequest};
use tddy_core::provider_route::{HarnessRoute, RouteSecret};

use common::{a_spec_backend, in_spec_workspace, spec_recorded};

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

fn a_request_in(dir: &Path) -> InvokeRequest {
    InvokeRequest {
        prompt: "hello".to_string(),
        working_dir: Some(dir.to_path_buf()),
        ..InvokeRequest::default()
    }
}

/// A request with no working directory: [`in_spec_workspace`] points it at the fixture's checkout.
fn a_request() -> InvokeRequest {
    InvokeRequest {
        prompt: "hello".to_string(),
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
    let (backend, dir) = a_spec_backend(
        "codex",
        r#"printf '%s\n' '{"type":"item.completed","item":{"text":"ok"}}'"#,
    );
    let route = a_route(
        &[],
        ("CODEX_PROVIDER_KEY", "a-meta-key"),
        &[
            "-c",
            "model_provider=\"tddy-route\"",
            "-m",
            "muse-wonder-1.0",
        ],
    );
    let backend = backend
        .with_provider_route(route)
        .expect("codex is routable");
    let request = in_spec_workspace(&dir, a_request());

    // When
    run(backend, request);

    // Then
    let argv = spec_recorded(&dir, "argv.txt");
    assert!(argv.contains("model_provider=\"tddy-route\""), "{argv}");
    assert!(argv.contains("muse-wonder-1.0"), "{argv}");
    assert!(!argv.contains("a-meta-key"), "the key is never an argument");
    assert!(spec_recorded(&dir, "env.txt").contains("CODEX_PROVIDER_KEY=a-meta-key"));
}

#[test]
fn headless_claude_on_fireworks_gets_the_anthropic_route_env() {
    // Given
    let (backend, dir) = a_spec_backend(
        "claude",
        r#"printf '%s\n' '{"type":"result","result":"ok"}'"#,
    );
    let route = a_route(
        &[("ANTHROPIC_BASE_URL", "https://api.fireworks.ai/inference")],
        ("ANTHROPIC_AUTH_TOKEN", "a-fireworks-key"),
        &[],
    );
    let backend = backend
        .with_provider_route(route)
        .expect("claude is routable");
    let request = in_spec_workspace(&dir, a_request());

    // When
    run(backend, request);

    // Then
    let env = spec_recorded(&dir, "env.txt");
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

        // When: the spec backend for that agent, on the route (its spec's shape and providers,
        // read from its YAML — codex-acp routes OpenAI Responses as toml args with its key in
        // `CODEX_PROVIDER_KEY`, claude-acp routes Anthropic through the environment)
        let id = if codex { "codex-acp" } else { "claude-acp" };
        let spec = tddy_coding_agent_specs::embedded_spec(id).expect("the agent's spec loads");
        let backend = tddy_core::SpecAcpBackend::new(
            tddy_coding_agent_specs::routed_agent(&spec),
            tddy_core::AcpAgentCommand {
                program: agent,
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
            },
        )
        .with_provider_route(route)
        .expect("the agent is routable");
        run(backend, a_request_in(dir.path()));

        // Then
        let env = recorded(dir.path(), "env.txt");
        assert!(env.contains("ROUTED=yes"), "codex={codex}: {env}");
        assert!(env.contains("ROUTE_KEY=a-key"), "codex={codex}");
    }
}

#[test]
fn cursor_cannot_be_routed() {
    // Given
    let route = a_route(&[], ("K", "v"), &[]);
    let (cursor, _dir) = a_spec_backend("cursor", "exit 0");

    // Then (the stub's refusal is pinned in `tddy-coder`'s `spec_backend_build` tests)
    let refusal = cursor.with_provider_route(route);
    assert_eq!(
        refusal.err().map(|r| r.to_string()).as_deref(),
        Some("Cursor cannot be pointed at another provider; it always runs on its own vendor")
    );
}
