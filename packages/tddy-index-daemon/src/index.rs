//! The warm per-root state: which roots this process holds an index for, and the queue that keeps
//! two callers on one root out of each other's way.
//!
//! The servers themselves live in [`tddy_lsp::LspRegistry`], which is already keyed by
//! `(root, language)` and already reaps, respawns and shuts them down. What this adds is the two
//! things a *host* of that registry needs and the registry cannot know: which roots this host has
//! asked for (the registry answers by key, not by enumeration), and a queue per root, because
//! `.restructure/journal.jsonl` is keyed by root with no lock file and
//! [`tddy_code_restructuring::runner::open_run`] refuses a second plan with `JournalExists`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tddy_code_analysis::complexity_cache::InMemoryComplexityCache;
use tddy_lsp::client::LspClient;
use tddy_lsp::registry::{workspace_root_for, LspKey};
use tddy_lsp::{Language, LspRegistry};
use tddy_rpc::Status;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::activity::{reaped_line, Warmth};
use crate::graph::GraphLoad;
use crate::proto::code_index::WarmWorkspace;
use crate::status::status_of_lsp;
use crate::tree_changes::{watched_files_params, FileChange, TreeSnapshot};

/// How long one LSP request may go unanswered before the transport gives up on it.
///
/// **Not a way out of a wedged request.** `tddy_code_restructuring::backends::LspClientBridge`
/// drives every request with the serving task's cancellation token, so a client that hangs up
/// reaches a request already in flight and this bound is not what its disconnect waits on.
///
/// What is left is the reason it must stay high: the backend re-issues a request whose bound
/// expired, a bounded number of times, so the bound multiplied by that count is a hard ceiling on
/// how long a cold index may take — whatever the client is willing to wait for. Ten minutes puts
/// it clear of any single request against a cold graph. The same number, for the same reason, as
/// the one a one-shot command line installs.
const REQUEST_BOUND_ABOVE_ANY_COLD_INDEX: Duration = Duration::from_secs(600);

/// What this process knows about one workspace root.
struct RootState {
    /// The queue every operation that touches this root's tree or its `.restructure/` state waits
    /// in. Held as an `Arc` so a caller can carry the guard past the map's own lock.
    gate: Arc<Mutex<()>>,
    /// When an operation on this root last reached its language server — what idle reaping is
    /// measured against, and what a client reading [`WarmWorkspace::idle_seconds`] is told.
    last_used: Instant,
    /// Whether this root's crate graph has been observed loaded, watched from the moment its
    /// server was reached.
    ///
    /// `None` until a server has been reached for this root at all. Replaced rather than reused
    /// when the watcher behind it has ended, because that means the server it was about is gone and
    /// the registry has spawned another — whose graph starts unloaded again.
    graph: Option<GraphLoad>,
    /// The source tree as it stood when this root's server was last handed to a request, which is
    /// what the next request's changes are told against ([`crate::tree_changes`]).
    ///
    /// Replaced together with `graph`: a new server reads the tree for itself.
    tree: Option<TreeSnapshot>,
}

/// The roots this process holds an index for, and their queues.
#[derive(Clone)]
pub struct WorkspaceIndex {
    servers: LspRegistry,
    roots: Arc<Mutex<BTreeMap<PathBuf, RootState>>>,
    /// Complexity scores, kept against the hash of the source that produced them.
    ///
    /// One cache for the whole process rather than one per root, which is the one piece of warm
    /// state here that is *not* partitioned by root — deliberately. The key is the content, so a
    /// score cannot be attributed to the wrong tree, and two worktrees of the same repository are
    /// mostly the same bytes: partitioning would rescore every shared file once per root and hold
    /// two copies of the answer.
    scores: Arc<InMemoryComplexityCache>,
}

impl WorkspaceIndex {
    #[must_use]
    pub fn new(servers: LspRegistry) -> Self {
        Self {
            servers,
            roots: Arc::new(Mutex::new(BTreeMap::new())),
            scores: Arc::new(InMemoryComplexityCache::default()),
        }
    }

