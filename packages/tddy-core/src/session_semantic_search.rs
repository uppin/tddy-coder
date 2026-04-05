//! Local semantic search index over workflow sessions (SQLite + embeddings).
//!
//! The SQLite file lives at `{tddy_data_dir}/{SESSION_SEARCH_INDEX_FILENAME}`.
//! Schema version and embedding model metadata are stored in-DB for migrations/rebuilds.
//!
//! ## Embeddings
//!
//! V1 uses a deterministic **hashing trick** embedding (local, no network): tokens are hashed
//! into a fixed-size dense vector, L2-normalized, and compared with dot product (cosine
//! similarity). Model id [`SESSION_SEARCH_EMBEDDING_MODEL_ID`] + [`SESSION_SEARCH_EMBEDDING_DIM`]
//! are pinned for rebuild/migration. Corrupt or incompatible DB files can be deleted; sessions
//! re-index from each session dir's `changeset.yaml` via [`index_session_for_search`].

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};

use crate::changeset::{read_changeset, Changeset};
use crate::error::WorkflowError;

/// Stable basename for the session search SQLite database under the Tddy data directory root.
pub const SESSION_SEARCH_INDEX_FILENAME: &str = "session_search_index.sqlite3";

/// On-disk schema / migration lineage for the session search index (PRD: versioned migrations).
pub const SESSION_SEARCH_INDEX_SCHEMA_VERSION: u32 = 1;

/// Vector dimension for [`embed_document`] / stored blobs (must match migration DDL).
pub const SESSION_SEARCH_EMBEDDING_DIM: usize = 256;

/// Pinned embedding model identifier stored beside vectors for rebuild/migration (PRD).
pub const SESSION_SEARCH_EMBEDDING_MODEL_ID: &str = "tddy-hash-trick-v1-dim256";

/// Resolved absolute path to the SQLite index for a given Tddy data root.
pub fn session_search_index_path(data_root: &Path) -> PathBuf {
    data_root.join(SESSION_SEARCH_INDEX_FILENAME)
}

/// Deterministic searchable worktree string: prefer persisted `worktree`, else `worktree_suggestion`.
pub fn merge_worktree_label(cs: &Changeset) -> String {
    if let Some(w) = cs
        .worktree
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return w.to_string();
    }
    cs.worktree_suggestion
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

/// Deterministic searchable branch string: prefer persisted `branch`, else `branch_suggestion`.
pub fn merge_branch_label(cs: &Changeset) -> String {
    if let Some(b) = cs
        .branch
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return b.to_string();
    }
    cs.branch_suggestion
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

/// Concatenated text blob used for embedding and lexical fallbacks (prompt + worktree + branch labels).
pub fn build_index_document_text(cs: &Changeset) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = cs
        .initial_prompt
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        parts.push(p.to_string());
    }
    let wt = merge_worktree_label(cs);
    if !wt.is_empty() {
        parts.push(format!("worktree {wt}"));
    }
    let br = merge_branch_label(cs);
    if !br.is_empty() {
        parts.push(format!("branch {br}"));
    }
    parts.join("\n")
}

/// One ranked result from [`search_sessions_semantic`].
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSearchHit {
    pub session_id: String,
    pub relevance_score: f32,
    pub initial_prompt: String,
    pub worktree_label: String,
    pub branch_label: String,
}

fn map_sqlite(e: rusqlite::Error) -> WorkflowError {
    WorkflowError::SessionSearchIndex(e.to_string())
}

/// Stable FNV-1a 64-bit hash for token → embedding indices.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 1469598103934665603;
    const FNV_PRIME: u64 = 1099511628211;
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

