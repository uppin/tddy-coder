//! Where the embedded daemon's configuration comes from.
//!
//! **The build profile decides, and the two profiles share nothing.**
//!
//! A *release* build is an installed application: it reads `~/.tddy/desktop.yaml` and nothing else.
//! No `TDDY_DAEMON_CONFIG`, no repo-root `dev.desktop.yaml`, no walk up the directory tree looking
//! for a checkout. An installed `.app` launched from the Dock has `/` for a working directory and
//! lives under `~/Applications`, so every one of those rules could only ever find a checkout by
//! accident — and a desktop application whose daemon configuration depends on which directory
//! Finder happened to hand it is not configured, it is guessing. `./install --desktop` writes that
//! one file.
//!
//! A *debug* build is a development run: the rules are the ones `./web-dev` and `./desktop-dev`
//! follow, because a daemon hosted by a `cargo tauri dev` binary must be configurable exactly like
//! one started from a shell — a workspace root, the repo `.env` loaded without overriding anything
//! already set, and either `TDDY_DAEMON_CONFIG` or the repo-root `dev.desktop.yaml`.
//!
//! Nothing here guesses. A missing home directory, a workspace root that cannot be found and a
//! configuration file that does not exist are all errors that stop the application, because a
//! desktop app that silently starts a daemon with a configuration nobody chose is worse than one
//! that refuses to start.

use std::path::{Path, PathBuf};

use tddy_daemon::config::DaemonConfig;

/// Repo-root filename used when `TDDY_DAEMON_CONFIG` is unset (desktop dev).
const DESKTOP_DEV_CONFIG_FILENAME: &str = "dev.desktop.yaml";

/// The installed application's directory under the operator's home. The same `~/.tddy` the daemon
/// resolves as its data directory in a release build (`tddy_data_dir`), so an installed app keeps
/// its configuration next to the sessions that configuration produces.
const INSTALLED_DIRNAME: &str = ".tddy";

/// The one file a release build reads its daemon configuration from, inside [`INSTALLED_DIRNAME`].
/// Written by `./install --desktop` from the repo's `desktop.yaml.production` template.
const INSTALLED_CONFIG_FILENAME: &str = "desktop.yaml";

/// How far up a directory tree the workspace-root search walks before giving up.
const MAX_UPWARD_STEPS: usize = 20;

/// The workspace the daemon is configured from and runs in.
#[derive(Debug)]
pub struct DaemonConfigSource {
    /// The repo root. Also the process working directory once [`resolve`] has run, so relative
    /// paths in the YAML (`web_bundle_path`, log files, tool paths) mean what they mean for
    /// `./web-dev` and for the `tddy-daemon` binary started from the repo root.
    pub workspace_root: PathBuf,
    /// The YAML the daemon is loaded from.
    pub config_path: PathBuf,
}

/// Resolve where the daemon is configured from, move into that directory, and load its `.env`.
///
/// The working-directory change happens here rather than at the call site because everything after
/// it — the `.env` path and every relative path inside the YAML — is resolved against it.
pub fn resolve() -> anyhow::Result<DaemonConfigSource> {
    // Both branches run *before* the working directory moves, because both read paths that are
    // relative to wherever the application was launched from.
    let source = if cfg!(debug_assertions) {
        development_source()?
    } else {
        installed_source(&home_directory()?)?
    };

    std::env::set_current_dir(&source.workspace_root).map_err(|error| {
        anyhow::anyhow!(
            "could not enter {}: {error}",
            source.workspace_root.display()
        )
    })?;
    load_dot_env_without_overriding(&source.workspace_root)?;

    Ok(source)
}

/// An installed application: `~/.tddy/desktop.yaml`, and nothing else.
///
/// Takes the home directory as a parameter rather than reading it, so the rule is testable without
/// process-wide state — the same reason `tddy_data_dir_for` in the daemon's runtime does.
fn installed_source(home: &Path) -> anyhow::Result<DaemonConfigSource> {
    let workspace_root = home.join(INSTALLED_DIRNAME);
    let config_path = workspace_root.join(INSTALLED_CONFIG_FILENAME);
    if !config_path.is_file() {
        anyhow::bail!(
            "no daemon configuration at {}: run `./install --desktop` from a tddy-coder checkout to write one",
            config_path.display()
        );
    }
    Ok(DaemonConfigSource {
        workspace_root,
        config_path,
    })
}

