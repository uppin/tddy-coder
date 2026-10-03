//! Keeping a root's loaded plans current when the tree changes behind the daemon.
//!
//! [`crate::tree_changes`] already compares the tree with what it was when the previous request was
//! handed the server, to tell the server. The same comparison says which files a person (or a pull)
//! changed between two requests, and a loaded plan's item anchors in those files may have moved or
//! stopped meaning what they meant. This hands them to the store to re-resolve.
//!
//! What the daemon's own runs wrote is in that comparison too, and is not re-resolved: the run that
//! wrote it carried every loaded plan through the edit ([`PlanStore::fold_foreign_op`]), and the
//! store discounts those files itself.
//!
//! [`PlanStore::fold_foreign_op`]: tddy_code_restructuring::plan_store::PlanStore::fold_foreign_op

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_code_restructuring::runner;
use tddy_lsp::client::LspClient;
use tddy_rpc::Status;
use tokio_util::sync::CancellationToken;

use crate::index::SharedPlanStore;
use crate::operations::joined;
use crate::status::status_of;
use crate::tree_changes::FileChange;

/// Re-resolve the loaded plans' item anchors in the Rust files `changes` names.
///
/// A deleted file is not passed on: nothing can be resolved in it.
// TODO(live-plans): an item anchor in a deleted file should go stale as `item not found`; the
// resolver has no answer for a file that is not there, so the store would have to be told apart
// from "changed".
pub(crate) async fn reresolve_loaded_plans(
    store: SharedPlanStore,
    root: &Path,
    client: &Arc<LspClient>,
    changes: &[(PathBuf, FileChange)],
) -> Result<(), Status> {
    let files: Vec<String> = changes
        .iter()
        .filter(|(path, change)| *change != FileChange::Deleted && is_rust_source(path))
        .filter_map(|(path, _)| path.strip_prefix(root).ok())
        .map(|relative| relative.display().to_string())
        .collect();
    if files.is_empty() {
        return Ok(());
    }

    let client = Arc::clone(client);
    let root = root.to_path_buf();
    let cancel = CancellationToken::new();
    let _stop_when_dropped = cancel.clone().drop_guard();
    tokio::task::spawn_blocking(move || {
        let mut store = store.lock().expect("a root's plan store");
        if store.list().is_empty() {
            return Ok(());
        }
        let mut registry = runner::registry_for(client, cancel, logged_progress(), logged_trace);
        log::debug!(
            target: "tddy_index_daemon::plan_upkeep",
            "re-resolving the loaded plans of {} in {} changed file(s)", root.display(), files.len()
        );
        store.reresolve_files(&files, &mut registry)
    })
    .await
    .map_err(|failure| joined("re-resolve plans", &failure))?
    .map_err(|refusal| status_of(&refusal))
}

fn is_rust_source(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rs")
}

fn logged_progress() -> tddy_code_restructuring::backends::rust::ProgressSink {
    Arc::new(|line: &str| {
        log::debug!(target: "tddy_index_daemon::plan_upkeep", "indexing: {line}");
    })
}

fn logged_trace(line: &str) {
    log::debug!(target: "tddy_index_daemon::plan_upkeep", "trace: {line}");
}
