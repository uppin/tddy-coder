//! Execute a build target: lower → cache check → wave-based parallel run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tddy_actions::tool::{
    ProcessToolHost, ResolvedTool, TargetBinary, ToolError, ToolHost, ToolRegistry,
};
use tddy_actions::{ActionInput, ProcessRuntime};
use tddy_task::{TaskHandle, TaskRegistry, TaskStatus};

use crate::action_convert::build_action_to_spec;
use crate::action_sandbox::ActionSandbox;
use crate::builtin::{self, ToolTarget, TOOL};
use crate::cache::{compute_run_cache_key, lookup_cache, persist_cache, sanitize, CacheMode};
use crate::capabilities::BuildMode;
use crate::error::BuildError;
use crate::graph::{action_waves, BuildGraph};
use crate::plugin::PluginRegistry;
use crate::proto::{ActionCacheEntry, BuildAction, FileFingerprint};

/// Where an action's tool links live: one directory per action, under the repository's build state.
const TOOL_LINK_ROOT: &str = ".tddy-build/tools";

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
        let builtin = ToolRegistry::builtin().map_err(tool_error)?;
        let registry = ToolRegistry::load(builtin, tools_dir).map_err(tool_error)?;
        Ok(Self {
            registry,
            host,
            sandbox: None,
        })
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
///
/// Tool dependencies resolve against the built-in tools on this process, and no action runs in a
/// sandbox: [`execute_target_with_tools`] takes the `<tddyhome>/tools` overrides and a sandbox.
pub async fn execute_target(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    opts: &ExecuteOptions,
    mode: BuildMode,
    registry: &PluginRegistry,
) -> Result<BuildRecord, BuildError> {
    let tools = BuildTools {
        registry: ToolRegistry::builtin().map_err(tool_error)?,
        host: Arc::new(ProcessToolHost),
        sandbox: None,
    };
    execute_target_with_tools(repo_root, graph, target_id, opts, mode, registry, &tools).await
}

/// [`execute_target`] with `tool_dep_ids` resolved through `tools` (`#agent-tools 4/12`).
///
/// - A tool target's binary that is another target's output makes that target build first
///   ([`BuildGraph::build_order`]).
/// - An action's `PATH` gets each resolved tool's link directory, ahead of the inherited `PATH`.
/// - An action with `sandbox: true`, or every action when `opts.hermetic`, runs through
///   `tools.sandbox`.
/// - The cache key covers the resolved tool binaries and whether the action runs hermetically
///   ([`crate::cache::compute_run_cache_key`]).
pub async fn execute_target_with_tools(
    repo_root: &Path,
    graph: &BuildGraph,
    target_id: &str,
    opts: &ExecuteOptions,
    mode: BuildMode,
    registry: &PluginRegistry,
    tools: &BuildTools,
) -> Result<BuildRecord, BuildError> {
    let order = graph.build_order(target_id)?;
    let mut plans: Vec<(String, Vec<BuildAction>)> = Vec::new();
    for current_target in order {
        let current_mode = if current_target == target_id {
            mode
        } else {
            BuildMode::Compile
        };
        let actions = graph.actions_for_mode(&current_target, current_mode, registry)?;
        if !actions.is_empty() {
            plans.push((current_target, actions));
        }
    }

    // Every tool the build names is read before the first action runs: a tool that does not parse
    // stops the build here, not partway through it.
    let run = ToolRun::new(
        repo_root,
        graph,
        tools,
        plans.iter().flat_map(|(_, actions)| actions),
    )?;
    let mut record = BuildRecord {
        target: target_id.to_string(),
        actions: Vec::new(),
    };
    for (current_target, actions) in &plans {
        log::info!("building target {}", current_target);
        let outcomes = run_target_actions(&run, current_target, actions, opts).await?;
        if current_target == target_id {
            record.actions = outcomes;
        }
    }

    Ok(record)
}

/// The tools one build run resolves against: the tools' registry with the run's inline tool targets
/// added, the tool id each named tool target stands for, and the host the binaries resolve on.
struct ToolRun<'a> {
    repo_root: &'a Path,
    registry: ToolRegistry,
    /// Tool target id → the tool id a [`ToolRegistry::resolve_closure`] takes.
    tool_ids: BTreeMap<String, String>,
    host: TargetHost<'a>,
    sandbox: Option<&'a dyn ActionSandbox>,
}

