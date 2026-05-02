//! Per-session **action cache**: fingerprinted `tddy-tools submit` payloads persisted beside engine
//! state under `{session_dir}/.workflow/action-cache.json`.
//!
//! ## Concurrency
//!
//! Persistence uses write-then-rename on a single `.tmp` file in the same directory. Callers assume
//! a **single CLI writer per session directory**; concurrent multi-process writers against the same
//! session tree are unsupported and may race — align with daemon/CLI orchestration semantics.

use crate::workflow::context::Context;
use crate::workflow::session::workflow_engine_storage_dir;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const ACTION_CACHE_FILENAME: &str = "action-cache.json";

pub const ACTION_CACHE_SCHEMA_VERSION: u32 = 1;

/// Canonical fingerprint envelope id embedded in persisted records and fingerprint digests.
pub const FINGERPRINT_ALGORITHM_ID: &str = "tddy_fp_v1";

#[inline]
pub fn action_cache_file_path(session_dir: &Path) -> PathBuf {
    workflow_engine_storage_dir(session_dir).join(ACTION_CACHE_FILENAME)
}

/// Human-inspectable cache document (`schema_version` forwards compatibility).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionCacheDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub entries: std::collections::HashMap<String, serde_json::Value>,
}

/// Inputs that must fingerprint identically across invocations sharing a cache bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionFingerprintParts {
    pub goal_id: String,
    pub effective_prompt: String,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
}

#[derive(Serialize)]
struct CanonicalFingerprintEnvelope<'a> {
    alg: &'static str,
    v: u32,
    goal_id: &'a str,
    effective_prompt: &'a str,
    system_prompt: &'a str,
    model: &'a str,
}

/// Returns true when action-cache reads/writes must be skipped (explicit opt-out from context flag
/// **`disable_action_cache`** or environment **`TDDY_DISABLE_ACTION_CACHE`**).
pub fn action_cache_disabled(context: &Context) -> bool {
    let flag = context
        .get_sync::<bool>("disable_action_cache")
        .unwrap_or(false);
    let env_toggle = disable_action_cache_via_env();

    log::debug!(
        target: "tddy_core::workflow::action_cache",
        "action_cache_disabled: context_flag={}, env_toggle={}",
        flag,
        env_toggle
    );

    flag || env_toggle
}

fn disable_action_cache_via_env() -> bool {
    match std::env::var("TDDY_DISABLE_ACTION_CACHE") {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        Err(_) => false,
    }
}

/// Deterministic fingerprint digest (**`algorithm:hex`**). Normalizes textual fields (trim prompts,
/// drop empty optional strings) before hashing canonical JSON so logically equivalent payloads match.
///
/// Prefer digesting hashed system prompts for very large bodies instead of emitting raw blobs in
/// the canonical JSON — current implementation hashes only the compact canonical structure; callers
/// may pass hashed `system_prompt` upstream if secrecy requires it without changing this envelope.
pub fn fingerprint_action_inputs(parts: &ActionFingerprintParts) -> Option<String> {
    let goal_id = parts.goal_id.trim();
    let prompt = parts.effective_prompt.trim();

    let system_prompt = parts
        .system_prompt
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("");

    let model = parts
        .model
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("");

    let canon = CanonicalFingerprintEnvelope {
        alg: FINGERPRINT_ALGORITHM_ID,
        v: 1,
        goal_id,
        effective_prompt: prompt,
        system_prompt,
        model,
    };

    let bytes = serde_json::to_vec(&canon).ok()?;
    let fnv_digest = fnv1a64(&bytes);

    Some(format!("{}:{:016x}", FINGERPRINT_ALGORITHM_ID, fnv_digest))
}

#[inline]
fn fnv_offset() -> u64 {
    0xcbf29ce484222325 // FNV offset basis — std does not expose FNVHasher on stable reliably
}

fn fnv_prime() -> u64 {
    0x100000001b3
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = fnv_offset();
    let prime = fnv_prime();
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(prime);
    }
    hash
}

