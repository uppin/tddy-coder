//! Where the daemon's coding agents find their binaries.

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

use super::host_coding_agents;

fn an_executable(path: &Path) {
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn cursor_runs_the_binary_daemon_yaml_names_under_cursor_cli() {
    // Given — a stub `agent` named by `cursor_cli.binary_path` (the process sets no TDDY_CURSOR_AGENT)
    let dir = tempfile::tempdir().unwrap();
    let stub = std::fs::canonicalize(dir.path()).unwrap().join("agent");
    an_executable(&stub);
    let config: DaemonConfig =
        serde_yaml::from_str(&format!("cursor_cli:\n  binary_path: {}\n", stub.display()))
            .expect("a daemon config naming the cursor binary");

    // When
    let resolved = host_coding_agents(&config, Path::new("/nonexistent-tddy-home"))
        .expect("the built-in coding agents load")
        .resolve("cursor", None)
        .map(|agent| agent.tool.binary);

    // Then
    assert_eq!(resolved.ok(), Some(stub));
}
