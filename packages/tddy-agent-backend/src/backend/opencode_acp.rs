//! OpenCode as a tool-session agent: `opencode acp` driven over the Agent Client Protocol, the same
//! way [`super::CodexAcpBackend`] drives `codex-acp` and [`super::ClaudeAcpBackend`] drives
//! `claude-agent-acp`, on the shared [`AcpDriver`](super::acp_driver::AcpDriver).
//!
//! # Provider routes
//!
//! On a provider route the agent reads its provider from the configuration file `OPENCODE_CONFIG`
//! names, and that file reads the account's key from the environment. The route's files are written
//! owner-only into a directory the driver holds, so each file lives as long as the agent child and is
//! removed when the driver ends.
//!
//! The route's arguments (`--model <provider>/<model>`) are not passed to `opencode acp`. The
//! generated configuration already selects that model on the provider it declares. The flag could
//! not be checked against a real `opencode acp` here: the `opencode` on the development machine is
//! a wrapper that exits with status 127 when no OpenCode is on `PATH`, so `opencode acp --help` never
//! ran. Passing an argument the agent may not take would fail the whole launch.

use super::acp_driver::{self, AcpAgentCommand, AcpAgentHooks, AcpDriver, AllowOnce, DriverSlot};
use super::{BackendModels, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The variable `opencode` reads its configuration file's path from.
const OPENCODE_CONFIG_ENV: &str = "OPENCODE_CONFIG";

/// A route's files hold the account's endpoint, so only the owner may read the directory or them.
const OWNER_ONLY_DIR: u32 = 0o700;
const OWNER_ONLY_FILE: u32 = 0o600;

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

/// The environment a route gives the `opencode acp` child: the route's variables, and
/// `OPENCODE_CONFIG` naming the route's first file.
fn opencode_route_env(
    route: &HarnessRoute,
    files: &RouteFiles,
) -> Result<Vec<(String, String)>, BackendError> {
    let mut env = acp_driver::route_env(route);
    if let Some(config) = &files.config {
        let config = config.to_str().ok_or_else(|| {
            BackendError::InvocationFailed(format!(
                "the OpenCode route file {} has a path that is not UTF-8",
                config.display()
            ))
        })?;
        env.push((OPENCODE_CONFIG_ENV.to_string(), config.to_string()));
    }
    Ok(env)
}

/// The message for an `opencode` that cannot be found: where to get one.
fn missing_opencode(error: BackendError, program: &Path) -> BackendError {
    match error {
        BackendError::BinaryNotFound(_) => {
            let p = program.display();
            BackendError::BinaryNotFound(format!(
                "{p} — install OpenCode on PATH, or set TDDY_OPENCODE_CLI / --opencode-cli-path / coder-config `opencode_cli_path`"
            ))
        }
        other => other,
    }
}

/// `opencode acp` behind [`CodingBackend`](super::CodingBackend).
///
/// The driver starts the agent on the first request rather than at construction, so a route set by
/// [`OpenCodeAcpBackend::with_provider_route`] is known before the agent exists.
pub struct OpenCodeAcpBackend {
    agent_path: PathBuf,
    /// The provider route the agent runs on (`with_provider_route`); `None` for the person's own setup.
    provider_route: Option<HarnessRoute>,
    driver: DriverSlot,
}

impl std::fmt::Debug for OpenCodeAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCodeAcpBackend")
            .field("agent_path", &self.agent_path)
            .finish()
    }
}

impl OpenCodeAcpBackend {
    /// Default executable on `PATH`.
    pub const DEFAULT_CLI_BINARY: &'static str = "opencode";

    #[must_use]
    pub fn new() -> Self {
        Self::with_agent_path(PathBuf::from(Self::DEFAULT_CLI_BINARY))
    }

    /// Run the given binary (`opencode`, or a stub agent in tests) as `<binary> acp`.
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        Self {
            agent_path: path,
            provider_route: None,
            driver: Mutex::new(None),
        }
    }

    /// Run the agent on `route`. A driver started before the route was set runs the person's own
    /// setup: it is dropped here, which ends its child, and the next request starts one on the route.
    pub(super) fn set_provider_route(&mut self, route: HarnessRoute) {
        self.provider_route = Some(route);
        self.driver = Mutex::new(None);
    }

    /// The driver, started on first use. On a route the child gets the route's environment and
    /// `OPENCODE_CONFIG` naming the route's first file.
    fn driver(&self) -> Result<Arc<AcpDriver>, BackendError> {
        acp_driver::started(&self.driver, || {
            let files = self
                .provider_route
                .as_ref()
                .map(RouteFiles::write)
                .transpose()
                .map_err(|e| {
                    BackendError::InvocationFailed(format!(
                        "failed to write the OpenCode route files: {e}"
                    ))
                })?;
            let env = match (&self.provider_route, &files) {
                (Some(route), Some(files)) => opencode_route_env(route, files)?,
                _ => Vec::new(),
            };
            let command = AcpAgentCommand {
                program: self.agent_path.clone(),
                args: vec!["acp".to_string()],
                env,
                cwd: None,
            };
            let hooks = Arc::new(RouteFilesHold { _files: files });
            AcpDriver::spawn(command, Arc::new(AllowOnce), hooks)
                .map_err(|e| missing_opencode(e, &self.agent_path))
        })
    }
}

impl Default for OpenCodeAcpBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for OpenCodeAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let prompt = super::codex::merge_codex_prompt(&request)?;
        let driver = self.driver()?;
        let open = acp_driver::open_for_request(&request, self.provider_route.is_some())?;
        let session = driver.open_session(open).await?;
        acp_driver::answer(&driver, session, prompt, request.progress_sink).await
    }

    fn name(&self) -> &str {
        "opencode"
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        self.driver()?.list_models().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_route::RouteFile;

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
        let mode = |p: &Path| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777
        };
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
}
