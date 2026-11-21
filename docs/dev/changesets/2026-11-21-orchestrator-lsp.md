# 2026-11-21 — An orchestrator queries the LSP of any tddy worktree and adds or removes roots

**Type:** Feature · **Stack:** `#orchestra` 8/9 (PR #986) · **Packages:** `tddy-session-activity`,
`tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-tools` ·
Feature: [project-concept.md § LSP roots](../../ft/daemon/project-concept.md#lsp-roots),
[reusable-lsp.md](../../ft/coder/reusable-lsp.md), [warm-code-intelligence-daemon.md](../../ft/coder/warm-code-intelligence-daemon.md)

## Problem

The `Lsp*` tools were offered only behind a session's `TDDY_LSP_TOOLS` flag, the orchestrator role
guard refused them, and a call always ran on the caller's own worktree, so an orchestrator could not
navigate another session's or project's code.

## What changed

- **`tddy-session-activity::lsp_roots`** (new): `SessionLspRoots` — the roots a session added, in
  `<session_dir>/lsp-roots.json`, written atomically; adding twice keeps one, removing reports whether
  it was there.
- **`tddy-tool-engine::lsp_root_tools`** (new): `AddLspRoot` / `RemoveLspRoot` / `ListLspRoots`
  definitions and `execute_lsp_root_tool` over an `LspRootPort`; `orchestrator_lsp_tool_defs` (the five
  `Lsp*` with an optional `root`); `lsp_root_for` — home worktree when no root, the named root when in
  reach or added, otherwise a refusal naming `AddLspRoot`. Both sets join the project-home catalogue
  and `is_project_home_allowed_tool`.
- **`tddy-daemon-rpc::exec_tool::lsp_roots`** (new): `DaemonLspRoots` — a root outside the read reach
  goes through `RequestPathAccess`'s request (read mode) and is recorded only on Allow; removal needs
  no request. `run_orchestrator_lsp` runs an orchestrator's `Lsp*` call locally with the resolved root
  as the worktree. `path_access_port` is extracted so both tools build the same port.
- **`tddy-tools::exec_tool_admission`**: the `Lsp*` and root tools are in the remote catalogue; an
  ordinary session still gets `Lsp*` when `TDDY_LSP_TOOLS` is set (`is_advertised`).

## Decisions

- **No `UnloadRoot` RPC.** Removing a root drops it from the session's list; the index daemon's idle
  reaping unloads it once no session queries it. An explicit unload could stop a root another session
  is using, and would add a method to every `CodeIndexService` implementation.
- **The orchestrator `Lsp*` definitions live in `tddy-tool-engine`, not `tddy-lsp-executor`'s
  catalogue.** `tddy-tool-engine` does not depend on `tddy-lsp-executor` (which pulls in
  `tddy-index-daemon`, `tddy-lsp` and `tddy-build`), and the `root` argument is a daemon routing fact
  (which worktree the call runs on), not an executor one: the executor ignores `root` because the
  daemon passes the resolved root as the worktree. The two catalogues share names and descriptions;
  `mcp_tool_advertisement_audit` pins the advertised set.
- **`AddLspRoot` warms with an empty symbol query**, best effort, because the handler has no index
  channel for `code_index.Warm`. Backlog:
  [`2026-11-21-add-lsp-root-warms-by-symbol-query.md`](../todo/2026-11-21-add-lsp-root-warms-by-symbol-query.md).

## Tests

`packages/tddy-daemon-rpc/tests/orchestrator_lsp_acceptance.rs` (5 scenarios over the real
`ExecuteTool`/`ActivityService`), unit tests in `lsp_roots.rs` (4) and `lsp_root_tools.rs` (5), the
project-home catalogue and `read_only` name lists, and `tddy-tools-cli`'s
`mcp_tool_advertisement_audit`.
