use crate::action_convert::build_action_to_spec;
use crate::action_sandbox::ActionSandbox;
use crate::builtin::{self, ToolTarget, TOOL};
use crate::error::BuildError;
use crate::executor::ActionOutcome;
use crate::executor::BuildTools;
use crate::graph::BuildGraph;
use crate::proto::BuildAction;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tddy_actions::tool::{ResolvedTool, TargetBinary, ToolError, ToolHost, ToolRegistry};
use tddy_actions::ActionInput;

/// The tools one build run resolves against: the tools' registry with the run's inline tool targets
/// added, the tool id each named tool target stands for, and the host the binaries resolve on.
pub(super) struct ToolRun<'a> {
    pub(super) repo_root: &'a Path,
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
    pub(super) fn new<'b>(
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
    pub(super) fn closure(&self, action: &BuildAction) -> Result<Vec<ResolvedTool>, BuildError> {
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

pub(super) fn tool_error(error: ToolError) -> BuildError {
    BuildError::Manifest(error.to_string())
}

/// Run `action` through the build's sandbox. Without one the action is refused, never run unconfined.
pub(super) async fn run_hermetic(
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
