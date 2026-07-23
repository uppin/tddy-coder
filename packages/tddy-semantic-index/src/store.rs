//! Per-session vector store, backed by a `sqlite-vec` database at `<session_dir>/semantic-index.db`.

use std::path::Path;
use std::sync::{Mutex, Once};

use anyhow::Context;
use rusqlite::{ffi::sqlite3_auto_extension, Connection, OptionalExtension};

/// A chunk paired with its embedding vector, ready to persist.
#[derive(Debug, Clone)]
pub struct IndexedChunk {
    /// Worktree-relative path of the source file.
    pub source_path: String,
    /// The chunk's text content.
    pub text: String,
    /// The chunk's embedding vector.
    pub vector: Vec<f32>,
}

/// A single search result: a stored chunk and its similarity score (higher = nearer).
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Worktree-relative path of the source file.
    pub source_path: String,
    /// The stored chunk text.
    pub text: String,
    /// Similarity score; results are returned in descending (nearest-first) order.
    pub score: f32,
}

/// A per-session vector store. Opened once at `<session_dir>/semantic-index.db`.
///
/// Chunk metadata (`source_path`, `text`) lives in an ordinary `chunks` table; the embedding
/// vectors live in a `sqlite-vec` `vec0` virtual table (`vec_chunks`) keyed by the same rowid, so a
/// KNN match yields rowids we join back to their metadata.
pub struct SemanticIndexStore {
    // rusqlite `Connection` is `Send` but not `Sync`; the `Mutex` makes the store `Sync` so it can
    // be held across `.await` points inside the (Send) index task future.
    conn: Mutex<Connection>,
}

/// Register the `sqlite-vec` extension as an auto-extension exactly once per process, so every
/// connection opened afterwards has the `vec0` virtual table available.
fn ensure_vec_extension() {
    /// The `sqlite3_auto_extension` entry-point signature (in rusqlite's own FFI types).
    type AutoExtensionEntry = unsafe extern "C" fn(
        *mut rusqlite::ffi::sqlite3,
        *mut *mut std::os::raw::c_char,
        *const rusqlite::ffi::sqlite3_api_routines,
    ) -> std::os::raw::c_int;

    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is the C entry point sqlite-vec exposes for exactly this
        // registration; it uses sqlite-vec's own `sqlite3` bindgen types, so the transmute
        // reinterprets it as the identical entry point expressed in rusqlite's FFI types.
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute::<*const (), AutoExtensionEntry>(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Serialize an embedding vector to the little-endian `float32` byte layout `vec0` expects.
fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

impl SemanticIndexStore {
    /// Open (creating if missing) the vector store at `db_path`, ensuring its schema exists.
    pub async fn open(db_path: &Path) -> anyhow::Result<Self> {
        ensure_vec_extension();
        let conn = Connection::open(db_path)
            .with_context(|| format!("open semantic index db at {}", db_path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chunks (
                 id          INTEGER PRIMARY KEY AUTOINCREMENT,
                 source_path TEXT NOT NULL,
                 text        TEXT NOT NULL
             );",
        )
        .context("ensure chunks schema")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Persist `chunks` (text + vector) into the store.
    pub async fn insert(&self, chunks: &[IndexedChunk]) -> anyhow::Result<()> {
        let Some(first) = chunks.first() else {
            return Ok(());
        };
        let dims = first.vector.len();
        anyhow::ensure!(dims > 0, "cannot index a zero-dimension embedding");

        let conn = self.conn.lock().expect("store mutex poisoned");
        // The vec0 table's dimension is fixed at creation; create it lazily from the first vector.
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(embedding float[{dims}]);"
        ))
        .context("ensure vec_chunks schema")?;

        for chunk in chunks {
            anyhow::ensure!(
                chunk.vector.len() == dims,
                "embedding dimension mismatch: expected {dims}, got {}",
                chunk.vector.len()
            );
            conn.execute(
                "INSERT INTO chunks (source_path, text) VALUES (?1, ?2)",
                rusqlite::params![chunk.source_path, chunk.text],
            )
            .context("insert chunk metadata")?;
            let rowid = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO vec_chunks (rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![rowid, vector_to_blob(&chunk.vector)],
            )
            .context("insert chunk vector")?;
        }
        Ok(())
    }

    /// Return the `k` chunks nearest to `query` by vector similarity, nearest first.
    pub async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<SearchHit>> {
        let conn = self.conn.lock().expect("store mutex poisoned");

        // Nothing indexed yet — the vec0 table is created lazily on first insert.
        let has_vectors: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'vec_chunks'",
                [],
                |_| Ok(true),
            )
            .optional()
            .context("probe vec_chunks table")?
            .unwrap_or(false);
        if !has_vectors {
            return Ok(Vec::new());
        }

        // KNN over the vec0 table, then join each hit's rowid back to its metadata. The two-step
        // form keeps the vector MATCH as a standalone query, which the sqlite-vec planner requires.
        let hits: Vec<(i64, f64)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT rowid, distance FROM vec_chunks
                     WHERE embedding MATCH ?1
                     ORDER BY distance
                     LIMIT ?2",
                )
                .context("prepare KNN query")?;
            let rows = stmt
                .query_map(rusqlite::params![vector_to_blob(query), k as i64], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
                })
                .context("run KNN query")?;
            rows.collect::<Result<_, _>>().context("collect KNN rows")?
        };

        let mut results = Vec::with_capacity(hits.len());
        for (rowid, distance) in hits {
            let (source_path, text): (String, String) = conn
                .query_row(
                    "SELECT source_path, text FROM chunks WHERE id = ?1",
                    rusqlite::params![rowid],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .context("load chunk metadata for hit")?;
            results.push(SearchHit {
                source_path,
                text,
                // Vectors are L2-normalized, so squared-L2 distance `d` maps to cosine similarity
                // `1 - d/2`. Monotonic decreasing in distance → higher score = nearer.
                score: (1.0 - distance / 2.0) as f32,
            });
        }
        Ok(results)
    }
}
