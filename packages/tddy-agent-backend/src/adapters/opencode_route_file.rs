//! `opencode-route-file`: a provider route's OpenCode configuration written owner-only and named in
//! `OPENCODE_CONFIG`, held for the agent's lifetime, as an ACP hooks adapter (`#agent-tools 11/12`).
//!
//! Moved here from `backend/opencode_acp.rs` (`RouteFiles`, `RouteFilesHold`).

use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::acp_hooks::{AcpHooksAdapter, AcpHooksLaunch, PreparedAcpHooks};
use crate::backend::acp_driver::AcpAgentHooks;
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;

/// The adapter that writes and holds an OpenCode route's configuration file.
pub const OPENCODE_ROUTE_FILE: &str = "opencode-route-file";

/// The variable `opencode` reads its configuration file's path from.
const OPENCODE_CONFIG_ENV: &str = "OPENCODE_CONFIG";

/// A route's files hold the account's endpoint, so only the owner may read the directory or them.
const OWNER_ONLY_DIR: u32 = 0o700;
const OWNER_ONLY_FILE: u32 = 0o600;

/// [`OPENCODE_ROUTE_FILE`].
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenCodeRouteFile;

impl AcpHooksAdapter for OpenCodeRouteFile {
    fn name(&self) -> &str {
        OPENCODE_ROUTE_FILE
    }

    /// On a route with files: each written owner-only into a fresh owner-only directory,
    /// `OPENCODE_CONFIG` added to the command naming the first, and hooks that hold the directory
    /// until the driver ends. Without a route, or a route with no files, the command is not changed
    /// and the hooks hold nothing. A route file name that is not a bare file name is refused.
    fn prepare(&self, launch: AcpHooksLaunch<'_>) -> Result<PreparedAcpHooks, BackendError> {
        let AcpHooksLaunch { command, route, .. } = launch;
        let files = match route.filter(|route| !route.files.is_empty()) {
            Some(route) => Some(RouteFiles::write(route).map_err(|e| {
                BackendError::InvocationFailed(format!(
                    "failed to write the OpenCode route files: {e}"
                ))
            })?),
            None => None,
        };
        if let Some(config) = files.as_ref().and_then(|files| files.config.as_deref()) {
            let config = config.to_str().ok_or_else(|| {
                BackendError::InvocationFailed(format!(
                    "the OpenCode route file {} has a path that is not UTF-8",
                    config.display()
                ))
            })?;
            command
                .env
                .push((OPENCODE_CONFIG_ENV.to_string(), config.to_string()));
        }
        Ok(PreparedAcpHooks {
            hooks: Arc::new(RouteFilesHold { _files: files }),
            opens: None,
        })
    }
}

/// The directory a route's files are written into. It is created owner-only, held by the driver
/// that runs the agent, and removed when that driver ends.
struct RouteFiles {
    dir: PathBuf,
    /// The file `OPENCODE_CONFIG` names: the route's first file.
    config: Option<PathBuf>,
}

impl RouteFiles {
    fn write(route: &HarnessRoute) -> io::Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "tddy-opencode-acp-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(name);
        // `create` fails rather than reusing an existing path, so nothing planted there is written to.
        std::fs::DirBuilder::new()
            .mode(OWNER_ONLY_DIR)
            .create(&dir)?;
        // Built before any file is written, so a failed write still removes the directory.
        let mut files = RouteFiles { dir, config: None };
        for file in &route.files {
            let path = files.dir.join(bare_file_name(&file.name)?);
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(OWNER_ONLY_FILE)
                .open(&path)?;
            out.write_all(file.contents.as_bytes())?;
        }
        files.config = route.files.first().map(|first| files.dir.join(&first.name));
        Ok(files)
    }
}

/// `name` when it is a bare file name, so joining it to the route's directory stays inside it. A
/// route names its files, and a name with a separator, `..` or a root would write elsewhere.
fn bare_file_name(name: &str) -> io::Result<&Path> {
    let path = Path::new(name);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(path),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("route file name {name:?} is not a bare file name"),
        )),
    }
}

impl Drop for RouteFiles {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            log::warn!(
                "could not remove the OpenCode route files at {}: {e}",
                self.dir.display()
            );
        }
    }
}