/// Compose a deterministic `action_key` string unique within one session (**graph × task × goal**).
pub fn stable_action_cache_key(graph_id: &str, task_id: &str, goal_id: &str) -> String {
    format!("{}|{}|{}", graph_id.trim(), task_id.trim(), goal_id.trim())
}

fn entry_storage_bucket_key(action_key: &str, fingerprint_digest: &str) -> String {
    format!(
        "{}_{:016x}",
        FINGERPRINT_ALGORITHM_ID.replace('-', "_"),
        fnv1a64(format!("{}\0{}", action_key, fingerprint_digest).as_bytes(),),
    )
}

/// Load every JSON object entry under `.entries`; return the **`output`** string when **`action_key`**
/// and **`fingerprint`** match.
pub fn lookup_cached_completed_submit(
    session_dir: &Path,
    action_key: &str,
    fingerprint_digest: &str,
) -> Option<String> {
    let path = action_cache_file_path(session_dir);
    if !path.exists() {
        log::debug!(
            target: "tddy_core::workflow::action_cache",
            "lookup_cached_completed_submit miss: {:?} absent",
            path
        );
        return None;
    }

    let raw = fs::read_to_string(&path).ok()?;
    let doc: ActionCacheDocument = serde_json::from_str(&raw).ok()?;

    for (bucket, entry_val) in &doc.entries {
        let entry = entry_val.as_object()?;
        let ak = entry.get("action_key")?.as_str()?;
        let fp = entry.get("fingerprint")?.as_str()?;
        if ak == action_key && fp == fingerprint_digest {
            let out_str = normalize_cache_output_payload(entry.get("output")?)?;
            log::info!(
                target: "tddy_core::workflow::action_cache",
                "action-cache HIT bucket={:?} ak_len={} fp_len={} out_len={}",
                bucket,
                action_key.len(),
                fingerprint_digest.len(),
                out_str.len(),
            );
            return Some(out_str);
        }
    }

    log::debug!(
        target: "tddy_core::workflow::action_cache",
        "lookup_cached_completed_submit miss: {:?} examined {} entries — no fingerprint match",
        path,
        doc.entries.len(),
    );

    None
}

fn normalize_cache_output_payload(token: &Value) -> Option<String> {
    match token {
        Value::String(body) => Some(body.clone()),
        other => Some(other.to_string()),
    }
}

/// Upsert persisted cache after **`tddy-tools submit`** ingestion (successful submit path only).
pub fn persist_successful_submit_to_action_cache(
    session_dir: &Path,
    goal_id_for_record: &str,
    action_key: &str,
    fingerprint_digest: &str,
    output_submit_json: &str,
) -> io::Result<()> {
    log::debug!(
        target: "tddy_core::workflow::action_cache",
        "persist_successful_submit: session_dir=? goal={} ak_len={} fp_len={} output_len={}",
        goal_id_for_record,
        action_key.len(),
        fingerprint_digest.len(),
        output_submit_json.len(),
    );

    let path = action_cache_file_path(session_dir);
    let bucket = entry_storage_bucket_key(action_key, fingerprint_digest);

    log::debug!(
        target: "tddy_core::workflow::action_cache",
        "persist_successful_submit: bucket={:?}",
        bucket
    );

    let mut doc = load_or_default_action_cache_document(&path)?;
    merge_action_cache_entry_stub(
        &mut doc,
        &bucket,
        action_key,
        fingerprint_digest,
        goal_id_for_record,
        output_submit_json,
    );
    flush_action_cache_document_atomic(&path, &doc)?;

    log::info!(
        target: "tddy_core::workflow::action_cache",
        "action-cache WRITE {:?} buckets={}",
        path,
        doc.entries.len()
    );

    Ok(())
}

