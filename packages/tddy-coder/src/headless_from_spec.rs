//! The `cursor` and `codex` workflow backends from their tool definitions: `cursor.yaml`'s and
//! `codex.yaml`'s `coding_agent.headless` sections render each invocation, and the adapters they
//! name read the output (`#agent-tools 9/12`).
//!
//! PRD: `docs/ft/coder/changelog/2026-11-03-headless-cursor-codex-oz-from-specs.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use tddy_actions::tool::{ResolvedTool, ToolHost};
use tddy_coding_agent_specs::headless::SpecHeadlessRenderer;
use tddy_coding_agent_specs::{load_agent_registry, routed_agent, CodingAgentSpec};
use tddy_core::adapters::AdapterRegistry;
use tddy_core::{CodexBackend, CursorBackend, SpecHeadlessBackend};

/// `cursor.yaml`'s `binary.env`: the variable that names the Cursor `agent` binary.
pub const CURSOR_BINARY_ENV: &str = "TDDY_CURSOR_AGENT";
/// `codex.yaml`'s `binary.env`: the variable that names the Codex CLI.
pub const CODEX_BINARY_ENV: &str = "TDDY_CODEX_CLI";

/// A [`ToolHost`] on which `binary_env` names `binary` (an explicit `--cursor-agent-path` /
/// `--codex-cli-path`), so the spec's own binary resolution honours the flag; everything else is
/// `inner`'s. With no `binary`, `inner` itself.
pub fn host_naming_binary(
    inner: Arc<dyn ToolHost>,
    binary_env: &'static str,
    binary: Option<&Path>,
) -> Arc<dyn ToolHost> {
    match binary {
        Some(binary) => Arc::new(HostNamingBinary {
            inner,
            binary_env,
            binary: binary.to_path_buf(),
        }),
        None => inner,
    }
}

struct HostNamingBinary {
    inner: Arc<dyn ToolHost>,
    binary_env: &'static str,
    binary: PathBuf,
}

impl ToolHost for HostNamingBinary {
    fn env_var(&self, name: &str) -> Option<String> {
        if name == self.binary_env {
            return Some(self.binary.to_string_lossy().into_owned());
        }
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

    fn target_output(&self, binary: &tddy_actions::tool::TargetBinary) -> Option<PathBuf> {
        self.inner.target_output(binary)
    }
}

/// A [`SpecHeadlessRenderer`] over the declared coding agent `agent`: the registry loaded with
/// `tools_dir` applied, `agent` and its deps resolved against `host`.
fn renderer_for(
    agent: &str,
    tools_dir: &Path,
    host: &dyn ToolHost,
) -> anyhow::Result<SpecHeadlessRenderer> {
    renderer_and_spec_for(agent, tools_dir, host).map(|(_, renderer)| renderer)
}

/// [`renderer_for`], with the agent's spec the renderer was built from.
fn renderer_and_spec_for(
    agent: &str,
    tools_dir: &Path,
    host: &dyn ToolHost,
) -> anyhow::Result<(CodingAgentSpec, SpecHeadlessRenderer)> {
    let registry = load_agent_registry(tools_dir).context("the coding agent registry")?;
    let def = registry
        .get(agent)
        .ok_or_else(|| anyhow!("no `{agent}` tool is declared"))?;
    let spec = CodingAgentSpec::of(def)?;
    // The closure lists deps before their dependents, so the agent's own tool comes last.
    let mut closure = registry.resolve_closure(&[agent], host)?;
    let tool: ResolvedTool = closure
        .pop()
        .ok_or_else(|| anyhow!("`{agent}` resolved to no tool"))?;
    let renderer =
        SpecHeadlessRenderer::new(spec.clone(), tool, &closure, AdapterRegistry::builtin())?;
    Ok((spec, renderer))
}

/// A `cursor` backend rendered from its declaration: the agent registry loaded with `tools_dir`
/// (`<tddyhome>/tools`) applied, `cursor` and its deps resolved against `host`, and a
/// `SpecHeadlessRenderer` over the result installed with `with_headless_renderer`. Refused when the
/// registry does not load, `cursor` does not resolve, or its spec has no usable headless section.
pub fn cursor_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<CursorBackend> {
    let renderer = renderer_for("cursor", tools_dir, host.as_ref())?;
    Ok(CursorBackend::new().with_headless_renderer(Arc::new(renderer)))
}

/// A `codex` backend rendered from its declaration, as [`cursor_backend_from_spec`] for `codex`.
/// `TDDY_CODEX_CLI` names the binary through the spec's `binary.env`; a provider route is applied
/// afterwards with `with_provider_route`, as today.
pub fn codex_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<CodexBackend> {
    let renderer = renderer_for("codex", tools_dir, host.as_ref())?;
    Ok(CodexBackend::new().with_headless_renderer(Arc::new(renderer)))
}

/// The workflow backend of the declared headless coding agent `agent` (`#agent-tools 15/19`) —
/// `claude`, `cursor`, `codex`, `oz`, or any agent a `<tddyhome>/tools` YAML declares with a
/// `coding_agent.headless` section naming a registered adapter: the registry loaded with
/// `tools_dir` applied, `agent` and its deps resolved against `host`, and a [`SpecHeadlessBackend`]
/// over its `SpecHeadlessRenderer`, the section's protocol, its `routed_agent` and its model
/// listing. Refused when the registry does not load, `agent` does not resolve, or its spec has no
/// usable headless section.
///
/// Replaces [`cursor_backend_from_spec`], [`codex_backend_from_spec`] and
/// `claude_from_spec::claude_backend_from_spec`, which are deleted with the bespoke backends.
pub fn headless_backend_from_spec(
    agent: &str,
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<SpecHeadlessBackend> {
    let (spec, renderer) = renderer_and_spec_for(agent, tools_dir, host.as_ref())?;
    let protocol = renderer.protocol();
    let listing = renderer.model_listing()?;
    let backend = SpecHeadlessBackend::new(Arc::new(renderer), protocol, routed_agent(&spec));
    Ok(match listing {
        Some(listing) => backend.with_model_listing(listing),
        None => backend,
    })
}
