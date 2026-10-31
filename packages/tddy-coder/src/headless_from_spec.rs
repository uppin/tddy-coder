//! The `cursor` and `codex` workflow backends from their tool definitions: `cursor.yaml`'s and
//! `codex.yaml`'s `coding_agent.headless` sections render each invocation, and the adapters they
//! name read the output (`#agent-tools 9/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md`.

use std::path::Path;
use std::sync::Arc;

use tddy_actions::tool::ToolHost;
use tddy_core::{CodexBackend, CursorBackend};

/// A `cursor` backend rendered from its declaration: the agent registry loaded with `tools_dir`
/// (`<tddyhome>/tools`) applied, `cursor` and its deps resolved against `host`, and a
/// `SpecHeadlessRenderer` over the result installed with `with_headless_renderer`. Refused when the
/// registry does not load, `cursor` does not resolve, or its spec has no usable headless section.
pub fn cursor_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<CursorBackend> {
    // TODO(headless-others): implement — as `claude_from_spec::claude_backend_from_spec`, for
    // `cursor`; `create_backend("cursor")` then builds it, and green deletes
    // `resolve_cursor_agent_binary`'s role in it.
    let _ = (tools_dir, host);
    todo!("headless-others: cursor_backend_from_spec")
}

/// A `codex` backend rendered from its declaration, as [`cursor_backend_from_spec`] for `codex`.
/// `TDDY_CODEX_CLI` names the binary through the spec's `binary.env`; a provider route is applied
/// afterwards with `with_provider_route`, as today.
pub fn codex_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<CodexBackend> {
    // TODO(headless-others): implement — and green replaces `codex_binaries::resolve_codex_binary`
    // in `create_backend("codex")` with the spec's binary resolution.
    let _ = (tools_dir, host);
    todo!("headless-others: codex_backend_from_spec")
}