    /// The warm complexity scores, for an operation that will score a source file.
    ///
    /// Handed out as an `Arc` because every caller of it scores on a blocking thread — `syn` is
    /// synchronous — and a borrow could not cross into one.
    #[must_use]
    pub fn complexity_scores(&self) -> Arc<InMemoryComplexityCache> {
        Arc::clone(&self.scores)
    }

    /// The workspace root a request names, or the refusal that says why it names none.
    ///
    /// A relative path is refused rather than resolved: one process serves several trees, so there
    /// is no process directory to resolve it against — which is the whole reason every request
    /// carries a root. A path that is not a reachable directory is a `FailedPrecondition`, because
    /// the tree is wrong rather than the request.
    pub fn workspace_root_of(requested: &str) -> Result<PathBuf, Status> {
        if requested.trim().is_empty() {
            return Err(Status::invalid_argument(
                "the request names no workspace_root",
            ));
        }
        let named = Path::new(requested);
        if !named.is_absolute() {
            return Err(Status::invalid_argument(format!(
                "workspace_root `{requested}` is relative: this process serves several trees and \
                 has no directory of its own to resolve it against"
            )));
        }
        if !named.is_dir() {
            return Err(Status::failed_precondition(format!(
                "workspace_root `{requested}` is not a directory this process can reach"
            )));
        }
        workspace_root_for(named).map_err(|err| {
            Status::failed_precondition(format!(
                "workspace_root `{requested}` has a manifest this process cannot read: {err}"
            ))
        })
    }

    /// Wait for this root's turn, and hold it until the returned guard is dropped.
    ///
    /// Every operation that reads the tree or writes `.restructure/` takes this, so two clients on
    /// one root queue rather than collide on a journal that carries no plan identity and no lock.
    /// [`Self::client_for`] and [`Self::warm_workspaces`] deliberately do not: warming is
    /// documented idempotent and reaching a server touches neither the tree nor the journal, so
    /// queueing them behind a seven-minute apply would make "is this root warm?" unanswerable for
    /// as long as the apply runs.
    pub async fn hold(&self, root: &Path) -> OwnedMutexGuard<()> {
        let gate = {
            let mut roots = self.roots.lock().await;
            Arc::clone(
                &roots
                    .entry(root.to_path_buf())
                    .or_insert_with(RootState::new)
                    .gate,
            )
        };
        gate.lock_owned().await
    }

    /// The warm language server for `root`, spawning one if this process holds none yet.
    ///
    /// Concurrent cold callers on one root are the registry's problem and it solves them: its
    /// per-key spawn gate hands every one of them the single server it started.
    ///
    /// A server this process already held is first told of every source file created, changed or
    /// deleted under `root` since the previous request was handed it (see [`crate::tree_changes`]).
    /// The tree is read before the server is reached, so a server spawned here loads a tree no older
    /// than the one the next request is compared against.
    pub async fn client_for(&self, root: &Path) -> Result<Arc<LspClient>, Status> {
        let tree = read_tree(root).await?;
        let key = LspKey {
            root: root.to_path_buf(),
            language: Language::Rust,
        };
        let service = self
            .servers
            .get_or_spawn(key)
            .await
            .map_err(|failure| status_of_lsp(&failure))?;
        service
            .client
            .set_request_timeout(REQUEST_BOUND_ABOVE_ANY_COLD_INDEX);
        let changes = self.record_use(root, &service.client, tree).await;
        if !changes.is_empty() {
            service
                .client
                .notify_raw(
                    "workspace/didChangeWatchedFiles",
                    watched_files_params(&changes),
                )
                .await
                .map_err(|failure| status_of_lsp(&failure))?;
            log::info!(
                target: "tddy_index_daemon::index",
                "told the server for {} of {} file change(s) on disk since its last request",
                root.display(),
                changes.len()
            );
            log::debug!(
                target: "tddy_index_daemon::index",
                "changes told for {}: {}",
                root.display(),
                described(&changes)
            );
        }
        log::debug!(
            target: "tddy_index_daemon::index",
            "serving {} from the warm index", root.display()
        );
        Ok(Arc::clone(&service.client))
    }

