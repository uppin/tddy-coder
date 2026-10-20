//! Warming a session's code index in the background once its worktree exists.
//!
//! [`warm_for_session`] asks the index daemon this runtime manages (`code_index.Warm`, over
//! [`IndexChannelSource::connect`] — the same forwarding path `code_navigation` uses, which for the
//! daemon's registry starts the index daemon on first use) to load the worktree's index, and drains
//! the answer. The warm is only a trigger: what the index is doing is per worktree root in the index
//! daemon, which `code_navigation.WatchCodeIndex` and the code pane's `CodeSession` follow through
//! `code_index.WatchRoot` — whoever started the load. Nothing is recorded here.
//!
//! Warming never blocks a session: it runs on its own task, a failure is logged and nothing else (the
//! root reports `FAILED` with its reason to whoever watches it), and a daemon without an
//! `index_daemon:` section warms nothing.

use std::path::Path;
use std::sync::Arc;

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;

use tddy_session_lifecycle::connection_service::SessionWorktreeObserver;

use tddy_code_restructuring::SupportRegistry;

use crate::code_navigation::{has_an_index, index_daemon_error, IndexChannelSource};

/// Starts warming `worktree`'s code index for `session_id` in the background, for a host with no
/// languages beyond Rust: only a Rust workspace is warmed, as before extensions existed.
#[must_use = "dropping the handle detaches the warm, it does not cancel it"]
pub fn warm_for_session(
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
    session_id: &str,
    worktree: &Path,
) -> Option<tokio::task::JoinHandle<()>> {
    start_warm(index_daemon, None, session_id, worktree)
}

/// Starts warming `worktree`'s code index for `session_id` in the background.
///
/// Returns the warm's task, or `None` when nothing was started: no index daemon is configured, or
/// `worktree` has no index for `supports` — the registry the host built its index daemon over
/// ([`has_an_index`]: neither a Rust workspace nor a tree holding a file a support with a server
/// claims; `#reshape` extension-language-server, R7). The task never fails the session — a failed
/// warm is logged.
#[must_use = "dropping the handle detaches the warm, it does not cancel it"]
pub fn warm_for_session_with_supports(
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
    supports: &SupportRegistry,
    session_id: &str,
    worktree: &Path,
) -> Option<tokio::task::JoinHandle<()>> {
    start_warm(index_daemon, Some(supports), session_id, worktree)
}

/// The warm both entry points start, `supports` being `None` for a host with no extension
/// language.
fn start_warm(
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
    supports: Option<&SupportRegistry>,
    session_id: &str,
    worktree: &Path,
) -> Option<tokio::task::JoinHandle<()>> {
    let source = Arc::clone(index_daemon?);
    if !has_an_index(worktree, supports) {
        return None;
    }
    let session_id = session_id.to_string();
    let workspace_root = worktree.display().to_string();
    Some(tokio::spawn(async move {
        if let Err(reason) = warm(source.as_ref(), workspace_root).await {
            log::warn!("code index warm-up of session {session_id} failed: {reason}");
        }
    }))
}

/// Drains `workspace_root`'s `Warm`, ending when the stream does. An `Err` is the reason the warm
/// could not run, broke off, or ended short of `ready`.
async fn warm(source: &dyn IndexChannelSource, workspace_root: String) -> Result<(), String> {
    let channel = source
        .connect()
        .await
        .map_err(|err| index_daemon_error(&err))?;
    let stream = CodeIndexServiceClient::new(channel)
        .warm(index::WarmRequest { workspace_root })
        .await
        .map_err(warm_failed)?
        .into_inner();
    if drain_warm(stream).await.map_err(warm_failed)? {
        Ok(())
    } else {
        Err("warm: the index daemon ended the stream before the index was ready".to_string())
    }
}

/// Reads a `Warm` stream to its end — the warm is the trigger, its messages are not the progress
/// anyone reads — and answers whether its last message said the index is ready. An `Err` is the
/// status the stream broke off with.
pub(crate) async fn drain_warm(
    mut stream: tonic::Streaming<index::IndexProgress>,
) -> Result<bool, tonic::Status> {
    let mut ready = false;
    while let Some(update) = stream.message().await? {
        ready = update.ready;
    }
    Ok(ready)
}

/// A warm the index daemon refused or broke off, as one line.
fn warm_failed(status: tonic::Status) -> String {
    format!("warm: {}", status.message())
}

/// Starts a session's warm-up when its worktree appears: the daemon's end of the session host's
/// [`SessionWorktreeObserver`] port.
pub struct IndexWarmupObserver {
    index_daemon: Arc<dyn IndexChannelSource>,
    /// The languages beyond Rust the index daemon serves. `None` — Rust only — unless
    /// [`IndexWarmupObserver::with_supports`] says otherwise.
    supports: Option<Arc<SupportRegistry>>,
}

