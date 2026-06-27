use std::path::Path;

use tddy_sandbox::{SandboxError, SandboxSpec};

/// Canonicalize a path for use in an SBPL rule.
///
/// Seatbelt evaluates file rules against the **fully symlink-resolved** path. On macOS
/// `/tmp`, `/etc`, `/var` are symlinks into `/private/...`, so a rule spelled
/// `(subpath "/tmp/…")` never matches an access the kernel reports as `/private/tmp/…`.
/// This bit creating an AF_UNIX socket file under a `/tmp` project root: the write was
/// denied even though the project subpath was "allowed". Canonicalize best-effort and
/// fall back to the original spelling when the path does not yet exist (e.g. unit tests).
fn canonical_rule_path(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Render the SBPL profile from the embedded template.
pub fn render_profile(spec: &SandboxSpec) -> Result<String, SandboxError> {
    let template = include_str!("../profiles/sandbox-claude.sb.tmpl");
    let project_root = canonical_rule_path(&spec.project_root);
    let scratch_dir = canonical_rule_path(&spec.scratch_dir);
    let egress_dir = canonical_rule_path(&spec.egress_dir);
    let darwin_base = canonical_rule_path(std::path::Path::new(&darwin_user_temp_base()?));
    let read_paths = spec
        .allow_read_paths
        .iter()
        .map(|p| format!("  (subpath \"{}\")", canonical_rule_path(p)))
        .collect::<Vec<_>>()
        .join("\n");

    // AF_UNIX sockets are pure local IPC and never reach an external host, so they are
    // always permitted: the in-jail tool-IPC server binds one. `network*` otherwise stays
    // denied; loopback TCP (gRPC bridge + egress shim) is re-allowed per pre-declared port.
    //
    // NOTE on `localhost`: Seatbelt's TCP address filter only accepts `*` or the keyword
    // `localhost` as the host ("host must be * or localhost in network address"); a literal
    // `127.0.0.1` is rejected as an invalid profile. `localhost` here is resolved by Seatbelt
    // itself and matches loopback binds. This is the *policy* layer only — the runner must
    // still bind/dial the literal `127.0.0.1` at runtime, because the clean-env jail has no
    // resolver and getaddrinfo("localhost") fails before the bind is ever attempted.
    let mut lines = vec![
        "(deny network*)".to_string(),
        "(allow network-bind (local unix-socket))".to_string(),
        "(allow network-inbound (local unix-socket))".to_string(),
        "(allow network-outbound (remote unix-socket))".to_string(),
    ];
    if !spec.loopback_allow_ports.is_empty() {
        lines.push("(allow network-bind (local tcp \"localhost:*\"))".to_string());
        for port in &spec.loopback_allow_ports {
            lines.push(format!(
                "(allow network-outbound (remote tcp \"localhost:{port}\"))"
            ));
            lines.push(format!(
                "(allow network-inbound (local tcp \"localhost:{port}\"))"
            ));
        }
    }
    // Explicit read+write for a short, out-of-tree tool-IPC socket (see SandboxSpec::ipc_socket).
    if let Some(sock) = &spec.ipc_socket {
        let p = canonical_rule_path(sock);
        lines.push(format!("(allow file-read* (literal \"{p}\"))"));
        lines.push(format!("(allow file-write* (literal \"{p}\"))"));
    }
    let network_policy = lines.join("\n");

    Ok(template
        .replace("@PROJECT_ROOT@", &project_root)
        .replace("@SCRATCH_DIR@", &scratch_dir)
        .replace("@OUTPUT_DIR@", &egress_dir)
        .replace("@DARWIN_BASE@", &darwin_base)
        .replace("@ALLOW_READ_PATHS@", &read_paths)
        .replace("@NETWORK_POLICY@", &network_policy))
}

fn darwin_user_temp_base() -> Result<String, SandboxError> {
    let darwin_tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp/Name".to_string());
    let path = Path::new(darwin_tmp.trim_end_matches('/'));
    let mut base = path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/private/var/folders".to_string());
    // TMPDIR=/tmp makes parent^2 collapse to `/`, which would allow writes anywhere.
    if base == "/" || base.is_empty() {
        base = "/private/var/folders".to_string();
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tddy_sandbox::SandboxSpec;

    #[test]
    fn rendered_profile_denies_writes_and_reallows_project_tree() {
        // Given
        let spec = SandboxSpec {
            project_root: PathBuf::from("/tmp/tddy-sandbox-project"),
            scratch_dir: PathBuf::from("/tmp/tddy-sandbox-project/.work"),
            egress_dir: PathBuf::from("/tmp/tddy-sandbox-project/out"),
            allow_read_paths: vec![PathBuf::from("/usr/bin")],
            command: vec!["/bin/echo".into(), "hi".into()],
            env: Default::default(),
            profile_path: PathBuf::from("/tmp/tddy-sandbox-project/profile.sb"),
            loopback_allow_ports: vec![],
            ipc_socket: None,
        };

        // When
        let profile = render_profile(&spec).expect("render must succeed");

        // Then
        assert!(profile.contains("(deny file-write*)"));
        assert!(profile.contains("/tmp/tddy-sandbox-project"));
        assert!(profile.contains("/usr/bin"));
        assert!(profile.contains("/usr/bin"));
        assert!(profile.contains("/var/folders"));
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(allow file-read*"));
    }
}