    /// Whether this process already holds an index for `root`.
    ///
    /// Asked *before* the work starts, because it is what an operator needs to read the request
    /// that follows: the same `check` answers in milliseconds against a warm root and waits six to
    /// ten minutes for a cold one. Answered against the registry rather than against this host's
    /// own map, for the reason [`Self::warm_workspaces`] gives — a root whose server has been
    /// reaped is not warm, whatever this host once asked for.
    pub(crate) async fn warmth_of(&self, root: &Path) -> Warmth {
        let key = LspKey {
            root: root.to_path_buf(),
            language: Language::Rust,
        };
        if self.servers.get(&key).await.is_some() {
            Warmth::AlreadyWarm
        } else {
            Warmth::NotYetIndexed
        }
    }

    /// The roots this process currently holds an index for.
    ///
    /// Answered against the registry rather than from this map alone: a root whose server the
    /// registry has reaped is no longer warm, whatever this host once asked for, so its record is
    /// dropped here instead of being reported as an index that is not there.
    pub async fn warm_workspaces(&self) -> Vec<WarmWorkspace> {
        let known: Vec<(PathBuf, u64)> = {
            let roots = self.roots.lock().await;
            roots
                .iter()
                .map(|(root, state)| (root.clone(), state.last_used.elapsed().as_secs()))
                .collect()
        };

        let mut warm = Vec::new();
        let mut reaped = Vec::new();
        for (root, idle_seconds) in known {
            let key = LspKey {
                root: root.clone(),
                language: Language::Rust,
            };
            if self.servers.get(&key).await.is_none() {
                reaped.push(root);
                continue;
            }
            warm.push(WarmWorkspace {
                workspace_root: root.to_string_lossy().to_string(),
                ready: true,
                idle_seconds,
            });
        }

        if !reaped.is_empty() {
            let mut roots = self.roots.lock().await;
            for root in reaped {
                log::info!(target: "tddy_index_daemon::index", "{}", reaped_line(&root));
                roots.remove(&root);
            }
        }
        warm
    }

    /// Whether `root`'s crate graph has been observed loaded, if a server has been reached for it.
    ///
    /// `None` means no server has been asked for yet, which is a different thing from a graph that
    /// is not loaded: nothing has been watched, so there is nothing to report. [`Self::client_for`]
    /// is what turns the first into the second.
    pub(crate) async fn graph_load_of(&self, root: &Path) -> Option<GraphLoad> {
        self.roots.lock().await.get(root)?.graph.clone()
    }

    /// Note that `root`'s index has just served something, which is what its idle timer is read
    /// against — and what makes the root appear in [`Self::warm_workspaces`].
    ///
    /// Also where this root's crate-graph watcher is attached, because this is the earliest moment
    /// a host has a client to attach one to — and `experimental/serverStatus` is reported on the
    /// transition only, so anything later would be too late. Attached once per server: a latch
    /// whose watcher has ended is about a server that is gone, and is replaced rather than read.
    ///
    /// Returns what changed on disk since the server was last handed out, for the caller to tell it:
    /// nothing for a server reached for the first time, which reads the tree for itself.
    async fn record_use(
        &self,
        root: &Path,
        client: &LspClient,
        tree: TreeSnapshot,
    ) -> Vec<(PathBuf, FileChange)> {
        let mut roots = self.roots.lock().await;
        let state = roots
            .entry(root.to_path_buf())
            .or_insert_with(RootState::new);
        state.last_used = Instant::now();
        let same_server = state
            .graph
            .as_ref()
            .is_some_and(|graph| graph.still_watching());
        if !same_server {
            state.graph = Some(GraphLoad::watching(client));
        }
        let changes = match (&state.tree, same_server) {
            (Some(earlier), true) => tree.changes_since(earlier),
            _ => Vec::new(),
        };
        state.tree = Some(tree);
        changes
    }
}