impl IndexWarmupObserver {
    /// An observer warming through `index_daemon`.
    #[must_use]
    pub fn new(index_daemon: Arc<dyn IndexChannelSource>) -> Self {
        Self {
            index_daemon,
            supports: None,
        }
    }

    /// Warms an extension-only worktree too when `supports` — the registry the host built its index
    /// daemon over — serves it (`#reshape` extension-language-server, R7).
    #[must_use]
    pub fn with_supports(mut self, supports: Arc<SupportRegistry>) -> Self {
        self.supports = Some(supports);
        self
    }
}

impl SessionWorktreeObserver for IndexWarmupObserver {
    fn worktree_ready(&self, session_id: &str, worktree: &Path) {
        // Detached on purpose: a session's start never waits on its index.
        drop(start_warm(
            Some(&self.index_daemon),
            self.supports.as_deref(),
            session_id,
            worktree,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tddy_code_restructuring::{LanguageBackend, LanguageSupport, ServerHandle};
    use tddy_lsp::{LanguageId, LaunchSpec};
    use tonic::transport::Channel;

    use super::*;
    use crate::code_navigation::IndexChannelError;

    /// An index daemon nothing can dial: the warm this suite starts fails and is logged, and only
    /// whether one was started is read.
    struct AnUnreachableIndexDaemon;

    #[async_trait::async_trait]
    impl IndexChannelSource for AnUnreachableIndexDaemon {
        async fn connect(&self) -> Result<Channel, IndexChannelError> {
            Err("no index daemon runs in this suite".into())
        }

        async fn connect_if_running(&self) -> Result<Option<Channel>, IndexChannelError> {
            Ok(None)
        }

        async fn keep_alive(&self) {}

        fn keep_alive_every(&self) -> Duration {
            Duration::from_secs(60)
        }
    }

    /// An installed extension's TOML support with a configured server, standing in for one so the
    /// warm-up is pinned without loading a grammar.
    struct ATomlSupportWithAServer {
        id: LanguageId,
    }

    impl LanguageSupport for ATomlSupportWithAServer {
        fn id(&self) -> &LanguageId {
            &self.id
        }

        fn claims(&self, path: &Path) -> bool {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        }

        fn launch(&self) -> Option<LaunchSpec> {
            Some(LaunchSpec::new("/opt/fake-ls"))
        }

        fn workspace_root(&self, _file: &Path, root: &Path) -> PathBuf {
            root.to_path_buf()
        }

        fn backend(&self, _server: ServerHandle<'_>, _root: &Path) -> Box<dyn LanguageBackend> {
            unreachable!("a warm-up builds no backend")
        }
    }

    fn supports_serving_toml() -> SupportRegistry {
        let mut supports = SupportRegistry::new();
        supports.register(Box::new(ATomlSupportWithAServer {
            id: LanguageId::new("demo-toml"),
        }));
        supports
    }

    /// A worktree with no `Cargo.toml`, holding `config/app.toml`.
    fn an_extension_only_worktree() -> tempfile::TempDir {
        let worktree = tempfile::tempdir().expect("a temporary worktree");
        std::fs::create_dir_all(worktree.path().join("config")).expect("config dir");
        std::fs::write(worktree.path().join("config/app.toml"), "[server]\n").expect("a toml file");
        worktree
    }

    #[tokio::test]
    async fn an_extension_only_worktree_whose_language_has_a_server_is_warmed() {
        // Given an index daemon, an extension-only worktree, and a host serving its TOML file
        let index_daemon: Arc<dyn IndexChannelSource> = Arc::new(AnUnreachableIndexDaemon);
        let worktree = an_extension_only_worktree();

        // When the session's worktree appears
        let warming = warm_for_session_with_supports(
            Some(&index_daemon),
            &supports_serving_toml(),
            "a-session",
            worktree.path(),
        );

        // Then a warm is started for it, as for a Rust worktree
        assert!(
            warming.is_some(),
            "an extension-only worktree with a served language was not warmed"
        );
    }

    #[test]
    fn an_extension_only_worktree_whose_language_has_a_server_is_not_reported_none() {
        // Given an extension-only worktree, and a host serving its TOML file
        let worktree = an_extension_only_worktree();

        // When a code session or `WatchCodeIndex` asks whether it has an index
        let indexed = has_an_index(worktree.path(), Some(&supports_serving_toml()));

        // Then it has one: the session follows it rather than reporting `NONE`
        assert!(
            indexed,
            "an extension-only worktree was reported as having no index"
        );
    }
}