impl<'a> ToolRun<'a> {
    /// Read the tool targets `actions` name. An inline definition joins the registry; a `ref` must name
    /// a tool it holds. A name that is not a tool target, or a definition that does not parse, refuses
    /// the build.
    fn new<'b>(
        repo_root: &'a Path,
        graph: &BuildGraph,
        tools: &'a BuildTools,
        actions: impl Iterator<Item = &'b BuildAction>,
    ) -> Result<Self, BuildError> {
        let names: BTreeSet<&str> = actions
            .flat_map(|action| action.tool_dep_ids.iter().map(String::as_str))
            .collect();
        let mut registry = tools.registry.clone();
        let mut tool_ids = BTreeMap::new();
        for name in names {
            let tool_id = match tool_config(graph, name)? {
                ToolTarget::Inline(def) => {
                    let id = def.id.clone();
                    registry = registry
                        .with_tool(*def, PathBuf::from(name))
                        .map_err(tool_error)?;
                    id
                }
                ToolTarget::Ref { ref_ } => {
                    if registry.get(&ref_).is_none() {
                        return Err(tool_error(ToolError::UnknownTool(ref_)));
                    }
                    ref_
                }
            };
            tool_ids.insert(name.to_string(), tool_id);
        }
        Ok(Self {
            repo_root,
            registry,
            tool_ids,
            host: TargetHost {
                inner: tools.host.as_ref(),
                repo_root,
            },
            sandbox: tools.sandbox.as_deref(),
        })
    }

    /// The closure of `action`'s tool dependencies, deps before dependents; empty when it names none.
    fn closure(&self, action: &BuildAction) -> Result<Vec<ResolvedTool>, BuildError> {
        if action.tool_dep_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = action
            .tool_dep_ids
            .iter()
            .map(|name| {
                self.tool_ids
                    .get(name)
                    .map(String::as_str)
                    .ok_or_else(|| BuildError::UnknownTarget(name.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.registry
            .resolve_closure(&ids, &self.host)
            .map_err(|e| BuildError::Exec(e.to_string()))
    }
}

/// The parsed config of the tool target `name`, which must be a tool target.
fn tool_config(graph: &BuildGraph, name: &str) -> Result<ToolTarget, BuildError> {
    let target = graph
        .target(name)
        .ok_or_else(|| BuildError::UnknownTarget(name.to_string()))?;
    match &target.config {
        Some(config) if config.r#type == TOOL => builtin::tool_target(name, &config.fields),
        _ => Err(BuildError::Manifest(format!(
            "tool_dep_ids names {name}, which is not a tool target"
        ))),
    }
}

/// The host a build resolves tools on: the tools' host, except that a [`TargetBinary`] answers with
/// the producing target's output under the repository root. The build order has already built that
/// producer before any action that resolves it.
struct TargetHost<'a> {
    inner: &'a dyn ToolHost,
    repo_root: &'a Path,
}

impl ToolHost for TargetHost<'_> {
    fn env_var(&self, name: &str) -> Option<String> {
        self.inner.env_var(name)
    }
    fn path_dirs(&self) -> Vec<PathBuf> {
        self.inner.path_dirs()
    }
    fn is_executable_file(&self, path: &Path) -> bool {
        self.inner.is_executable_file(path)
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }
    fn target_output(&self, binary: &TargetBinary) -> Option<PathBuf> {
        Some(self.repo_root.join(&binary.path))
    }
}

fn tool_error(error: ToolError) -> BuildError {
    BuildError::Manifest(error.to_string())
}

/// Run `action` through the build's sandbox. Without one the action is refused, never run unconfined.
async fn run_hermetic(
    run: &ToolRun<'_>,
    action: &BuildAction,
    closure: &[ResolvedTool],
) -> Result<ActionOutcome, BuildError> {
    let sandbox = run.sandbox.ok_or_else(|| {
        BuildError::Exec(format!(
            "action {} runs hermetically, but this build has no sandbox",
            action.id
        ))
    })?;
    let mut spec = build_action_to_spec(run.repo_root, action);
    if spec.working_dir.is_none() {
        spec.working_dir = Some(run.repo_root.to_path_buf());
    }
    spec.inputs = matched_inputs(&spec.inputs)?;
    sandbox.run(&spec, closure).await
}

/// The concrete paths each declared input pattern matches, as read-only inputs: a sandbox mounts
/// paths, not globs.
fn matched_inputs(inputs: &[ActionInput]) -> Result<Vec<ActionInput>, BuildError> {
    let mut matched = Vec::new();
    for input in inputs {
        let pattern = input.host_path.to_string_lossy();
        let paths = glob::glob(&pattern)
            .map_err(|e| BuildError::Manifest(format!("input {pattern}: {e}")))?;
        for path in paths {
            let host_path = path.map_err(|e| BuildError::Io(e.to_string()))?;
            matched.push(ActionInput {
                host_path,
                jail_path: None,
                writable: false,
            });
        }
    }
    Ok(matched)
}

/// Expose each tool of `closure` by its command in `links`: one symlink per command, to its binary.
/// A link an earlier run left is replaced, so a re-resolved binary is never reached by a stale name.
fn link_tools(links: &Path, closure: &[ResolvedTool]) -> Result<(), BuildError> {
    std::fs::create_dir_all(links).map_err(|e| io_error(links, e))?;
    for tool in closure {
        let link = links.join(&tool.command);
        // A link that is absent is the normal case; anything else is reported by `symlink_file`.
        let _ = std::fs::remove_file(&link);
        symlink_file(&tool.binary, &link).map_err(|e| io_error(&link, e))?;
    }
    Ok(())
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink_file(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "tool links need a unix host",
    ))
}

