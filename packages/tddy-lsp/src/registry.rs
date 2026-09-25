//! The reuse core: a per-`(workspace root, language)` registry of live language servers.
//!
//! This adds the lookup-or-spawn-by-stable-key layer that [`tddy_task::TaskRegistry`]
//! lacks (it keys by generated UUID and evicts terminal tasks). Two requests with the
//! same [`LspKey`] return the same running server; idle servers are reaped; a server
//! whose task has become terminal is re-spawned on the next request.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_task::{ChannelKind, IdleTimeoutTracker, TaskChannel, TaskId, TaskRegistry};
use tokio::sync::{oneshot, Mutex};

use crate::allowlist::{Language, LspAllowList};
use crate::client::LspClient;
use crate::error::LspError;
use crate::server_body::LspServerBody;

/// How long to wait for a freshly-spawned server to complete its handshake.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);

/// A live service together with its idle-timer (reset on each reuse, and on each use of its
/// client). The tracker is shared because the client holds it too, through the activity hook.
type ServiceEntry = (Arc<LspService>, Arc<IdleTimeoutTracker>);

/// The in-flight placeholder for one key's cold spawn: concurrent callers queue on it, and the
/// second one through finds the first's server rather than starting another.
type SpawnGate = Arc<Mutex<()>>;

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
    /// Live services keyed by workspace+language, each with its idle-timer.
    services: Arc<Mutex<HashMap<LspKey, ServiceEntry>>>,
    /// One gate per key with a spawn in flight. Held only across that key's spawn, so two roots
    /// still start their servers concurrently.
    spawn_gates: Arc<Mutex<HashMap<LspKey, SpawnGate>>>,
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
            spawn_gates: Arc::new(Mutex::new(HashMap::new())),
            idle_timeout,
        }
    }

    /// Lazily get (or spawn) the server for `key`. Rejects disallowed languages before
    /// spawning. A repeated key returns the same [`LspService`]; a key whose task has
    /// become terminal is re-spawned.
    ///
    /// Concurrent callers on one cold key queue on that key's spawn gate, so exactly one server
    /// is spawned and all of them are handed it. Without the gate the second `insert` overwrote
    /// the first, orphaning a live task that nothing could cancel or reap.
    pub async fn get_or_spawn(&self, key: LspKey) -> Result<Arc<LspService>, LspError> {
        if !self.allow.is_allowed(key.language) {
            return Err(LspError::LanguageNotAllowed(key.language.id().to_string()));
        }

        if let Some(live) = self.live_service(&key).await {
            return Ok(live);
        }

        let gate = self.spawn_gate(&key).await;
        let spawning = gate.lock().await;
        // Whoever held the gate before us may have spawned the very server we came for.
        let result = match self.live_service(&key).await {
            Some(live) => Ok(live),
            None => self.spawn_service(key.clone()).await,
        };
        drop(spawning);
        self.release_spawn_gate(&key, gate).await;
        result
    }

    /// The running server for `key`, with its idle timer refreshed — or `None`, having dropped a
    /// stale entry whose task has become terminal so a fresh server can replace it.
    async fn live_service(&self, key: &LspKey) -> Option<Arc<LspService>> {
        let mut services = self.services.lock().await;
        let (existing, tracker) = services
            .get(key)
            .map(|(svc, tracker)| (Arc::clone(svc), Arc::clone(tracker)))?;
        let alive = match self.task_registry.get(&existing.task_id).await {
            Some(handle) => !handle.status().is_terminal(),
            None => false,
        };
        if alive {
            tracker.record_activity();
            return Some(existing);
        }
        // The task died — drop the stale entry and let the caller spawn a fresh server.
        services.remove(key);
        None
    }

    /// The gate for `key`'s spawn, creating it if this is the first caller to need it.
    async fn spawn_gate(&self, key: &LspKey) -> SpawnGate {
        let mut gates = self.spawn_gates.lock().await;
        Arc::clone(
            gates
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    /// Forget `key`'s gate once no caller is queued behind it, so a host reaching many roots does
    /// not accumulate one gate per root it has ever seen.
    ///
    /// Holding the gate map's lock means nobody can take a new reference while we count, so two
    /// references — the map's and ours — is exactly "nobody else is here".
    async fn release_spawn_gate(&self, key: &LspKey, gate: SpawnGate) {
        let mut gates = self.spawn_gates.lock().await;
        if Arc::strong_count(&gate) == 2 {
            gates.remove(key);
        }
    }

    /// Spawn a server for `key`, complete its handshake, and record it as the live service.
    async fn spawn_service(&self, key: LspKey) -> Result<Arc<LspService>, LspError> {
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

        let tracker = Arc::new(IdleTimeoutTracker::new(self.idle_timeout));
        // Using the client is activity. A caller that borrows it and then works for minutes never
        // comes back here, so without this hook the reaper can take rust-analyzer out from under
        // an operation still using it.
        let for_hook = Arc::clone(&tracker);
        client.set_activity_hook(Arc::new(move || for_hook.record_activity()));

        let service = Arc::new(LspService {
            task_id: handle.id.clone(),
            client,
        });
        self.services
            .lock()
            .await
            .insert(key, (Arc::clone(&service), tracker));
        Ok(service)
    }

    /// Get-or-spawn, then bind a target by announcing each of its `srcs` as an LSP document.
    ///
    /// A source the server has not been told about is opened; one it already holds is announced as
    /// an edit at the next version. No bind can assume it is the first: a host that outlives one
    /// request binds the same target again on the next one, and re-opening a document restarts a
    /// version sequence the server has already advanced — after which it is entitled to ignore
    /// what the bind announced.
    pub async fn bind_target(
        &self,
        key: LspKey,
        srcs: &[DocumentSource],
    ) -> Result<Arc<LspService>, LspError> {
        let service = self.get_or_spawn(key).await?;
        for src in srcs {
            service
                .client
                .sync_document(&src.uri, &src.language_id, &src.text)
                .await?;
        }
        Ok(service)
    }

    /// Cancel and drop every server idle for at least the idle timeout; returns the
    /// keys that were reaped.
    pub async fn reap_idle(&self) -> Vec<LspKey> {
        let mut services = self.services.lock().await;
        let expired: Vec<LspKey> = services
            .iter()
            .filter(|(_, (_, tracker))| tracker.should_shutdown())
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

/// Resolve the workspace root for a target directory. Getting this stable is what makes two
/// targets in one workspace actually share a server.
///
/// The rule is `cargo locate-project --workspace`'s, bounded by the repository: walking outward
/// from `target_dir`, the nearest `Cargo.toml` that declares `[workspace]` is the root; failing
/// that, the nearest manifest, since a package in no workspace is its own root; failing that,
/// `target_dir` itself. The walk ends at the first directory holding `.git` — a directory for a
/// checkout, a file for a linked worktree — because a repository nested inside another (a worktree
/// under `<main>/.worktrees/`) is a separate tree, and rooting it at the enclosing checkout serves,
/// and edits, the wrong one.
///
/// Cargo itself does not stop at `.git`, and it honours `package.workspace` and `exclude`. Neither
/// changes the answer for a tree cargo accepts: a package below a `[workspace]` that does not list
/// it is an error to cargo unless it declares its own `[workspace]`, which this walk finds first.
///
/// # Errors
///
/// [`LspError::Io`] when a `Cargo.toml` on the walk cannot be read, since whether it declares a
/// workspace is the whole question.
pub fn workspace_root_for(target_dir: &Path) -> Result<PathBuf, LspError> {
    let mut nearest_manifest = None;
    for ancestor in target_dir.ancestors() {
        let manifest = ancestor.join("Cargo.toml");
        if manifest.is_file() {
            let text = std::fs::read_to_string(&manifest)
                .map_err(|err| LspError::Io(format!("{}: {err}", manifest.display())))?;
            if declares_workspace(&text) {
                return Ok(ancestor.to_path_buf());
            }
            nearest_manifest.get_or_insert_with(|| ancestor.to_path_buf());
        }
        if ancestor.join(".git").exists() {
            break;
        }
    }
    Ok(nearest_manifest.unwrap_or_else(|| target_dir.to_path_buf()))
}

/// Whether a manifest opens a `[workspace]` table, or one of its `[workspace.*]` subtables — either
/// makes the directory holding it a workspace root.
fn declares_workspace(manifest: &str) -> bool {
    let without_comment = |line: &str| line.split('#').next().unwrap_or_default().trim().to_owned();
    manifest.lines().map(without_comment).any(|line| {
        line == "[workspace]"
            || line
                .strip_prefix("[workspace.")
                .is_some_and(|rest| rest.ends_with(']'))
    })
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
