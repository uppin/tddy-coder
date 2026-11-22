//! The [`ToolHost`] a test declares: an environment and a `PATH` it names, over the real filesystem.
//!
//! Every suite that resolves a coding agent or a declared tool needs a host whose answers it
//! controls — so nothing of the developer's own `PATH`, `$HOME` or `TDDY_*_BINARY` variables reaches
//! resolution — while the binaries it resolves are real files the test wrote. That host was written
//! out once per suite (`AFixtureHost`, `APathHost`, `AHostOfStubs`, `AHostOfBinaries`, …); this crate
//! is the one copy (`#tab-hardening` 2/4,
//! `docs/dev/todo/2026-11-21-tool-host-fixtures-repeat-across-the-tab-jail-tests.md`).
//!
//! ```ignore
//! let host = a_tool_host()
//!     .with_env("TDDY_CLAUDE_BINARY", stub.display().to_string())
//!     .with_path([bin_dir])
//!     .build();
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_actions::tool::ToolHost;

/// A [`ToolHost`] builder, empty until told otherwise: no environment variable is set and `PATH`
/// is empty, so a test names every input resolution reads.
#[must_use]
pub fn a_tool_host() -> AToolHostBuilder {
    AToolHostBuilder::default()
}

/// Builds an [`AToolHost`]; see [`a_tool_host`].
#[derive(Debug, Clone, Default)]
pub struct AToolHostBuilder {
    env: BTreeMap<String, String>,
    path: Vec<PathBuf>,
}

impl AToolHostBuilder {
    /// Set the environment variable `name` to `value`. An empty value reads as unset, as
    /// [`ToolHost::env_var`] reads one.
    #[must_use]
    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    /// Set every `(name, value)` of `vars`, as [`Self::with_env`] sets one.
    #[must_use]
    pub fn with_envs<K, V>(self, vars: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        vars.into_iter()
            .fold(self, |builder, (name, value)| builder.with_env(name, value))
    }

    /// `$HOME`: [`Self::with_env`] for `HOME`.
    #[must_use]
    pub fn with_home(self, home: impl AsRef<Path>) -> Self {
        self.with_env("HOME", home.as_ref().display().to_string())
    }

    /// Append `dirs` to `PATH`, in search order.
    #[must_use]
    pub fn with_path<P: Into<PathBuf>>(mut self, dirs: impl IntoIterator<Item = P>) -> Self {
        self.path.extend(dirs.into_iter().map(Into::into));
        self
    }

    /// The host.
    #[must_use]
    pub fn build(self) -> AToolHost {
        AToolHost {
            env: self.env,
            path: self.path,
        }
    }
}

/// A [`ToolHost`] whose environment and `PATH` are what its builder declared, and whose filesystem
/// is the real one: a path is executable when it is a regular file with an execute bit, and is
/// canonicalized by the OS.
#[derive(Debug, Clone)]
pub struct AToolHost {
    env: BTreeMap<String, String>,
    path: Vec<PathBuf>,
}

impl ToolHost for AToolHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env
            .get(name)
            .filter(|value| !value.is_empty())
            .cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use pretty_assertions::assert_eq;

    use super::*;

    /// A directory holding one executable `agent` and one non-executable `notes`.
    fn a_bin_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent");
        std::fs::write(&agent, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.path().join("notes"), "").unwrap();
        dir
    }

    #[test]
    fn a_tool_host_answers_only_the_environment_it_was_given() {
        // Given
        let host = a_tool_host()
            .with_env("TDDY_CLAUDE_BINARY", "/opt/stub/claude")
            .with_envs([("TDDY_CURSOR_BINARY", "/opt/stub/cursor")])
            .with_home("/home/fixture")
            .build();

        // When
        let answers = [
            host.env_var("TDDY_CLAUDE_BINARY"),
            host.env_var("TDDY_CURSOR_BINARY"),
            host.env_var("HOME"),
            host.env_var("PATH"),
        ];

        // Then
        assert_eq!(
            answers,
            [
                Some("/opt/stub/claude".to_string()),
                Some("/opt/stub/cursor".to_string()),
                Some("/home/fixture".to_string()),
                None,
            ]
        );
    }

    #[test]
    fn an_empty_value_reads_as_unset() {
        // Given
        let host = a_tool_host().with_env("TDDY_CLAUDE_BINARY", "").build();

        // When
        let answer = host.env_var("TDDY_CLAUDE_BINARY");

        // Then
        assert_eq!(answer, None);
    }

    #[test]
    fn a_tool_host_searches_the_path_it_was_given_in_order() {
        // Given
        let host = a_tool_host()
            .with_path(["/first"])
            .with_path(["/second", "/third"])
            .build();

        // When
        let path = host.path_dirs();

        // Then
        assert_eq!(
            path,
            vec![
                PathBuf::from("/first"),
                PathBuf::from("/second"),
                PathBuf::from("/third")
            ]
        );
    }

    #[test]
    fn a_tool_host_reads_the_real_filesystem() {
        // Given
        let bin = a_bin_dir();
        let host = a_tool_host().with_path([bin.path()]).build();

        // When
        let answers = (
            host.is_executable_file(&bin.path().join("agent")),
            host.is_executable_file(&bin.path().join("notes")),
            host.is_executable_file(bin.path()),
            host.canonicalize(&bin.path().join("agent")).unwrap(),
        );

        // Then
        assert_eq!(
            answers,
            (
                true,
                false,
                false,
                std::fs::canonicalize(bin.path().join("agent")).unwrap()
            )
        );
    }
}
