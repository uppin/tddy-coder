//! High-level JSON entry points shared by `tddy-tools` (local CLI) and the
//! `tddy-coder` relay executor. Keeping the JSON shapes here ensures the local
//! and relayed paths return identical output.

use std::path::Path;

use serde_json::{json, Value};

use crate::capabilities::BuildMode;
use crate::discovery::discover_build_manifests;
use crate::error::BuildError;
use crate::executor::{execute_target, ExecuteOptions};
use crate::graph::BuildGraph;
use crate::manifest::BuildTarget;
use crate::plugin::PluginRegistry;

/// Filters for [`build_list_json`].
#[derive(Debug, Clone, Default)]
pub struct BuildListQuery {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: usize,
}

/// Discover and list build targets across the repo as
/// `{"targets":[…],"total":N,"offset":X,"limit":Y}`.
pub fn build_list_json(repo_root: &Path, query: &BuildListQuery) -> Result<Value, BuildError> {
    let manifests = discover_build_manifests(repo_root)?;
    let mut summaries: Vec<Value> = manifests
        .iter()
        .flat_map(|(_, manifest)| manifest.targets.iter())
        .map(target_summary)
        .collect();
    summaries.sort_by(|a, b| target_id(a).cmp(target_id(b)));

    if let Some(needle) = &query.query {
        let needle = needle.to_lowercase();
        summaries.retain(|s| {
            ["id", "name", "type"].iter().any(|field| {
                s.get(*field)
                    .and_then(Value::as_str)
                    .map(|v| v.to_lowercase().contains(&needle))
                    .unwrap_or(false)
            })
        });
    }

    let total = summaries.len();
    let targets: Vec<Value> = summaries
        .into_iter()
        .skip(query.offset)
        .take(query.limit.unwrap_or(usize::MAX))
        .collect();

    Ok(json!({
        "targets": targets,
        "total": total,
        "offset": query.offset,
        "limit": query.limit,
    }))
}

/// Build `target` in `mode` and return the build record as JSON (`status`, `target`, `actions`).
pub async fn build_json(
    repo_root: &Path,
    target: &str,
    no_cache: bool,
    dry_run: bool,
    mode: BuildMode,
    registry: &PluginRegistry,
) -> Result<Value, BuildError> {
    let manifests = discover_build_manifests(repo_root)?
        .into_iter()
        .map(|(_, manifest)| manifest)
        .collect();
    let graph = BuildGraph::from_manifests(manifests)?;
    let opts = ExecuteOptions {
        no_cache,
        dry_run,
        ..ExecuteOptions::default()
    };
    let record = execute_target(repo_root, &graph, target, &opts, mode, registry).await?;

    let mut value =
        serde_json::to_value(&record).map_err(|e| BuildError::Manifest(e.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("status".to_string(), Value::String("ok".to_string()));
    }
    Ok(value)
}

fn target_summary(target: &BuildTarget) -> Value {
    json!({
        "id": target.id,
        "name": target.name,
        "type": target_type_name(target),
        "deps": target.deps,
    })
}

fn target_id(summary: &Value) -> &str {
    summary.get("id").and_then(Value::as_str).unwrap_or("")
}

fn target_type_name(target: &BuildTarget) -> &str {
    target
        .config
        .as_ref()
        .map(|c| c.r#type.as_str())
        .unwrap_or("actions")
}
