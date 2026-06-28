//! Sandbox egress log paths and helpers for host-side diagnostics.
//!
//! All paths are written under the session [`SandboxSpec::egress_dir`] so the host daemon
//! and acceptance tests can inspect sandbox failures without parsing inherited stderr.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// stderr captured from the `sandbox-exec` wrapper process.
pub const SANDBOX_EXEC_STDERR_LOG: &str = "sandbox-exec.stderr.log";
/// stdout captured from the `sandbox-exec` wrapper process.
pub const SANDBOX_EXEC_STDOUT_LOG: &str = "sandbox-exec.stdout.log";
/// Structured log from `tddy-tools sandbox-runner` inside the jail.
pub const SANDBOX_RUNNER_LOG: &str = "sandbox-runner.log";
/// Written when sandbox-runner exits with an error (message body is the failure reason).
pub const SANDBOX_RUNNER_FAILURE: &str = "sandbox-runner.failure";
/// JSON manifest written at spawn time (profile path, argv, pid, log paths).
pub const SANDBOX_SPAWN_MANIFEST: &str = "sandbox-spawn.json";

/// Path for a named egress log file.
pub fn egress_log_path(egress_dir: &Path, filename: &str) -> PathBuf {
    egress_dir.join(filename)
}

/// Append one line to an egress log file (creates parent dirs if needed).
pub fn append_line(log_path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(line.as_bytes())?;
    if !line.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    Ok(())
}

/// Read all known sandbox egress logs into one string for error messages / test output.
pub fn format_egress_logs(egress_dir: &Path) -> String {
    format_sandbox_diagnostics(egress_dir, None)
}

/// Like [`format_egress_logs`] but also includes `sandbox-runner.boot.log` under `project_root`.
pub fn format_sandbox_diagnostics(egress_dir: &Path, project_root: Option<&Path>) -> String {
    let mut out = String::new();
    for name in [
        SANDBOX_SPAWN_MANIFEST,
        SANDBOX_EXEC_STDERR_LOG,
        SANDBOX_EXEC_STDOUT_LOG,
        SANDBOX_RUNNER_LOG,
        SANDBOX_RUNNER_FAILURE,
    ] {
        append_log_section(&mut out, &egress_dir.join(name), name);
    }
    if let Some(root) = project_root {
        append_log_section(
            &mut out,
            &root.join("sandbox-runner.boot.log"),
            "sandbox-runner.boot.log",
        );
    }
    if out.is_empty() {
        out.push_str("(no sandbox egress logs found)\n");
    }
    out
}

fn append_log_section(out: &mut String, path: &Path, name: &str) {
    if !path.exists() {
        return;
    }
    let body = std::fs::read_to_string(path).unwrap_or_else(|e| format!("<read error: {e}>"));
    let _ = writeln!(out, "--- {name} ({}) ---\n{body}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_line_creates_log_under_egress_dir() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let log_path = egress_log_path(dir.path(), SANDBOX_RUNNER_LOG);

        // When
        append_line(&log_path, "sandbox-runner started").expect("append must succeed");

        // Then
        let body = std::fs::read_to_string(&log_path).expect("log file must exist");
        assert!(body.contains("sandbox-runner started"));
    }
}
