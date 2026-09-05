//! Where the embedded daemon's configuration comes from.
//!
//! The rules are the ones `./web-dev` and `scripts/desktop-dev.sh` follow, because a daemon this
//! application hosts must be configurable exactly like one started from a shell: a workspace root,
//! the repo `.env` loaded without overriding anything already set, and either
//! `TDDY_DAEMON_CONFIG` or the repo-root `dev.desktop.yaml`.
//!
//! Nothing here guesses. A workspace root that cannot be found and a configuration file that does
//! not exist are both errors that stop the application, because a desktop app that silently starts
//! a daemon with a configuration nobody chose is worse than one that refuses to start.

use std::path::{Path, PathBuf};

use tddy_daemon::config::DaemonConfig;

/// Repo-root filename used when `TDDY_DAEMON_CONFIG` is unset (desktop dev).
const DESKTOP_DEV_CONFIG_FILENAME: &str = "dev.desktop.yaml";

/// How far up a directory tree the workspace-root search walks before giving up.
const MAX_UPWARD_STEPS: usize = 20;

/// The workspace the daemon is configured from and runs in.
pub struct DaemonConfigSource {
    /// The repo root. Also the process working directory once [`resolve`] has run, so relative
    /// paths in the YAML (`web_bundle_path`, log files, tool paths) mean what they mean for
    /// `./web-dev` and for the `tddy-daemon` binary started from the repo root.
    pub workspace_root: PathBuf,
    /// The YAML the daemon is loaded from.
    pub config_path: PathBuf,
}

/// Resolve the workspace root, move into it, load its `.env`, and name the daemon's YAML.
///
/// The working-directory change happens here rather than at the call site because everything after
/// it — the `.env` path, the default config path, and every relative path inside the YAML — is
/// resolved against the repo root.
pub fn resolve() -> anyhow::Result<DaemonConfigSource> {
    // Read before the working directory moves: a relative `TDDY_DAEMON_CONFIG` names a file
    // relative to wherever the application was launched from.
    let explicit_config = env_path("TDDY_DAEMON_CONFIG")
        .map(|path| absolutise(&path))
        .transpose()?;

    let workspace_root = workspace_root()?;
    std::env::set_current_dir(&workspace_root).map_err(|error| {
        anyhow::anyhow!(
            "could not enter the workspace root {}: {error}",
            workspace_root.display()
        )
    })?;
    load_dot_env_without_overriding(&workspace_root)?;

    let config_path = match explicit_config {
        Some(path) => path,
        None => {
            let default = workspace_root.join(DESKTOP_DEV_CONFIG_FILENAME);
            if !default.is_file() {
                anyhow::bail!(
                    "no daemon configuration: set TDDY_DAEMON_CONFIG, or add {DESKTOP_DEV_CONFIG_FILENAME} at {}",
                    workspace_root.display()
                );
            }
            default
        }
    };
    if !config_path.is_file() {
        anyhow::bail!(
            "daemon configuration {} does not exist",
            config_path.display()
        );
    }

    Ok(DaemonConfigSource {
        workspace_root,
        config_path,
    })
}

impl DaemonConfigSource {
    /// Parse [`Self::config_path`] into a daemon configuration.
    ///
    /// `CURRENT_USER` is substituted with the OS user running the application, the way `./web-dev`
    /// substitutes it before starting a daemon: the dev configs map a GitHub login to
    /// `os_user: "CURRENT_USER"`, and a daemon that took that literally would spawn every tool as
    /// a user that does not exist. The substituted text is parsed through a
    /// temporary copy so the configuration is read by exactly the loader the `tddy-daemon` binary
    /// uses, error messages included.
    pub fn load_config(&self) -> anyhow::Result<DaemonConfig> {
        let text = std::fs::read_to_string(&self.config_path).map_err(|error| {
            anyhow::anyhow!(
                "failed to read config {}: {error}",
                self.config_path.display()
            )
        })?;
        if !text.contains(CURRENT_USER_PLACEHOLDER) {
            return DaemonConfig::load(&self.config_path);
        }

        let user = current_os_user().ok_or_else(|| {
            anyhow::anyhow!(
                "{} contains {CURRENT_USER_PLACEHOLDER}, but neither USER nor USERNAME names the OS user to substitute",
                self.config_path.display()
            )
        })?;
        let substituted = SubstitutedConfig::write(&text.replace(CURRENT_USER_PLACEHOLDER, &user))?;
        DaemonConfig::load(&substituted.path)
    }
}

/// The dev configs' stand-in for whoever is running the daemon.
const CURRENT_USER_PLACEHOLDER: &str = "CURRENT_USER";

/// A temporary copy of the configuration, removed as soon as it has been parsed.
struct SubstitutedConfig {
    directory: PathBuf,
    path: PathBuf,
}

