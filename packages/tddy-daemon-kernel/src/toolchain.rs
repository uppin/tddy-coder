//! Where this deployment's **tddy binaries** live.
//!
//! `tddy-tools`, `tddy-coder`, `tddy-sandbox-runner`, `tddy-index-daemon`,
//! `tddy-remote-git-repo` and `tddy-session-sync` are installed as a *set*, beside the daemon that
//! invokes them (`./install`'s `DESKTOP_BINARIES`, and `scripts/dev-runtime-binaries.sh` for a dev
//! tree). This module is the single place that says so, and the only one entitled to answer "where
//! is binary X".
//!
//! It exists because the answer used to be derived from `allowed_tools[0].path` — a **UI menu**
//! entry naming which `tddy-coder` build to offer an operator. That path is relative in every dev
//! config (`target/debug/tddy-coder`), so swapping the filename for `tddy-tools` produced a
//! relative command that resolved against whatever cwd the consuming process happened to have.
//! For an agent running *beside* a jailed checkout — cwd `…/sessions/<id>/context` — nothing
//! resolved, the MCP server never launched, and the agent came up with every native tool withdrawn
//! and every replacement unreachable. Nothing logged it.
//!
//! Two rules follow from that, and both are enforced here rather than left to callers:
//! a resolved directory is **absolute**, and [`TddyToolchain::binary`] **checks the file exists**
//! so a missing binary fails where it is looked up, naming the path, instead of much later as a
//! session that will not start.

use std::path::{Path, PathBuf};

/// Environment variable naming the toolchain directory explicitly (highest priority).
pub const TOOLCHAIN_DIR_ENV: &str = "TDDY_TOOLCHAIN_DIR";

/// Which rule answered [`TddyToolchain::resolve_from`] — reported in the startup log so a
/// deployment that resolved its binaries from an unexpected place says so once, on the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainSource {
    /// `TDDY_TOOLCHAIN_DIR`.
    Env,
    /// `toolchain_dir:` in the daemon configuration.
    Config,
    /// Beside the running executable — the install contract, and the dev `target/debug` tree.
    SiblingOfExe,
    /// Nothing named a directory: bare names, resolved by `PATH` at spawn time.
    SearchPath,
}

/// A missing binary, named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainBinaryMissing {
    pub name: String,
    pub looked_in: Option<PathBuf>,
}

impl std::fmt::Display for ToolchainBinaryMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.looked_in {
            Some(dir) => write!(
                f,
                "tddy toolchain binary {:?} not found in {}",
                self.name,
                dir.display()
            ),
            None => write!(
                f,
                "tddy toolchain binary {:?} not found: no toolchain directory is configured and \
                 it is not on PATH",
                self.name
            ),
        }
    }
}

impl std::error::Error for ToolchainBinaryMissing {}

/// The directory holding this deployment's tddy binaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TddyToolchain {
    dir: Option<PathBuf>,
    source: ToolchainSource,
}

impl TddyToolchain {
    /// Resolve from already-read inputs, so the rules are testable without touching the process
    /// environment. First match wins; a relative directory is made absolute against `cwd`.
    pub fn resolve_from(
        env_dir: Option<&str>,
        configured_dir: Option<&str>,
        exe_dir: Option<&Path>,
        cwd: &Path,
    ) -> Self {
        let named = |value: Option<&str>| {
            value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        // Absolute here, once, rather than at each spawn: a consumer's cwd is its own business —
        // the agent's is its session context dir, the daemon's is wherever it was started — and a
        // relative toolchain path silently means a different directory to each of them.
        let absolute = |dir: PathBuf| {
            if dir.is_absolute() {
                dir
            } else {
                cwd.join(dir)
            }
        };

        if let Some(dir) = named(env_dir) {
            return Self {
                dir: Some(absolute(dir)),
                source: ToolchainSource::Env,
            };
        }
        if let Some(dir) = named(configured_dir) {
            return Self {
                dir: Some(absolute(dir)),
                source: ToolchainSource::Config,
            };
        }
        if let Some(dir) = exe_dir {
            return Self {
                dir: Some(absolute(dir.to_path_buf())),
                source: ToolchainSource::SiblingOfExe,
            };
        }
        Self {
            dir: None,
            source: ToolchainSource::SearchPath,
        }
    }

    /// Resolve from this process: `TDDY_TOOLCHAIN_DIR`, then `configured_dir`, then the directory
    /// holding the running executable (`target/debug/deps` folded up to `target/debug`, so a test
    /// binary finds the same set a daemon does).
    pub fn resolve(configured_dir: Option<&str>) -> Self {
        let env_dir = std::env::var(TOOLCHAIN_DIR_ENV).ok();
        let exe_dir = std::env::current_exe().ok().and_then(|exe| {
            let mut dir = exe.parent()?.to_path_buf();
            if dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
                dir.pop();
            }
            Some(dir)
        });
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::resolve_from(env_dir.as_deref(), configured_dir, exe_dir.as_deref(), &cwd)
    }

    /// The directory, when one was resolved.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Which rule answered.
    pub fn source(&self) -> ToolchainSource {
        self.source
    }

