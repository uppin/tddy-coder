//! sqlx SQLite store: pool, schema (JSON blob + projected `package` index), transactional
//! rebuild, and read queries. Uses the runtime query API only (no `query!` macro).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

use super::entry::{BuildTargetCatalogEntry, CatalogCapabilities, CatalogEntry};
use super::error::CatalogError;
use crate::session_actions::{ActionListResult, ActionSummary, DiscoveryQuery};

/// Open (creating if missing) a WAL-mode pool at `db_path` and ensure the schema exists.
pub async fn open_pool(db_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    ensure_schema(&pool).await?;
    Ok(pool)
}

/// Create the `catalog` table (with the VIRTUAL generated `package` column + index) and the
/// `meta` table if they do not exist.
pub async fn ensure_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS catalog (
            kind TEXT NOT NULL,
            path TEXT NOT NULL,
            json TEXT NOT NULL,
            package TEXT GENERATED ALWAYS AS (json_extract(json,'$.package')) VIRTUAL,
            PRIMARY KEY (kind, path)
        ) WITHOUT ROWID;",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_catalog_package ON catalog(package);")
        .execute(pool)
        .await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
        .execute(pool)
        .await?;
    // Dedicated, typed build-target table — the authoritative source for the BSP service. Kept
    // beside the shared `catalog` table (which still carries lightweight build_target rows for the
    // unified `list`). JSON arrays for the list-valued columns; INTEGER bools for capabilities.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS build_targets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            package TEXT NOT NULL,
            base_dir TEXT,
            target_type TEXT,
            tags TEXT NOT NULL DEFAULT '[]',
            languages TEXT NOT NULL DEFAULT '[]',
            deps TEXT NOT NULL DEFAULT '[]',
            sources TEXT NOT NULL DEFAULT '[]',
            outputs TEXT NOT NULL DEFAULT '[]',
            can_compile INTEGER NOT NULL DEFAULT 0,
            can_test INTEGER NOT NULL DEFAULT 0,
            can_run INTEGER NOT NULL DEFAULT 0,
            can_debug INTEGER NOT NULL DEFAULT 0,
            source_path TEXT NOT NULL
        ) WITHOUT ROWID;",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_build_targets_package ON build_targets(package);")
        .execute(pool)
        .await?;
    Ok(())
}

/// The rich per-build-target read shape served to the BSP layer (unlike the collapsed
/// [`ActionSummary`], this preserves capabilities/tags/languages/deps/sources/outputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTargetSummary {
    pub id: String,
    pub name: String,
    pub package: String,
    pub base_dir: Option<String>,
    pub target_type: Option<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub deps: Vec<String>,
    pub sources: Vec<String>,
    pub outputs: Vec<String>,
    pub capabilities: super::entry::CatalogCapabilities,
    pub source_path: String,
}

/// List build targets from the dedicated `build_targets` table, ascending by id, applying the
/// optional case-insensitive substring `query` over id/name.
pub async fn list_build_targets(
    pool: &SqlitePool,
    discovery: &DiscoveryQuery,
) -> Result<Vec<BuildTargetSummary>, CatalogError> {
    let substr = discovery.query.as_deref();
    let rows = sqlx::query(
        "SELECT id, name, package, base_dir, target_type, tags, languages, deps, sources, outputs,
                can_compile, can_test, can_run, can_debug, source_path
         FROM build_targets
         WHERE (?1 IS NULL
                OR instr(lower(id), lower(?1)) > 0
                OR instr(lower(name), lower(?1)) > 0)
         ORDER BY id ASC",
    )
    .bind(substr)
    .fetch_all(pool)
    .await?;

    rows.iter().map(row_to_build_target).collect()
}

/// List build targets whose `package` equals `package`, ascending by id.
pub async fn list_build_targets_for_package(
    pool: &SqlitePool,
    package: &str,
) -> Result<Vec<BuildTargetSummary>, CatalogError> {
    let rows = sqlx::query(
        "SELECT id, name, package, base_dir, target_type, tags, languages, deps, sources, outputs,
                can_compile, can_test, can_run, can_debug, source_path
         FROM build_targets
         WHERE package = ?1
         ORDER BY id ASC",
    )
    .bind(package)
    .fetch_all(pool)
    .await?;

    rows.iter().map(row_to_build_target).collect()
}

