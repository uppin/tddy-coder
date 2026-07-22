# Reusable Language Server (LSP) Support

**Product Area**: Coder
**Status**: Draft
**Updated**: 2026-07-21

## Summary

Agents driven by tddy-coder have no real language intelligence. `ReadLints` is a
hard-coded stub, and there is no go-to-definition, find-references, hover, or symbol
search. This feature introduces a **reusable language server** capability: a real
language server (rust-analyzer first, any LSP thereafter) runs as a **long-running
`tddy-task`**, is **reused across build targets**, and is surfaced to agents through a
**single, language-agnostic ad-hoc MCP interface**.

The language server for a target is chosen from the **target type** (`config.type` in
`BUILD.yaml`), gated by an **allow-list** that currently enables only Rust but is
designed to harbour any LSP. The server is owned primarily by **`tddy-daemon`** (so it
is shared across sessions and targets); standalone **`tddy-coder`** is the in-process
fallback owner. Servers are keyed by **(workspace root, language)** — two targets in
the same workspace share one server. Idle servers are torn down after a timeout.

## Background

The extension-point pattern already used for builds is the template: `tddy-core`
defines a `BuildExecutor` trait and a process-global `OnceLock` registry, and
`tddy-coder` registers a concrete executor on top of `tddy-build`. This keeps
`tddy-core` free of recipe-specific dependencies. Language servers follow the same
shape via a new `LspExecutor` trait.

The long-running task abstraction (`tddy-task`) already supports a task body that never
returns until cancelled, with multi-subscriber output streaming and stdin — exactly
what an interactive language-server process needs. What it lacks is a
lookup-or-spawn-by-stable-key layer (it keys by generated UUID and evicts terminal
tasks); this feature adds that as a per-`(root, language)` registry.

The MCP surface already exists as a single in-repo server (`tddy-tools --mcp`), whose
tool set is assembled dynamically at startup behind env-var gates. LSP tools plug into
this seam: they appear only when a language server is available.

## Requirements

### Language selection & allow-list

1. The language server for a target is chosen from the target's `config.type`
   (`rust_binary` / `rust_library` → Rust). Mapping is a pure function, independent of
   any policy.
2. An **allow-list** gates which languages may spawn a server. The default allow-list
   enables **Rust only** (rust-analyzer), but the architecture accepts any language +
   launch command with no code changes to the core mechanics.
3. Requesting a server for a disallowed language returns an explicit, agent-visible
   error and spawns no process.

### Reusable, long-running servers

4. A language server runs as a **long-running `tddy-task`**: its task body stays
   `Running` until cancelled; output is streamed incrementally (not drained at EOF).
5. Servers are keyed by **(workspace root, language)**. Two requests with the same key
   return the **same** running server (one task). Different workspace roots get
   separate servers.
6. Servers are lazily started on first use (get-or-spawn) and **torn down after an idle
   timeout**. Activity resets the idle timer.
7. If a server task has terminated (e.g. crash), the next request **re-spawns** it
   rather than returning a dead handle.
8. Workspace-root detection resolves to the nearest ancestor workspace root (for Rust,
   the `Cargo.toml` workspace root), so targets in one workspace actually share.

### Target binding

9. Binding a target attaches its **package + srcs** to the server: each src file is
   opened as an LSP document (`textDocument/didOpen`) so the server indexes it.
   "References" means LSP find-references over the indexed documents.

### Single language-agnostic MCP interface

10. Once **≥1** language server is available for the repo, agents receive an **ad-hoc
    MCP tool set** exposing: **Diagnostics, Definition, References, Hover, Symbols**.
11. The interface is **one set of tools for all languages** — tool names carry no
    language prefix (`LspDiagnostics`, `LspDefinition`, `LspReferences`, `LspHover`,
    `LspSymbols`). The same code path serves every allowed language.
12. The tools are **absent** when no language server is available (gated by a
    per-session env flag set by the owner), and **present** when one is.
13. Tool calls dispatch over the existing session-tool transport to the owner
    (daemon-primary, coder-fallback); no new transport is introduced.

### Ownership

14. **`tddy-daemon`** is the primary owner: it holds the registry beside the shared
    `TaskRegistry`, runs the idle-reaper loop, and serves LSP tool calls. Servers appear
    as ordinary long-running tasks in the existing task RPCs.
15. Standalone **`tddy-coder`** registers a concrete `LspExecutor` as the in-process
    fallback when no daemon transport is configured.

### First consumer

16. `ReadLints` is upgraded to route to the LSP `diagnostics` path when a language
    server is available for the target, falling back to the existing stub otherwise.

## Testing Plan

**Test levels:** Unit (allow-list, mapping, trait registry, MCP catalog), Integration
(registry reuse/idle/respawn, server-body lifecycle, LSP client round-trips — all
against a **deterministic fake LSP server**, never real rust-analyzer).

**Determinism:** `packages/tddy-lsp/tests/bin/fake_lsp.rs` is a fixed-response LSP
server used by every integration test (referenced via `CARGO_BIN_EXE_fake_lsp`). It
answers `initialize`/`definition`/`references`/`hover`/`documentSymbol` with known data,
emits one `publishDiagnostics` after `didOpen`, and has a "hang/ignore-shutdown" mode
for kill-escalation tests.

**Acceptance tests (headline behaviours):**

- `two_targets_in_one_workspace_reuse_a_single_language_server`
  (`packages/tddy-lsp/tests/registry_reuse_test.rs`) — the core reuse claim: one task,
  same handle for both targets.
- `finds_references_across_the_workspace_through_the_client`
  (`packages/tddy-lsp/tests/client_roundtrip_test.rs`) — references round-trip.
- `lsp_tools_are_language_agnostic_and_gated_on_availability`
  (`packages/tddy-tools` tests) — the five tools appear only behind the availability
  gate and carry no language prefix.

**Unit / integration tests:** see the changeset for the full per-crate list.

**Assertions:** exact equality on task counts (reuse == 1 task), task IDs (same handle
reused), task status transitions (`Running` until cancel → `Cancelled`), returned LSP
locations/diagnostics (fixed fake values), and the exact set of MCP tool names.

## Acceptance Criteria

- [x] `config.type` `rust_binary`/`rust_library` maps to the Rust language; unknown
      types map to no language.
- [x] The default allow-list enables Rust; a disallowed language is rejected before any
      spawn.
- [x] Two targets in the same workspace + language reuse one running server task.
- [x] Different workspace roots get separate servers.
- [x] A server task stays `Running` until cancelled, then reaches `Cancelled`.
- [x] An idle server is torn down after the timeout; activity resets the timer.
- [x] A crashed (terminal) server is re-spawned on next request.
- [x] `initialize`/`didOpen`/`definition`/`references`/`hover`/`symbols`/`diagnostics`
      round-trip against the fake server; concurrent requests correlate by id.
- [x] `LspExecutor` registry returns `None` before registration; first registration
      wins.
- [x] The five `Lsp*` MCP tools are absent without the gate and present with it; names
      are language-agnostic.
- [x] `ReadLints` routes to workspace-level LSP diagnostics (`workspace/diagnostic` pull)
      when a language server is available, else the existing no-linter stub.

## Future Considerations (Not In Scope)

- Additional languages (TypeScript / Python / Go) beyond the Rust allow-list entry.
- Rename / code-actions / formatting LSP operations.
- Sharing one server across distinct sessions in the same workspace (initial slice
  reuses across targets within an owner; cross-session sharing is a follow-up).
