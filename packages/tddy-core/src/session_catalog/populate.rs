//! The catalog populate task: discovers action manifests + build targets and rebuilds the store.
//! Modeled as a [`tddy_task::TaskBody`] so it is observable and cancellable via the `TaskRegistry`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use tddy_task::{TaskBody, TaskContext, TaskStatus};

use super::entry::{project_package, CatalogEntry, CatalogEntryKind};
use super::provider::BuildCatalogProvider;
use super::store;
use crate::session_actions::{list_action_summaries, DiscoveryQuery};

/// The `kind` label recorded on the populate task in the registry.
pub const POPULATE_TASK_KIND: &str = "session_catalog_populate";

/// Scans a session's action manifests and (via the injected provider) its build targets, then
/// rebuilds the catalog at `pool` in one transaction.
pub struct PopulateCatalogTask {
    pub pool: SqlitePool,
    pub session_dir: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub tddy_data_dir: PathBuf,
    /// Build-target source (injected). Production passes [`super::provider::build_catalog_provider`].
    pub build_provider: Option<Arc<dyn BuildCatalogProvider>>,
}

#[async_trait]
impl TaskBody for PopulateCatalogTask {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        if ctx.is_cancelled() {
            return TaskStatus::Cancelled;
        }

        let mut entries: Vec<CatalogEntry> = Vec::new();

        // 1. Action manifests, via the existing glob-based discovery.
        let manifests = list_action_summaries(
            Some(&self.session_dir),
            self.repo_root.as_deref(),
            &self.tddy_data_dir,
            &DiscoveryQuery::default(),
        );
        match manifests {
            Ok(result) => {
                for s in result.actions {
                    let package = project_package(CatalogEntryKind::ActionManifest, &s.path);
                    entries.push(CatalogEntry {
                        kind: CatalogEntryKind::ActionManifest,
                        id: s.id,
                        package,
                        summary: s.summary,
                        path: s.path,
                        has_input_schema: s.has_input_schema,
                        has_output_schema: s.has_output_schema,
                        source_path: None,
                    });
                }
            }
            Err(e) => {
                return TaskStatus::Failed {
                    message: format!("action manifest discovery failed: {e}"),
                };
            }
        }

        // 2. Build targets, via the injected provider (absent provider = no build targets).
        if let Some(provider) = &self.build_provider {
            let repo_root = self.repo_root.as_deref().unwrap_or_else(|| Path::new("."));
            match provider.discover(repo_root) {
                Ok(targets) => {
                    for t in targets {
                        entries.push(CatalogEntry {
                            kind: CatalogEntryKind::BuildTarget,
                            id: t.id.clone(),
                            package: t.package,
                            summary: t.name,
                            path: t.id,
                            has_input_schema: false,
                            has_output_schema: false,
                            source_path: Some(t.source_path),
                        });
                    }
                }
                Err(message) => {
                    return TaskStatus::Failed {
                        message: format!("build target discovery failed: {message}"),
                    };
                }
            }
        }

        // 3. Rebuild the catalog in one transaction.
        match store::rebuild(&self.pool, &entries).await {
            Ok(()) => TaskStatus::Completed { exit_code: Some(0) },
            Err(e) => TaskStatus::Failed {
                message: format!("catalog rebuild failed: {e}"),
            },
        }
    }
}
