//! Execute a build target: lower → cache check → wave-based parallel run.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::Arc;

use tddy_actions::tool::{ToolHost, ToolRegistry};
use tddy_actions::ProcessRuntime;
use tddy_task::{TaskHandle, TaskRegistry, TaskStatus};

use crate::action_convert::build_action_to_spec;
use crate::action_sandbox::ActionSandbox;
use crate::builtin::{self, TOOL};
use crate::cache::{compute_cache_key, lookup_cache, persist_cache, CacheMode};
use crate::capabilities::BuildMode;
use crate::error::BuildError;
use crate::graph::{action_waves, BuildGraph};
use crate::plugin::PluginRegistry;
use crate::proto::{ActionCacheEntry, BuildAction, FileFingerprint};

/// Options controlling a build run.
#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    /// Bypass cache read and write.
    pub no_cache: bool,
    /// Emit the planned argv per action without executing anything.
    pub dry_run: bool,
    /// Cache read/write policy (ignored when `no_cache`).
    pub cache_mode: CacheMode,
    /// Run every action hermetically, as if each declared `sandbox: true` (`tddy build
    /// --hermetic`). Needs a [`BuildTools`] with an [`ActionSandbox`]; without one the run is
    /// refused, never silently unconfined.
    pub hermetic: bool,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            no_cache: false,
            dry_run: false,
            cache_mode: CacheMode::ReadWrite,
            hermetic: false,
        }
    }
}

/// What a build resolves `tool_dep_ids` against, and how it runs hermetic actions
/// (`#agent-tools 4/12`).
pub struct BuildTools {
    /// The built-ins plus `<tddyhome>/tools` overrides; a run adds its inline tool targets.
    pub registry: ToolRegistry,
    /// The host tools resolve against. The executor wraps it so a [`TargetBinary`] answers with
    /// the producing target's output under the repository root.
    ///
    /// [`TargetBinary`]: tddy_actions::tool::TargetBinary
    pub host: Arc<dyn ToolHost>,
    /// Runs `sandbox: true` actions; `None` refuses them.
    pub sandbox: Option<Arc<dyn ActionSandbox>>,
}

impl BuildTools {
    /// The built-in tools with `tools_dir` (`<tddyhome>/tools`) applied as overrides, resolving
    /// against `host`, with no sandbox.
    pub fn load(tools_dir: &Path, host: Arc<dyn ToolHost>) -> Result<Self, BuildError> {
        // TODO(build-tools): implement — `ToolRegistry::load(ToolRegistry::builtin()?, tools_dir)`,
        // a `ToolError` becoming a `BuildError` that keeps its message.
        let _ = (tools_dir, host);
        todo!("build-tools: BuildTools::load")
    }

