//! Acceptance: the OAuth secret channel keeps the token value out of the `sandbox-exec` argv.
//!
//! Requires macOS (the darwin spawn argv). Skipped on other platforms.

#![cfg(target_os = "macos")]

use tddy_sandbox::{SandboxBuilder, SecretSource};

/// **the_oauth_secret_is_passed_to_the_claude_child_and_never_appears_in_the_sandbox_exec_argv**:
/// a declared secret is delivered out-of-band — the `sandbox-exec`/`env -i` argv carries only the
/// `TDDY_SECRET_<NAME>=<scratch file path>` reference, never the secret value itself (which is
/// written to a `0600` file the runner reads and sets on the inner Claude child).
#[test]
fn the_oauth_secret_is_passed_to_the_claude_child_and_never_appears_in_the_sandbox_exec_argv() {
    // Given — a plan that declares the OAuth token as an out-of-band secret
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("project");
    let scratch = project_root.join(".work");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    let token = "sk-ant-oat01-SECRET-VALUE";
    let plan = SandboxBuilder::new(
        &project_root,
        &scratch,
        &egress,
        vec!["/usr/bin/true".into()],
    )
    .profile_path(project_root.join("profile.sb"))
    .secret("CLAUDE_CODE_OAUTH_TOKEN", SecretSource::Value(token.into()))
    .build()
    .expect("plan must build");

    // When — the sandbox-exec argv is constructed
    let argv = tddy_sandbox_darwin::sandbox_exec_argv(&plan);

    // Then — the secret value never appears in argv, but the secret file reference does
    assert!(
        argv.iter().all(|a| !a.contains(token)),
        "secret value must never appear in sandbox-exec argv: {argv:?}"
    );
    assert!(
        argv.iter()
            .any(|a| a.starts_with("TDDY_SECRET_CLAUDE_CODE_OAUTH_TOKEN=")),
        "argv must pass the secret file reference, not the value: {argv:?}"
    );
}
