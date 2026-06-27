//! Shared test helpers for tddy-daemon integration and acceptance tests.
//!
//! Import with:
//! ```ignore
//! use tddy_daemon::test_util::{test_config, test_service, TEST_TOKEN, TEST_USER};
//! ```

use crate::claude_cli_session::ClaudeCliSessionManager;
use crate::config::DaemonConfig;
use crate::connection_service::{ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver};
use std::path::PathBuf;
use std::sync::Arc;

/// Token accepted by [`test_service`] as a valid session token.
pub const TEST_TOKEN: &str = "valid-token";
/// OS user returned for [`TEST_TOKEN`] by [`test_service`].
pub const TEST_USER: &str = "testuser";

const CONFIG_YAML: &str = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;

/// Build a minimal [`DaemonConfig`] suitable for unit/acceptance tests.
pub fn test_config() -> DaemonConfig {
    let dir = tempfile::tempdir().expect("create temp dir for test config");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, CONFIG_YAML).expect("write test config");
    DaemonConfig::load(&path).expect("load test config")
}

/// Build a [`ConnectionServiceImpl`] wired to `sessions_base` with the standard test resolvers.
///
/// [`TEST_TOKEN`] resolves to [`TEST_USER`]; any other token returns `None`.
pub fn test_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let config = test_config();
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == TEST_TOKEN {
            Some(TEST_USER.to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}
