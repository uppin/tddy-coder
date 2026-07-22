//! Per-session catalog handle, the process-global registry, and the block-until-populate read path.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use sqlx::SqlitePool;
use tddy_task::{TaskHandle, TaskRegistry};

use super::entry::CatalogEntry;
use super::error::CatalogError;
use super::populate::{self, PopulateCatalogTask};
use super::provider::BuildCatalogProvider;
use super::store;
use crate::session_actions::{ActionListResult, DiscoveryQuery};

/// The catalog database filename within a session directory.
pub const CATALOG_DB_FILENAME: &str = "catalog.db";

/// Path to the catalog database for a session directory: `<session_dir>/catalog.db`.
pub fn catalog_db_path(session_dir: &Path) -> PathBuf {
    session_dir.join(CATALOG_DB_FILENAME)
}

/// A per-session catalog: its sqlite pool plus the optional populate task the first read awaits.
pub struct SessionCatalog {
    pool: SqlitePool,
    /// Populate task handle; `None` when the catalog was opened without a scan (e.g. tests, or a
    /// cross-process reader). Reads await this to terminal before querying.
    populate: Mutex<Option<Arc<TaskHandle>>>,
}

/// Process-global registry of per-session catalogs, keyed by canonical session dir.
static CATALOG: Lazy<DashMap<PathBuf, Arc<SessionCatalog>>> = Lazy::new(DashMap::new);

impl SessionCatalog {
    /// Borrow the underlying pool (for advanced/store-level access in tests).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// The populate task handle, if this catalog was opened with a scan.
    pub fn populate_handle(&self) -> Option<Arc<TaskHandle>> {
        self.populate.lock().unwrap().clone()
    }

    /// Open (creating if missing) a catalog at `db_path` with **no** populate task attached.
    /// Reads do not block. Used by store-level tests and cross-process readers.
    pub async fn open(db_path: &Path) -> Result<Arc<SessionCatalog>, CatalogError> {
        let pool = store::open_pool(db_path).await?;
        Ok(Arc::new(SessionCatalog {
            pool,
            populate: Mutex::new(None),
        }))
    }

    /// Open the catalog at `<session_dir>/catalog.db`, spawn the [`super::PopulateCatalogTask`] on
    /// `registry`, register the handle in the process-global map, and return it. The scan runs
    /// asynchronously; reads block until it is terminal.
    pub async fn open_and_populate(
        session_dir: &Path,
        repo_root: Option<&Path>,
        tddy_data_dir: &Path,
        registry: &TaskRegistry,
        session_id: &str,
        build_provider: Option<Arc<dyn BuildCatalogProvider>>,
    ) -> Result<Arc<SessionCatalog>, CatalogError> {
        let db = catalog_db_path(session_dir);
        let pool = store::open_pool(&db).await?;

        let task = PopulateCatalogTask {
            pool: pool.clone(),
            session_dir: session_dir.to_path_buf(),
            repo_root: repo_root.map(Path::to_path_buf),
            tddy_data_dir: tddy_data_dir.to_path_buf(),
            build_provider,
        };
        let handle = registry
            .spawn(task, populate::POPULATE_TASK_KIND, session_id, vec![])
            .await;

        let catalog = Arc::new(SessionCatalog {
            pool,
            populate: Mutex::new(Some(handle)),
        });

        let key = std::fs::canonicalize(session_dir).unwrap_or_else(|_| session_dir.to_path_buf());
        CATALOG.insert(key, Arc::clone(&catalog));

        Ok(catalog)
    }

    /// Replace the entire catalog with `entries` (single transaction). Used by the populate task
    /// and by store-level tests.
    pub async fn rebuild(&self, entries: &[CatalogEntry]) -> Result<(), CatalogError> {
        store::rebuild(&self.pool, entries).await
    }

    /// Block until the populate task (if any) reaches a terminal status. Returns immediately
    /// once it has (terminal status is sticky), so repeat reads never re-block.
    async fn await_populate(&self) {
        let handle = self.populate.lock().unwrap().clone();
        if let Some(h) = handle {
            let mut rx = h.status_watch();
            let _ = rx.wait_for(|s| s.is_terminal()).await;
        }
    }

    /// List catalog entries matching `query`. Blocks until the populate task (if any) is terminal,
    /// then reads; subsequent reads never re-block (terminal status is sticky).
    pub async fn list(&self, query: &DiscoveryQuery) -> Result<ActionListResult, CatalogError> {
        self.await_populate().await;
        store::query(&self.pool, query).await
    }

    /// List catalog entries whose projected `package` equals `package` (the first index).
    pub async fn list_for_package(&self, package: &str) -> Result<ActionListResult, CatalogError> {
        self.await_populate().await;
        store::query_for_package(&self.pool, package).await
    }
}

/// Look up the registered catalog for `session_dir`, if `open_and_populate` has run for it.
///
/// Canonicalizes `session_dir` so the lookup key matches the canonical key used at insertion
/// (`open_and_populate`), regardless of symlinks / relative form / trailing slash.
pub fn session_catalog(session_dir: &Path) -> Option<Arc<SessionCatalog>> {
    let key = std::fs::canonicalize(session_dir).unwrap_or_else(|_| session_dir.to_path_buf());
    CATALOG.get(&key).map(|entry| Arc::clone(entry.value()))
}
