//! Acceptance: the platform-agnostic [`tddy_sandbox::spawn`] facade returns
//! [`SandboxError::Unsupported`] — callers on macOS use `tddy_sandbox_darwin` directly.

use std::path::PathBuf;

use tddy_sandbox::{SandboxError, SandboxSpec, spawn};

fn minimal_spec() -> SandboxSpec {
    SandboxSpec {
        project_root: PathBuf::from("/tmp/tddy-sandbox-facade-test"),
        scratch_dir: PathBuf::from("/tmp/tddy-sandbox-facade-test/.work"),
        egress_dir: PathBuf::from("/tmp/tddy-sandbox-facade-test/out"),
        allow_read_paths: vec![],
        command: vec!["/bin/echo".into(), "hi".into()],
        env: Default::default(),
        profile_path: PathBuf::from("/tmp/tddy-sandbox-facade-test/profile.sb"),
        loopback_allow_ports: vec![],
        ipc_socket: None,
    }
}

/// **spawn_facade_returns_unsupported**: the cross-platform entrypoint never spawns a child;
/// it always directs callers to the platform crate.
#[test]
fn spawn_facade_returns_unsupported() {
    // Given
    let spec = minimal_spec();

    // When
    let err = match spawn(spec) {
        Err(err) => err,
        Ok(_) => panic!("facade spawn must not succeed"),
    };

    // Then
    match err {
        SandboxError::Unsupported { platform, message } => {
            assert_eq!(platform, std::env::consts::OS);
            assert!(
                message.contains("tddy_sandbox_darwin"),
                "message must point to darwin crate: {message}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}