fn load_or_default_action_cache_document(path: &Path) -> io::Result<ActionCacheDocument> {
    if !path.exists() {
        return Ok(ActionCacheDocument {
            schema_version: ACTION_CACHE_SCHEMA_VERSION,
            entries: Default::default(),
        });
    }

    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<ActionCacheDocument>(&raw) {
            Ok(mut doc) => {
                doc.schema_version = doc.schema_version.max(ACTION_CACHE_SCHEMA_VERSION).max(1);

                Ok(doc)
            }
            Err(e) => {
                log::warn!(
                    target: "tddy_core::workflow::action_cache",
                    "action-cache JSON parse {:?} broken ({}): resetting in-memory envelope",
                    path,
                    e
                );
                Ok(ActionCacheDocument {
                    schema_version: ACTION_CACHE_SCHEMA_VERSION,
                    entries: Default::default(),
                })
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(ActionCacheDocument {
            schema_version: ACTION_CACHE_SCHEMA_VERSION,
            entries: Default::default(),
        }),
        Err(e) => Err(e),
    }
}

fn flush_action_cache_document_atomic(path: &Path, doc: &ActionCacheDocument) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_name = format!("action-cache.json.part{}.tmp", uuid::Uuid::now_v7());
    let tmp_path = path.with_file_name(&tmp_name);

    log::trace!(
        target: "tddy_core::workflow::action_cache",
        "flush_action_cache tmp={:?} final={:?}",
        tmp_path,
        path,
    );

    let serialized = serde_json::to_vec_pretty(doc).map_err(|e| io::Error::other(e.to_string()))?;

    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&serialized)?;
        file.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Upsert (**merge**) a serialized cache entry keyed by **`storage_key`** without removing siblings.
///
/// Used by persistence and unit tests asserting HashMap merges.
pub fn merge_action_cache_entry_stub(
    doc: &mut ActionCacheDocument,
    storage_key: &str,
    action_key: &str,
    fingerprint_digest: &str,
    goal_id: &str,
    output_submit_json: &str,
) {
    doc.schema_version = doc.schema_version.max(ACTION_CACHE_SCHEMA_VERSION).max(1);

    let entry = json!({
        "action_key": action_key,
        "fingerprint": fingerprint_digest,
        "fingerprint_algorithm_id": FINGERPRINT_ALGORITHM_ID,
        "goal_id": goal_id,
        "output": output_submit_json,
    });

    doc.entries.insert(storage_key.to_string(), entry);

    log::trace!(
        target: "tddy_core::workflow::action_cache",
        "merge_action_cache_entry_stub inserted key {:?} entries={}",
        storage_key,
        doc.entries.len()
    );
}