/// A development run (`./desktop-dev`, `cargo tauri dev`): `TDDY_DAEMON_CONFIG`, else the repo-root
/// `dev.desktop.yaml` of the checkout this binary was started from or built in.
fn development_source() -> anyhow::Result<DaemonConfigSource> {
    let explicit_config = env_path("TDDY_DAEMON_CONFIG")
        .map(|path| absolutise(&path))
        .transpose()?;

    let workspace_root = workspace_root()?;

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

/// The operator's home directory. A release build has nowhere else to look, so an unset `HOME` is a
/// startup failure rather than a guess at `/root` or the current directory.
fn home_directory() -> anyhow::Result<PathBuf> {
    env_path("HOME").ok_or_else(|| {
        anyhow::anyhow!(
            "HOME names no directory, so the installed daemon configuration \
             (~/{INSTALLED_DIRNAME}/{INSTALLED_CONFIG_FILENAME}) cannot be found"
        )
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

    /// A throwaway home directory. Built the way `SubstitutedConfig` builds its own directory — a
    /// named child of the system temp directory — so these tests add no dependency to a crate that
    /// is shipped as an application.
    struct FakeHome(PathBuf);

    impl FakeHome {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "tddy-desktop-config-source-{}-{label}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create the fake home");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        /// Write `~/.tddy/desktop.yaml`, as `./install --desktop` does.
        fn with_installed_config(self) -> Self {
            let directory = self.0.join(INSTALLED_DIRNAME);
            std::fs::create_dir_all(&directory).expect("create the installed directory");
            std::fs::write(directory.join(INSTALLED_CONFIG_FILENAME), "users: []\n")
                .expect("write the installed config");
            self
        }
    }

    impl Drop for FakeHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_installed_application_is_configured_from_the_one_file_under_the_home_directory() {
        // Given a home directory holding the configuration `./install --desktop` writes
        let home = FakeHome::new("installed").with_installed_config();

        // When the installed application resolves where it is configured from
        let source = installed_source(home.path()).expect("resolve the installed source");

        // Then it is that file, and the directory holding it is what the process moves into, so the
        // relative paths inside the YAML resolve against the same `~/.tddy` the sessions live in
        assert_eq!(source.workspace_root, home.path().join(INSTALLED_DIRNAME));
        assert_eq!(
            source.config_path,
            home.path()
                .join(INSTALLED_DIRNAME)
                .join(INSTALLED_CONFIG_FILENAME)
        );
    }

    #[test]
    fn an_installed_application_without_that_file_refuses_to_start_and_names_the_installer() {
        // Given a home directory that has never had `./install --desktop` run against it
        let home = FakeHome::new("uninstalled");

        // When the installed application resolves where it is configured from
        let error =
            installed_source(home.path()).expect_err("an unconfigured home must not resolve");

        // Then it fails naming the file it looked for and the command that writes it, rather than
        // falling back to a checkout that happens to be above the working directory
        let message = error.to_string();
        assert!(
            message.contains(INSTALLED_CONFIG_FILENAME),
            "the error must name the file it looked for; got: {message}"
        );
        assert!(
            message.contains("./install --desktop"),
            "the error must name the command that writes it; got: {message}"
        );
    }

    /// Sets an environment variable for the body of one test and puts the process back afterwards,
    /// panic or not.
    struct EnvGuard {
        name: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(name: &'static str, value: &Path) -> Self {
            let previous = std::env::var_os(name);
            std::env::set_var(name, value);
            Self { name, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }

    #[test]
    fn an_installed_application_never_reads_the_development_configuration() {
        // Given a home directory holding the installed configuration, and a checkout inside it
        // that a development build would have preferred — named by both development variables
        let home = FakeHome::new("ignores-dev-config").with_installed_config();
        let checkout = home.path().join("some-checkout");
        std::fs::create_dir_all(&checkout).expect("create the decoy checkout");
        let decoy = checkout.join(DESKTOP_DEV_CONFIG_FILENAME);
        std::fs::write(&decoy, "users: []\n").expect("write the decoy config");
        let _config = EnvGuard::set("TDDY_DAEMON_CONFIG", &decoy);
        let _root = EnvGuard::set("TDDY_WORKSPACE_ROOT", &checkout);

        // When the installed application resolves where it is configured from
        let source = installed_source(home.path()).expect("resolve the installed source");

        // Then none of it is consulted: a release build reads `~/.tddy/desktop.yaml` and nothing
        // else, so which directory Finder launched the bundle from cannot change its configuration
        assert_eq!(
            source.config_path,
            home.path()
                .join(INSTALLED_DIRNAME)
                .join(INSTALLED_CONFIG_FILENAME)
        );
        assert_eq!(source.workspace_root, home.path().join(INSTALLED_DIRNAME));
    }
}