/// Decode one `build_targets` row into a [`BuildTargetSummary`], parsing the JSON-array text
/// columns and the `can_*` INTEGER bools.
fn row_to_build_target(row: &sqlx::sqlite::SqliteRow) -> Result<BuildTargetSummary, CatalogError> {
    let json_vec =
        |value: String| -> Result<Vec<String>, CatalogError> { Ok(serde_json::from_str(&value)?) };
    Ok(BuildTargetSummary {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        package: row.try_get("package")?,
        base_dir: row.try_get("base_dir")?,
        target_type: row.try_get("target_type")?,
        tags: json_vec(row.try_get("tags")?)?,
        languages: json_vec(row.try_get("languages")?)?,
        deps: json_vec(row.try_get("deps")?)?,
        sources: json_vec(row.try_get("sources")?)?,
        outputs: json_vec(row.try_get("outputs")?)?,
        capabilities: CatalogCapabilities {
            compile: row.try_get::<i64, _>("can_compile")? != 0,
            test: row.try_get::<i64, _>("can_test")? != 0,
            run: row.try_get::<i64, _>("can_run")? != 0,
            debug: row.try_get::<i64, _>("can_debug")? != 0,
        },
        source_path: row.try_get("source_path")?,
    })
}

/// Replace the entire catalog (both the shared `catalog` table and the dedicated `build_targets`
/// table) in a single transaction, and stamp `meta.populated_at`.
pub async fn rebuild(
    pool: &SqlitePool,
    entries: &[CatalogEntry],
    build_targets: &[BuildTargetCatalogEntry],
) -> Result<(), CatalogError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM catalog").execute(&mut *tx).await?;
    for e in entries {
        let json = serde_json::to_string(e)?;
        sqlx::query("INSERT OR REPLACE INTO catalog (kind, path, json) VALUES (?1, ?2, ?3)")
            .bind(e.kind.as_str())
            .bind(&e.path)
            .bind(&json)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM build_targets")
        .execute(&mut *tx)
        .await?;
    for t in build_targets {
        sqlx::query(
            "INSERT OR REPLACE INTO build_targets
             (id, name, package, base_dir, target_type, tags, languages, deps, sources, outputs,
              can_compile, can_test, can_run, can_debug, source_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )
        .bind(&t.id)
        .bind(&t.name)
        .bind(&t.package)
        .bind(&t.base_dir)
        .bind(&t.target_type)
        .bind(serde_json::to_string(&t.tags)?)
        .bind(serde_json::to_string(&t.languages)?)
        .bind(serde_json::to_string(&t.deps)?)
        .bind(serde_json::to_string(&t.sources)?)
        .bind(serde_json::to_string(&t.outputs)?)
        .bind(t.capabilities.compile as i64)
        .bind(t.capabilities.test as i64)
        .bind(t.capabilities.run as i64)
        .bind(t.capabilities.debug as i64)
        .bind(&t.source_path)
        .execute(&mut *tx)
        .await?;
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    sqlx::query("INSERT OR REPLACE INTO meta (key, value) VALUES ('populated_at', ?1)")
        .bind(now_ms.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Run a [`DiscoveryQuery`] against the catalog: literal `path`-prefix filter (matching the prior
/// `starts_with` semantics of `session_actions::list`, not SQL `LIKE`, so wildcard chars in the
/// prefix stay literal) + case-insensitive substring over id/summary/path + `ORDER BY path` +
/// `limit`/`offset`, with a pre-pagination `total`. Per-package lookup uses the `package` index via
/// [`query_for_package`].
pub async fn query(
    pool: &SqlitePool,
    discovery: &DiscoveryQuery,
) -> Result<ActionListResult, CatalogError> {
    let prefix = discovery.path_prefix.as_deref();
    let substr = discovery.query.as_deref();

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM catalog
         WHERE (?1 IS NULL OR instr(path, ?1) = 1)
           AND (?2 IS NULL
                OR instr(lower(json_extract(json,'$.id')), lower(?2)) > 0
                OR instr(lower(json_extract(json,'$.summary')), lower(?2)) > 0
                OR instr(lower(path), lower(?2)) > 0)",
    )
    .bind(prefix)
    .bind(substr)
    .fetch_one(pool)
    .await?;

    let limit = discovery.limit.map(|l| l as i64).unwrap_or(-1);
    let offset = discovery.offset as i64;

    let rows = sqlx::query(
        "SELECT json FROM catalog
         WHERE (?1 IS NULL OR instr(path, ?1) = 1)
           AND (?2 IS NULL
                OR instr(lower(json_extract(json,'$.id')), lower(?2)) > 0
                OR instr(lower(json_extract(json,'$.summary')), lower(?2)) > 0
                OR instr(lower(path), lower(?2)) > 0)
         ORDER BY path ASC LIMIT ?3 OFFSET ?4",
    )
    .bind(prefix)
    .bind(substr)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let mut actions = Vec::with_capacity(rows.len());
    for row in rows {
        let json: String = row.try_get("json")?;
        actions.push(entry_to_summary(&json)?);
    }

    Ok(ActionListResult {
        actions,
        total: total as usize,
    })
}

/// List entries whose projected `package` equals `package`, ascending by path.
pub async fn query_for_package(
    pool: &SqlitePool,
    package: &str,
) -> Result<ActionListResult, CatalogError> {
    let rows = sqlx::query("SELECT json FROM catalog WHERE package = ?1 ORDER BY path ASC")
        .bind(package)
        .fetch_all(pool)
        .await?;

    let mut actions = Vec::with_capacity(rows.len());
    for row in rows {
        let json: String = row.try_get("json")?;
        actions.push(entry_to_summary(&json)?);
    }

    let total = actions.len();
    Ok(ActionListResult { actions, total })
}

/// Deserialize a stored `json` blob into a [`CatalogEntry`] and project it to an [`ActionSummary`].
fn entry_to_summary(json: &str) -> Result<ActionSummary, CatalogError> {
    let entry: CatalogEntry = serde_json::from_str(json)?;
    Ok(ActionSummary {
        id: entry.id,
        summary: entry.summary,
        has_input_schema: entry.has_input_schema,
        has_output_schema: entry.has_output_schema,
        path: entry.path,
    })
}

#[cfg(test)]
mod build_target_tests {
    use super::*;

    async fn seeded_pool() -> SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let pool = open_pool(&dir.path().join("catalog.db"))
            .await
            .expect("open pool");
        // Two targets under packages/foo, seeded directly into the dedicated table.
        sqlx::query(
            "INSERT INTO build_targets
             (id, name, package, base_dir, target_type, tags, languages, deps, sources, outputs,
              can_compile, can_test, can_run, can_debug, source_path)
             VALUES
             ('packages/foo:lib','Foo library','packages/foo','packages/foo','rust_library',
              '[\"library\"]','[\"rust\"]','[]','[\"packages/foo/src/lib.rs\"]','[]',
              1,1,0,0,'/repo/packages/foo/BUILD.yaml'),
             ('packages/bar:app','Bar app','packages/bar','packages/bar','rust_binary',
              '[\"application\"]','[\"rust\"]','[\"packages/foo:lib\"]','[]','[\"target/debug/bar\"]',
              1,1,1,0,'/repo/packages/bar/BUILD.yaml')",
        )
        .execute(&pool)
        .await
        .expect("seed build_targets");
        // Keep the tempdir alive for the pool's lifetime by leaking it (test-only).
        std::mem::forget(dir);
        pool
    }

    #[tokio::test]
    async fn list_build_targets_round_trips_the_rich_projection() {
        // Given
        let pool = seeded_pool().await;

        // When
        let targets = list_build_targets(&pool, &DiscoveryQuery::default())
            .await
            .expect("list build targets");

        // Then — ascending by id, all rich fields decoded.
        let ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["packages/bar:app", "packages/foo:lib"]);

        let lib = targets.iter().find(|t| t.id == "packages/foo:lib").unwrap();
        assert_eq!(lib.name, "Foo library");
        assert_eq!(lib.target_type.as_deref(), Some("rust_library"));
        assert_eq!(lib.base_dir.as_deref(), Some("packages/foo"));
        assert_eq!(lib.tags, vec!["library".to_string()]);
        assert_eq!(lib.languages, vec!["rust".to_string()]);
        assert_eq!(lib.sources, vec!["packages/foo/src/lib.rs".to_string()]);
        assert!(lib.capabilities.compile && lib.capabilities.test);
        assert!(!lib.capabilities.run);

        let app = targets.iter().find(|t| t.id == "packages/bar:app").unwrap();
        assert_eq!(app.deps, vec!["packages/foo:lib".to_string()]);
        assert_eq!(app.outputs, vec!["target/debug/bar".to_string()]);
        assert!(app.capabilities.run);
    }

    #[tokio::test]
    async fn list_build_targets_for_package_filters_by_projected_package() {
        // Given
        let pool = seeded_pool().await;

        // When
        let targets = list_build_targets_for_package(&pool, "packages/foo")
            .await
            .expect("list for package");

        // Then — only the single packages/foo target.
        let ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["packages/foo:lib"]);
    }
}