    /// The absolute path to `name`, verified to exist.
    ///
    /// The existence check is the point: the MCP config, the hook command and every spawned
    /// sibling used to be written from an unchecked path, so a wrong one surfaced as an agent with
    /// no tools rather than as an error naming the file.
    pub fn binary(&self, name: &str) -> Result<PathBuf, ToolchainBinaryMissing> {
        let Some(dir) = self.dir.as_deref() else {
            return Err(ToolchainBinaryMissing {
                name: name.to_string(),
                looked_in: None,
            });
        };
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
        Err(ToolchainBinaryMissing {
            name: name.to_string(),
            looked_in: Some(dir.to_path_buf()),
        })
    }

    /// One line for the startup log: where the binaries are, and which rule said so.
    pub fn describe(&self) -> String {
        match self.dir.as_deref() {
            Some(dir) => format!("{} (from {:?})", dir.display(), self.source),
            None => format!("<PATH lookup> (from {:?})", self.source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"#!/bin/sh\n").expect("write fixture binary");
    }

    #[test]
    fn the_environment_variable_wins_over_every_other_rule() {
        // Given — an env dir, a config dir and an exe dir all naming different places
        let tmp = tempfile::tempdir().expect("tempdir");
        let env_dir = tmp.path().join("from-env");
        fs::create_dir_all(&env_dir).expect("mkdir");

        // When
        let toolchain = TddyToolchain::resolve_from(
            Some(env_dir.to_str().expect("utf8")),
            Some("/from/config"),
            Some(Path::new("/from/exe")),
            tmp.path(),
        );

        // Then
        assert_eq!(toolchain.dir(), Some(env_dir.as_path()));
        assert_eq!(toolchain.source(), ToolchainSource::Env);
    }

    #[test]
    fn configuration_is_used_when_the_environment_names_nothing() {
        let toolchain = TddyToolchain::resolve_from(
            None,
            Some("/from/config"),
            Some(Path::new("/from/exe")),
            Path::new("/cwd"),
        );

        assert_eq!(toolchain.dir(), Some(Path::new("/from/config")));
        assert_eq!(toolchain.source(), ToolchainSource::Config);
    }

    #[test]
    fn the_running_executables_directory_is_the_install_contract() {
        // Given — neither env nor config names a directory, which is every ordinary deployment:
        // `./install` ships the binaries beside the daemon, and a dev tree has them in target/debug
        let toolchain = TddyToolchain::resolve_from(
            None,
            None,
            Some(Path::new("/opt/tddy/bin")),
            Path::new("/cwd"),
        );

        assert_eq!(toolchain.dir(), Some(Path::new("/opt/tddy/bin")));
        assert_eq!(toolchain.source(), ToolchainSource::SiblingOfExe);
    }

    #[test]
    fn a_relative_directory_is_made_absolute_rather_than_left_to_the_consumers_cwd() {
        // Given — a relative configured dir, the exact shape that broke the MCP config: the
        // consumer's cwd was the session context dir, not the workspace root
        let toolchain = TddyToolchain::resolve_from(
            None,
            Some("target/debug"),
            None,
            Path::new("/workspace/root"),
        );

        // Then — resolved against the workspace root, once, here
        assert_eq!(
            toolchain.dir(),
            Some(Path::new("/workspace/root/target/debug"))
        );
    }

    #[test]
    fn nothing_named_leaves_the_search_path_as_the_last_resort() {
        let toolchain = TddyToolchain::resolve_from(None, None, None, Path::new("/cwd"));

        assert_eq!(toolchain.dir(), None);
        assert_eq!(toolchain.source(), ToolchainSource::SearchPath);
    }

    #[test]
    fn a_binary_that_exists_resolves_to_its_absolute_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(tmp.path(), "tddy-tools");
        let toolchain = TddyToolchain::resolve_from(
            Some(tmp.path().to_str().expect("utf8")),
            None,
            None,
            Path::new("/cwd"),
        );

        let resolved = toolchain.binary("tddy-tools").expect("binary resolves");

        assert_eq!(resolved, tmp.path().join("tddy-tools"));
        assert!(
            resolved.is_absolute(),
            "a spawned command must not depend on the caller's cwd"
        );
    }

    #[test]
    fn a_missing_binary_is_refused_naming_the_directory_searched() {
        // Given — a toolchain directory that does not hold the binary (the #518 failure: the MCP
        // config was written pointing at a file that was never there, and nothing said so)
        let tmp = tempfile::tempdir().expect("tempdir");
        let toolchain = TddyToolchain::resolve_from(
            Some(tmp.path().to_str().expect("utf8")),
            None,
            None,
            Path::new("/cwd"),
        );

        let err = toolchain.binary("tddy-tools").expect_err("must refuse");

        assert_eq!(err.name, "tddy-tools");
        assert_eq!(err.looked_in.as_deref(), Some(tmp.path()));
        assert!(
            err.to_string().contains("tddy-tools")
                && err.to_string().contains(&tmp.path().display().to_string()),
            "the refusal must name the binary and where it looked: {err}"
        );
    }
}
