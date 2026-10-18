//! The runner hosts OpenCode and Muse Code in its PTY with the same argv the host would use, and
//! hands their route key in from its secret file.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-sandboxed-routes.md

use std::collections::BTreeMap;

use tddy_sandbox_runner::harness_pty::harness_pty_argv;
use tddy_sandbox_runner::resolve_secret_envs;

#[test]
fn the_runner_spawns_opencode_with_its_model_inside_the_jail() {
    // Then
    assert_eq!(
        harness_pty_argv(
            "opencode",
            "/usr/local/bin/opencode",
            "tddy-route/kimi",
            Some("go"),
            false
        ),
        Ok(vec![
            "/usr/local/bin/opencode".to_string(),
            "--model".to_string(),
            "tddy-route/kimi".to_string(),
            "--prompt".to_string(),
            "go".to_string(),
        ])
    );
}

#[test]
fn the_runner_resumes_muse_by_continuing_its_session() {
    // Then
    assert_eq!(
        harness_pty_argv("muse", "muse", "muse-spark-1.3", None, true),
        Ok(vec!["muse".to_string(), "--continue".to_string()])
    );
}

#[test]
fn an_agent_kind_the_runner_does_not_host_is_refused() {
    // Then
    assert!(harness_pty_argv("vim", "vim", "", None, false).is_err());
}

#[test]
fn the_runner_spawns_muse_with_the_key_resolved_from_its_secret_file() {
    // Given the route's key, delivered as a jail secret file
    let tmp = tempfile::tempdir().unwrap();
    let secret = tmp.path().join("META_API_KEY");
    std::fs::write(&secret, "a-meta-key").unwrap();
    let vars = BTreeMap::from([(
        "TDDY_SECRET_META_API_KEY".to_string(),
        secret.to_string_lossy().to_string(),
    )]);

    // When
    let resolved = resolve_secret_envs(&vars);

    // Then: a route secret resolves like the OAuth token always has
    assert_eq!(
        resolved,
        vec![("META_API_KEY".to_string(), "a-meta-key".to_string())]
    );
}
