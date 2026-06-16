//! Execute a build target: lower → cache check → wave-based parallel run.

use std::path::Path;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::{compute_cache_key, lookup_cache, persist_cache, CacheMode};
use crate::error::BuildError;
use crate::graph::{action_waves, BuildGraph};
use crate::proto::{build_target::Config, ActionCacheEntry, BuildAction, FileFingerprint};

/// Options controlling a build run.
#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    /// Bypass cache read and write.
    pub no_cache: bool,
    /// Emit the planned argv per action without executing anything.
    pub dry_run: bool,
    /// Cache read/write policy (ignored when `no_cache`).
    pub cache_mode: CacheMode,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            no_cache: false,
            dry_run: false,
            cache_mode: CacheMode::ReadWrite,
        }
    }
}

/// Result of running a single action.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActionOutcome {
    pub action_id: String,
    /// True when the action was served from cache (not executed).
    pub cached: bool,
    pub exit_code: i32,
    /// The resolved argv (also populated on `dry_run`).
    pub argv: Vec<String>,
    pub stdout: String,
    pub stderr: String,
}

/// Result of building a target (its own actions, after deps are built).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildRecord {
    pub target: String,
    pub actions: Vec<ActionOutcome>,
}

/// Build `target_id` (dependencies and group members first), running each
/// topological wave in parallel. Honors caching and `dry_run` per `opts`.
pub async fn execute_target(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    opts: &ExecuteOptions,
) -> Result<BuildRecord, BuildError> {
    let order = graph.build_order(target_id)?;
    let mut record = BuildRecord {
        target: target_id.to_string(),
        actions: Vec::new(),
    };

    for current_target in &order {
        let actions = graph.actions_for(current_target)?;
        if actions.is_empty() {
            continue;
        }
        let outcomes = run_target_actions(repo_root, graph, current_target, &actions, opts).await?;
        if current_target == target_id {
            record.actions = outcomes;
        }
    }

    Ok(record)
}

async fn run_target_actions(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    actions: &[BuildAction],
    opts: &ExecuteOptions,
) -> Result<Vec<ActionOutcome>, BuildError> {
    let waves = action_waves(actions)?;
    let mut outcomes: Vec<Option<ActionOutcome>> = vec![None; actions.len()];

    for wave in waves {
        let futures = wave.iter().map(|&idx| {
            let action = &actions[idx];
            run_one(repo_root, graph, target_id, action, opts)
        });
        let results = futures::future::join_all(futures).await;
        for (&idx, result) in wave.iter().zip(results.into_iter()) {
            outcomes[idx] = Some(result?);
        }
    }

    Ok(outcomes
        .into_iter()
        .map(|o| o.expect("every action ran"))
        .collect())
}

async fn run_one(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    action: &BuildAction,
    opts: &ExecuteOptions,
) -> Result<ActionOutcome, BuildError> {
    if opts.dry_run {
        return Ok(ActionOutcome {
            action_id: action.id.clone(),
            cached: false,
            exit_code: 0,
            argv: action.command.clone(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    let use_cache = !opts.no_cache;
    let fingerprints = fingerprint_inputs(repo_root, action);
    let cache_key = compute_cache_key(action, &fingerprints);

    if use_cache && lookup_cache(repo_root, target_id, &action.id, &cache_key).is_some() {
        return Ok(ActionOutcome {
            action_id: action.id.clone(),
            cached: true,
            exit_code: 0,
            argv: action.command.clone(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    let outcome = run_action(repo_root, graph, action).await?;

    if outcome.exit_code == 0 && use_cache && opts.cache_mode.writes() {
        let entry = ActionCacheEntry {
            schema_version: 1,
            cache_key,
            created_at_ms: now_ms(),
            input_fingerprints: fingerprints,
            output_paths: action.outputs.iter().map(|o| o.path.clone()).collect(),
            action_id: action.id.clone(),
            target_id: target_id.to_string(),
        };
        persist_cache(repo_root, target_id, &entry)?;
    }

    Ok(outcome)
}

async fn run_action(
    repo_root: &Path,
    graph: &BuildGraph,
    action: &BuildAction,
) -> Result<ActionOutcome, BuildError> {
    let argv = action.command.clone();
    if argv.is_empty() {
        return Err(BuildError::Exec(format!(
            "action {} has an empty command",
            action.id
        )));
    }

    let cwd = if action.working_dir.is_empty() {
        repo_root.to_path_buf()
    } else {
        repo_root.join(&action.working_dir)
    };

    let mut command = tokio::process::Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &action.env {
        command.env(key, value);
    }
    if let Some(path) = tool_path(repo_root, graph, action) {
        command.env("PATH", path);
    }

    let output = command
        .output()
        .await
        .map_err(|e| BuildError::Exec(format!("{}: {}", argv[0], e)))?;

    Ok(ActionOutcome {
        action_id: action.id.clone(),
        cached: false,
        exit_code: output.status.code().unwrap_or(-1),
        argv,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Build a `PATH` value with each tool dep's `bin_dir` prepended, or `None` when
/// the action has no tool deps.
fn tool_path(repo_root: &Path, graph: &BuildGraph, action: &BuildAction) -> Option<String> {
    if action.tool_dep_ids.is_empty() {
        return None;
    }
    let mut dirs: Vec<String> = Vec::new();
    for tool_id in &action.tool_dep_ids {
        if let Some(target) = graph.target(tool_id) {
            if let Some(Config::Tool(tool)) = &target.config {
                dirs.push(repo_root.join(&tool.bin_dir).to_string_lossy().into_owned());
            }
        }
    }
    if dirs.is_empty() {
        return None;
    }
    if let Ok(existing) = std::env::var("PATH") {
        dirs.push(existing);
    }
    Some(dirs.join(":"))
}

fn fingerprint_inputs(repo_root: &Path, action: &BuildAction) -> Vec<FileFingerprint> {
    let mut fingerprints: Vec<FileFingerprint> = Vec::new();
    for file_set in &action.inputs {
        let base = if file_set.root.is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(&file_set.root)
        };
        for include in &file_set.include {
            let pattern = base.join(include);
            let Ok(paths) = glob::glob(&pattern.to_string_lossy()) else {
                continue;
            };
            for path in paths.flatten() {
                let Ok(metadata) = std::fs::metadata(&path) else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let relative = path
                    .strip_prefix(repo_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                let mtime_ms = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                fingerprints.push(FileFingerprint {
                    path: relative,
                    size: metadata.len(),
                    mtime_ms,
                });
            }
        }
    }
    fingerprints.sort_by(|a, b| a.path.cmp(&b.path));
    fingerprints.dedup_by(|a, b| a.path == b.path);
    fingerprints
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