fn normalize_l2(v: &mut [f32]) {
    let sum: f32 = v.iter().map(|x| x * x).sum();
    let n = sum.sqrt();
    if n > 1e-8 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Deterministic embedding: multi-probe hashing trick + L2 normalization (cosine = dot product).
pub fn embed_document(text: &str) -> Vec<f32> {
    let tokens = tokenize(text);
    let nt = tokens.len().max(1) as f32;
    let scale = 1.0 / nt.sqrt();
    let mut v = vec![0.0f32; SESSION_SEARCH_EMBEDDING_DIM];
    for t in &tokens {
        let h = fnv1a64(t.as_bytes());
        for probe in 0u32..3 {
            let mixed = h.wrapping_add(u64::from(probe).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let idx = (mixed % SESSION_SEARCH_EMBEDDING_DIM as u64) as usize;
            v[idx] += scale;
        }
    }
    normalize_l2(&mut v);
    v
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>, WorkflowError> {
    if !blob.len().is_multiple_of(4) {
        return Err(WorkflowError::SessionSearchIndex(
            "embedding blob length is not a multiple of 4".to_string(),
        ));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Score: cosine similarity plus small boosts for substring / token overlap (stable, local).
fn relevance_score(query: &str, doc_text: &str, q_emb: &[f32], d_emb: &[f32]) -> f32 {
    let mut s = dot_product(q_emb, d_emb).clamp(-1.0, 1.0);
    let ql = query.to_lowercase();
    let dl = doc_text.to_lowercase();
    if !ql.is_empty() && dl.contains(&ql) {
        s += 0.35;
    }
    let q_tokens = tokenize(query);
    let d_tokens = tokenize(doc_text);
    if !q_tokens.is_empty() && !d_tokens.is_empty() {
        let overlap: usize = q_tokens
            .iter()
            .filter(|t| d_tokens.iter().any(|d| d == *t))
            .count();
        s += 0.02 * (overlap as f32 / q_tokens.len() as f32).min(1.0);
    }
    s
}

fn open_index_connection(path: &Path) -> Result<Connection, WorkflowError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
    Connection::open_with_flags(path, flags).map_err(map_sqlite)
}

fn ensure_schema(conn: &Connection) -> Result<(), WorkflowError> {
    let v: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(map_sqlite)?;
    log::debug!(
        target: "tddy_core::session_semantic_search",
        "session search DB user_version={}",
        v
    );
    if v < SESSION_SEARCH_INDEX_SCHEMA_VERSION as i32 {
        log::info!(
            target: "tddy_core::session_semantic_search",
            "migrating session search index to schema v{}",
            SESSION_SEARCH_INDEX_SCHEMA_VERSION
        );
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS session_search_index (
                session_id TEXT PRIMARY KEY NOT NULL,
                initial_prompt TEXT NOT NULL,
                worktree_label TEXT NOT NULL,
                branch_label TEXT NOT NULL,
                embedding BLOB NOT NULL,
                embedding_dim INTEGER NOT NULL,
                embedding_model_id TEXT NOT NULL,
                document_text TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_session_search_updated ON session_search_index(updated_at);
            "#,
        )
        .map_err(map_sqlite)?;
        conn.pragma_update(None, "user_version", SESSION_SEARCH_INDEX_SCHEMA_VERSION)
            .map_err(map_sqlite)?;
    }
    Ok(())
}

/// Writes or updates the search index for a single session directory under `data_root`.
///
/// Must create the database file on first successful index operation (see acceptance tests).
pub fn index_session_for_search(
    data_root: &Path,
    session_id: &str,
    session_dir: &Path,
) -> Result<(), WorkflowError> {
    log::info!(
        target: "tddy_core::session_semantic_search",
        "index_session_for_search: session_id={} dir={}",
        session_id,
        session_dir.display()
    );

    std::fs::create_dir_all(data_root).map_err(|e| {
        WorkflowError::SessionSearchIndex(format!("create data_root {}: {e}", data_root.display()))
    })?;

    let cs = read_changeset(session_dir)?;
    let initial_prompt = cs
        .initial_prompt
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();
    let worktree_label = merge_worktree_label(&cs);
    let branch_label = merge_branch_label(&cs);
    let document_text = build_index_document_text(&cs);
    log::debug!(
        target: "tddy_core::session_semantic_search",
        "index_session_for_search: doc_len={} worktree_len={} branch_len={}",
        document_text.len(),
        worktree_label.len(),
        branch_label.len()
    );

    let embedding = embed_document(&document_text);
    debug_assert_eq!(embedding.len(), SESSION_SEARCH_EMBEDDING_DIM);
    let blob = embedding_to_blob(&embedding);
    let now = chrono::Utc::now().to_rfc3339();

    let db_path = session_search_index_path(data_root);
    let conn = open_index_connection(&db_path)?;
    ensure_schema(&conn)?;

    conn.execute(
        r#"
        INSERT INTO session_search_index (
            session_id, initial_prompt, worktree_label, branch_label,
            embedding, embedding_dim, embedding_model_id, document_text, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(session_id) DO UPDATE SET
            initial_prompt = excluded.initial_prompt,
            worktree_label = excluded.worktree_label,
            branch_label = excluded.branch_label,
            embedding = excluded.embedding,
            embedding_dim = excluded.embedding_dim,
            embedding_model_id = excluded.embedding_model_id,
            document_text = excluded.document_text,
            updated_at = excluded.updated_at
        "#,
        params![
            session_id,
            initial_prompt,
            worktree_label,
            branch_label,
            blob,
            SESSION_SEARCH_EMBEDDING_DIM as i64,
            SESSION_SEARCH_EMBEDDING_MODEL_ID,
            document_text,
            now,
        ],
    )
    .map_err(map_sqlite)?;

    Ok(())
}

/// Runs semantic similarity search across indexed sessions; results are ordered by descending relevance.
pub fn search_sessions_semantic(
    data_root: &Path,
    query: &str,
) -> Result<Vec<SessionSearchHit>, WorkflowError> {
    log::debug!(
        target: "tddy_core::session_semantic_search",
        "search_sessions_semantic: query_len={}",
        query.len()
    );
    let db_path = session_search_index_path(data_root);
    if !db_path.exists() {
        log::debug!(
            target: "tddy_core::session_semantic_search",
            "search_sessions_semantic: no index file at {:?}",
            db_path
        );
        return Ok(vec![]);
    }

    let conn = open_index_connection(&db_path)?;
    ensure_schema(&conn)?;

    let q_trim = query.trim();
    if q_trim.is_empty() {
        return Ok(vec![]);
    }

    let q_emb = embed_document(q_trim);

    let mut stmt = conn
        .prepare(
            "SELECT session_id, initial_prompt, worktree_label, branch_label, embedding, document_text, embedding_model_id
             FROM session_search_index",
        )
        .map_err(map_sqlite)?;

    let mut rows = stmt.query([]).map_err(map_sqlite)?;
    let mut scored: Vec<(f32, SessionSearchHit)> = Vec::new();

    while let Some(row) = rows.next().map_err(map_sqlite)? {
        let sid: String = row.get(0).map_err(map_sqlite)?;
        let initial_prompt: String = row.get(1).map_err(map_sqlite)?;
        let worktree_label: String = row.get(2).map_err(map_sqlite)?;
        let branch_label: String = row.get(3).map_err(map_sqlite)?;
        let emb_blob: Vec<u8> = row.get(4).map_err(map_sqlite)?;
        let doc_text: String = row.get(5).map_err(map_sqlite)?;
        let model_id: String = row.get(6).map_err(map_sqlite)?;

        if model_id != SESSION_SEARCH_EMBEDDING_MODEL_ID {
            log::warn!(
                target: "tddy_core::session_semantic_search",
                "row {} stored with model {}; current is {}",
                sid,
                model_id,
                SESSION_SEARCH_EMBEDDING_MODEL_ID
            );
        }

        let d_emb = blob_to_embedding(&emb_blob)?;
        if d_emb.len() != SESSION_SEARCH_EMBEDDING_DIM {
            log::warn!(
                target: "tddy_core::session_semantic_search",
                "skip session {}: embedding dim mismatch",
                sid
            );
            continue;
        }

        let score = relevance_score(q_trim, &doc_text, &q_emb, &d_emb);
        scored.push((
            score,
            SessionSearchHit {
                session_id: sid,
                relevance_score: score,
                initial_prompt,
                worktree_label,
                branch_label,
            },
        ));
    }

    // No strong match anywhere → empty results (avoids spurious hits on irrelevant probes).
    const MIN_BEST_SCORE: f32 = 0.15;
    let max_score = scored.iter().map(|(s, _)| *s).fold(0.0f32, f32::max);
    if max_score < MIN_BEST_SCORE {
        log::debug!(
            target: "tddy_core::session_semantic_search",
            "search_sessions_semantic: best score {} below floor {}",
            max_score,
            MIN_BEST_SCORE
        );
        return Ok(vec![]);
    }

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.session_id.cmp(&a.1.session_id))
    });

    let out: Vec<SessionSearchHit> = scored.into_iter().map(|(_, h)| h).collect();
    log::info!(
        target: "tddy_core::session_semantic_search",
        "search_sessions_semantic: {} hit(s)",
        out.len()
    );
    Ok(out)
}
