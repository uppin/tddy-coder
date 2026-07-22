//! The reuse core: a per-`(workspace root, language)` registry of live language servers.
//!
//! This adds the lookup-or-spawn-by-stable-key layer that [`tddy_task::TaskRegistry`]
//! lacks (it keys by generated UUID and evicts terminal tasks). Two requests with the
//! same [`LspKey`] return the same running server; idle servers are reaped; a server
//! whose task has become terminal is re-spawned on the next request.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tddy_task::{ChannelKind, TaskChannel, TaskId, TaskRegistry};
use tokio::sync::{oneshot, Mutex};

use crate::allowlist::{Language, LspAllowList};
use crate::client::LspClient;
use crate::error::LspError;
use crate::server_body::LspServerBody;

/// How long to wait for a freshly-spawned server to complete its handshake.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);

/// A live service together with the instant it was last used (its idle-timer anchor).
type ServiceEntry = (Arc<LspService>, Instant);

/// The stable reuse key: one server per workspace root + language.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LspKey {
    pub root: PathBuf,
    pub language: Language,
}

/// A live, reusable server: its task id plus an initialized client.
pub struct LspService {
    pub task_id: TaskId,
    pub client: Arc<LspClient>,
}

/// A source file to open as an LSP document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSource {
    pub uri: String,
    pub language_id: String,
    pub text: String,
}

/// Per-`(root, language)` registry with lazy get-or-spawn and idle teardown.
#[derive(Clone)]
pub struct LspRegistry {
    allow: LspAllowList,
    task_registry: TaskRegistry,
    /// Live services keyed by workspace+language, each with its last-activity instant.
    services: Arc<Mutex<HashMap<LspKey, ServiceEntry>>>,
    idle_timeout: Duration,
}

impl LspRegistry {
    /// Create a registry over `allow`, spawning servers on `task_registry`, tearing down
    /// servers idle for longer than `idle_timeout`.
    pub fn new(allow: LspAllowList, task_registry: TaskRegistry, idle_timeout: Duration) -> Self {
        Self {
            allow,
            task_registry,
            services: Arc::new(Mutex::new(HashMap::new())),
            idle_timeout,
        }
    }

    /// Lazily get (or spawn) the server for `key`. Rejects disallowed languages before
    /// spawning. A repeated key returns the same [`LspService`]; a key whose task has
    /// become terminal is re-spawned.
    pub async fn get_or_spawn(&self, key: LspKey) -> Result<Arc<LspService>, LspError> {
        if !self.allow.is_allowed(key.language) {
            return Err(LspError::LanguageNotAllowed(key.language.id().to_string()));
        }

        {
            let mut services = self.services.lock().await;
            if let Some(existing) = services.get(&key).map(|(svc, _)| Arc::clone(svc)) {
                let alive = match self.task_registry.get(&existing.task_id).await {
                    Some(handle) => !handle.status().is_terminal(),
                    None => false,
                };
                if alive {
                    // Reuse: refresh the idle timer and hand back the same server.
                    services.insert(key.clone(), (Arc::clone(&existing), Instant::now()));
                    return Ok(existing);
                }
                // The task died — drop the stale entry and spawn a fresh server.
                services.remove(&key);
            }
        }

        let spec = self
            .allow
            .launch_spec(key.language)
            .cloned()
            .ok_or_else(|| LspError::LanguageNotAllowed(key.language.id().to_string()))?;

        let channel = TaskChannel::output_only("0", "lsp", ChannelKind::Combined);
        let (client_tx, client_rx) = oneshot::channel();
        let body = LspServerBody {
            spec,
            root_dir: key.root.clone(),
            client_tx,
        };
        let handle = self
            .task_registry
            .spawn(
                body,
                format!("lsp:{}", key.language.id()),
                "",
                vec![channel],
            )
            .await;

        let client = match tokio::time::timeout(SPAWN_TIMEOUT, client_rx).await {
            Ok(Ok(client)) => client,
            Ok(Err(_)) => {
                self.task_registry.cancel_task(&handle.id).await;
                return Err(LspError::ServerExited);
            }
            Err(_) => {
                self.task_registry.cancel_task(&handle.id).await;
                return Err(LspError::Timeout);
            }
        };

        let service = Arc::new(LspService {
            task_id: handle.id.clone(),
            client,
        });
        self.services
            .lock()
            .await
            .insert(key, (Arc::clone(&service), Instant::now()));
        Ok(service)
    }

    /// Get-or-spawn, then bind a target by opening each of its `srcs` as an LSP document.
    pub async fn bind_target(
        &self,
        key: LspKey,
        srcs: &[DocumentSource],
    ) -> Result<Arc<LspService>, LspError> {
        let service = self.get_or_spawn(key).await?;
        for src in srcs {
            service
                .client
                .did_open(&src.uri, &src.language_id, &src.text)
                .await?;
        }
        Ok(service)
    }

    /// Cancel and drop every server idle for at least the idle timeout; returns the
    /// keys that were reaped.
    pub async fn reap_idle(&self) -> Vec<LspKey> {
        let now = Instant::now();
        let mut services = self.services.lock().await;
        let expired: Vec<LspKey> = services
            .iter()
            .filter(|(_, (_, last))| now.duration_since(*last) >= self.idle_timeout)
            .map(|(key, _)| key.clone())
            .collect();

        let mut reaped = Vec::new();
        for key in expired {
            if let Some((service, _)) = services.remove(&key) {
                self.task_registry.cancel_task(&service.task_id).await;
                reaped.push(key);
            }
        }
        reaped
    }

    /// The live service for `key`, if one is currently running.
    pub async fn get(&self, key: &LspKey) -> Option<Arc<LspService>> {
        self.services
            .lock()
            .await
            .get(key)
            .map(|(svc, _)| Arc::clone(svc))
    }

    /// Cancel and drop all servers.
    pub async fn shutdown_all(&self) {
        let drained: Vec<ServiceEntry> = {
            let mut services = self.services.lock().await;
            services.drain().map(|(_, value)| value).collect()
        };
        for (service, _) in drained {
            self.task_registry.cancel_task(&service.task_id).await;
        }
    }
}

/// Resolve the workspace root for a target directory: the nearest ancestor that is the
/// root of a workspace (for Rust, the `Cargo.toml` workspace root). Getting this stable
/// is what makes two targets in one workspace actually share a server.
pub fn workspace_root_for(target_dir: &Path) -> PathBuf {
    // Walk from the target dir outward; the outermost ancestor holding a `Cargo.toml`
    // is the workspace root (ancestors iterate inner→outer, so the last hit wins).
    let mut root = None;
    for ancestor in target_dir.ancestors() {
        if ancestor.join("Cargo.toml").is_file() {
            root = Some(ancestor.to_path_buf());
        }
    }
    root.unwrap_or_else(|| target_dir.to_path_buf())
}

/// Expand a target's `srcs` (glob patterns rooted at `root`, relative to `repo_dir`) into
/// concrete [`DocumentSource`]s with absolute `file://` URIs.
pub fn srcs_to_document_sources(
    repo_dir: &Path,
    srcs: &[String],
    root: &str,
) -> Result<Vec<DocumentSource>, LspError> {
    let mut sources = Vec::with_capacity(srcs.len());
    for pattern in srcs {
        let path = repo_dir.join(root).join(pattern);
        let text = std::fs::read_to_string(&path)
            .map_err(|err| LspError::Io(format!("{}: {err}", path.display())))?;
        sources.push(DocumentSource {
            uri: format!("file://{}", path.display()),
            language_id: "rust".to_string(),
            text,
        });
    }
    Ok(sources)
}
