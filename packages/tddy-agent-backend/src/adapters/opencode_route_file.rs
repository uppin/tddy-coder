//! `opencode-route-file`: a provider route's OpenCode configuration written owner-only and named in
//! `OPENCODE_CONFIG`, held for the agent's lifetime, as an ACP hooks adapter (`#agent-tools 11/12`).
//!
//! Today's `RouteFiles`/`RouteFilesHold` (`backend/opencode_acp.rs`), named.

use super::acp_hooks::{AcpHooksAdapter, AcpHooksLaunch, PreparedAcpHooks};
use crate::error::BackendError;

/// The adapter that writes and holds an OpenCode route's configuration file.
pub const OPENCODE_ROUTE_FILE: &str = "opencode-route-file";

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
        // TODO(acp-agents): implement — green moves `RouteFiles` here and `OpenCodeAcpBackend`
        // and the daemon launcher use it (the launcher's inline `OPENCODE_CONFIG_CONTENT` route
        // config becomes this file).
        let _ = launch;
        todo!("acp-agents: OpenCodeRouteFile::prepare")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::acp_driver::AcpAgentCommand;
    use crate::provider_route::{HarnessRoute, RouteFile};

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
