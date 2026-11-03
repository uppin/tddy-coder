//! A tmux-run agent's environment, handed to its pane through a file — never through an argv.
//!
//! A tmux pane inherits the tmux *server's* environment, so the agent's own (toolcall socket,
//! hooks, provider route — a credential among them) has to be given to the pane explicitly.
//! `new-session -e K=V` would put every value on tmux's argv, readable by any local user through
//! `ps` while that client runs. Instead the values are written, shell-quoted, to a `0600` file
//! owned by the session's OS user, and the pane's command sources it, deletes it and `exec`s the
//! agent ([`PANE_SCRIPT`]). No value ever appears on an argv.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::TmuxError;

/// The directory, inside a session's directory, its pane environment files are written to. It is
/// owned by the session's OS user (mode `0700`) so the pane — running as that user — can delete the
/// file it sourced.
pub const PANE_ENV_DIR: &str = "tmux-env";

/// The shell a pane's command runs: absolute, so it does not depend on the pane's `PATH`.
const PANE_SHELL: &str = "/bin/sh";

/// What the pane runs, as `sh -c PANE_SCRIPT sh <env file> <argv…>`: source the environment file,
/// delete it, then `exec` the agent's argv in its place.
pub const PANE_SCRIPT: &str = r#". "$1"; rm -f -- "$1"; shift; exec "$@""#;

/// The owner a pane environment file (and its directory) is handed to, when it is not this
/// process's own user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneEnvOwner {
    pub uid: u32,
    pub gid: u32,
}

/// The pane's command: [`PANE_SCRIPT`] sourcing `env_file`, then `argv`.
pub fn pane_command(env_file: &Path, argv: &[String]) -> Vec<String> {
    let mut command = vec![
        PANE_SHELL.to_string(),
        "-c".to_string(),
        PANE_SCRIPT.to_string(),
        "sh".to_string(),
        env_file.to_string_lossy().into_owned(),
    ];
    command.extend(argv.iter().cloned());
    command
}

/// Write `env` to a new `0600` file under `session_dir/`[`PANE_ENV_DIR`], one `export K='v'` per
/// variable, and hand the file and its directory to `owner` when given. Returns the file's path.
///
/// The file is created with mode `0600` (never wider, not even briefly) and a fresh name, so an
/// existing file is never written through. A relative `session_dir`, or a variable name that is not
/// a shell name, is refused.
pub fn write_pane_env_file(
    session_dir: &Path,
    owner: Option<PaneEnvOwner>,
    env: &[(String, String)],
) -> Result<PathBuf, TmuxError> {
    if !session_dir.is_absolute() {
        return Err(TmuxError::new(format!(
            "the pane environment needs the session's absolute directory, not {}",
            session_dir.display()
        )));
    }
    let contents = shell_exports(env)?;
    let dir = session_dir.join(PANE_ENV_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| io_error("create", &dir, e))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| io_error("restrict", &dir, e))?;
    hand_to(&dir, owner)?;
    let path = dir.join(format!("{}.env", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| io_error("create", &path, e))?;
    let written = file
        .write_all(contents.as_bytes())
        .map_err(|e| io_error("write", &path, e))
        .and_then(|()| hand_to(&path, owner));
    if let Err(error) = written {
        remove_pane_env_file(&path);
        return Err(error);
    }
    Ok(path)
}

/// Remove a pane environment file the pane will never source (its start failed). A file that is
/// already gone is fine; any other failure is logged, since the start's own error is the one the
/// caller reports.
pub fn remove_pane_env_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!(
            target: "tddy_daemon::tmux",
            "could not remove the pane environment file {}: {e}",
            path.display()
        ),
    }
}

/// `env` as `export K='v'` lines, each value single-quoted (`'` written as `'\''`).
fn shell_exports(env: &[(String, String)]) -> Result<String, TmuxError> {
    let mut out = String::new();
    for (key, value) in env {
        if !is_shell_name(key) {
            return Err(TmuxError::new(format!(
                "{key:?} is not a shell variable name, so it cannot be set on a tmux pane"
            )));
        }
        out.push_str(&format!(
            "export {key}='{}'\n",
            value.replace('\'', r"'\''")
        ));
    }
    Ok(out)
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
fn is_shell_name(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn hand_to(path: &Path, owner: Option<PaneEnvOwner>) -> Result<(), TmuxError> {
    match owner {
        None => Ok(()),
        Some(PaneEnvOwner { uid, gid }) => std::os::unix::fs::chown(path, Some(uid), Some(gid))
            .map_err(|e| io_error(&format!("give uid {uid}"), path, e)),
    }
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> TmuxError {
    TmuxError::new(format!(
        "could not {action} the pane environment file {}: {error}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINEL: &str = "sk-sentinel-it's-$HOME-`x`";

    fn the_session_env() -> Vec<(String, String)> {
        vec![
            ("ANTHROPIC_AUTH_TOKEN".to_string(), SENTINEL.to_string()),
            ("TDDY_SOCKET".to_string(), "/tmp/s.sock".to_string()),
        ]
    }

    #[test]
    fn the_env_file_is_created_owner_only_in_the_sessions_pane_env_directory() {
        // Given
        let session_dir = tempfile::tempdir().unwrap();

        // When
        let path = write_pane_env_file(session_dir.path(), None, &the_session_env()).unwrap();

        // Then
        assert_eq!(
            path.parent().unwrap(),
            session_dir.path().join(PANE_ENV_DIR)
        );
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{mode:o}");
        let dir_mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700, "{dir_mode:o}");
    }

    #[test]
    fn the_pane_command_carries_no_environment_value() {
        // Given
        let session_dir = tempfile::tempdir().unwrap();
        let path = write_pane_env_file(session_dir.path(), None, &the_session_env()).unwrap();

        // When
        let command = pane_command(&path, &["/usr/bin/claude".to_string()]);

        // Then
        assert!(
            command.iter().all(|arg| !arg.contains("sk-sentinel")),
            "{command:?}"
        );
        assert_eq!(command.last().unwrap(), "/usr/bin/claude");
    }

    #[test]
    fn the_pane_command_sources_the_values_exactly_deletes_the_file_and_runs_the_argv() {
        // Given
        let session_dir = tempfile::tempdir().unwrap();
        let path = write_pane_env_file(session_dir.path(), None, &the_session_env()).unwrap();
        let argv = [
            "/bin/sh".to_string(),
            "-c".to_string(),
            r#"printf %s "$ANTHROPIC_AUTH_TOKEN""#.to_string(),
        ];

        // When
        let command = pane_command(&path, &argv);
        let output = std::process::Command::new(&command[0])
            .args(&command[1..])
            .env_remove("ANTHROPIC_AUTH_TOKEN")
            .output()
            .unwrap();

        // Then
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), SENTINEL);
        assert!(!path.exists(), "the pane deletes the file it sourced");
    }

    #[test]
    fn a_variable_name_that_is_not_a_shell_name_is_refused_and_nothing_is_written() {
        // Given
        let session_dir = tempfile::tempdir().unwrap();
        let env = vec![("NOT A NAME".to_string(), "x".to_string())];

        // When
        let refusal = write_pane_env_file(session_dir.path(), None, &env).unwrap_err();

        // Then
        assert!(refusal.to_string().contains("NOT A NAME"), "{refusal}");
        assert!(!session_dir.path().join(PANE_ENV_DIR).exists());
    }
}