    /// The same tools, running hermetic actions through `sandbox`.
    pub fn with_sandbox(self, sandbox: Arc<dyn ActionSandbox>) -> Self {
        Self {
            sandbox: Some(sandbox),
            ..self
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
///
/// `mode` applies to `target_id` itself; its dependencies are always compiled.
pub async fn execute_target(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    opts: &ExecuteOptions,
    mode: BuildMode,
    registry: &PluginRegistry,
) -> Result<BuildRecord, BuildError> {
    let order = graph.build_order(target_id)?;
    let mut record = BuildRecord {
        target: target_id.to_string(),
        actions: Vec::new(),
    };

    for current_target in &order {
        let current_mode = if current_target == target_id {
            mode
        } else {
            BuildMode::Compile
        };
        let actions = graph.actions_for_mode(current_target, current_mode, registry)?;
        if actions.is_empty() {
            continue;
        }
        log::info!("building target {}", current_target);
        let outcomes = run_target_actions(repo_root, graph, current_target, &actions, opts).await?;
        if current_target == target_id {
            record.actions = outcomes;
        }
    }

    Ok(record)
}

/// [`execute_target`] with `tool_dep_ids` resolved through `tools` (`#agent-tools 4/12`).
///
/// - A tool target's binary that is another target's output makes that target build first
///   ([`BuildGraph::build_order`]).
/// - An action's `PATH` gets each resolved tool's link directory, ahead of the inherited `PATH`.
/// - An action with `sandbox: true`, or every action when `opts.hermetic`, runs through
///   `tools.sandbox`.
/// - The cache key covers the resolved tool binaries
///   ([`crate::cache::compute_cache_key_with_tools`]).
pub async fn execute_target_with_tools(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    opts: &ExecuteOptions,
    mode: BuildMode,
    registry: &PluginRegistry,
    tools: &BuildTools,
) -> Result<BuildRecord, BuildError> {
    // TODO(build-tools): implement, then make `execute_target` this with the process host and
    // `<tddyhome>/tools`, delete `tool_path` and the legacy `tool_bin_dir`.
    let _ = (repo_root, graph, target_id, opts, mode, registry, tools);
    todo!("build-tools: execute_target_with_tools")
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
        log::debug!("cache hit for action {} (target {})", action.id, target_id);
        return Ok(ActionOutcome {
            action_id: action.id.clone(),
            cached: true,
            exit_code: 0,
            argv: action.command.clone(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    log::debug!("cache miss for action {} (target {})", action.id, target_id);
    log::debug!("running action {}: {:?}", action.id, action.command);

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
        log::debug!(
            "persisted cache entry for action {} (target {})",
            action.id,
            target_id
        );
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

    // Ensure declared output parent directories exist. Many tools (e.g. compilers,
    // `docker build --iidfile`) do not create the directory for a declared output.
    for output in &action.outputs {
        if let Some(parent) = repo_root.join(&output.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let mut spec = build_action_to_spec(repo_root, action);
    if spec.working_dir.is_none() {
        spec.working_dir = Some(repo_root.to_path_buf());
    }
    if let Some(path) = tool_path(repo_root, graph, action)? {
        spec.env.insert("PATH".to_string(), path);
    }

    let registry = TaskRegistry::new();
    let handle = ProcessRuntime::spawn(&registry, spec, "tddy-build")
        .await
        .map_err(|e| BuildError::Exec(e.to_string()))?;
    wait_task_terminal(&handle).await;

    let exit_code = match handle.status() {
        TaskStatus::Completed { exit_code } => exit_code.unwrap_or(-1),
        TaskStatus::Cancelled => -1,
        TaskStatus::Failed { .. } => -1,
        _ => -1,
    };
    let stdout = handle
        .channel("stdout")
        .map(|ch| String::from_utf8_lossy(&ch.replay_capture()).into_owned())
        .unwrap_or_default();
    let stderr = handle
        .channel("stderr")
        .map(|ch| String::from_utf8_lossy(&ch.replay_capture()).into_owned())
        .unwrap_or_default();

    Ok(ActionOutcome {
        action_id: action.id.clone(),
        cached: false,
        exit_code,
        argv,
        stdout,
        stderr,
    })
}

async fn wait_task_terminal(handle: &TaskHandle) {
    let mut rx = handle.status_watch();
    loop {
        if rx.borrow().is_terminal() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Build a `PATH` value with each tool dep's `bin_dir` prepended, or `None` when
/// the action has no tool deps.
// TODO(build-tools): delete — `execute_target_with_tools` puts the resolved tool closure on PATH.
fn tool_path(
    repo_root: &Path,
    graph: &BuildGraph,
    action: &BuildAction,
) -> Result<Option<String>, BuildError> {
    if action.tool_dep_ids.is_empty() {
        return Ok(None);
    }
    let mut dirs: Vec<String> = Vec::new();
    for tool_id in &action.tool_dep_ids {
        if let Some(target) = graph.target(tool_id) {
            if let Some(config) = &target.config {
                if config.r#type == TOOL {
                    let bin_dir = builtin::tool_bin_dir(&config.fields)?;
                    dirs.push(repo_root.join(&bin_dir).to_string_lossy().into_owned());
                }
            }
        }
    }
    if dirs.is_empty() {
        return Ok(None);
    }
    if let Ok(existing) = std::env::var("PATH") {
        dirs.push(existing);
    }
    Ok(Some(dirs.join(":")))
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