/// The source tree under `root`, read off the async runtime since it walks the whole tree.
async fn read_tree(root: &Path) -> Result<TreeSnapshot, Status> {
    let walked = root.to_path_buf();
    tokio::task::spawn_blocking(move || TreeSnapshot::read(&walked))
        .await
        .map_err(|join| Status::internal(format!("the tree walk did not finish: {join}")))?
        .map_err(|error| {
            Status::failed_precondition(format!(
                "the tree under `{}` could not be read to tell its language server what changed \
                 on disk: {error}",
                root.display()
            ))
        })
}

/// The changes, as a log line can carry them.
fn described(changes: &[(PathBuf, FileChange)]) -> String {
    changes
        .iter()
        .map(|(path, change)| format!("{change:?} {}", path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

impl RootState {
    fn new() -> Self {
        Self {
            gate: Arc::new(Mutex::new(())),
            last_used: Instant::now(),
            graph: None,
            tree: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_rpc::Code;

    #[test]
    fn refuses_a_root_that_is_not_a_directory_as_a_failed_precondition() {
        // Given a path no tree stands at
        let requested = "/nonexistent/workspace/root";

        // When it is read as a workspace root
        let outcome = WorkspaceIndex::workspace_root_of(requested);

        // Then the tree is named as the thing that is wrong, not the request
        assert_eq!(
            outcome.expect_err("an unreachable root is refused").code(),
            Code::FailedPrecondition
        );
    }

    /// A relative root has nowhere to be resolved from in a process serving several trees, and
    /// resolving it against the process directory is the fallback this crate exists to remove.
    #[test]
    fn refuses_a_relative_root_as_an_invalid_argument() {
        // Given a root named relatively
        let requested = "packages/tddy-index-daemon";

        // When it is read as a workspace root
        let outcome = WorkspaceIndex::workspace_root_of(requested);

        // Then the request is named as the thing that is wrong
        assert_eq!(
            outcome.expect_err("a relative root is refused").code(),
            Code::InvalidArgument
        );
    }

    /// A worktree under `<main>/.worktrees/` is its own tree. Serving the enclosing checkout instead
    /// answered `check` from another branch, and would have had `apply` write there.
    #[test]
    fn serves_a_nested_worktree_at_its_own_root_rather_than_the_enclosing_checkout() {
        // Given a main checkout and a worktree nested under it, each its own cargo workspace
        let main = tempfile::tempdir().expect("a temporary directory");
        let main_root = main.path().canonicalize().expect("the root resolves");
        let worktree = main_root.join(".worktrees/x");
        std::fs::create_dir_all(main_root.join(".git")).expect("the main checkout's .git");
        std::fs::create_dir_all(&worktree).expect("the worktree directory");
        std::fs::write(main_root.join("Cargo.toml"), "[workspace]\n").expect("the main manifest");
        std::fs::write(worktree.join("Cargo.toml"), "[workspace]\n")
            .expect("the worktree manifest");
        std::fs::write(worktree.join(".git"), "gitdir: ../../.git/worktrees/x\n")
            .expect("the worktree's .git file");

        // When the worktree is named as a request's root
        let root = WorkspaceIndex::workspace_root_of(worktree.to_str().expect("a UTF-8 path"));

        // Then the worktree is served, not the checkout around it
        assert_eq!(root.expect("the worktree is a root"), worktree);
    }

    #[test]
    fn refuses_a_request_that_names_no_root_at_all() {
        // Given a request whose root is blank
        let requested = "   ";

        // When it is read as a workspace root
        let outcome = WorkspaceIndex::workspace_root_of(requested);

        // Then the request is named as the thing that is wrong
        assert_eq!(
            outcome.expect_err("a blank root is refused").code(),
            Code::InvalidArgument
        );
    }
}