/// The OpenCode driver's hooks. They hold the route's files, so the files outlive the agent child
/// and are removed with the driver.
struct RouteFilesHold {
    _files: Option<RouteFiles>,
}

impl AcpAgentHooks for RouteFilesHold {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::acp_driver::AcpAgentCommand;
    use crate::provider_route::{HarnessRoute, RouteFile};

    fn route_with_file(name: &str) -> HarnessRoute {
        HarnessRoute {
            files: vec![RouteFile {
                name: name.to_string(),
                contents: "{}".to_string(),
            }],
            ..HarnessRoute::default()
        }
    }

    #[test]
    fn a_route_file_is_written_owner_only_and_named_for_opencode_config() {
        // When
        let files = RouteFiles::write(&route_with_file("opencode.json")).expect("written");

        // Then
        let config = files.config.clone().expect("the first file is the config");
        assert_eq!(config, files.dir.join("opencode.json"));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "{}");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&files.dir), OWNER_ONLY_DIR);
        assert_eq!(mode(&config), OWNER_ONLY_FILE);
    }

    #[test]
    fn the_route_directory_is_removed_with_its_files() {
        // Given
        let files = RouteFiles::write(&route_with_file("opencode.json")).expect("written");
        let dir = files.dir.clone();

        // When
        drop(files);

        // Then
        assert!(!dir.exists(), "{}", dir.display());
    }

    #[test]
    fn a_route_file_name_that_leaves_its_directory_is_refused() {
        for name in [
            "../escape.json",
            "nested/opencode.json",
            "/tmp/opencode.json",
            "..",
            "",
        ] {
            // When
            let refused = RouteFiles::write(&route_with_file(name));

            // Then
            let error = refused
                .err()
                .unwrap_or_else(|| panic!("{name:?} was accepted"));
            assert_eq!(
                error.kind(),
                io::ErrorKind::InvalidInput,
                "{name:?}: {error}"
            );
        }
    }
    const ROUTE_CONFIG: &str = r#"{"provider":{"tddy-route":{}}}"#;

    #[test]
    fn opencode_route_file_names_the_routes_first_file_in_opencode_config_written_owner_only() {
        // Given
        let mut command = AcpAgentCommand {
            program: PathBuf::from("/usr/local/bin/opencode"),
            args: vec!["acp".to_string()],
            env: Vec::new(),
            cwd: None,
        };
        let route = HarnessRoute {
            files: vec![RouteFile {
                name: "opencode.json".to_string(),
                contents: ROUTE_CONFIG.to_string(),
            }],
            ..HarnessRoute::default()
        };

        // When
        let prepared = OpenCodeRouteFile
            .prepare(AcpHooksLaunch {
                command: &mut command,
                route: Some(&route),
                deps: &BTreeMap::new(),
            })
            .expect("the route file is written");

        // Then
        let named: Vec<(String, String)> = command.env.clone();
        let config = Path::new(&named[0].1);
        assert_eq!(
            (
                named[0].0.as_str(),
                config.file_name().and_then(|name| name.to_str()),
                std::fs::read_to_string(config).expect("the route file exists"),
                std::fs::metadata(config)
                    .expect("file")
                    .permissions()
                    .mode()
                    & 0o777,
                std::fs::metadata(config.parent().expect("dir"))
                    .expect("dir")
                    .permissions()
                    .mode()
                    & 0o777,
                prepared.opens.is_none(),
            ),
            (
                "OPENCODE_CONFIG",
                Some("opencode.json"),
                ROUTE_CONFIG.to_string(),
                0o600,
                0o700,
                true,
            )
        );
    }

    #[test]
    fn opencode_route_file_without_a_route_leaves_the_command_alone() {
        // Given
        let mut command = AcpAgentCommand {
            program: PathBuf::from("/usr/local/bin/opencode"),
            args: vec!["acp".to_string()],
            env: Vec::new(),
            cwd: None,
        };

        // When
        let prepared = OpenCodeRouteFile.prepare(AcpHooksLaunch {
            command: &mut command,
            route: None,
            deps: &BTreeMap::new(),
        });

        // Then
        assert_eq!(
            (
                prepared.map(|hooks| hooks.opens.is_none()).ok(),
                command.env
            ),
            (Some(true), Vec::new())
        );
    }
}