/// Where `action`'s tool links live: one directory per action, under the repository's build state.
fn tool_link_dir(repo_root: &Path, target_id: &str, action_id: &str) -> PathBuf {
    repo_root
        .join(TOOL_LINK_ROOT)
        .join(sanitize(target_id))
        .join(sanitize(action_id))
        .join("bin")
}

/// `PATH` with `links` first, ahead of the inherited `PATH`.
fn path_with(links: &Path) -> String {
    match std::env::var("PATH") {
        Ok(existing) => format!("{}:{existing}", links.display()),
        Err(_) => links.display().to_string(),
    }
}

/// The fingerprint of one tool binary, as an input's is: its path, size and mtime.
fn file_fingerprint(path: &Path) -> Result<FileFingerprint, BuildError> {
    let metadata = std::fs::metadata(path).map_err(|e| io_error(path, e))?;
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok(FileFingerprint {
        path: path.to_string_lossy().into_owned(),
        size: metadata.len(),
        mtime_ms,
    })
}

fn io_error(path: &Path, error: std::io::Error) -> BuildError {
    BuildError::Io(format!("{}: {error}", path.display()))
}

/// Create the parent directory of each declared output. Many tools (e.g. compilers, `docker build
/// --iidfile`) do not create the directory for a declared output.
fn prepare_outputs(repo_root: &Path, action: &BuildAction) {
    for output in &action.outputs {
        if let Some(parent) = repo_root.join(&output.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
}

async fn run_target_actions(
    run: &ToolRun<'_>,
    target_id: &str,
    actions: &[BuildAction],
    opts: &ExecuteOptions,
) -> Result<Vec<ActionOutcome>, BuildError> {
    let waves = action_waves(actions)?;
    let mut outcomes: Vec<Option<ActionOutcome>> = vec![None; actions.len()];

    for wave in waves {
        let futures = wave.iter().map(|&idx| {
            let action = &actions[idx];
            run_one(run, target_id, action, opts)
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
    run: &ToolRun<'_>,
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
    let fingerprints = fingerprint_inputs(run.repo_root, action);
    let closure = run.closure(action)?;
    let tool_fingerprints = closure
        .iter()
        .map(|tool| file_fingerprint(&tool.binary))
        .collect::<Result<Vec<_>, _>>()?;
    let hermetic = opts.hermetic || action.sandbox;
    let cache_key = compute_run_cache_key(action, &fingerprints, &tool_fingerprints, hermetic);

    if use_cache && lookup_cache(run.repo_root, target_id, &action.id, &cache_key).is_some() {
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

    prepare_outputs(run.repo_root, action);
    let outcome = if hermetic {
        run_hermetic(run, action, &closure).await?
    } else {
        run_action(run.repo_root, target_id, action, &closure).await?
    };

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
        persist_cache(run.repo_root, target_id, &entry)?;
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
    target_id: &str,
    action: &BuildAction,
    closure: &[ResolvedTool],
) -> Result<ActionOutcome, BuildError> {
    let argv = action.command.clone();
    if argv.is_empty() {
        return Err(BuildError::Exec(format!(
            "action {} has an empty command",
            action.id
        )));
    }

    let mut spec = build_action_to_spec(repo_root, action);
    if spec.working_dir.is_none() {
        spec.working_dir = Some(repo_root.to_path_buf());
    }
    if !closure.is_empty() {
        let links = tool_link_dir(repo_root, target_id, &action.id);
        link_tools(&links, closure)?;
        spec.env.insert("PATH".to_string(), path_with(&links));
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
