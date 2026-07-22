//! sqlx SQLite store: pool, schema (JSON blob + projected `package` index), transactional
//! rebuild, and read queries. Uses the runtime query API only (no `query!` macro).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

use super::entry::CatalogEntry;
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
    Ok(())
}

/// Replace the entire catalog with `entries` in a single transaction, and stamp `meta.populated_at`.
pub async fn rebuild(pool: &SqlitePool, entries: &[CatalogEntry]) -> Result<(), CatalogError> {
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