impl SubstitutedConfig {
    fn write(contents: &str) -> anyhow::Result<Self> {
        let directory =
            std::env::temp_dir().join(format!("tddy-desktop-daemon-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|error| {
            anyhow::anyhow!("could not create {}: {error}", directory.display())
        })?;
        let path = directory.join("config.yaml");
        std::fs::write(&path, contents)
            .map_err(|error| anyhow::anyhow!("could not write {}: {error}", path.display()))?;
        Ok(Self { directory, path })
    }
}

impl Drop for SubstitutedConfig {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory) {
            log::warn!(
                "[tddy-desktop] could not remove the temporary config directory {}: {error}",
                self.directory.display()
            );
        }
    }
}

/// The OS user this application runs as, as the shell reports it.
fn current_os_user() -> Option<String> {
    for name in ["USER", "USERNAME"] {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// The repo root: `TDDY_WORKSPACE_ROOT`, else the nearest ancestor of the working directory that
/// looks like this repo, else the nearest such ancestor of the executable (a `cargo tauri dev`
/// binary lives under `target/`, and a bundled one does not).
fn workspace_root() -> anyhow::Result<PathBuf> {
    if let Some(explicit) = env_path("TDDY_WORKSPACE_ROOT") {
        if !explicit.is_dir() {
            anyhow::bail!(
                "TDDY_WORKSPACE_ROOT points at {}, which is not a directory",
                explicit.display()
            );
        }
        return absolutise(&explicit);
    }
    let from_cwd = std::env::current_dir()?;
    if let Some(root) = search_upwards(&from_cwd) {
        return Ok(root);
    }
    let executable = std::env::current_exe()?;
    if let Some(root) = executable.parent().and_then(search_upwards) {
        return Ok(root);
    }
    anyhow::bail!(
        "could not find the tddy-coder workspace root above {} or {}: set TDDY_WORKSPACE_ROOT",
        from_cwd.display(),
        executable.display()
    )
}

/// Walk up from `start` for the first directory that is a tddy-coder checkout.
fn search_upwards(start: &Path) -> Option<PathBuf> {
    let mut directory = start;
    for _ in 0..MAX_UPWARD_STEPS {
        if is_workspace_root(directory) {
            return Some(directory.to_path_buf());
        }
        directory = directory.parent()?;
    }
    None
}

/// A directory is the repo root when it holds the desktop dev config, or the workspace manifest
/// alongside this package.
fn is_workspace_root(directory: &Path) -> bool {
    directory.join(DESKTOP_DEV_CONFIG_FILENAME).is_file()
        || (directory.join("Cargo.toml").is_file()
            && directory
                .join("packages/tddy-desktop/package.json")
                .is_file())
}

/// Apply `repo_root/.env` to this process, leaving every variable that is already set alone —
/// the same rule `./web-dev` and `scripts/desktop-dev.sh` follow, so a variable exported in the
/// shell wins over the file.
fn load_dot_env_without_overriding(repo_root: &Path) -> anyhow::Result<()> {
    let path = repo_root.join(".env");
    if !path.is_file() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("could not read {}: {error}", path.display()))?;
    for line in text.lines() {
        let Some((key, value)) = parse_dot_env_line(line) else {
            continue;
        };
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

/// One `KEY=value` assignment, with surrounding quotes stripped. Blank lines, comments and lines
/// with no assignment yield nothing.
fn parse_dot_env_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, unquote(value.trim())))
}

/// Strip one matching pair of surrounding quotes.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// A non-blank environment variable as a path.
fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// `path` against the current working directory, without requiring it to exist.
fn absolutise(path: &Path) -> anyhow::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::a_plain_assignment("LIVEKIT_URL=ws://host:7880", "LIVEKIT_URL", "ws://host:7880")]
    #[case::double_quoted("GITHUB_TOKEN=\"ghp_secret\"", "GITHUB_TOKEN", "ghp_secret")]
    #[case::single_quoted("GITHUB_TOKEN='ghp_secret'", "GITHUB_TOKEN", "ghp_secret")]
    #[case::an_equals_sign_inside_the_value("QUERY=a=b", "QUERY", "a=b")]
    fn reads_the_name_and_value_an_assignment_carries(
        #[case] line: &str,
        #[case] name: &str,
        #[case] value: &str,
    ) {
        // Given a line of a `.env` file holding an assignment

        // When it is parsed
        let assignment = parse_dot_env_line(line);

        // Then the name and the value come back, without the quotes that wrapped it
        assert_eq!(assignment, Some((name, value)));
    }

    #[rstest]
    #[case::empty("")]
    #[case::only_whitespace("   ")]
    #[case::a_comment("# LIVEKIT_URL=ws://host:7880")]
    #[case::a_name_with_no_value("LIVEKIT_URL")]
    fn reads_no_assignment_from_a_line_that_carries_none(#[case] line: &str) {
        // Given a line of a `.env` file that assigns nothing

        // When it is parsed
        let assignment = parse_dot_env_line(line);

        // Then nothing is read from it, rather than an empty name or value
        assert_eq!(assignment, None);
    }
}