/// Human-readable deterministic inspection map (sorted keys) for fingerprints / merges.
///
/// Exported for tooling; keep stable ordering when rendering debug panels.
pub fn debug_canonical_inputs_map(parts: &ActionFingerprintParts) -> BTreeMap<String, Value> {
    let mut btree = BTreeMap::new();
    btree.insert("goal_id".into(), json!(parts.goal_id.trim()));
    btree.insert(
        "effective_prompt".into(),
        json!(parts.effective_prompt.trim()),
    );

    btree.insert(
        "system_prompt".into(),
        json!(parts.system_prompt.as_deref().unwrap_or("").trim()),
    );
    btree.insert(
        "model".into(),
        json!(parts.model.as_deref().unwrap_or("").trim()),
    );
    btree
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dot_workflow(prefix: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tddy-actcache-{prefix}-{}", uuid::Uuid::now_v7()));
        let wf = workflow_engine_storage_dir(&dir);
        fs::create_dir_all(&wf).unwrap();
        dir
    }

    #[test]
    fn action_fingerprint_canonicalization() {
        let loosened = ActionFingerprintParts {
            goal_id: "acceptance-tests".to_string(),
            effective_prompt: "  SAME PROMPT BODY  ".to_string(),
            system_prompt: None,
            model: None,
        };
        let tight = ActionFingerprintParts {
            goal_id: "acceptance-tests".to_string(),
            effective_prompt: "SAME PROMPT BODY".to_string(),
            system_prompt: None,
            model: None,
        };
        let divergent_prompt = ActionFingerprintParts {
            goal_id: "acceptance-tests".to_string(),
            effective_prompt: "OTHER PROMPT".to_string(),
            system_prompt: None,
            model: None,
        };
        let fa = fingerprint_action_inputs(&loosened).expect(
            "fingerprint_action_inputs must be implemented for deterministic action cache keys",
        );
        let fb = fingerprint_action_inputs(&tight).expect(
            "fingerprint_action_inputs must be implemented for deterministic action cache keys",
        );
        assert_eq!(
            fa, fb,
            "logical prompt equivalence must produce identical fingerprints (whitespace trimming etc.)",
        );

        let fd = fingerprint_action_inputs(&divergent_prompt).expect(
            "fingerprint_action_inputs must be implemented for deterministic action cache keys",
        );
        assert_ne!(
            fa, fd,
            "different effective prompts must change the fingerprint",
        );
    }

    #[test]
    fn action_cache_opt_out_reflects_disable_action_cache_flag() {
        let ctx = Context::new();
        ctx.set_sync("disable_action_cache", true);
        assert!(
            action_cache_disabled(&ctx),
            "disable_action_cache on context disables read/write for that execution",
        );
    }

    #[test]
    fn action_cache_opt_out_reflects_tddy_disable_env() {
        unsafe { std::env::set_var("TDDY_DISABLE_ACTION_CACHE", "1") };
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { std::env::remove_var("TDDY_DISABLE_ACTION_CACHE") };
            }
        }
        let _g = Guard;
        let ctx = Context::new();
        assert!(
            action_cache_disabled(&ctx),
            "TDDY_DISABLE_ACTION_CACHE=1 disables action cache reads and writes",
        );
    }

    #[test]
    fn action_stable_identity_key_documents_graph_task_goal() {
        let graph = "action_cache_invoke_graph";
        let task = "accept_invoke";
        let goal = "acceptance-tests";
        let key = stable_action_cache_key(graph, task, goal);
        assert!(
            key.contains(graph),
            "`action_key` must include graph identity (got {:?})",
            key
        );
        assert!(
            key.contains(task),
            "`action_key` must include task id (got {:?})",
            key
        );
        assert!(
            key.contains(goal),
            "`action_key` must include goal id (got {:?})",
            key
        );
    }

    #[test]
    fn lookup_reads_matching_disk_placeholder_entry() {
        let dir = temp_dot_workflow("lookup");
        let path = action_cache_file_path(&dir);

        let expected_output = "{\"goal\":\"submit\",\"stub\":true}".to_string();
        let fingerprint = "fp-red-placeholder";
        let action_key_full = stable_action_cache_key("g-test", "t-test", "goal-test");

        let doc_on_disk = json!({
            "schema_version": ACTION_CACHE_SCHEMA_VERSION,
            "entries": {
                "primary": {
                    "action_key": action_key_full,
                    "fingerprint": fingerprint,
                    "output": serde_json::to_value(&expected_output).unwrap()
                }
            }
        });

        fs::write(&path, serde_json::to_string_pretty(&doc_on_disk).unwrap()).unwrap();

        let got = lookup_cached_completed_submit(&dir, &action_key_full, fingerprint);
        assert_eq!(
            got.as_deref(),
            Some(expected_output.as_str()),
            "`lookup_cached_completed_submit` returns stored submit JSON verbatim",
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_stub_inserts_requested_entry_under_storage_key() {
        let mut doc = ActionCacheDocument {
            schema_version: ACTION_CACHE_SCHEMA_VERSION,
            entries: std::collections::HashMap::from([
                ("alpha".to_string(), json!({"old": true})),
                ("beta".to_string(), json!({"keep": 1})),
            ]),
        };
        merge_action_cache_entry_stub(&mut doc, "gamma", "ak", "fp", "goal-z", r#"{"ok":true}"#);

        assert!(
            doc.entries.contains_key("gamma"),
            "GREEN merge must introduce `gamma` without dropping neighbouring keys ({:?})",
            doc.entries.keys().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn persist_writes_action_cache_adjacent_to_session_storage() {
        let dir = temp_dot_workflow("persist-write");
        let path = action_cache_file_path(&dir);

        persist_successful_submit_to_action_cache(
            &dir,
            "acceptance-tests",
            "ak-stable",
            "fp-stable",
            r#"{"goal":"submit"}"#,
        )
        .unwrap();

        assert!(
            path.exists(),
            "`persist_successful_submit_to_action_cache` must emit {:?}",
            path
        );

        let raw = fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"action_key\""),
            "persisted envelope must serialize action_key markers: {}",
            raw
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
